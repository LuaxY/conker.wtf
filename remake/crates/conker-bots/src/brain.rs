//! The multiplayer AI: the level script ([`crate::script`]) and the game's C helpers it calls through
//! natives, sharing the script's globals, which are the game's multiplayer struct (`*0x800D23B0`; the
//! offsets below are that struct's). Ported from the code (C) unless tagged.
//!
//! Participants are indexed as the game does: humans first, then bots ([`crate::Bots`] maps them to
//! the match's slots); a roster change keeps the indices (a leaver's falls vacant, a joiner takes
//! the lowest vacant or comes last).

use crate::nav::{Graph, NONE};
use crate::npc::{AT_GOAL, Npc};
use crate::script::{Globals, Host};
use conker_camera::volume::CameraHint;
use conker_rules::Rng;
use conker_rules::beach::Plunger;
use conker_rules::colors::{self, Pose};
use conker_rules::damage;
use conker_rules::gas::{self, Effect};
use conker_sim::collision::CollisionWorld;
use std::collections::BTreeMap;

/// Offsets in the multiplayer struct (G). `[p]` arrays hold one s32 per participant.
pub mod g {
    pub const HANDLE: i32 = 0x0;
    pub const CUR: i32 = 0x4;
    pub const COUNT: i32 = 0x10;
    pub const ALIVE: i32 = 0x14;
    pub const SEEN: i32 = 0x18;
    pub const REPLAN: i32 = 0x1C;
    pub const REEVAL: i32 = 0x5C;
    pub const BAND: i32 = 0x9C;
    pub const ENEMY: i32 = 0x11C;
    pub const STATE: i32 = 0x19C;
    pub const ATTACKING: i32 = 0x21C;
    pub const COOL: i32 = 0x25C;
    pub const FIRE: i32 = 0x29C;
    pub const TARGET: i32 = 0x31C;
    pub const DEAD: i32 = 0x35C;
    pub const HEALTH: i32 = 0x39C;
    pub const HIT_BY: i32 = 0x3DC;
    pub const JUMPED: i32 = 0x41C;
    pub const THREAT_DIST: i32 = 0x45C;
    pub const THREAT: i32 = 0x49C;
    pub const GRENADES: i32 = 0x4DC;
    pub const NEAREST: i32 = 0x51C;
    pub const HEARD: i32 = 0x55C;
    pub const DIED_AT: i32 = 0x59C;
    pub const RANK: i32 = 0x5DC;
    pub const RANK_SCORE: i32 = 0x9DC;
    pub const TEAM_COUNT: i32 = 0xDDC;
    pub const TEAM: i32 = 0xE64;
    pub const PICKUPS: i32 = 0xEA4;
    pub const RESPAWN: i32 = 0xEAC;
    pub const HELD: i32 = 0xEB0;
    pub const PROP: i32 = 0xEF0;
    pub const HOLDER: i32 = 0xF70;
    pub const OVERRIDE: i32 = 0xFF0;
    pub const PICKUP_TIMER: i32 = 0x1070;
    pub const GIVES: i32 = 0x1174;
    pub const PICKUP_FLAGS: i32 = 0x11F4;
    pub const PLACE: i32 = 0x1274;
    pub const NODE_OF: i32 = 0x14F4;
    pub const TIMER3: i32 = 0x1574;
    pub const PREFER: i32 = 0x15B4;
    pub const DT: i32 = 0x16B4;
    pub const HUMAN_SCALE: i32 = 0x16B8;
    pub const SCRIPT_ID: i32 = 0x16BC;
    pub const SEES: i32 = 0x16C0;
    pub const RR_A: i32 = 0x1700;
    pub const RR_B: i32 = 0x1701;
    pub const RR_C: i32 = 0x1702;
    pub const RR_D: i32 = 0x1704;
    pub const JIT_X: i32 = 0x1705;
    pub const JIT_Z: i32 = 0x1715;
    pub const JIT_SX: i32 = 0x1725;
    pub const JIT_SZ: i32 = 0x1735;
    pub const NOISES: i32 = 0x1745;
    pub const NOISE: i32 = 0x1748;
    pub const AIM_X: i32 = 0x17A8;
    pub const AIM_Y: i32 = 0x17E8;
    pub const AIM_Z: i32 = 0x1828;
    pub const AIM_RATE: i32 = 0x1868;
    /// Total War's gas (`[0x14, 0xBA]` at 0x3DC6): frames until the gas kills (`conker_rules::gas`).
    pub const COUNTDOWN: i32 = 0x1FE4;
    /// Bytes of the struct plus the scripts' own globals: 0xBB's reach 0x26A8 (its pickup choice
    /// at 0x2658-0x2694); in s5's RAM the next block starts at G+0x26B0.
    pub const SIZE: usize = 0x26B0;
}

/// The bot skill tables (`0x800872B0`.., indexed by the skill byte `0x800E0BD0`: 0-4, INBRED,
/// CRAP, NORMAL, BASTARD, EINSTEIN; the oracle's is 2). Five entries each (V: RAM, 2026-10-11).
mod skill {
    /// `0x800872B0`: f32 scale of the distance to a human in the target ranking.
    pub const HUMAN_SCALE: [f32; 5] = [1.5, 1.2, 0.8, 0.4, 0.15];
    /// `0x800872C4` (s16): VIs of attacking one target before the shot goes for the head (−1 never).
    pub const HEAD_AFTER: [i32; 5] = [-1, -1, 600, 240, 0];
    /// `0x800872D0`: VIs between fire cycles.
    pub const CYCLE: [i32; 5] = [0x50, 0x46, 0x3C, 0x3C, 0x2D];
    /// `0x800872D8`: VIs a twin-gun burst lasts.
    pub const BURST: [i32; 5] = [0x0A, 0x14, 0x1E, 0x3C, 0x2D];
    /// `0x800872E0`: the fire counter's start while not attacking (negated).
    pub const FIRST: [i32; 5] = [0x78, 0x50, 0x28, 0x0A, 0x00];
    /// `0x800872E8`: percent under which the aim stays on target each cycle.
    pub const STEADY: [i32; 5] = [0x0A, 0x14, 0x32, 0x50, 0x64];
    /// `0x800872F0`: G+0xC.
    pub const G_C: [i32; 5] = [0x1E, 0x19, 0x14, 0x0F, 0x0F];
    /// `0x800872F8`: f32 aim following rate per VI.
    pub const AIM_RATE: [f32; 5] = [0.05, 0.1, 0.125, 0.25, 1.0];
}

/// A participant as the AI sees it each frame (filled from the match by [`crate::Bots`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Body {
    pub pos: [f32; 3],
    pub old: [f32; 3],
    /// obj+0x2C as the AI reads it, before the frame's updates: where it stood a frame earlier
    /// (the lead of skills 3 and 4, [`World::attack`]).
    pub prev: [f32; 3],
    /// Height above the floor (obj+0x28).
    pub height: f32,
    /// The facing as the game's heading in degrees (obj+0x40).
    pub heading: f32,
    pub facing: u16,
    pub health: u8,
    /// s126+0x75 & 0x7F: the held item; s126+0x78: the drawn weapon, from the draw's first frame
    /// and 0 from the holster's (`Fighter::camera_weapon`; V o34draw: the scripts' holster
    /// counter G+0x2208 restarts 2 reads after it goes 0).
    pub item: u8,
    pub drawn: u8,
    /// s126+0x1AC: a fire burns on it (set by the flame's reaction 0x10 and the fire's attach,
    /// cleared at its end: `func_1505E874`, `func_15194FF4`, `func_151906C4`, C).
    pub burning: bool,
    pub grenades: u8,
    /// 0x80 | the last attacker, set on the frame of a hit (`0x800E0BB8`).
    pub hit_by: u8,
    pub is_bot: bool,
    pub team: u8,
    /// gGameState+0x70+0xC·p: the deaths it caused on its own team, its own included.
    pub suicides: u16,
    /// `0x800E0AC0[p]` (`func_151E565C`): the place, 1 the best; the scripts ask the humans'.
    pub place: u8,
    /// obj+0x104 ≠ 0: hit (query 0xA8, a flag carrier's drop).
    pub stunned: bool,
    /// The pad's buttons (a human's: query 0xED cuts the capture's freeze short).
    pub buttons: u16,
    /// s126+0x16C, +0x170: the scope's yaw and pitch while it is open (a bot on a mounted gun aims
    /// it with its stick, [`World::attack`]).
    pub scope: Option<(f32, f32)>,
    /// The record's class (+0, `func_150859AC` 1; War: Beach's voices by it).
    pub class: u8,
    /// obj+0xF4 bit 0x10000: the wall routine moved it last frame (conker-sim's `walled`).
    pub walled: bool,
}

/// Natives the AI called that this port does not implement, by (native, handle type, code); the
/// harness reports them (they must stay at 0 in a match).
pub type Unknown = BTreeMap<(i32, i32, i32), u32>;

/// What the natives work on: the participants, the navigation graph, the world, the RNG.
pub struct World<'a> {
    pub bodies: &'a [Body],
    pub npcs: &'a mut [Npc],
    pub graph: &'a Graph,
    pub world: &'a CollisionWorld,
    pub rng: &'a mut Rng,
    pub skill: usize,
    /// The level's trigger records (`*0x800D3098`, the zone natives' ids), each with its switch
    /// (`+0x14` = 1: off), which type-4 commands 0 and 1 flip.
    pub zones: &'a mut [(CameraHint, bool)],
    /// Each pickup's position (the arena's), for its nearest node.
    pub unknown: &'a mut Unknown,
    /// The free-for-all flag `0x8008FDC8` (player query 0xD5): 1 in Deathmatch, 0 in Total War,
    /// where the script runs its team branches.
    pub ffa: bool,
    /// Per participant, its +0x70 ([`Body::suicides`]) when the AI last zeroed it (command 0x17,
    /// `func_1508B1D4`): the match keeps the count, the AI counts on from there.
    pub forgiven: &'a mut [u16],
    /// The match slot of each participant; `None`: vacant, its player left (it never appears, of no
    /// team: [`World::frame`]).
    pub slots: &'a [Option<usize>],
    /// Each gas canister's +0x80 at the frame's start, in participants (+ 1; 0 none), and what the
    /// script does to the match (`conker_rules::gas`).
    pub touched: [i32; 2],
    pub effects: &'a mut Vec<Effect>,
    /// War: Colors' flags at the frame's start (their carriers in match slots).
    pub flags: [colors::Flag; 2],
    /// The script's level-start entry runs: `obj` only looks a flag up (it appears with the
    /// script's own create, 300 frames in: I, the objects stand unchanged in RAM before, e1).
    pub starting: bool,
    /// Each mounted gun's gunner (a match slot; `conker_rules::gun`), by the gun's index.
    pub gunners: Vec<Option<usize>>,
    /// War: Beach's plunger at the frame's start (its toucher a match slot).
    pub plunger: Plunger,
    /// War: Beach, the participants that appear this frame, a mask: all on the first, a joiner on
    /// its own (`query(0x50B9, 0x1D)` leaves them out once, so the respawn handler sees them and
    /// the deadline starts 49 frames in: V `beach.md` §2.4, x1's strike at read 2772; I for the
    /// other modes, left as they were).
    pub appear: i32,
}

