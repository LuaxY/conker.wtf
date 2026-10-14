//! Server-side bots for the Deathmatch, ported from the original's multiplayer AI. Tags: V matched on
//! oracle traces, C read in the recompiled code, I inferred.
//!
//! The original drives a bot with three layers, all ported:
//! - the multiplayer AI script (`[0x14, 0xBA]`, run by [`script::Machine`]) with the game's helpers
//!   ([`brain`]): roles, targets, which weapon to fetch and where to go, fire timing and aim;
//! - the character class's behaviour scripts ([`npc::Scripts`], `[0x0F, 0]`): walk the navigation
//!   graph ([`nav`], `[0x19, scene]`), chase, stand and face;
//! - a virtual pad (buttons) the fight writes.
//!
//! A bot's character then moves like a human's here: its walk target and speed become a stick and a
//! camera yaw, its pad the buttons, and it goes through the same [`conker_rules::Match::step`].
//! The level's lifts ([`lift`]) are a level rule conker-rules lacks; the caller rides them for every
//! player before each step.
//!
//! Per frame: [`lift::Lifts::ride`] (and [`Bots::lifted`] for every slot it moved), [`Bots::tick`]
//! (fills the bot slots' inputs and returns what the script did to the match, for `Match::apply`),
//! [`Bots::scopes`] (the snipers' aim and the bots' pickup reach into the match), `Match::step`,
//! then [`Bots::observe`] with the step's events. The War modes run the script with no bot too: it
//! holds Total War's gas, Colors' flags and Beach's escapes. Notes, oracle numbers and the
//! approximations:
//! `lab/oracle/bot_README.md` (outside this tree).

pub mod brain;
pub use conker_rules::lift;
pub mod nav;
pub mod npc;
pub mod script;

use brain::{Body, Unknown, World, g};
use conker_camera::Scope;
use conker_camera::volume::{CameraHint, CameraVolume};
use conker_rules::game::{Action, Arena, BUTTON_R, Event, Match};
use conker_rules::gas::{self, Effect};
use conker_rules::heist::{self, Cash};
use conker_rules::tank;
use conker_rules::traps::{self, Traps};
use conker_rules::weapon::Gives;
use conker_rules::{Rng, beach, colors};
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::{
    BUTTON_Z, Input, angle_diff, atan2_u16, cos_sin, deadzone, stick_angle,
};
use nav::Graph;
use nav::NONE;
use npc::{AT_GOAL, Npc, Scripts, View};
use script::{Globals, Machine, Script};

/// The game's skill byte (`0x800E0BD0`) in the oracle's matches, the menu's default (NORMAL).
pub const DEFAULT_SKILL: u8 = 2;
/// The smallest stick past the deadzone: it turns a standing player without a step, and the facing
/// settles on the multiple of 256 below the stick's angle (V: conker-sim's movement).
const TURN_STICK: i8 = 8;

/// The ROM assets the bots need for one scene.
#[derive(Clone, Copy)]
pub struct Assets<'a> {
    /// `[0x19, scene]`: the navigation graph.
    pub nav: &'a [u8],
    /// `[0x0C, scene, 5]`: the level's trigger volumes (zones the path search crosses straight).
    pub triggers: &'a [u8],
    /// `[0x14, 0xBA]`: the multiplayer AI script.
    pub ai: &'a [u8],
    /// `[0x0F, class]`: the players' class data with their behaviour scripts ([`class`]).
    pub class: &'a [u8],
}

impl Assets<'_> {
    /// Asset paths for `extract_core::Rom::asset`, in field order.
    pub fn paths(scene: usize) -> [Vec<usize>; 4] {
        [
            vec![0x19, scene],
            vec![0x0C, scene, 5],
            vec![0x14, script(scene)],
            vec![0x0F, class(scene)],
        ]
    }
}

/// The class data `[0x0F, id]` the scene's players walk by: Tank's tanks their own (model 119:
/// its script 4 walks the path at 33, 5 waits, 6 chases; V b1's bots cruise at 33), every other
/// map's characters Conker's, 0.
pub fn class(scene: usize) -> usize {
    if scene == 0x24 { 119 } else { 0 }
}

/// The scene's multiplayer AI script, `[0x14, id]` (V: G+0x16BC in the oracle's RAM; 0x2B's in War: Beach's). Each holds its
/// scene's tables (the pickups' gifts and nodes, the pickup nodes an unarmed bot heads for), so
/// another scene's script sends bots to nodes that mean nothing there.
pub fn script(scene: usize) -> usize {
    match scene {
        0x30 => 0xBB,
        0x34 => 0xBD,
        0x3F => 0xBE,
        0x2B => 0xB9,
        // 0x1A's Deathmatch runs Heist's script, its FFA branch (V dm6).
        0x1A => 0xB7,
        0x24 => 0xB8,
        0x33 => 0xBC,
        _ => 0xBA,
    }
}

/// Every bot of a match, and the AI state they share.
#[derive(Clone, Debug)]
pub struct Bots {
    script: Script,
    scripts: Scripts,
    graph: Graph,
    machine: Machine,
    g: Globals,
    npcs: Vec<Npc>,
    /// The match slot of each participant, humans first at the start (the game's order); `None`:
    /// its player left ([`Bots::leave`]). Indices never move within a match ([`Bots::join`]).
    slots: Vec<Option<usize>>,
    /// Per participant: a bot.
    bot: Vec<bool>,
    /// Participants joined since the last frame ([`Bots::join`]).
    joined: Vec<usize>,
    /// Per participant: the pickup whose weapon it holds (MP+0xEB0), and who hit it last frame.
    held: Vec<i32>,
    hit_by: Vec<u8>,
    /// Per participant, a bit: hit since the script last read it ([`World::struck`]).
    struck: u32,
    /// Race (0xBC), as of the last frame: its globals are its own ([`Self::leave`]).
    race: bool,
    /// Per participant: its +0x70 when the AI last zeroed it ([`World::forgiven`]).
    forgiven: Vec<u16>,
    /// Per participant: the point it aims at this frame while R is held.
    aims: Vec<Option<[f32; 3]>>,
    /// Per participant: its scope's yaw and pitch this frame ([`Bots::scopes`]), and on a mounted
    /// gun the scope its stick turns.
    scopes: Vec<Option<(f32, f32)>>,
    guns: Vec<Scope>,
    /// Per participant: where it stood at the last [`Bots::tick`] ([`brain::Body::prev`]).
    prev: Vec<[f32; 3]>,
    noises: Vec<([f32; 3], f32)>,
    /// Per trap of 0x34: the frames its emitter is heard on ([`jets`]).
    emit: Vec<u8>,
    /// The trigger records and their switches ([`World::zones`]).
    zones: Vec<(CameraHint, bool)>,
    rng: Rng,
    skill: usize,
    started: bool,
    /// War: Heist's bag walker ([`walk`]).
    walker: Npc,
    pub unknown: Unknown,
    pub faults: u32,
}