impl Host for World<'_> {
    fn native(&mut self, g: &mut Globals, index: i32, args: &[i32]) -> i32 {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        match index {
            // Debug prints.
            0..=2 | 13 => 1,
            3 | 4 => self.object(index, a(0), args),
            5 => self.query(g, a(0), a(1), args),
            6 => self.command(g, a(0), a(1), args),
            7 => match a(0) {
                0 => 0,
                n => (self.rng.next_u32() % n as u32) as i32,
            },
            8 => 1,
            9 => {
                let t = a(0);
                match t {
                    0 => 1,
                    t if t < 0 && t + 1 >= 0 => 0,
                    t => t + 1,
                }
            }
            10 => {
                let t = a(0);
                if t >= 2 && t - 1 < 2 {
                    1
                } else if t == 1 {
                    0
                } else {
                    t - 1
                }
            }
            11 => ((i64::from(a(0)) * i64::from(a(1))) >> 16) as i32,
            12 => match a(1) {
                0 => 0,
                b => ((i64::from(a(0)) << 16) / i64::from(b)) as i32,
            },
            _ => self.unknown(index, 0, 0),
        }
    }
}

fn kind(h: i32) -> i32 {
    (h >> 12) & 0xF
}

fn index(h: i32) -> usize {
    (h & !0xF000) as usize
}

/// War: Colors' flag a handle names (`0x2000 |` its object index, [`colors::FLAGS`]).
fn flag(h: i32) -> Option<usize> {
    let i = (kind(h) == 2).then(|| index(h))?;
    colors::FLAGS
        .iter()
        .position(|f| usize::from(f.handle) == i)
}

/// The mounted gun a handle names, by its index in the match: 0x30's (scene uids 0x13, 0x14 less
/// one, V `query(uid, 0xB7)`; 0x34's one takes the first), and War: Beach's 0x2010 (uid 0x11, the
/// map's only gun; no participant has an index past 15).
fn gun(h: i32) -> Option<usize> {
    match (kind(h), index(h)) {
        (2, i @ (0x12 | 0x13)) => Some(i - 0x12),
        (2, 0x10) => Some(0),
        _ => None,
    }
}

impl World<'_> {
    fn unknown(&mut self, native: i32, kind: i32, code: i32) -> i32 {
        *self.unknown.entry((native, kind, code)).or_default() += 1;
        0
    }

    /// Participant `p` is inside trigger `t`: its bit in the player's trigger set (`func_150A29C8`,
    /// `0x800D30F0`), which `func_150A11C4` fills with `func_150A1DA0` (mode 0) at the start of each
    /// player's update. Taken here on the current position (V: 79 of the 80 set bits and every
    /// clear one of the type-3 triggers in the RAM of s4, s5 and s7; the miss, a player that had
    /// moved since).
    pub(crate) fn inside(&self, p: usize, t: usize) -> bool {
        match (self.bodies.get(p), self.zones.get(t)) {
            (Some(b), Some((z, off))) => !off && z.contains(b.old, b.pos, b.pos[1] - b.height),
            _ => false,
        }
    }

    /// Natives 3 and 4 (handle tables `0x80088420` / `0x8008845C`): start and stop. In Deathmatch
    /// only the level-start fades (type 0xE) reach them. Total War's gas starts effects (0xE013 a
    /// player's gassed screen, [`Effect::Haze`]; 0xE016 the room's alarm lights, 0xE017 a
    /// canister's light) and sounds (type 7: 0x74CF the gas's loop, which it keeps the handle of
    /// and stops; 0x7599 and 0x7640 a canister taken): the client's to show from the match's
    /// state, but the gas's sound, whose span is the script's ([`Effect::Gas`]), and the gassed
    /// screen, whose players are the script's (those outside trigger 0).
    fn object(&mut self, native: i32, h: i32, args: &[i32]) -> i32 {
        match (native, kind(h)) {
            (3, 0xE) if h == 0xE013 => {
                if let Some(slot) = self.slot(args.get(1).copied().unwrap_or(-1)) {
                    self.effects.push(Effect::Haze(slot));
                }
                0
            }
            // War: Beach's deadline strike on participant `a(1)` (`func_150FCFD4`); its smoke,
            // light and sounds are the client's, as 0xE01B's detonation is.
            (3, 0xE) if h == 0xE02A => {
                if let Some(slot) = self.slot(args.get(1).copied().unwrap_or(-1)) {
                    self.effects.push(Effect::Strike(slot));
                }
                0
            }
            (_, 0xE) => 0,
            (3, 7) if h == 0x74CF => {
                self.effects.push(Effect::Gas(true));
                h
            }
            (3, 7) => 0,
            (4, 7) => {
                self.effects.push(Effect::Gas(false));
                0
            }
            // Colors (0xBB): `obj(0x20uu)` creates the flag of scene uid uu and answers its handle;
            // `obj2` destroys it. The mounted guns (uids 0x13, 0x14), destroyed and recreated after
            // each capture: the create brings gun uid − 0x13 back afresh ([`Effect::Gun`]; I: answered
            // as their handles were).
            (3, 2) => match (colors::FLAGS.iter()).position(|f| usize::from(f.uid) == index(h)) {
                Some(k) => {
                    if !self.starting {
                        self.effects.push(Effect::Flag(k, true));
                    }
                    0x2000 | i32::from(colors::FLAGS[k].handle)
                }
                None if matches!(index(h), 0x13 | 0x14) => {
                    if !self.starting {
                        self.effects.push(Effect::Gun(index(h) - 0x13));
                    }
                    h - 1
                }
                None => self.unknown(native, 2, -1),
            },
            (4, 2) => match flag(h) {
                Some(k) => {
                    self.effects.push(Effect::Flag(k, false));
                    0
                }
                None if gun(h).is_some() => 0,
                None => self.unknown(native, 2, -1),
            },
            (_, k) => self.unknown(native, k, -1),
        }
    }

    /// The placement of the prop the script names `h` (`0x30nn`, nn its id), if the gas drives it.
    fn prop(h: i32) -> Option<u16> {
        (gas::PROPS.iter())
            .find(|p| i32::from(p.1) == h & 0xFFF)
            .map(|p| p.0)
    }

    /// The match slot of participant `p`.
    fn slot(&self, p: i32) -> Option<usize> {
        usize::try_from(p)
            .ok()
            .and_then(|p| self.slots.get(p).copied().flatten())
    }

    /// The vacant participants, a mask.
    fn vacant(&self) -> i32 {
        (self.slots.iter().enumerate())
            .filter(|s| s.1.is_none())
            .fold(0, |m, (k, _)| m | 1 << k)
    }

    fn query(&mut self, g: &mut Globals, h: i32, code: i32, args: &[i32]) -> i32 {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        if let Some(k) = flag(h) {
            return self.flag_query(k, code, args);
        }
        match (kind(h), code) {
            // `func_150AE140`: a handle of type 0 answers 0 (0xBE asks 0xCA of a participant index).
            (0, _) => 0,
            // Type 1 (`func_1509C7C8`) 4: no cutscene running.
            (1, 4) => 1,
            // `func_15083E0C`: the object index of a scene uid (Colors' guns, 0x13 and 0x14).
            (2, 0xB7) => index(h) as i32 - 1,
            // A mounted gun's +0x13C: its gunner (a participant) + 100, else 0 (`func_150BDB70`, C).
            (2, 0xBC) if gun(h).is_some() => (gun(h).and_then(|k| self.gunners.get(k)))
                .copied()
                .flatten()
                .and_then(|s| self.slots.iter().position(|&x| x == Some(s)))
                .map_or(0, |p| p as i32 + 100),
            (2, _) => self.player_query(index(h), code, args),
            // Type 3 (props, `func_1509ED74`) 1: their state; only the team modes' flags use it.
            (3, 1) => 0,
            // War: Beach's plunger (id 0xFF): 0x1E its y (+0x12), 0x17 who stood on it last (its
            // toucher | 0x2000; none: 0).
            (3, 0x1E) if h & 0xFFF == 0xFF => self.plunger.y as i32,
            (3, 0x17) if h & 0xFFF == 0xFF => (self.plunger.toucher)
                .and_then(|s| self.slots.iter().position(|&x| x == Some(s)))
                .map_or(0, |p| p as i32 | 0x2000),
            // 0x21: +0x80, a gas canister's toucher + 1 (0 for the other props here).
            (3, 0x21) => match Self::prop(h) {
                Some(n) => (gas::CANISTERS.iter())
                    .position(|c| c.0 == n)
                    .map_or(0, |c| self.touched[c]),
                None => 0,
            },
            // Type 4, the triggers (`func_150A04D8`): 6, the player `a(2)` is inside; 16, someone is.
            // ponytail: a camera handle (6) and 16's camera test (`func_150A2D84`, the humans' view
            // eyes) read outside; the views are the clients'.
            (4, 6) => i32::from(kind(a(2)) == 2 && self.inside(index(a(2)), index(h))),
            (4, 0x10) => i32::from((0..self.bodies.len()).any(|p| self.inside(p, index(h)))),
            // Type 5, the level (`func_1509FE0C`).
            (5, 0x1D) => {
                let appear = if self.starting { 0 } else { self.appear };
                ((1 << self.bodies.len()) - 1) & !self.vacant() & !appear
            }
            (5, 0x1E) => i32::from(self.bodies.get(a(2) as usize).map_or(0, |b| b.team)),
            (5, 0x22) => {
                let n = a(2).max(0) as usize;
                let w: Vec<i32> = (0..n).map(|k| a(3 + k)).collect();
                let total: i32 = w.iter().sum();
                let r = (self.rng.next_f32() * total as f32) as i32;
                let mut sum = 0;
                for (k, &x) in w.iter().enumerate() {
                    sum += x;
                    if sum >= r && x != 0 {
                        return k as i32;
                    }
                }
                n as i32 - 1
            }
            // The humans, the participants before the first bot (the script reads it at its start).
            (5, 0x23) => (self.bodies.iter())
                .position(|b| b.is_bot)
                .unwrap_or(self.bodies.len()) as i32,
            (5, 0x24) => 0,
            (5, 0x25) => 0,
            (5, 0x26) => self.bodies.len() as i32,
            // Music 3 on 0x61A8 (`func_151F2CDC() == 1`): War: Beach's escape jingle is playing.
            // ponytail: never (the clients play it), so a human's freeze is always 150 frames.
            (6, 3) => 0,
            // Type 15, the multiplayer helpers (`func_1509E3DC`).
            (0xF, 3) => self.next_ai(g),
            (0xF, 4) => self.visibility(g, a(2) as usize, a(3)),
            (0xF, 5) => self.skill as i32,
            (0xF, 6) => self.pick_target(g, a(2) as usize),
            (0xF, 7) => give_range(a(2)),
            // `func_1508B194`: a human's +0x70 (a bot's 0); the team script turns a human with more
            // than 3 on everyone's bots (0x4CFE, "Killed too many team mates!!!") until it dies.
            (0xF, 0x11) => self.human(a(2)).map_or(0, |p| {
                i32::from(self.bodies[p].suicides.saturating_sub(self.forgiven[p]))
            }),
            (k, c) => self.unknown(5, k, c),
        }
    }

    fn command(&mut self, g: &mut Globals, h: i32, code: i32, args: &[i32]) -> i32 {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        // Colors' flags: 2 (touchable, which the create makes it here) and 0x7D (its radar blip,
        // the client's: not carried) ask nothing of the match; 0x23 clears "just let go of".
        if let Some(k) = flag(h) {
            return match code {
                2 | 0x7D => 1,
                0x23 => {
                    self.effects.push(Effect::Settle(k));
                    1
                }
                c => self.unknown(6, 2, c),
            };
        }
        match (kind(h), code) {
            (2, _) => self.player_command(index(h), code, args),
            // Props (type 3) 0 shows a prop; 4 sets +0x7C.., +0x80 and +0x84 (0x7FFFFFFF keeps
            // one): camera/zone setup at level start, and the gas's props ([`Effect`]). The level
            // (5) 0xA, 0xF: HUD and music; (0xA, 0) a timer display.
            (3, 0) => {
                if let Some(n) = Self::prop(h) {
                    self.effects.push(Effect::Show(n));
                }
                1
            }
            (3, 4) => {
                if let Some(placement) = Self::prop(h) {
                    let keep = |v: i32| (v != 0x7FFF_FFFF).then_some(v);
                    self.effects.push(Effect::Set {
                        placement,
                        flag: keep(a(3)),
                        gate: keep(a(4)),
                    });
                }
                1
            }
            (5, 0xA) | (5, 0xF) | (0xA, 0) => 1,
            // Colors: 0x10 a start record's respawn mask (the match's `spawn::COLORS_MASKS_30`
            // already); 0x12 the screen's fade around the capture's respawn (out to black in 60
            // VIs, back in 100: [`Effect::Fade`]); kind 6's 0 on 0x61A4 / 0x61A7 at a capture (I:
            // unread, a team's jingle by its handles).
            (5, 0x10) | (6, 0) => 1,
            (5, 0x12) => {
                self.effects.push(Effect::Fade(a(6) == 1, a(5) as u16));
                1
            }
            // Props 7: a display flag (+0x73 bit 4); sounds (type 0xA, `func_1509E730`) 3: a fade.
            // Props 1 hides a prop: 0xBA's Deathmatch hides the masks' props 0x14-0x19 (0x48DE),
            // and 0xBD's camera cutaway on 0x34 (0x335C) hides 0xF3-0xF5 while a human's view is
            // in trigger 0x12-0x14 (C; `(4, 0x10)` here asks for any participant instead, I). The
            // client's to show; no noise comes of it (V o34emit: G+0x23F0.. stay 1 while 0x34's
            // emitters come and go).
            (3, 1 | 7) | (0xA, 3) => 1,
            // War: Beach's plunger: 0xD sets +0x4F's bits a(2) and clears a(3); bit 4 holds it down.
            (3, 0xD) if h & 0xFFF == 0xFF => {
                match (a(2) & 4 != 0, a(3) & 4 != 0) {
                    (true, _) => self.effects.push(Effect::Plunger(true)),
                    (_, true) => self.effects.push(Effect::Plunger(false)),
                    _ => {}
                }
                1
            }
            // Cameras (type 9) 0xC: War: Beach's cut to the detonation (one viewport): the client's.
            (9, 0xC) => 1,
            // Triggers 0 and 1 (`func_150A2864`): switch one on, or off (`+0x14`).
            (4, c @ (0 | 1)) => {
                if let Some(z) = self.zones.get_mut(index(h)) {
                    z.1 = c == 1;
                }
                1
            }
            (0xF, 4) => {
                self.init(g);
                0
            }
            (0xF, 6) => self.frame(g),
            (0xF, 0xA) => {
                // func_1508DA1C: no pickup held or carried, all present.
                for p in 0..self.bodies.len() {
                    g.set(g::HELD, p, -1);
                }
                0
            }
            (0xF, 0xB) => 0, // the weapon manager: mirrored from the match ([`crate::Bots`])
            // `func_1508DAEC`: release the pickup held (0xBA calls it in the gas only: the masks).
            (0xF, 0xC) => {
                if let Some(slot) = self.slot(a(2)) {
                    self.effects.push(Effect::Release(slot));
                }
                0
            }
            (0xF, 0xD) => {
                self.threats(g);
                0
            }
            // The team modes' frame (0xC88) ranks and shares out targets itself.
            (0xF, 5) => {
                self.rank(g);
                0
            }
            (0xF, 7) => {
                self.team_threats(g);
                0
            }
            (0xF, 0x17) => {
                if let Some(p) = self.human(a(2)) {
                    self.forgiven[p] = self.bodies[p].suicides;
                }
                0
            }
            (0xF, 1) => 0,
            (k, c) => {
                let _ = a(0);
                self.unknown(6, k, c)
            }
        }
    }

    /// Participant `p` if it is a human with a +0x70 slot here.
    fn human(&self, p: i32) -> Option<usize> {
        usize::try_from(p).ok().filter(|&p| {
            p < self.forgiven.len()
                && self.slot(p as i32).is_some()
                && self.bodies.get(p).is_some_and(|b| !b.is_bot)
        })
    }

    /// `func_1508C9CC` (query 3): the next AI participant after G+0x1702, round robin.
    fn next_ai(&mut self, g: &mut Globals) -> i32 {
        let n = self.bodies.len() as i32;
        let mut at = i32::from(g.i8(g::RR_C));
        let mut found = -1;
        let mut k = 0;
        while k < n {
            at += 1;
            if at >= n {
                at = 0;
            }
            if self.bodies[at as usize].is_bot {
                k = n;
                found = at;
            }
            k += 1;
        }
        g.set_i8(g::RR_C, at as i8);
        found
    }

    /// `func_15089BC0` (command 4) for the AI script: the struct's start, with `func_1508BC20`'s
    /// random preference matrix.
    fn init(&mut self, g: &mut Globals) {
        let n = self.bodies.len();
        prefer(g, n, self.rng);
        for b in [g::RR_A, g::RR_B, g::RR_C, g::RR_C + 1, g::RR_D] {
            g.set_i8(b, 0);
        }
        g.0[g::SCRIPT_ID as usize..g::SCRIPT_ID as usize + 2]
            .copy_from_slice(&0xBAu16.to_be_bytes());
        g.set_f32(g::HUMAN_SCALE, skill::HUMAN_SCALE[self.skill]);
        g.set_i8(g::NOISES, 0);
        for p in 0..16 {
            g.set(g::SEES, p, 0);
            g.set_f32(g::AIM_X + 4 * p as i32, 0.0);
            g.set_f32(g::AIM_Y + 4 * p as i32, 0.0);
            g.set_f32(g::AIM_Z + 4 * p as i32, 0.0);
            g.set_f32(g::AIM_RATE + 4 * p as i32, skill::AIM_RATE[self.skill]);
        }
        g.set_i32(0xC, skill::G_C[self.skill]);
        g.set_i32(g::COUNT, n as i32);
    }

    /// A player query (`func_15099C14`).
    fn player_query(&mut self, p: usize, code: i32, args: &[i32]) -> i32 {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        let Some(&b) = self.bodies.get(p) else {
            return match code {
                0x9A => 0x7D00,
                _ => 0,
            };
        };
        let n = self.npcs[p];
        match code {
            0x91 => i32::from(n.script),
            0x93 => i32::from(b.health),
            // `func_150859AC` 3: the stats record's +8 (`0x800D2144 + 0x1C·p`), 6 for every player
            // in the RAM of s4, s5 and s7; 1: its +0, the class (War: Beach's voices).
            0xA5 if a(2) == 3 => 6,
            0xA5 if a(2) == 1 => i32::from(b.class),
            // s126+0x78, the drawn item (War: Beach's weapon zones).
            0xAA => i32::from(b.drawn),
            // `func_150A2AEC`: the first listed trigger the player is in, | 0x4000; else 0.
            0xAC => (args.get(2..).unwrap_or_default().iter())
                .map(|&t| (t & 0xFFF) as usize)
                .find(|&t| self.inside(p, t))
                .map_or(0, |t| t as i32 | 0x4000),
            // obj+0x13C, what it rides: nothing here (0 in every dump).
            0xBC => 0,
            // Colors: it holds the flag `a(2)` (obj+0x13C = the flag + 100); it was hit (+0x104);
            // a human's buttons, the pad byte `0x800BE710 + 2p` (A, B, Z, start; I its byte).
            0x95 => i32::from(flag(a(2)).is_some_and(|k| {
                self.slot(p as i32)
                    .is_some_and(|s| self.flags[k].carrier == Some(s))
            })),
            0xA8 => i32::from(b.stunned),
            0xED => i32::from(b.buttons >> 8),
            0x9A => match self.at(a(2)) {
                Some(o) => {
                    let (dx, dz) = (b.pos[0] - o[0], b.pos[2] - o[2]);
                    (dx * dx + dz * dz).sqrt() as i32
                }
                None => 0x7D00,
            },
            0xBF => (b.height as i32 as f32 * 65536.0) as i32,
            // Bit 0x10000: the wall moved it (C `func_15058898`, V q2f4): the scripts press Z+A then,
            // the high jump (0xBB at 0x1CA6, 0xBD at 0x10FA, 0xBA at 0x1AEC).
            0xC3 => n.f4 as i32 | if b.walled { 0x10000 } else { 0 },
            0xC4 => n.flags as i32,
            0xC7 => {
                if n.goal == NONE {
                    1
                } else {
                    (n.flags & AT_GOAL) as i32
                }
            }
            0xC8 => self.goal_distance(p),
            0xCA => self.node_id(p),
            0xCD => match self.bodies.get(index(a(2))) {
                Some(o) => off_heading(&b, o, a(3) == 1),
                None => 0,
            },
            0xCF => self.pick_node(p, args),
            0xD3 => weapon_class(b.item),
            0xD4 => i32::from(b.drawn),
            0xD5 => i32::from(self.ffa),
            0xD6 => 0,
            0xDC => 0,
            // obj+0, the object's type: 1, a player (every dump; the scripts look for 0x1F).
            0xEF => 1,
            0xF0 => i32::from(b.place),
            c => self.unknown(5, 2, c),
        }
    }

    /// Where the object `h` names stands: a participant, or a Colors flag.
    fn at(&self, h: i32) -> Option<[f32; 3]> {
        match flag(h) {
            Some(k) => Some(self.flags[k].pos),
            None => self.bodies.get(index(h)).map(|b| b.pos),
        }
    }

    /// A query on a Colors flag (`func_15099C14` on its object): its height (0x98), "just let go
    /// of" (0xB2, +0x2E4), its height band (0xC9) and node (0xCA), and 0xCB: the participant
    /// `a(2)` is in sight of a point 100 above it (`func_150AC9C0`'s ray and the barrier lines),
    /// its distance, else 0 (the collectors' straight run, 0x1D7A).
    fn flag_query(&mut self, k: usize, code: i32, args: &[i32]) -> i32 {
        let f = self.flags[k];
        match code {
            0x98 => f.pos[1] as i32,
            0xB2 => i32::from(f.dropped),
            0xC9 => i32::from(self.graph.band(f.pos[1])),
            0xCA => {
                let near = self.graph.approach(f.pos, &[], &[]);
                self.graph.node(near).map_or(0xFF, |x| i32::from(x.id))
            }
            0xCB => {
                let Some(to) = self.at(args.get(2).copied().unwrap_or(-1)) else {
                    return 0;
                };
                let up = |p: [f32; 3]| [p[0], p[1] + 100.0, p[2]];
                let d = [to[0] - f.pos[0], to[1] - f.pos[1], to[2] - f.pos[2]];
                match sight(self.world, self.graph, up(f.pos), up(to)) {
                    Some(true) => ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as i32).max(1),
                    _ => 0,
                }
            }
            c => self.unknown(5, 2, c),
        }
    }

    /// Query 0xC8: the distance left to the goal through the current node (or straight, if farther).
    fn goal_distance(&self, p: usize) -> i32 {
        let (n, b) = (&self.npcs[p], &self.bodies[p]);
        if n.cur == NONE || n.goal == NONE {
            return 0;
        }
        let d = |node: u8| {
            let q = self.graph.at(node);
            let v = [q[0] - b.pos[0], q[1] - b.pos[1], q[2] - b.pos[2]];
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() as i32
        };
        let via = d(n.cur) + n.length;
        if n.goal != n.cur {
            let straight = d(n.goal);
            if via < straight {
                return straight;
            }
        }
        via
    }

    /// Query 0xCA: the id of the participant's node (its current one, or the nearest).
    fn node_id(&mut self, p: usize) -> i32 {
        let n = &mut self.npcs[p];
        if n.cur != NONE {
            return i32::from(self.graph.nodes[usize::from(n.cur)].id);
        }
        // ponytail: the game seeds the search with the last answer (`D_8008729C`); without it the
        // nearest node is searched afresh, which differs only when that hint is nearer.
        let near = self.graph.approach(self.bodies[p].pos, &[], &[]);
        n.near = near;
        self.graph.node(near).map_or(0xFF, |x| i32::from(x.id))
    }

    /// Query 0xCF(n, ids…): of the nodes named, the one the shortest path from here reaches first.
    fn pick_node(&mut self, p: usize, args: &[i32]) -> i32 {
        let count = args.get(2).copied().unwrap_or(0).max(0) as usize;
        let goals: Vec<u8> = (0..count)
            .map(|k| self.graph.by_id(args.get(3 + k).copied().unwrap_or(-1)))
            .filter(|&x| x != NONE)
            .collect();
        if goals.is_empty() {
            return -1;
        }
        let start = self.graph.approach(self.bodies[p].pos, &goals, &[]);
        if start == NONE {
            return -1;
        }
        let (_, _, reached) = self.graph.next_toward(start, &goals, &[]);
        match self.graph.node(reached) {
            Some(x) => i32::from(x.id),
            None => -1,
        }
    }

    /// A player command (`func_15097A8C`).
    fn player_command(&mut self, p: usize, code: i32, args: &[i32]) -> i32 {
        let a = |k: usize| args.get(k).copied().unwrap_or(0);
        // Colors (0xBB), carried out by the match: 0x62 on a team's record (`0x2000 | team`, its
        // score S += n, `func_15085710(team, 10, n)`); on a player 7 lets go of the flag `a(2)`,
        // 0x13 sets +0x125, 0x53 +0x89 (0x3F's +0x8A has no reader here: I), 0x45 a capture
        // pose and 0x82 respawns now.
        let slot = self.slot(p as i32);
        let effect = match (code, slot) {
            (0x62, _) => Some(Effect::Score(p as u8, a(2) as u16)),
            (7, Some(_)) => flag(a(2)).map(Effect::LetGo),
            (0x13, Some(s)) => Some(Effect::Immune(s, a(2) as u8)),
            (0x53, Some(s)) => Some(Effect::Hold(s, a(2) as u8)),
            (0x45, Some(s)) => Pose::of(a(2)).map(|pose| Effect::Pose(s, pose)),
            (0x82, Some(s)) => Some(Effect::Respawn(s)),
            // War: Beach: 0x1F the start record (the match keeps it as the respawn mask, Beach
            // only), 0x27 the weapon zones' item (`func_15080620`); 0x54 the escaper's model fade
            // and 0x8B a human's screen fade are the client's (from the escape's freeze).
            (0x1F, Some(s)) => Some(Effect::Start(s, a(2) as u8)),
            (0x27, Some(s)) => Some(Effect::Give(s, a(2) as u8)),
            (0x3F | 0x54 | 0x8B, _) => return 0,
            // On a vacant participant: no player to act on (the match let its flag go as it left).
            (7 | 0x13 | 0x53 | 0x45 | 0x82 | 0x1F | 0x27, None) => return 0,
            _ => None,
        };
        if let Some(e) = effect {
            self.effects.push(e);
            return 0;
        }
        if p >= self.npcs.len() {
            return 0;
        }
        let by_id = |g: &Graph, id: i32| if id < 0 { NONE } else { g.by_id(id) };
        let n = &mut self.npcs[p];
        match code {
            0x14 => {
                n.script = a(2) as u8;
                n.at = None;
                n.wait = 0;
            }
            0x19 => n.target = (a(2) & !0xF000) as u8,
            0x1F | 0x7D => {}
            // The match places the player; the AI only sees the placement's flag (`func_15172D28`).
            0x5A => n.placed = true,
            0x5B => n.goal = by_id(self.graph, a(2)),
            0x5C => n.cur = by_id(self.graph, a(2)),
            0x5D => n.avoid = by_id(self.graph, a(2)),
            0x5F => n.speed = n.speed.min(a(2) as f32),
            0x7C => n.radius = a(2) as u8,
            0x60 => {
                n.goal = by_id(self.graph, a(2));
                n.cur = NONE;
                n.flags &= !AT_GOAL;
                n.script = a(3) as u8;
                n.at = None;
                n.wait = 0;
            }
            0x61 => {
                let b = if a(2) == -1 { 0x8000 } else { a(2) as u16 };
                n.pressed = b;
                n.press = 0;
                if matches!(a(2), -1 | 4 | 0x2000) {
                    n.held = b;
                    n.hold = 2;
                }
            }
            0x7B => n.pickup = a(2) as u8,
            // `func_1505D024` on the player: the gas's doorway hit (0x2A, no attacker) and kill
            // (0x2B, the deliverer's).
            0x2A | 0x2B => match (damage::script_type(a(2)), self.slot(p as i32)) {
                (Some(_), None) => {}
                (Some(kind), Some(slot)) => {
                    let attacker = (code == 0x2B).then(|| self.slot(a(4))).flatten();
                    self.effects.push(Effect::Hit {
                        slot,
                        kind,
                        attacker,
                    });
                }
                _ => return self.unknown(6, 2, code),
            },
            c => return self.unknown(6, 2, c),
        }
        0
    }

    /// `func_1508A6FC` (command 6), every participant: deaths, fire timers, the attack, the noises.
    /// Returns the mask of those that died (or appeared) this frame.
    fn frame(&mut self, g: &mut Globals) -> i32 {
        let count = self.bodies.len();
        let (alive, mut seen) = (g.i32(g::ALIVE), g.i32(g::SEEN));
        let mut died = 0;
        let dt = 2;
        for p in 0..count {
            let bit = 1 << p;
            g.set(g::REPLAN, p, 0);
            // Vacant: of no team, so no human takes it over (0xBA 0x4B16, 0xBB 0x511A) and no one
            // reckons with it; its going is no loss to its team's tally.
            let vacant = self.slot(p as i32).is_none();
            if vacant {
                g.set(g::TEAM, p, -1);
            }
            let mut first = false;
            if alive & bit == 0 {
                if seen & bit == 0 {
                    first = true;
                }
                seen |= bit;
            }
            if alive & bit == 0 && !first {
                g.set(g::HEALTH, p, 0);
                g.set(g::BAND, p, -1);
                continue;
            }
            let b = self.bodies[p];
            let mut idle = true;
            let fire_was = g.get(g::FIRE, p);
            g.set(g::ATTACKING, p, g.get(g::ATTACKING, p) + dt);
            g.set(g::FIRE, p, fire_was + dt);
            for off in [g::COOL, g::JUMPED] {
                g.set(off, p, (g.get(off, p) - dt).max(0));
            }
            g.set(g::TIMER3, p, (g.get(g::TIMER3, p) - dt).max(0));
            if g.get(g::STATE, p) == 3 && (g.get(g::COOL, p) & 0x1F) >= 0x1D {
                let n = &mut self.npcs[p];
                (n.pressed, n.press) = (0x8000, 0);
            }
            let hit_by = i32::from(b.hit_by);
            g.set(g::HIT_BY, p, hit_by);
            g.set(g::GRENADES, p, i32::from(b.grenades));
            g.set(g::HEALTH, p, i32::from(b.health));
            g.set(g::BAND, p, i32::from(self.graph.band(b.pos[1])));
            if b.health == 0 || first {
                if g.get(g::DEAD, p) == 0 {
                    let cur = self.npcs[p].cur;
                    // The node's index, which the script then uses as an id (the game's own mix-up;
                    // the two differ from node 65 on).
                    g.set(g::DIED_AT, p, if cur != NONE { i32::from(cur) } else { -1 });
                    g.set(g::DEAD, p, 1);
                    g.set(g::COOL, p, 0);
                    g.set(g::STATE, p, 0);
                    g.set(g::REEVAL, p, 1);
                    g.set(g::ENEMY, p, -1);
                    // The teams' tally G+0xDDC by G+0xE64's value: −1 for the victim's, +1 for its
                    // last attacker's if another (the team roles at 0x37F2 reassign on net losses).
                    let team = g.get(g::TEAM, p);
                    let tally = |team: i32| g::TEAM_COUNT + 4 * team;
                    let killer = (hit_by & 0x7F) as usize;
                    if !vacant {
                        g.set_i32(tally(team), g.i32(tally(team)) - 1);
                    }
                    let other = (killer < count).then(|| g.get(g::TEAM, killer));
                    if let Some(other) = other.filter(|&o| !vacant && o >= 0 && o != team) {
                        g.set_i32(tally(other), g.i32(tally(other)) + 1);
                    }
                    g.set(g::OVERRIDE, p, 0);
                    died |= bit;
                }
            } else {
                g.set(g::DEAD, p, 0);
            }
            let target = g.get(g::TARGET, p);
            if target != 0 {
                if g.get(g::STATE, p) != 1 && g.get(g::ATTACKING, p) < 0x1E {
                    let n = &mut self.npcs[p];
                    n.target = (target - 2) as u8;
                    let s = n.script;
                    if s != 4 && s != 5 {
                        (n.script, n.at, n.wait) = (4, None, 0);
                    }
                    if s == 4 {
                        g.set(g::ATTACKING, p, 0);
                    }
                    if s == 5 {
                        g.set(g::ATTACKING, p, 0x1E);
                        g.set_f32(g::AIM_RATE + 4 * p as i32, skill::AIM_RATE[self.skill]);
                        let o = 4 * p as i32;
                        g.set_f32(g::AIM_X + o, self.rng.next_f32() * 50.0 + b.pos[0]);
                        g.set_f32(g::AIM_Y + o, b.pos[1] + 40.0);
                        g.set_f32(g::AIM_Z + o, self.rng.next_f32() * 50.0 + b.pos[2]);
                        if target - 2 >= 2
                            && let Some(t) = self.bodies.get((target - 2) as usize)
                        {
                            g.set_f32(
                                g::AIM_X + o,
                                g.f32(g::AIM_X + o) + (t.pos[0] - b.pos[0]) * 0.5,
                            );
                            g.set_f32(
                                g::AIM_Z + o,
                                g.f32(g::AIM_Z + o) + (t.pos[2] - b.pos[2]) * 0.5,
                            );
                        }
                    }
                }
                idle = false;
                let mut fire = g.get(g::FIRE, p);
                let mut was = fire_was;
                if skill::CYCLE[self.skill] < fire {
                    g.set(g::FIRE, p, 0);
                    was = -1;
                    fire = 0;
                }
                let shoot = fire >= 0 && was < 0;
                let head = skill::HEAD_AFTER[self.skill];
                let head = head != -1 && g.get(g::ATTACKING, p) >= head;
                for (avg, aim) in [(g::JIT_SX, g::JIT_X), (g::JIT_SZ, g::JIT_Z)] {
                    let s = i32::from(g.i8(avg + p as i32));
                    let t = i32::from(g.i8(aim + p as i32));
                    g.set_i8(avg + p as i32, (s + ((t - s) >> 2)) as i8);
                }
                let (jx, jz) = (
                    i32::from(g.i8(g::JIT_SX + p as i32)),
                    i32::from(g.i8(g::JIT_SZ + p as i32)),
                );
                if self.attack(g, p, (target - 2) as usize, shoot, head, jx, jz)
                    && g.get(g::STATE, p) != 1
                {
                    g.set(g::ATTACKING, p, 0);
                }
                if g.get(g::FIRE, p) == 0 {
                    g.set_i8(g::JIT_X + p as i32, 0);
                    g.set_i8(g::JIT_Z + p as i32, 0);
                    if skill::STEADY[self.skill] < (self.rng.next_f32() * 100.0) as i32 {
                        let x = if self.rng.next_f32() > 0.5 {
                            -0x32
                        } else {
                            0x32
                        };
                        g.set_i8(g::JIT_X + p as i32, x);
                        let z = if self.rng.next_f32() > 0.5 { 0x32 } else { 0 };
                        g.set_i8(g::JIT_Z + p as i32, z);
                    }
                }
            }
            if idle {
                g.set(g::FIRE, p, -skill::FIRST[self.skill]);
            }
            // obj+0x22C bit 0x200 (a placement since the last frame), taken here.
            let placed = std::mem::take(&mut self.npcs[p].placed);
            if hit_by & 0x80 != 0 || placed {
                g.set(g::STATE, p, 0);
                g.set(g::REPLAN, p, 1);
            }
            // Burning holds the cooldown (C, `func_1508A6FC` after the placement's test).
            if b.burning {
                g.set(g::COOL, p, 0x14);
            }
        }
        g.set_i32(g::SEEN, seen);
        g.set_f32(g::DT, dt as f32);
        self.noises(g);
        died
    }

    /// `func_1508B3F8`: a bot within a noise's radius (+100 on the square, and 200 above to 100
    /// below it) hears it: of the walkway's nodes just outside the radius ([`walkway`]), the one
    /// nearest the bot among those nearer it than the noise is, by id, in G+0x55C.
    fn noises(&mut self, g: &mut Globals) {
        let count = self.bodies.len();
        for p in 0..count {
            if g.get(g::HEARD, p) != -1 {
                g.set(g::HEARD, p, -2);
            }
        }
        let n = g.i8(g::NOISES).max(0) as usize;
        for k in 0..n {
            let at = g::NOISE + 12 * k as i32;
            let r2 = g.f32(at);
            let h = |o: i32| {
                f32::from(i16::from_be_bytes([
                    g.0[(at + o) as usize],
                    g.0[(at + o + 1) as usize],
                ]))
            };
            let noise = [h(4), h(6), h(8)];
            let mut around: Option<Vec<(u8, f32)>> = None;
            for p in (0..count).filter(|&p| self.bodies[p].is_bot) {
                let b = self.bodies[p];
                let dy = b.pos[1] - noise[1];
                if !(dy < 200.0 && dy > -100.0) {
                    continue;
                }
                let (dx, dz) = (b.pos[0] - noise[0], b.pos[2] - noise[2]);
                if dx * dx + dz * dz >= r2 + 100.0 {
                    continue;
                }
                let list = around.get_or_insert_with(|| {
                    let (start, _) =
                        self.graph
                            .nearest(noise, Some(0), Some(self.graph.band(noise[1])));
                    let mut out = Vec::new();
                    if start != NONE {
                        walkway(
                            self.graph,
                            start,
                            [noise[0], noise[2]],
                            r2,
                            &mut [0u8; 32],
                            &mut out,
                        );
                    }
                    out
                });
                let mut best = (100_000_000.0f32, NONE);
                for &(node, limit) in list.iter() {
                    let q = self.graph.at(node);
                    let (ex, ez) = (q[0] - b.pos[0], q[2] - b.pos[2]);
                    let d2 = ex * ex + ez * ez;
                    if d2 < limit && d2 < best.0 {
                        best = (d2, node);
                    }
                }
                if best.1 != NONE {
                    if g.get(g::HEARD, p) != -2 {
                        g.set(g::REEVAL, p, 1);
                    }
                    g.set(
                        g::HEARD,
                        p,
                        i32::from(self.graph.nodes[usize::from(best.1)].id),
                    );
                }
            }
        }
        for p in 0..count {
            if g.get(g::HEARD, p) < 0 {
                g.set(g::HEARD, p, -1);
            }
        }
        g.set_i8(g::NOISES, 0);
    }

    /// `func_1508B9BC` (command 0xD): visibility pairs, the threat ranking, then for the participant
    /// G+4 its best target by the preference matrix (G+0x49C) and its distance (G+0x45C).
    fn threats(&mut self, g: &mut Globals) {
        let n = self.bodies.len();
        let pairs = if n >= 9 { 4 } else { 1 };
        self.visibility(g, 0, pairs);
        self.rank(g);
        let me = g.i32(g::CUR);
        if me < 0 || me as usize >= n {
            return;
        }
        let me = me as usize;
        let (mut best, mut least) = (-1i32, 10_000_000i32);
        for t in 0..n {
            if g.get(g::HEALTH, t) <= 0 || t == me {
                continue;
            }
            let mut score = -1;
            for k in 0..16 {
                let r = g.get(g::RANK, 16 * t + k);
                if r == -1 {
                    break;
                }
                if r == me as i32 {
                    score = g.get(g::RANK_SCORE, 16 * t + k);
                    break;
                }
            }
            if score == -1 {
                continue;
            }
            let mut f = f32::from(g.0[(g::PREFER as usize) + 16 * me + t]) * 0.003_921_569; // 1/255
            if t as i32 == g.get(g::ENEMY, me) {
                f *= 0.8;
            }
            let s = (score as f32 * f) as i32;
            if s < least {
                (least, best) = (s, t as i32);
            }
        }
        g.set(g::THREAT, me, best);
        let d = if best != -1 {
            let (a, b) = (self.bodies[me].pos, self.bodies[best as usize].pos);
            let (dx, dz) = (a[0] - b[0], a[2] - b[2]);
            (dx * dx + dz * dz).sqrt() as i32
        } else {
            least
        };
        g.set(g::THREAT_DIST, me, d);
    }

    /// `func_1508BF14` (command 7, the team modes): the team of the participant G+4 shares out its
    /// targets. Each member, those in roles 2 and 3 (G+0x15C) first, takes the living enemy whose
    /// ranking ([`Self::rank`]) scores it lowest, the score doubled for each member already on that
    /// enemy: G+0x49C the enemy (−1 none), G+0x45C the score (10,000,000 none).
    fn team_threats(&mut self, g: &mut Globals) {
        let n = self.bodies.len();
        // G+4 may be −1 (no AI this frame): the game then reads G+0xE60, the team mask.
        let team = g.i32(g::TEAM + 4 * g.i32(g::CUR));
        let mut taken = vec![0u32; n];
        for first in [true, false] {
            for t in 0..n {
                if matches!(g.get(0x15C, t), 2 | 3) != first || g.get(g::TEAM, t) != team {
                    continue;
                }
                let (mut best, mut least) = (-1i32, 10_000_000i32);
                for (e, &on) in taken.iter().enumerate() {
                    if g.get(g::HEALTH, e) <= 0 || g.get(g::TEAM, e) == team {
                        continue;
                    }
                    let score = (16 * e..16 * e + 16)
                        .take_while(|&k| g.get(g::RANK, k) != -1)
                        .find(|&k| g.get(g::RANK, k) == t as i32)
                        .map(|k| g.get(g::RANK_SCORE, k).wrapping_shl(on));
                    if let Some(s) = score.filter(|&s| s < least) {
                        (least, best) = (s, e as i32);
                    }
                }
                // The game bumps a stack byte for −1.
                if let Some(c) = usize::try_from(best).ok().and_then(|e| taken.get_mut(e)) {
                    *c += 1;
                }
                g.set(g::THREAT_DIST, t, least);
                g.set(g::THREAT, t, best);
            }
        }
    }

    /// `func_1508A1BC`: one participant v per frame (G+0x1704, round robin) ranks the enemies that
    /// may hunt it, nearest first (G+0x5DC, scores G+0x9DC): the xz distance, +500 across bands; if
    /// v has no clear line to them, a bot's doubled and roles 2 and 3 left out; a human's scaled by
    /// the skill. G+0x51C[v] = the nearest distance.
    fn rank(&mut self, g: &mut Globals) {
        let n = self.bodies.len();
        if n == 0 {
            return;
        }
        let mut v = i32::from(g.i8(g::RR_D)) + 1;
        if v == n as i32 {
            v = 0;
        }
        g.set_i8(g::RR_D, v as i8);
        let v = v as usize;
        let mut list: Vec<(i32, i32)> = Vec::new();
        if g.i32(g::ALIVE) & (1 << v) != 0 {
            let sees = g.get(g::SEES, v);
            let mut nearest = 0x7D00;
            for s in 0..n {
                if g.get(g::HEALTH, s) <= 0 || g.get(g::TEAM, v) == g.get(g::TEAM, s) {
                    continue;
                }
                let (a, b) = (self.bodies[v].pos, self.bodies[s].pos);
                let (dx, dz) = (a[0] - b[0], a[2] - b[2]);
                let raw = (dx * dx + dz * dz).sqrt() as i32;
                let mut d = raw;
                if g.get(g::BAND, v) != g.get(g::BAND, s) {
                    d += 0x1F4;
                }
                if (0x10000 << s) & sees != 0 {
                    // obj+0x65 is 0 for every player (V, the oracle's RAM): never left out here.
                } else {
                    if self.bodies[s].is_bot {
                        d *= 2;
                    }
                    if matches!(g.get(0x15C, s), 2 | 3) {
                        d = -1;
                    }
                }
                if !self.bodies[s].is_bot && d != -1 {
                    d = (d as f32 * g.f32(g::HUMAN_SCALE)) as i32;
                }
                if d >= 0 {
                    let at = list.iter().position(|e| e.1 >= d).unwrap_or(list.len());
                    list.insert(at, (s as i32, d));
                }
                nearest = nearest.min(raw);
            }
            g.set(g::NEAREST, v, nearest);
        }
        for k in 0..16 {
            match list.get(k) {
                Some(&(s, d)) => {
                    g.set(g::RANK, 16 * v + k, s);
                    g.set(g::RANK_SCORE, 16 * v + k, d);
                }
                None => g.set(g::RANK, 16 * v + k, -1),
            }
        }
    }

    /// `func_15089F9C` (query 6, War: Beach's Tediz): the target of participant `p`, of those in play
    /// (G+0x14) with no wall between their eyes (G+0x16C0[p] bit 16 + q: only enemy pairs are tested)
    /// and G+0xE64 ≠ 3, the one whose xz distance d, clamped to the role's band (G+0x15C: 0, the east
    /// top, 750-3500, else 1500-8000), scores best on sin(−π/2 + 2π·(d − lo)/(hi − lo)) + 1, the
    /// band's middle; the first on a tie, −1 with none (C). ponytail: the start score
    /// (`0x8009DA50`, in the compressed data) taken as −1, so any candidate beats none.
    fn pick_target(&self, g: &Globals, p: usize) -> i32 {
        let Some(me) = self.bodies.get(p) else {
            return -1;
        };
        let (lo, hi) = match g.get(0x15C, p) {
            0 => (750.0f32, 3500.0f32),
            _ => (1500.0, 8000.0),
        };
        let (alive, sees) = (g.i32(g::ALIVE), g.get(g::SEES, p));
        let mut best = (-1, -1.0f32);
        for (q, o) in self
            .bodies
            .iter()
            .enumerate()
            .take(g.i32(g::COUNT).max(0) as usize)
        {
            if g.get(g::TEAM, q) == 3 || alive & 1 << q == 0 || sees & 1 << (16 + q) == 0 {
                continue;
            }
            let (dx, dz) = (me.pos[0] - o.pos[0], me.pos[2] - o.pos[2]);
            let d = (dx * dx + dz * dz).sqrt().clamp(lo, hi);
            let score = ((d - lo) * (std::f32::consts::TAU / (hi - lo))
                - std::f32::consts::FRAC_PI_2)
                .sin()
                + 1.0;
            if best.1 < score {
                best = (q as i32, score);
            }
        }
        best.0
    }

    /// `func_1508C5B8(0, pairs)` (query 4): each human tests its lines of sight to three enemy bots
    /// it does not see yet (the game measures their distance as the human's own x − z, so the first
    /// three in slot order), then `pairs` more pairs are tested round robin (G+0x1700/1701).
    /// G+0x16C0[p] bit q: p sees q along a clear line; bit 16+q: no wall between their eyes.
    fn visibility(&mut self, g: &mut Globals, who: usize, pairs: i32) -> i32 {
        let n = self.bodies.len();
        if pairs != 0 {
            for h in (0..n).filter(|&h| !self.bodies[h].is_bot && self.slots[h].is_some()) {
                let mut near: Vec<usize> = Vec::new();
                let quirk = {
                    let b = self.bodies[h].pos;
                    let d = b[0] - b[2];
                    d * d
                };
                for q in (0..n).filter(|&q| self.bodies[q].is_bot) {
                    if g.get(g::TEAM, h) == g.get(g::TEAM, q)
                        || g.get(g::SEES, h) & (0x10001 << q) != 0
                    {
                        continue;
                    }
                    if near.len() < 3 && quirk < 900_000_000.0 {
                        near.push(q);
                    }
                }
                for q in near {
                    self.clear_pair(g, h, q);
                    self.line_of_sight(g, h, q);
                }
            }
            let (mut a, mut b) = (
                usize::from(g.0[g::RR_A as usize]),
                usize::from(g.0[g::RR_B as usize]),
            );
            if n >= 2 {
                for _ in 0..pairs {
                    let mut tries = 0;
                    loop {
                        b += 1;
                        if b >= n {
                            a += 1;
                            if a >= n - 1 {
                                a = 0;
                            }
                            b = a + 1;
                        }
                        self.clear_pair(g, a, b);
                        tries += 1;
                        if !(tries < 0x40 && g.get(g::TEAM, a) == g.get(g::TEAM, b)) {
                            break;
                        }
                    }
                    self.line_of_sight(g, a, b);
                }
            }
            g.0[g::RR_A as usize] = a as u8;
            g.0[g::RR_B as usize] = b as u8;
        }
        g.get(g::SEES, who)
    }

    fn clear_pair(&self, g: &mut Globals, a: usize, b: usize) {
        g.set(g::SEES, a, g.get(g::SEES, a) & !(0x10001 << b));
        g.set(g::SEES, b, g.get(g::SEES, b) & !(0x10001 << a));
    }

    /// `func_1508C1A4`: eye to eye (a bot's at 0.75 of its height 90, 50 lower with the twin guns; a
    /// human's 40 up, or its head when a bot looks at it); clear of walls, and of the navigation's
    /// barrier lines for bit q.
    fn line_of_sight(&self, g: &mut Globals, a: usize, b: usize) {
        let eye = |me: usize, other: usize| {
            let x = self.bodies[me];
            let mut p = x.pos;
            if x.is_bot {
                p[1] += 67.5;
                if x.item == 9 {
                    p[1] -= 50.0;
                }
            } else if self.bodies[other].is_bot && skill::HEAD_AFTER[self.skill] != -1 {
                p[1] += HEAD;
            } else {
                p[1] += 40.0;
            }
            p
        };
        let Some(open) = sight(self.world, self.graph, eye(a, b), eye(b, a)) else {
            return;
        };
        for (x, y) in [(a, b), (b, a)] {
            let mut v = g.get(g::SEES, x) | (0x10000 << y);
            if open {
                v |= 1 << y;
            }
            g.set(g::SEES, x, v);
        }
    }

    /// `func_1508CAD8`: the attacker `p` aims at `t`: an aim point follows the target (40 above its
    /// feet, or its head) at the skill's rate, the jitter shifts it sideways and down; the aim yaw
    /// snaps onto it. The virtual pad: R always; Z held in bursts (twin guns), held (flamethrower),
    /// released on the cycle's shot frame and held otherwise (single shots), or pressed on it (the
    /// rest). Returns whether the aim had to swing more than 45°.
    #[allow(clippy::too_many_arguments)]
    fn attack(
        &mut self,
        g: &mut Globals,
        p: usize,
        t: usize,
        shoot: bool,
        head: bool,
        jx: i32,
        jz: i32,
    ) -> bool {
        let Some(&tb) = self.bodies.get(t) else {
            return true;
        };
        let b = self.bodies[p];
        let weapon = b.drawn;
        let single = matches!(weapon, 0x24 | 0x18 | 0x23 | 0x41);
        let automatic = match weapon {
            9 => 1,
            0x15 => 3,
            0x38 => 2,
            _ => 0,
        };
        let o = 4 * p as i32;
        let (mut tx, ty, mut tz) = if head && !tb.is_bot {
            (tb.pos[0], tb.pos[1] + HEAD, tb.pos[2])
        } else {
            (tb.pos[0], tb.pos[1] + 40.0, tb.pos[2])
        };
        // Skills 3 and 4 lead a moving target with these weapons: its last move times its distance
        // times the weapon's factor over the frame's VIs (C; V lead/l1 at skill 4: 145 of 146 aim
        // points of 0x14 and 0x16 to 1e-4, the target's head joint plus the lead, all as the frame
        // before; I: script 0xB9's lead at any skill, not an MP scene's).
        let lead = match weapon {
            0x18 | 0x41 | 0x1B => f32::from_bits(0x3C83_126F), // 0.016
            0x14 | 0x3F => f32::from_bits(0x3CCA_E5D8),        // 0.0248
            0x16 => f32::from_bits(0x3D03_126F),               // 0.032
            _ => 0.0,
        };
        let dt = g.f32(g::DT);
        if self.skill >= 3 && lead != 0.0 && dt > 0.0 {
            let d: [f32; 3] = core::array::from_fn(|i| tb.pos[i] - b.pos[i]);
            let k = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() * lead / dt;
            tx += (tb.pos[0] - tb.prev[0]) * k;
            tz += (tb.pos[2] - tb.prev[2]) * k;
        }
        let rate = (g.f32(g::AIM_RATE + o) * 2.0).min(1.0);
        let (mut ax, mut az) = (g.f32(g::AIM_X + o), g.f32(g::AIM_Z + o));
        ax += (tx - ax) * rate;
        az += (tz - az) * rate;
        g.set_f32(g::AIM_X + o, ax);
        g.set_f32(g::AIM_Z + o, az);
        let (mut dx, mut dz) = (ax - b.pos[0], az - b.pos[2]);
        let d = (dx * dx + dz * dz).sqrt();
        let (mut jx, mut jz) = (jx, jz);
        if d >= 1.0 {
            if d < 100.0 && jz != 0 {
                jz = 0;
                if jx == 0 {
                    jx = 0x32;
                }
            }
            let mut k = jx as f32;
            if weapon == 9 {
                k *= 1.25;
            }
            k /= d;
            (dx, dz) = (dx - dz * k, dz + dx * k);
        }
        let to = conker_sim::movement::atan2_u16(-dz, dx);
        let to_deg = wrap360(f32::from(to) * 0.005_493_164);
        let n = &mut self.npcs[p];
        if weapon == 0x1B {
            // The mounted gun: the AI turns its scope with the stick, 8 a degree off the aim point's
            // yaw (±90) and off its pitch from the eye, 67.5 up (±30, up looks down), and holds Z
            // once both are within about 4° and it sees the target; R always (C). Z then stays
            // held while it fights on (the pad's hold), so the gun overheats and fires again as
            // the lock ends (V b30).
            let wrap180 = |a: f32| {
                let a = wrap360(a);
                if a > 180.0 { a - 360.0 } else { a }
            };
            let (yaw, pitch) =
                (b.scope).unwrap_or((wrap360(f32::from(b.facing) * 0.005_493_164), 0.0));
            let off = wrap180(yaw - to_deg);
            let ay = g.f32(g::AIM_Y + o);
            let ay = ay + ((ty - jz as f32) - ay) * rate;
            g.set_f32(g::AIM_Y + o, ay);
            let rise = b.pos[1] + 67.5 - ay;
            let target = wrap180(
                f32::from(conker_sim::movement::atan2_u16(rise, d.max(1.0))) * 0.005_493_164,
            );
            let x = (off * 8.0).clamp(-90.0, 90.0);
            let y = (wrap180(wrap180(pitch) - target) * 8.0).clamp(-30.0, 30.0);
            n.stick = Some([x as i8, -y as i8]);
            if (1.5 * x.hypot(y) - 20.0).max(0.0) - 80.0 < -50.0
                && g.get(g::SEES, p) & (0x10001 << t) != 0
            {
                n.held |= 0x2000;
            }
            n.held |= 0x10;
            n.hold = 1;
            return !(-45.0..=45.0).contains(&off);
        }
        let yaw = n
            .aim_yaw
            .unwrap_or(wrap360(f32::from(b.facing) * 0.005_493_164));
        let mut off = wrap360(wrap360(yaw) - to_deg);
        if off > 180.0 {
            off -= 360.0;
        }
        let far = !(-45.0..=45.0).contains(&off);
        n.aim_yaw = Some(to_deg);
        let ay = g.f32(g::AIM_Y + o);
        g.set_f32(g::AIM_Y + o, ay + ((ty - jz as f32) - ay) * rate);
        n.aim = Some([b.pos[0] + dx, g.f32(g::AIM_Y + o), b.pos[2] + dz]);
        if automatic != 0 {
            if g.get(g::FIRE, p) >= 0 {
                n.held = 0x2000;
                match automatic {
                    1 if skill::BURST[self.skill] < g.get(g::FIRE, p) => n.held &= !0x2000,
                    2 if !shoot => n.held &= !0x2000,
                    _ => {}
                }
            }
        } else if shoot {
            if single {
                (n.pressed, n.held) = (0, 0);
            } else {
                (n.pressed, n.press) = (0x2000, 1);
            }
        } else if single {
            n.held = 0x2000;
        }
        n.held |= 0x10;
        n.hold = 1;
        far
    }
}