impl Bots {
    /// The AI over a roster: the match slots played by `humans`, then by `bots` (a free slot is
    /// in neither: it is no participant).
    pub fn new(
        assets: Assets,
        humans: &[usize],
        bots: &[usize],
        skill: u8,
        rng: Rng,
    ) -> Option<Self> {
        let slots: Vec<Option<usize>> = humans.iter().chain(bots).map(|&s| Some(s)).collect();
        let n = slots.len();
        Some(Self {
            script: Script::parse(assets.ai)?,
            scripts: Scripts::parse(assets.class)?,
            graph: Graph::parse(assets.nav, assets.triggers)?,
            machine: Machine::default(),
            g: Globals::new(g::SIZE),
            npcs: vec![Npc::default(); n],
            slots,
            bot: (0..n).map(|k| k >= humans.len()).collect(),
            joined: Vec::new(),
            held: vec![-1; n],
            hit_by: vec![0; n],
            struck: 0,
            race: false,
            forgiven: vec![0; n],
            aims: vec![None; n],
            scopes: vec![None; n],
            guns: vec![Scope::default(); n],
            prev: Vec::new(),
            noises: Vec::new(),
            emit: Vec::new(),
            zones: (assets.triggers.as_chunks::<0x34>().0.iter())
                .map(|r| {
                    let zone = CameraVolume::parse_record(r);
                    let (on_floor, kind) = (r[0x16] & 1 != 0, r[0x17]);
                    let hint = CameraHint {
                        zone,
                        on_floor,
                        kind,
                        command: 0,
                    };
                    (hint, r[0x14] == 1)
                })
                .collect(),
            rng,
            skill: usize::from(skill.min(4)),
            started: false,
            walker: Npc::default(),
            unknown: Unknown::new(),
            faults: 0,
        })
    }

    /// The slots played by bots.
    pub fn bot_slots(&self) -> Vec<usize> {
        (self.slots.iter().zip(&self.bot))
            .filter_map(|(&s, &bot)| s.filter(|_| bot))
            .collect()
    }

    /// A player joined mid-match in `slot`: the script's globals and the match's state stay, only
    /// this participant's memory starts over. It takes the lowest participant a leaver freed, else
    /// the next (at most 16, the struct's arrays: a 17th is not seen), and its team goes into the
    /// script's G+0xE64 on the next frame ([`Bots::tick`]; War: Beach's joiner appears then, as
    /// every participant does on the first frame). ponytail: the script splits humans from bots by
    /// index, counted once at the start (G+0x1EC8), so a human appended after the bots is one of
    /// the script's bots until the next match (no 90 s deadline in Beach, no traitor check in
    /// Total War); the natives here go by [`Body::is_bot`]. Remapping every per-participant array
    /// and every participant-valued global of each script is what would lift it.
    pub fn join(&mut self, slot: usize, bot: bool) {
        let k = (self.slots.iter())
            .position(Option::is_none)
            .unwrap_or(self.slots.len());
        if k >= 16 {
            return;
        }
        if k == self.slots.len() {
            self.slots.push(None);
            self.bot.push(false);
        }
        (self.slots[k], self.bot[k]) = (Some(slot), bot);
        self.forget(k);
        self.joined.push(k);
    }

    /// The player in `slot` left mid-match: its participant stays, vacant (out of play for the
    /// script, query 0x1D, and with no team: [`brain::World`]), for the next to join. Its team goes
    /// at once (no human takes a bot over by it), and the frame helper sees it go out once, as a
    /// death (War: Beach's participants were all seen out on the first frame).
    pub fn leave(&mut self, slot: usize) {
        if let Some(k) = self.by_slot(slot) {
            self.slots[k] = None;
            self.forget(k);
            if !self.race {
                self.g.set(g::TEAM, k, -1);
                self.g.set_i32(g::SEEN, self.g.i32(g::SEEN) & !(1 << k));
            }
        }
    }

    /// Participant `k`'s own memory as at the start.
    fn forget(&mut self, k: usize) {
        let n = self.slots.len();
        self.npcs.resize(n, Npc::default());
        self.held.resize(n, -1);
        self.hit_by.resize(n, 0);
        self.forgiven.resize(n, 0);
        self.aims.resize(n, None);
        self.scopes.resize(n, None);
        self.guns.resize(n, Scope::default());
        self.npcs[k] = Npc::default();
        (self.held[k], self.hit_by[k], self.forgiven[k]) = (-1, 0, 0);
        self.struck &= !(1 << k);
        (self.aims[k], self.scopes[k], self.guns[k]) = (None, None, Scope::default());
    }