/// The ray of `func_1508C1A4` from eye `from` to eye `to`: `None` when a wall is in the way, else
/// whether the line also stays clear of the navigation's barrier lines.
pub fn sight(world: &CollisionWorld, graph: &Graph, from: [f32; 3], to: [f32; 3]) -> Option<bool> {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let walls = len <= 0.0 || {
        let dir = d.map(|v| v / len);
        conker_rules::projectile::raycast(world, from, dir, len) >= len
    };
    walls.then(|| graph.crossing(from, d[0], d[2]) <= 0.0)
}

/// `func_1508B2A8` from `node`: the walkway out of a noise of radius² `r2` at `at` (x, z): nodes
/// inside are walked through, the first ones outside (at most 8) listed with their distance². The
/// visited set takes bit `node & 3` of byte `node >> 3`, so pairs of nodes share a bit (the game's).
fn walkway(
    g: &Graph,
    node: u8,
    at: [f32; 2],
    r2: f32,
    seen: &mut [u8; 32],
    out: &mut Vec<(u8, f32)>,
) {
    seen[usize::from(node >> 3)] |= 1 << (node & 3);
    let n = g.nodes[usize::from(node)];
    let (dx, dz) = (f32::from(n.pos[0]) - at[0], f32::from(n.pos[2]) - at[1]);
    let d2 = dx * dx + dz * dz;
    if r2 < d2 {
        if out.len() < 8 {
            out.push((node, d2));
        }
        return;
    }
    for nb in n.next {
        if nb != NONE && seen[usize::from(nb >> 3)] & (1 << (nb & 3)) == 0 {
            walkway(g, nb, at, r2, seen, out);
        }
    }
}

/// Query 0xCD: how far `o` lies off `b`'s heading, in degrees (−180..180, or its size when `abs`):
/// `func_1505A630(dx, −dz) + 0x4000` (the direction's angle, a quarter turn on, as the heading
/// obj+0x40 is the facing + 90°, V on bot_long3) against the heading.
fn off_heading(b: &Body, o: &Body, abs: bool) -> i32 {
    let to = conker_sim::movement::atan2_u16(b.pos[2] - o.pos[2], o.pos[0] - b.pos[0]);
    let deg = wrap360((to.wrapping_add(0x4000)) as f32 * 0.005_493_164);
    let mut d = wrap360(wrap360(b.heading) - deg);
    if d > 180.0 {
        d -= 360.0;
    }
    if abs && d < 0.0 {
        d = -d;
    }
    d as i32
}

/// A human's head above its feet, where a bot aims once it has fought it long enough (I: obj+0x1A6,
/// the head joint, read 47-57 above the feet in the oracle's RAM).
const HEAD: f32 = 52.0;

fn wrap360(d: f32) -> f32 {
    let r = d % 360.0;
    if r < 0.0 { r + 360.0 } else { r }
}

/// Query 0xD3: the held item's class: 2 automatic guns, 3 melee, 1 other weapons, 0 none.
pub fn weapon_class(item: u8) -> i32 {
    match item {
        0x03 | 0x09 | 0x15 => 2,
        0x21 | 0x22 => 3,
        0x40 => 3, // the club (I: melee like 0x21, as `give_range` has it)
        0x14 | 0x16 | 0x18 | 0x23 | 0x24 | 0x37 | 0x38 | 0x39 | 0x3F | 0x41 => 1,
        _ => 0,
    }
}