    /// The bots' RNG as far as it ran (the next match goes on from it).
    pub fn rng(&self) -> Rng {
        self.rng
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The AI's view of participant `k` (humans first): its AI state (G+0x19C), goal node and script.
    pub fn state(&self, k: usize) -> Option<(i32, u8, u8)> {
        let n = self.npcs.get(k)?;
        Some((self.g.get(g::STATE, k), n.goal, n.script))
    }

    /// The AI's memory: the game's multiplayer struct and the script's globals ([`brain::g`]).
    pub fn globals(&self) -> &Globals {
        &self.g
    }

    /// Where the bot in match slot `slot` aims this frame, while it holds R: the game's aim point
    /// (G+0x17A8.., with the skill's lag and jitter). The game fires a bot's shots along this aim
    /// (s126+0x16C yaw, +0x170 pitch), not along its facing (V bot_bullets_lldb.txt: facing 300.9°,
    /// aim and bullets 293.4°): [`Bots::scopes`] hands conker-rules the ray from the eye through it.
    pub fn aim(&self, slot: usize) -> Option<[f32; 3]> {
        *self.aims.get(self.by_slot(slot)?)?
    }

    /// Participant `k`'s walk and fight state (the game object's AI fields).
    pub fn npc(&self, k: usize) -> Option<&Npc> {
        self.npcs.get(k)
    }

    /// Writes each bot's scope into the match before [`Match::step`]: while a bot holds R with weapon
    /// 0x14, 0x24, the crossbow 0x18 or the rocket 0x16 drawn, the game sets its scope flag
    /// (s126+0x197 = 2: V, 3,793 of 3,793 rows with 0x14 and 3,161 of 3,161 with 0x24 in bot_long3,
    /// 6,556 of 6,560 in bot_long2; 0x18: I, as a human's aim, V combat5_ctlw18; 0x16: V bots1's
    /// bazooka Tedi, facing = aim yaw) and its shot follows the aim's yaw and pitch (s126+0x16C,
    /// +0x170; 0x16: V beach-gunner r1, 31 rockets to 0.3°), which conker-rules reads from
    /// `body.scope_yaw` and `scope_pitch`. Other weapons fire along the facing there (twin guns:
    /// flag 0, V), but a bot's bullets follow its aim ray (s126+0x13C, +0x130: `Match::aims`, from
    /// the eye 67.5 up through [`Self::aim`]), the twin guns' toward what it meets, the pistol's
    /// and 0x23's along its pitch, so they reach a target above or below it.
    ///
    /// And every slot's reach for pickups (`Match::reach`, s126+0x1AF): a bot's as its script
    /// last set it (command 0x7B, [`Npc::pickup`]), a human's the default. Without it a bot took
    /// every pickup it ran over (a Colors collector, the melee weapon on its way to the red flag at
    /// node 109, then turned back to fetch its preferred weapon again, and never took the flag).
    pub fn scopes(&self, m: &mut Match) {
        for (k, scope) in self.scopes.iter().enumerate().filter(|k| self.bot[k.0]) {
            if let Some(f) = self.slots[k].and_then(|s| m.players.get_mut(s)) {
                f.body.scope_yaw = scope.map(|s| s.0);
                f.scope_pitch = scope.map_or(0.0, |s| s.1);
            }
        }
        m.aims = (0..m.players.len())
            .map(|s| {
                let k = self.by_slot(s).filter(|&k| self.bot[k])?;
                let (at, p) = (self.aims[k]?, m.players[s].body.pos);
                let eye = [p[0], p[1] + 67.5, p[2]];
                let d: [f32; 3] = core::array::from_fn(|j| at[j] - eye[j]);
                let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                (len > 0.0).then(|| (eye, d.map(|c| c / len)))
            })
            .collect();
        m.reach = (0..m.players.len())
            .map(|s| match self.by_slot(s) {
                Some(k) if self.bot[k] => self.npcs[k].pickup,
                _ => conker_rules::weapon::REACH,
            })
            .collect();
    }

    /// The participant playing match slot `slot`.
    pub fn by_slot(&self, slot: usize) -> Option<usize> {
        self.slots.iter().position(|&s| s == Some(slot))
    }

    /// The match slot of participant `k` (`None`: none, or its player left).
    pub fn slot(&self, k: usize) -> Option<usize> {
        self.slots.get(k).copied().flatten()
    }

    /// A lift ([`lift::Lifts::ride`]) moved `slot`: its path restarts from where it now stands
    /// ([`Npc::lifted`]). Without it a bot lifted off its route keeps walking toward a node on the
    /// other floor, into the tower's wall. The AI replans too ([`Npc::placed`]): without that, a bot
    /// whose goal lies inside a lift keeps it and rides back every ~36 frames.
    pub fn lifted(&mut self, slot: usize) {
        if let Some(n) = self.by_slot(slot).and_then(|k| self.npcs.get_mut(k)) {
            n.lifted();
        }
    }

    /// Takes in what last frame's [`Match::step`] reported: pickups, hits, and the noises the AI
    /// hears ([`noises`]).
    pub fn observe(&mut self, events: &[Event], m: &Match) {
        self.noises = noises(m);
        self.noises.extend(jets(&m.traps, &mut self.emit));
        let k = |slot: usize| self.slots.iter().position(|&s| s == Some(slot));
        for e in events {
            match *e {
                Event::Pickup { player, pickup } => {
                    // The gift as `mirror` keeps it (2222: grenades), for any map's pickups.
                    if let Some(p) = k(player).filter(|_| self.g.get(g::GIVES, pickup) != 2222) {
                        self.held[p] = pickup as i32;
                    }
                }
                Event::Hit {
                    victim, attacker, ..
                } => {
                    if let (Some(v), Some(a)) = (k(victim), k(attacker)) {
                        self.hit_by[v] = 0x80 | a as u8;
                    }
                    if let Some(v) = k(victim) {
                        self.struck |= 1 << v;
                    }
                }
                Event::Shot { .. } | Event::Explosion { .. } | Event::Voice { .. } => {}
                Event::Respawn { player, .. } => {
                    if let Some(p) = k(player) {
                        self.npcs[p].placed = true;
                    }
                }
                // The human has the bot's pickup and last attacker now (the script copied its
                // globals); both bodies moved.
                Event::TakeOver { player, from } => {
                    if let (Some(i), Some(j)) = (k(player), k(from)) {
                        (self.held[i], self.hit_by[i]) = (self.held[j], self.hit_by[j]);
                        self.npcs[i].placed = true;
                        self.npcs[j].placed = true;
                    }
                }
            }
        }
    }

    /// One frame, before [`Match::step`]: the AI script and helpers on the match as it stands, then
    /// every bot's behaviour script and steering; writes each bot slot's input into `inputs` and
    /// returns the script's effects on the match (`Match::apply` them before the step).
    pub fn tick(
        &mut self,
        m: &Match,
        arena: &Arena,
        world: &CollisionWorld,
        inputs: &mut [Input],
    ) -> Vec<Effect> {
        let bodies = self.bodies(m);
        self.mirror(m, arena);
        self.race = m.settings.race;
        let st = &m.settings;
        let ffa = !st.total_war && !st.colors && !st.beach && !st.heist && !st.raptor;
        // The joiners: their team as the script's start writes it (`team << 1` in Total War's
        // 0xBA at 0x734, the team in Colors' and Beach's, the participant in a Deathmatch), and
        // a fresh start in the frame helper's eyes. War: Beach's appear on this frame (0x1D leaves
        // them out once), as every participant does on the first ([`World::appear`]).
        let mut appear = match m.settings.beach && !self.started {
            true => (1 << bodies.len()) - 1,
            false => 0,
        };
        // Race's globals overlap the frame helper's (0xBC keeps its racers at G+0x18, its route at
        // G+0x35C): a joiner there is the script's own business.
        let joined = std::mem::take(&mut self.joined);
        for k in joined.into_iter().filter(|_| !m.settings.race) {
            let team = i32::from(bodies[k].team);
            let code = match () {
                _ if ffa => k as i32,
                _ if m.settings.total_war => team << 1,
                _ => team,
            };
            self.g.set(g::TEAM, k, code);
            self.g.set_i32(g::SEEN, self.g.i32(g::SEEN) & !(1 << k));
            self.g.set(g::DEAD, k, 0);
            self.g.set_i32(g::COUNT, bodies.len() as i32);
            if m.settings.beach {
                appear |= 1 << k;
            }
        }
        let mut unknown = std::mem::take(&mut self.unknown);
        let mut effects = Vec::new();
        // A canister's or Tank's bomb's toucher, a match slot, as the participant the script
        // indexes by.
        let toucher =
            |s: Option<usize>| s.and_then(|s| self.by_slot(s)).map_or(0, |k| k as i32 + 1);
        let bomb = usize::try_from(m.tank.bomb.flag - 1).ok();
        let touched = [
            toucher(m.gas.toucher(0)),
            toucher(m.gas.toucher(1)),
            toucher(bomb),
        ];
        {
            let mut host = World {
                bodies: &bodies,
                npcs: &mut self.npcs,
                graph: &self.graph,
                world,
                rng: &mut self.rng,
                skill: self.skill,
                zones: &mut self.zones,
                unknown: &mut unknown,
                ffa,
                forgiven: &mut self.forgiven,
                slots: &self.slots,
                touched,
                effects: &mut effects,
                flags: m.colors.flags,
                starting: !self.started,
                gunners: (0..m.guns.len()).map(|k| m.gunner(k)).collect(),
                plunger: m.plunger,
                appear,
                cash: match m.settings.heist {
                    true => Some(brain::Heist {
                        cash: m.heist.cash,
                        scores: m.heist.scores,
                        walker: &mut self.walker,
                    }),
                    false => None,
                },
                raptor: m.settings.raptor.then_some(&m.raptor),
                race: m.settings.race.then_some((m, arena.track.nodes.len())),
                tank: m.settings.tank,
                struck: &mut self.struck,
            };
            if !self.started {
                self.started = true;
                if self
                    .machine
                    .run(&self.script, &mut self.g, &mut host, 1)
                    .is_err()
                {
                    self.faults += 1;
                }
                host.starting = false;
            }
            if self
                .machine
                .run(&self.script, &mut self.g, &mut host, 2)
                .is_err()
            {
                self.faults += 1;
            }
            // War: Beach: the engine's triggers 0x26 and 0x27 (type 0, kind 5) put a Tedi off the
            // villagers' beaches (I: done here, where the trigger table is).
            if m.settings.beach {
                for (k, b) in bodies.iter().enumerate() {
                    let at = (beach::EJECT.iter()).find(|e| host.inside(k, e.0));
                    if let (Some(&(_, at)), Some(slot)) = (at, self.slots[k])
                        && b.team == beach::TEDIZ
                        && b.health > 0
                    {
                        host.effects.push(Effect::Place(slot, at));
                    }
                }
            }
        }
        self.unknown = unknown;
        self.hit_by.fill(0);
        // The gas is Total War's (the other scripts' props and globals are their own): its
        // countdown, and the forced item G+0xFF0 as the canister carried (ponytail: from the frame
        // the script writes it, a frame before the game's item shows it, V d1 1040 → 1041; its
        // weapon manager reads it in the next frame's).
        if m.settings.total_war {
            effects.push(Effect::Countdown(self.g.i32(g::COUNTDOWN)));
            for (k, slot) in self.slots.iter().enumerate() {
                let carries = self.g.get(g::OVERRIDE, k) == i32::from(gas::CANISTER);
                if let Some(slot) = *slot
                    && m.players.get(slot).is_some_and(|f| f.carrying != carries)
                {
                    effects.push(Effect::Carry(slot, carries));
                }
            }
        } else if m.settings.tank {
            // Tank's bomb countdown (G+0x2008, 300 from the plant).
            effects.push(Effect::Countdown(self.g.i32(g::TANK_COUNTDOWN)));
        } else if !m.settings.colors
            && !m.settings.beach
            && !m.settings.heist
            && !m.settings.raptor
            && !m.settings.race
        {
            effects.clear();
        }
        // Heist: the bag walks home on its own, once on its feet (class 0x57's scripts).
        if m.settings.heist && m.heist.cash.walks(m.tick) {
            effects.extend(walk(&mut self.walker, &m.heist.cash, &self.graph, world));
        }
        let mut players: Vec<[f32; 3]> = bodies.iter().map(|b| b.pos).collect();
        // Colors' collectors run at a flag by its handle (command 0x19, [`Npc::target`]); Heist's
        // at the bag.
        if m.settings.heist {
            let i = usize::from(heist::HANDLE);
            if players.len() <= i {
                players.resize(i + 1, [0.0; 3]);
            }
            players[i] = m.heist.cash.pos;
        }
        if m.settings.colors {
            for (f, def) in m.colors.flags.iter().zip(&colors::FLAGS) {
                let i = usize::from(def.handle);
                if players.len() <= i {
                    players.resize(i + 1, [0.0; 3]);
                }
                players[i] = f.pos;
            }
        }
        let bots = (self.slots.iter().zip(&self.bot).zip(&bodies))
            .zip(
                self.npcs
                    .iter_mut()
                    .zip(&mut self.aims)
                    .zip(self.scopes.iter_mut().zip(&mut self.guns)),
            )
            .filter_map(|(((&slot, &bot), &b), rest)| Some((slot.filter(|_| bot)?, b, rest)));
        for (slot, b, ((n, aimed), (scope, gun))) in bots {
            (*aimed, *scope) = (None, None);
            let view = View {
                pos: b.pos,
                facing: b.facing,
                players: &players,
            };
            if b.health == 0 {
                *n = Npc {
                    script: 1,
                    ..Npc::default()
                };
                if let Some(i) = inputs.get_mut(slot) {
                    *i = Input::default();
                }
                continue;
            }
            // Race: the board's autopilot steers and speeds (the match's); the script's presses (Z
            // a power-up, B a kick) are the pad. A Z stays in it until the next press (V L1: the bot
            // that used a canister at 542 kept Z, s126+0x8C, and fired each missile the frame after
            // its take, 928 and 938; the two whose last press was B kept theirs all race).
            if m.settings.race {
                let (held, pressed) = n.pad();
                if let Some(i) = inputs.get_mut(slot) {
                    *i = Input {
                        buttons: held | pressed | n.last & BUTTON_Z,
                        ..Input::default()
                    };
                }
                continue;
            }
            // War: Raptor: a raptor's behaviours are class 83's, which the steering runs as
            // Conker's ([`brain::conker_behaviour`]). 0x22, the bite: Z once free and facing its
            // target within 30°, else it turns to it first (I: class 83's behaviour 34 is not in
            // the pack; the oracle's bot bites are often cut short by its next move, b1).
            let raptor = m.settings.raptor && b.class == conker_rules::raptor::CLASS;
            let bite = raptor && n.script == brain::BITE;
            if raptor {
                n.script = brain::conker_behaviour(n.script);
            }
            n.run(&self.scripts, &self.graph, &view, &mut self.rng);
            if raptor {
                n.script = brain::raptor_behaviour(n.script);
            }
            let mut face = None;
            if bite {
                n.script = brain::BITE;
                let free = m
                    .players
                    .get(slot)
                    .is_some_and(|f| f.action == Action::Free);
                let to = players.get(usize::from(n.target)).copied();
                let off = to.map(|t| {
                    angle_diff(atan2_u16(b.pos[2] - t[2], t[0] - b.pos[0]), b.facing).abs()
                });
                if free && off.is_some_and(|d| d < 0x1555) && b.buttons & BUTTON_Z == 0 {
                    (n.pressed, n.press) = (BUTTON_Z, 0);
                }
                face = to.filter(|_| off.is_some_and(|d| d >= 0x1555));
            }
            let steer = n.steer(&view);
            let (held, pressed) = n.pad();
            if held & 0x10 == 0 {
                n.aim_yaw = None;
            }
            let aim = n.aim.take().filter(|_| held & 0x10 != 0);
            *aimed = aim;
            *scope = aim
                .filter(|_| matches!(b.drawn, 0x14 | 0x24 | 0x18 | 0x16))
                .map(|a| look(b.pos, a, world));
            // On a mounted gun, R opens the gun's scope (`func_151279A0`, C) and the AI's stick
            // turns it (`brain::World::attack`); let go, it shuts.
            let stick = n.stick.take();
            if b.drawn == conker_camera::GUN && held & 0x10 != 0 {
                gun.steer(b.heading, stick.unwrap_or_default());
                *scope = Some((gun.yaw, gun.pitch));
            } else {
                *gun = Scope::default();
            }
            let toward = aim.unwrap_or(steer.toward);
            if m.settings.tank {
                // The turret's target (s126+0x129): the script's enemy (G+0x11C, set by the attack
                // states 4 and 5) while alive, within 4000 (state 5's range) and in sight (I), led
                // by its last step · distance / 100 (`func_150ED748`'s bot branch: its speed ·
                // distance / 200 at half-speed steps, C).
                let k = self.slots.iter().position(|&s| s == Some(slot));
                let eye = |p: [f32; 3]| [p[0], p[1] + tank::MUZZLE[1], p[2]];
                let lead = (k.and_then(|k| usize::try_from(self.g.get(g::ENEMY, k)).ok()))
                    .and_then(|e| bodies.get(e))
                    .filter(|t| t.health > 0)
                    .and_then(|t| {
                        let d =
                            ((t.pos[0] - b.pos[0]).powi(2) + (t.pos[2] - b.pos[2]).powi(2)).sqrt();
                        let seen = brain::sight(world, &self.graph, eye(b.pos), eye(t.pos));
                        (d < 4000.0 && seen.is_some()).then(|| {
                            let k = d / 100.0;
                            [
                                t.pos[0] + (t.pos[0] - t.prev[0]) * k,
                                t.pos[1],
                                t.pos[2] + (t.pos[2] - t.prev[2]) * k,
                            ]
                        })
                    });
                let mut input = tank_drive(b, steer.toward, steer.speed, lead);
                input.camera_yaw = input
                    .camera_yaw
                    .wrapping_add((i32::from(n.error) * 10 * 65536 / 360) as u16);
                // Z, the script's press (`brain::World::tank_fire`, on the skill's interval for
                // the enemy's distance); the rules fire on it once the gun is ready. The error
                // also shortens the gun's pitch (the rules').
                input.buttons = (held | pressed) & BUTTON_Z;
                let error = n.error as u8;
                if m.players.get(slot).is_some_and(|f| f.aim_error != error) {
                    effects.push(Effect::AimError(slot, error));
                }
                // A class script's op 0x67 this frame sets the mover's rates for it alone (the
                // tank's update puts back 0x32 each).
                let rates = std::mem::replace(&mut n.rates, [0x32; 2]);
                if rates != [0x32; 2] {
                    effects.push(Effect::TankRates(slot, rates));
                }
                if let Some(i) = inputs.get_mut(slot) {
                    *i = input;
                }
                continue;
            }
            let mut input = match face {
                Some(t) => drive(b, t, 1.0, false, world),
                None => drive(b, toward, steer.speed, aim.is_some(), world),
            };
            input.buttons = held | pressed;
            if matches!(b.drawn, 0x16 | 0x23) {
                // I: the game's bot aims with R held (its own aim mode, s126+0x197 = 2) and fires
                // these two that way; conker-rules models the human's R, which blocks their fire
                // (combat4_README §3), so a bot lets go of R with them.
                input.buttons &= !BUTTON_R;
            }
            if let Some(i) = inputs.get_mut(slot) {
                *i = input;
            }
        }
        self.prev = bodies.iter().map(|b| b.pos).collect();
        effects
    }

    /// The participants as the AI reads them; a vacant one far away, dead, of no team and place.
    fn bodies(&self, m: &Match) -> Vec<Body> {
        let places = m.places();
        // Only an untimed Deathmatch or Total War takes lives ([`Body::lives`]).
        let st = &m.settings;
        let counted = st.time.is_none() && !st.colors && !st.beach;
        self.slots
            .iter()
            .enumerate()
            .map(|(k, &s)| {
                let Some((s, f)) = s.and_then(|s| Some((s, m.players.get(s)?))) else {
                    return Body {
                        pos: [1e6, -1e6, 1e6],
                        team: u8::MAX,
                        place: u8::MAX,
                        ..Body::default()
                    };
                };
                let item = f.item(m.tick) & 0x7F;
                Body {
                    pos: f.body.pos,
                    old: f.body.pos,
                    prev: (self.prev.get(k).copied())
                        .filter(|_| !self.joined.contains(&k))
                        .unwrap_or(f.body.pos),
                    height: if f.body.airborne {
                        f.body.pos[1] - f.body.floor_y
                    } else {
                        0.0
                    },
                    heading: f.body.heading,
                    facing: f.body.facing,
                    health: f.health,
                    item,
                    // Not `f.drawn`, which lags the draw and the holster: a bot kept counting
                    // through its holster and its last B, on the holster's end, drew again
                    // (0x34's state-5 stall: draw 40 frames, drawn 45, holster 29, over and over).
                    drawn: f.camera_weapon(),
                    burning: f.burn.is_some(),
                    grenades: f.grenades,
                    hit_by: self.hit_by[k],
                    is_bot: self.bot[k],
                    team: f.team,
                    suicides: f.suicides,
                    place: places[s],
                    stunned: f.stun != 0,
                    buttons: f.buttons,
                    scope: f.body.scope_yaw.map(|y| (y, f.scope_pitch)),
                    class: f.class,
                    prey: f.prey,
                    walled: f.body.walled,
                    out: (f.out > 0).then(|| m.tick.saturating_sub(f.out_at)),
                    lives: if counted {
                        u8::try_from(m.lives_left(s)).unwrap_or(u8::MAX)
                    } else {
                        conker_rules::colors::LIVES
                    },
                }
            })
            .collect()
    }

    /// What the game's weapon manager keeps in the struct, from the match: the pickups' gifts and
    /// nodes, who holds which, the noises heard.
    fn mirror(&mut self, m: &Match, arena: &Arena) {
        let g = &mut self.g;
        g.set_i32(g::PICKUPS, arena.pickups.len() as i32);
        g.set_i32(g::PICKUPS + 4, arena.pickups.len() as i32);
        g.set_i32(g::RESPAWN, arena.respawn.map_or(240, |f| 2 * i32::from(f)));
        let roams = arena.roaming.count;
        for (k, (p, _)) in arena.pickups.iter().enumerate() {
            let gives = match p.gives {
                Gives::Weapon(w) => i32::from(w),
                Gives::Grenades => 2222,
                Gives::Mask => i32::from(gas::MASK),
                Gives::Power(bit) => i32::from(bit),
            };
            g.set(g::GIVES, k, gives);
            // 0x1A's: 3 the roaming ones (1 in a Heist of at most 4 players, `Match::unique`), 2 the
            // fixed (V dm6, h4a); 6 elsewhere.
            let flags = match roams {
                0 => 6,
                _ if k < roams && m.unique() => 1,
                _ if k < roams => 3,
                _ => 2,
            };
            g.set(g::PICKUP_FLAGS, k, flags);
            // NODE_OF[k] holds pickup k's node wherever it stands, so its spot is k. Only a flag-1
            // pickup records its holder; the others' stays -1 (`func_1508DA1C`, `func_1508DC24`,
            // C): 0xB7 goes for the pickups whose holder is -1 and timer 0.
            g.set(g::PLACE, k, k as i32);
            g.set(g::SPOT, k, k as i32);
            let holder = (m.pickups.get(k).and_then(|p| p.holder))
                .and_then(|s| self.slots.iter().position(|&x| x == Some(s)));
            g.set(g::HOLDER, k, holder.map_or(-1, |p| p as i32));
            g.set(g::PROP, k, 0x3001 + k as i32);
            let node = (self.graph).nearest(m.pickup_at(arena, k), Some(0), None).0;
            g.set(
                g::NODE_OF,
                k,
                self.graph.node(node).map_or(-1, |n| i32::from(n.id)),
            );
            let t = m.pickups.get(k).map_or(0, |s| i32::from(s.timer) * 2);
            g.set(g::PICKUP_TIMER, k, t);
        }
        for (k, &s) in self.slots.iter().enumerate() {
            // A mask worn keeps its pickup's index: the gas spares whoever's G+0x1174[G+0xEB0] is
            // 0x20 (C, the script at 0x3E7A).
            let gives = |f: &conker_rules::Fighter| match f.weapon {
                Some(w) => Some(Gives::Weapon(w)),
                None if f.power != 0 => Some(Gives::Power(f.power)),
                None => f.masked.then_some(Gives::Mask),
            };
            match s.and_then(|s| m.players.get(s)).and_then(gives) {
                None => self.held[k] = -1,
                Some(want) if self.held[k] < 0 => {
                    self.held[k] = (arena.pickups.iter())
                        .position(|(p, _)| p.gives == want)
                        .map_or(-1, |i| i as i32);
                }
                Some(_) => {}
            }
            g.set(g::HELD, k, self.held[k]);
        }
        let noises = std::mem::take(&mut self.noises);
        let n = noises.len().min(8);
        g.set_i8(g::NOISES, n as i8);
        for (k, (pos, r)) in noises.into_iter().take(8).enumerate() {
            let at = g::NOISE + 12 * k as i32;
            g.set_f32(at, r * r);
            let xyz = pos.map(|v| v as i32 as i16);
            for (j, v) in xyz.iter().enumerate() {
                let o = (at + 4 + 2 * j as i32) as usize;
                g.0[o..o + 2].copy_from_slice(&v.to_be_bytes());
            }
        }
    }
}

/// 0x34's fire jets as the AI hears them: while a plate's jets burn, the emitter of each row's middle
/// jet (the record's command 1) is heard 500 around (`func_151A6600` → `func_1508B20C` while its
/// +0x127 ≠ 0, C), from the jets' first burning frame to the frame after their last, 121 frames.
/// The electric floor has no emitter. V noise34 (plate 6 stood on from read 80, the jets burning
/// 83-202 as trap34): records 28, 31 and 64 heard 83-203; o34emit, bots' presses: 121 frames, the
/// three middle jets of plate 6, then of plate 7. A bot that hears one walks out of the 500
/// (`World`'s `noises`). `emit`: per trap, the frames its emitter has left.
fn jets(t: &Traps, emit: &mut Vec<u8>) -> Vec<([f32; 3], f32)> {
    emit.resize(t.list.len(), 0);
    let middle = |k: &usize| t.list[*k].command == 1 && !t.list[*k].electric;
    (0..t.list.len())
        .filter(middle)
        .filter_map(|k| {
            emit[k] = match t.left[k] == traps::FIRE_FRAMES - 1 {
                true => traps::FIRE_FRAMES as u8 + 1,
                false => emit[k].saturating_sub(1),
            };
            (emit[k] > 0).then(|| (t.list[k].zone.zone.center.map(f32::from), 500.0))
        })
        .collect()
}

/// The bag's walker reaches a node this near it, across (I: the game's NPC mover, `func_1504A730`
/// on obj+0x25C, is unported; this clears the bag's turning circle at 17.5 a frame).
const ARRIVE: f32 = 60.0;

/// War: Heist's bag walking home: class 0x57's script 5 (`0xD2` the next node, `0x57` at speed 35,
/// `0xD3` until at the goal) from node to node toward its goal, avoiding the node 0x5D named,
/// turning at most [`heist::TURN`] a frame, from 5 a frame up to 17.5, on the floor; at the goal it
/// stands (AT_GOAL, script 4). Its step for the match, if it walks or just stopped.
/// ponytail: no wall test (the nav graph's lines keep it off walls); I the speeds' shape.
fn walk(n: &mut Npc, c: &Cash, graph: &Graph, world: &CollisionWorld) -> Option<Effect> {
    let walking = c.state == heist::State::Walk;
    let stop = Effect::CashAt(c.pos, c.yaw, false);
    if n.script != 5 || n.flags & AT_GOAL != 0 || n.goal == NONE {
        return walking.then_some(stop);
    }
    let avoid: Vec<u8> = (n.avoid != NONE).then_some(n.avoid).into_iter().collect();
    if n.cur == NONE {
        n.cur = graph.approach(c.pos, &[n.goal], &avoid);
    }
    let across = |p: [f32; 3]| (p[0] - c.pos[0]).hypot(p[2] - c.pos[2]);
    while n.cur != NONE && across(graph.at(n.cur)) < ARRIVE {
        if n.cur == n.goal {
            n.flags |= AT_GOAL;
            n.script = 4;
            return walking.then_some(stop);
        }
        let next = graph.next_toward(n.cur, &[n.goal], &avoid).0;
        if next == n.cur {
            break;
        }
        n.cur = next;
    }
    if n.cur == NONE {
        return walking.then_some(stop);
    }
    n.length = graph.next_toward(n.cur, &[n.goal], &avoid).1;
    let to = graph.at(n.cur);
    let want = atan2_u16(-(to[2] - c.pos[2]), to[0] - c.pos[0]);
    let turn = angle_diff(want, c.yaw).clamp(-i32::from(heist::TURN), i32::from(heist::TURN));
    let yaw = c.yaw.wrapping_add(turn as u16);
    n.speed = match walking {
        true => (n.speed + heist::WALK_GAIN).min(heist::WALK_TOP),
        false => heist::WALK_FROM,
    };
    let (cs, sn) = cos_sin(yaw);
    let (x, z) = (c.pos[0] + n.speed * cs, c.pos[2] - n.speed * sn);
    let y = (world.floor(x, z, c.pos[1], c.pos[1], None)).map_or(c.pos[1], |f| f.y);
    Some(Effect::CashAt([x, y, z], yaw, true))
}

/// The noises the AI hears from the frame [`Match::step`] just ran (`func_1508B20C`'s callers,
/// C), each 900 around: a grenade's wind-up at its thrower, from 18 frames into the pull until it
/// leaves the hand (`func_15033AD8`), then its fuse at the grenade until it goes off
/// (`func_15054F74`). V q5gren2: pulled at read 42, the thrower heard 60-84, the grenade 85-203;
/// q5cook: 60-116, 117-177. No gun is heard, mounted or not, and a blast is silent (V q5m2: 22 MG
/// shots, no noise; q5gren2, q5cook: none after the fuse; C: no caller under the MG's fire
/// `func_150FC614`). 0x34's fire jets are [`jets`]. ponytail: the flame stream's noise is not
/// ported (`func_1519A9A4`, V q3s5b3: at the stream's middle, half its length around, to 352).
fn noises(m: &Match) -> Vec<([f32; 3], f32)> {
    /// The throw's release, frames into it (conker-rules' `RELEASE`).
    const RELEASE: u32 = 14;
    let t = m.tick.saturating_sub(1);
    let throwers = m.players.iter().filter_map(|f| {
        let pull = match f.action {
            Action::GrenadePull { pull } | Action::GrenadeHold { pull } => pull,
            // Item 0x3A leaves the hand on the throw's first frame (I: its noise as 0x26's).
            Action::GrenadeThrow { pull, .. } if t < f.since + RELEASE && !f.lobs() => pull,
            _ => return None,
        };
        (t >= pull + 18).then_some((f.body.pos, 900.0))
    });
    let fuses = m.grenades.iter().map(|(_, g)| (g.pos, 900.0));
    throwers.chain(fuses).collect()
}

/// The yaw and pitch in degrees (the scope's: yaw 0 along +x, 90 along −z; pitch positive down)
/// from a bot's eye to its aim point. The game's pitch is `func_1505A630(distance, s126+0x140 −
/// aim height)` (C, `func_1508CAD8`); I: the eye s126+0x140 taken 67.5 above the feet, the bot's
/// sight eye.
fn look(pos: [f32; 3], at: [f32; 3], world: &CollisionWorld) -> (f32, f32) {
    let (dx, dy, dz) = (at[0] - pos[0], pos[1] + 67.5 - at[1], at[2] - pos[2]);
    let angle = |x: f32, y: f32| match &world.acos {
        Some(a) => a.angle(x, y),
        None => atan2_u16(y, x),
    };
    let deg = |a: u16| f32::from(a) * 0.005_493_164;
    let pitch = deg(angle((dx * dx + dz * dz).sqrt(), dy));
    let pitch = if pitch > 180.0 { pitch - 360.0 } else { pitch };
    (deg(atan2_u16(-dz, dx)), pitch)
}

/// A walk target and speed as a stick and camera yaw (I: the human's +1 per frame acceleration and
/// turn caps then apply, not the bot class's). Aiming at a standstill, the stick turns the facing on
/// the spot onto the aim, to within 128 (0.7°: the game's bot fires along its exact aim yaw, but
/// conker-rules fires along the facing).
fn drive(b: Body, toward: [f32; 3], speed: f32, aiming: bool, world: &CollisionWorld) -> Input {
    let (dx, dz) = (toward[0] - b.pos[0], toward[2] - b.pos[2]);
    if dx == 0.0 && dz == 0.0 {
        return Input::default();
    }
    let mut angle = atan2_u16(-dz, dx);
    let raw: i8 = if speed >= conker_sim::movement::RUN_SPEED {
        82
    } else if speed >= 0.5 {
        ((speed / conker_sim::movement::STICK_SPEED).ceil() as i32 + 7).min(82) as i8
    } else if aiming {
        angle = angle.wrapping_add(128);
        TURN_STICK
    } else {
        return Input::default();
    };
    let forward = stick_angle(0, deadzone(raw), world.acos.as_ref());
    Input {
        stick_x: 0,
        stick_y: raw,
        buttons: 0,
        camera_yaw: angle.wrapping_sub(forward),
    }
}

/// Tank: a walk target and speed as the hull's stick (turned onto it, 18 units a stick unit at
/// rest; forward while it lies within 45° of the facing, the stick whose target is the speed: 52
/// for the class scripts' 33, I), and the turret's aim as the camera yaw (conker-rules turns a
/// bot's turret toward it): `aim`, else ahead.
fn tank_drive(b: Body, toward: [f32; 3], speed: f32, aim: Option<[f32; 3]>) -> Input {
    let (dx, dz) = (toward[0] - b.pos[0], toward[2] - b.pos[2]);
    let (x, y) = if speed >= 0.5 && (dx != 0.0 || dz != 0.0) {
        let d = angle_diff(atan2_u16(-dz, dx), b.facing);
        let y = ((speed / conker_sim::tank::DRIVE).round() as i32).min(75);
        (
            (-d / 18).clamp(-75, 75),
            if d.abs() < 0x2000 { y } else { 0 },
        )
    } else {
        (0, 0)
    };
    let raw = |v: i32| (v + 7 * v.signum()) as i8;
    let yaw = aim.map_or(b.facing, |a| atan2_u16(b.pos[2] - a[2], a[0] - b.pos[0]));
    Input {
        stick_x: raw(x),
        stick_y: raw(y),
        buttons: 0,
        camera_yaw: yaw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_sim::movement::angle_diff;

    #[test]
    fn the_stick_points_where_asked() {
        let world = CollisionWorld::default();
        let b = Body {
            pos: [0.0, 0.0, 0.0],
            ..Body::default()
        };
        for (to, speed, raw) in [
            ([100.0, 0.0, 0.0], 35.0, 82),
            ([0.0, 0.0, -100.0], 10.0, 24),
            ([-3.0, 0.0, 7.0], 0.5, 8),
        ] {
            let i = drive(b, to, speed, false, &world);
            assert_eq!(i.stick_y, raw);
            let target = conker_sim::movement::target_angle(&i, 0, deadzone(i.stick_y));
            assert_eq!(target, atan2_u16(-to[2], to[0]), "{to:?}");
        }
        // Standing: no stick, unless aiming: then a turn on the spot onto the aim.
        assert_eq!(
            drive(b, [100.0, 0.0, 0.0], 0.0, false, &world),
            Input::default()
        );
        let big = 100_000.0;
        let floor = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, -big]], 0),
            ([[big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 0),
        ]);
        for to in [[0.0, 0.0, 100.0], [100.0, 0.0, 35.0], [-50.0, 0.0, -80.0]] {
            let mut p = conker_sim::movement::PlayerState::spawn([0.0; 3], 0);
            for _ in 0..12 {
                let b = Body {
                    pos: p.pos,
                    facing: p.facing,
                    ..Body::default()
                };
                conker_sim::movement::step(&mut p, &drive(b, to, 0.0, true, &floor), &floor);
            }
            let aim = atan2_u16(-to[2], to[0]);
            assert!(
                angle_diff(p.facing, aim).abs() <= 128,
                "{to:?}: {} vs {aim}",
                p.facing
            );
            assert_eq!(p.pos, [0.0; 3], "turning on the spot");
        }
    }

    /// A grenade's noises against the oracle (q5gren2, q5cook: the pad shows C-down at reads 41-43,
    /// or 41-101): its thrower is heard from read 60 to the release, then the grenade until it goes
    /// off, and nothing after. The twin guns' shots are heard nowhere.
    #[test]
    fn a_grenade_is_heard_and_nothing_else() {
        use conker_rules::game::{BUTTON_B, C_DOWN};
        use conker_rules::{Fighter, Placement};

        let big = 20_000.0;
        let world = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ]);
        let arena = Arena {
            spawns: Vec::new(),
            pickups: Vec::new(),
            ..Default::default()
        };
        let start = || {
            let at = |x| Placement {
                index: 0,
                pos: [x, 0.0, 0.0],
                facing: 0,
                camera: [0.0; 3],
            };
            let players = vec![
                Fighter::new(&at(0.0), 0, 0),
                Fighter::new(&at(-5000.0), 1, 0),
            ];
            Match::new(&arena, players, Rng::default(), 0)
        };
        let cases = [
            (41..=43, 60..=84, 85..=203),
            (41..=101, 60..=116, 117..=177),
        ];
        for (held, thrower, grenade) in cases {
            let mut m = start();
            let (mut heard_thrower, mut heard_grenade) = (Vec::new(), Vec::new());
            for t in 0..300 {
                let mut inputs = [Input::default(); 2];
                if held.contains(&t) {
                    inputs[0].buttons = C_DOWN;
                }
                m.step(&arena, &inputs, &world);
                for (pos, r) in noises(&m) {
                    assert_eq!(r, 900.0);
                    match pos == m.players[0].body.pos {
                        true => heard_thrower.push(t),
                        false => heard_grenade.push(t),
                    }
                }
            }
            assert_eq!(heard_thrower, thrower.collect::<Vec<_>>());
            assert_eq!(heard_grenade, grenade.collect::<Vec<_>>());
        }
        let mut m = start();
        m.give(0, 0x09);
        let mut shots = 0;
        for t in 0..120 {
            let mut inputs = [Input::default(); 2];
            inputs[0].buttons = match t {
                0 => BUTTON_B,
                30.. => BUTTON_Z,
                _ => 0,
            };
            let ev = m.step(&arena, &inputs, &world);
            shots += ev
                .iter()
                .filter(|e| matches!(e, Event::Shot { .. }))
                .count();
            assert!(noises(&m).is_empty(), "frame {t}");
        }
        assert!(shots > 0);
    }

    /// 0x34's jets against the oracle (noise34: plate 6 stood on from read 80, its jets burning
    /// 83-202 as trap34): the middle jet of the row is heard 500 around on frames 83-203; its
    /// neighbours and the electric floor (plate 9, stood on too) never are.
    #[test]
    fn the_jets_are_heard_while_they_burn() {
        let record = |at: [i16; 3], arg: u32, id: u32, command: u32| {
            let mut r = [0u8; 0x34];
            for (k, v) in at.iter().enumerate() {
                r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
            }
            (r[0x15], r[0x17]) = (12 << 2, 9);
            for (o, w) in [(0x18, arg), (0x1C, id), (0x20, command)] {
                r[o..o + 4].copy_from_slice(&w.to_be_bytes());
            }
            r
        };
        let table = [
            record([-2442, -382, 3257], 4, 0, 0),
            record([-2587, -379, 3247], 4, 1, 1),
            record([-2710, -379, 3231], 4, 2, 0),
            record([301, -401, 2624], 8, 0, 0),
        ]
        .concat();
        let mut t = Traps::new(traps::SCENE, &table);
        let (mut emit, mut burning, mut heard) = (Vec::new(), Vec::new(), Vec::new());
        for frame in 0..300 {
            if t.hits(1) {
                burning.push(frame);
            }
            let on = (frame >= 80).then_some(0);
            t.step([on, None, None, on]);
            for (pos, r) in jets(&t, &mut emit) {
                assert_eq!((pos, r), ([-2587.0, -379.0, 3247.0], 500.0));
                heard.push(frame);
            }
        }
        assert_eq!(burning, (83..=202).collect::<Vec<_>>());
        assert_eq!(heard, (83..=203).collect::<Vec<_>>());
    }

    #[test]
    fn the_scope_looks_at_the_aim_point() {
        let world = CollisionWorld::default();
        // 100 along +x, level with the eye (67.5 up): yaw 0, no pitch.
        let (yaw, pitch) = look([0.0; 3], [100.0, 67.5, 0.0], &world);
        assert!(yaw.abs() < 0.01 && pitch.abs() < 0.01, "{yaw} {pitch}");
        // 100 along −z and 100 below the eye: yaw 90, pitch 45 down.
        let (yaw, pitch) = look([0.0; 3], [0.0, -32.5, -100.0], &world);
        assert!(
            (yaw - 90.0).abs() < 0.01 && (pitch - 45.0).abs() < 0.01,
            "{yaw} {pitch}"
        );
        // Above the eye: negative pitch.
        let (_, pitch) = look([0.0; 3], [100.0, 167.5, 0.0], &world);
        assert!((pitch + 45.0).abs() < 0.01, "{pitch}");
    }
}