/// `func_1508E6D0` (query 7): the range a weapon is worth using at.
pub fn give_range(id: i32) -> i32 {
    match id {
        0x3F => 0x708,
        0x14 => 0xC80,
        0x22 | 0x15 | 0x21 | 0x19 | 0x40 => 0x1F4,
        0x23 => 0x2710,
        0x09 | 0x38 => 0x1388,
        0x03 => 0x2EE,
        0x37 => 0x5DC,
        _ => 0x186A0,
    }
}

/// `func_1508BC20`: each participant's preference bytes for the others (G+0x15B4 + 16p), a zigzag
/// of steps 0xAA / (n/2) from 0x55 laid from a random start in a random direction.
fn prefer(g: &mut Globals, n: usize, rng: &mut Rng) {
    if n == 0 {
        return;
    }
    let step0 = 0xAA / (n as i32 / 2).max(2);
    for p in 0..n {
        // The game marks the start used in a table it clears for every participant, so it never
        // moves; both draws are taken.
        let mut s = (rng.next_f32() * n as f32) as i32;
        let _dir = rng.next_f32() > 0.5;
        let (mut v, mut step) = (0x55i32, step0);
        for _ in 0..n {
            let at = g::PREFER as usize + 16 * p + s as usize;
            g.0[at] = if v < 0x100 { v as u8 } else { 0xFF };
            s += 1;
            if s >= n as i32 {
                s = 0;
            }
            v += step;
            if v >= 0x100 {
                step = -step;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_camera::volume::CameraVolume;

    #[test]
    fn weapon_tables() {
        assert_eq!(
            (
                weapon_class(0x09),
                weapon_class(0x22),
                weapon_class(0x39),
                weapon_class(0)
            ),
            (2, 3, 1, 0)
        );
        assert_eq!(
            (give_range(0x09), give_range(0x24), give_range(0x22)),
            (5000, 100_000, 500)
        );
    }

    /// A placement (match start, respawn, lift) is taken by the next frame: state 0 and a replan,
    /// once (V: the oracle's 0x30 p6, lifted at read 323, state 6 → 0 at 324). Burning holds the
    /// cooldown at 0x14 every frame (C, `func_1508A6FC`'s s126+0x1AC test).
    #[test]
    fn a_placement_replans_once() {
        let mut bodies = [Body {
            health: 6,
            is_bot: true,
            ..Body::default()
        }; 2];
        bodies[1].burning = true;
        let mut npcs = [Npc::default(); 2];
        npcs[1].placed = true;
        let (graph, world) = (Graph::default(), CollisionWorld::default());
        let (mut rng, mut unknown) = (Rng::default(), Unknown::new());
        let mut w = World {
            bodies: &bodies,
            npcs: &mut npcs,
            graph: &graph,
            world: &world,
            rng: &mut rng,
            skill: 2,
            zones: &mut [],
            unknown: &mut unknown,
            ffa: true,
            forgiven: &mut [],
            slots: &[Some(0), Some(1)],
            touched: [0; 2],
            effects: &mut Vec::new(),
            flags: Default::default(),
            starting: false,
            gunners: Vec::new(),
            plunger: Plunger::default(),
            appear: 0,
        };
        let mut g = Globals::new(g::SIZE);
        g.set_i32(g::ALIVE, 3);
        g.set_i32(g::COUNT, 2);
        for frame in 0..2 {
            g.set(g::STATE, 0, 6);
            g.set(g::STATE, 1, 6);
            g.set(g::COOL, 0, 0x12C);
            g.set(g::COOL, 1, 0x12C);
            w.frame(&mut g);
            assert_eq!((g.get(g::COOL, 0), g.get(g::COOL, 1)), (0x12A, 0x14));
            let replan = i32::from(frame == 0);
            assert_eq!((g.get(g::STATE, 0), g.get(g::REPLAN, 0)), (6, 0));
            assert_eq!(
                (g.get(g::STATE, 1), g.get(g::REPLAN, 1)),
                (6 - 6 * replan, replan)
            );
        }
    }

    /// Type-4 queries 6 and 16, player query 0xAC and type-4 command 1 on one box (shape 2) with
    /// one participant inside and one outside.
    #[test]
    fn trigger_natives() {
        let mut r = [0; 0x34];
        r[6..0xC].copy_from_slice(&[0, 100, 0, 200, 0, 100]);
        r[0x15] = 3 << 2 | 2;
        let zone = CameraVolume::parse_record(&r);
        let hint = CameraHint {
            zone,
            on_floor: false,
            kind: 0,
            command: 0,
        };
        let at = |x| Body {
            pos: [x, 10.0, 0.0],
            old: [x, 10.0, 0.0],
            ..Body::default()
        };
        let bodies = [at(50.0), at(150.0)];
        let mut npcs = [Npc::default(); 2];
        let (graph, world) = (Graph::default(), CollisionWorld::default());
        let (mut rng, mut unknown) = (Rng::default(), Unknown::new());
        let mut zones = [(hint, false)];
        let mut w = World {
            bodies: &bodies,
            npcs: &mut npcs,
            graph: &graph,
            world: &world,
            rng: &mut rng,
            skill: 2,
            zones: &mut zones,
            unknown: &mut unknown,
            ffa: true,
            forgiven: &mut [],
            slots: &[Some(0), Some(1)],
            touched: [0; 2],
            effects: &mut Vec::new(),
            flags: Default::default(),
            starting: false,
            gunners: vec![None, Some(1)],
            plunger: Plunger::default(),
            appear: 0,
        };
        let mut g = Globals::new(g::SIZE);
        let mut q = |w: &mut World, args: &[i32]| w.query(&mut g, args[0], args[1], args);
        // The mounted guns' +0x13C: gun 1 (0x2013) manned by slot 1, participant 1.
        assert_eq!(q(&mut w, &[0x2012, 0xBC]), 0);
        assert_eq!(q(&mut w, &[0x2013, 0xBC]), 101);
        assert_eq!(q(&mut w, &[0x4000, 6, 0x2000]), 1);
        assert_eq!(q(&mut w, &[0x4000, 6, 0x2001]), 0);
        assert_eq!(q(&mut w, &[0x4000, 0x10]), 1);
        assert_eq!(q(&mut w, &[0x2000, 0xAC, 0x4001, 0x4000]), 0x4000);
        assert_eq!(q(&mut w, &[0x2001, 0xAC, 0x4000]), 0);
        w.command(&mut Globals::new(0), 0x4000, 1, &[0x4000, 1]);
        assert_eq!(q(&mut w, &[0x4000, 0x10]), 0);
        assert!(w.unknown.is_empty());
    }

    /// The lead against the oracle's lead/l1 (skill 4, aim rate 1): the aim point less the target's
    /// head joint, as the frame before showed the attacker, the target and its last move. Bot 3 with
    /// 0x14 at read 950 (the target running 17.5 a frame, 2965 away) and bot 2 with 0x16 at 2781;
    /// the mounted gun's factor is the crossbow's (C). Skill 2 does not lead.
    #[test]
    fn skill_3_and_4_lead_a_moving_target() {
        let (graph, world) = (Graph::default(), CollisionWorld::default());
        for (skill, weapon, me, pos, prev, lead) in [
            (
                4,
                0x14,
                [-1594.7598, 0.984375, 6413.4478],
                [-3478.55, 1.0, 4124.001],
                [-3463.239, 1.0, 4132.476],
                [-562.1628, -311.1733],
            ),
            (
                4,
                0x16,
                [-459.6799, 160.8789, 232.79465],
                [764.8716, 1254.0, -3242.486],
                [764.84625, 1254.0, -3246.486],
                [1.5576, 245.98],
            ),
            (
                4,
                0x1B,
                [0.0; 3],
                [300.0, 0.0, 400.0],
                [290.0, 0.0, 400.0],
                [40.0, 0.0],
            ),
            (
                2,
                0x14,
                [0.0; 3],
                [300.0, 0.0, 400.0],
                [290.0, 0.0, 400.0],
                [0.0, 0.0],
            ),
        ] {
            let bodies = [
                Body {
                    pos: me,
                    drawn: weapon,
                    is_bot: true,
                    health: 6,
                    ..Body::default()
                },
                Body {
                    pos,
                    prev,
                    is_bot: true,
                    health: 6,
                    ..Body::default()
                },
            ];
            let mut npcs = [Npc::default(); 2];
            let (mut rng, mut unknown) = (Rng::default(), Unknown::new());
            let mut w = World {
                bodies: &bodies,
                npcs: &mut npcs,
                graph: &graph,
                world: &world,
                rng: &mut rng,
                skill,
                zones: &mut [],
                unknown: &mut unknown,
                ffa: true,
                forgiven: &mut [],
                slots: &[Some(0), Some(1)],
                touched: [0; 2],
                effects: &mut Vec::new(),
                flags: Default::default(),
                starting: false,
                gunners: Vec::new(),
                plunger: Plunger::default(),
                appear: 0,
            };
            let mut g = Globals::new(g::SIZE);
            g.set_f32(g::AIM_RATE, 1.0);
            g.set_f32(g::DT, 2.0);
            w.attack(&mut g, 0, 1, false, false, 0, 0);
            let got = [g.f32(g::AIM_X) - pos[0], g.f32(g::AIM_Z) - pos[2]];
            assert!(
                (got[0] - lead[0]).abs() < 0.01 && (got[1] - lead[1]).abs() < 0.01,
                "skill {skill} {weapon:#x}: {got:?}"
            );
        }
    }

    #[test]
    fn angle_off_the_heading() {
        // Facing +x (facing 0, heading 90°, as the game's obj+0x40 reads).
        let me = Body {
            heading: 90.0,
            ..Body::default()
        };
        let at = |x, z| Body {
            pos: [x, 0.0, z],
            ..Body::default()
        };
        assert_eq!(off_heading(&me, &at(100.0, 0.0), false), 0);
        // The facing turns toward −z as it grows (a move by (cos a, −sin a)): −z is 90° to one side.
        assert_eq!(off_heading(&me, &at(0.0, -100.0), false).abs(), 90);
        assert_eq!(off_heading(&me, &at(0.0, 100.0), true), 90);
        assert_eq!(off_heading(&me, &at(-100.0, 0.0), true), 180);
    }

    #[test]
    fn preference_bytes_zigzag() {
        let mut g = Globals::new(g::SIZE);
        let mut rng = Rng::default();
        prefer(&mut g, 4, &mut rng);
        for p in 0..4 {
            let row: Vec<u8> = g.0[g::PREFER as usize + 16 * p..][..4].to_vec();
            let mut sorted = row.clone();
            sorted.sort();
            // 4 players: steps of 0xAA / 2 = 0x55 from 0x55: 0x55, 0xAA, 0xFF, then 0x154 ≥ 0x100 → 0xFF.
            assert_eq!(sorted, [0x55, 0xAA, 0xFF, 0xFF], "row {p}: {row:?}");
        }
    }
}
