//! Race (scenarios 11 and 12, scene 0x33): up to 8 racers on hoverboards (conker-sim's `board`)
//! race a number of laps along the nav graph's track. Here the race table (`func_15087350`: each
//! racer's place on the track, its laps and lap times, the places), the countdown, the power-ups
//! (the canister's boost, the missiles), B's kick, the walls' damage and the void, the deaths and
//! respawns on the track, the finish and the end. The AI script (0xBC, run by conker-bots) drives
//! the bots and the finished humans ([`Op`]). Research: `../lab/research/mp-original/race.md`
//! (V c_*, w1-w5, F1, L1, L2, b1, r0).

use crate::game::{Action, Arena, Event, Fighter, Match};
use crate::spawn::Placement;
use conker_sim::board::{self, Board, Drive};
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::{BUTTON_Z, Input, atan2_u16, cos_sin};

/// The human racers' class (model 136).
pub const HUMAN: u8 = 0xB;

/// A bot racer's class by slot (144 in its helmet): 0xD, 0xE, 0xF, 0xC from slot 1, then again
/// (`race.json`'s `racers`; V b2, cls).
pub fn bot_class(slot: usize) -> u8 {
    0xC | (slot & 3) as u8
}
/// Laps (opt[7]): 1-20, 5 by default.
pub const LAPS: u8 = 5;
/// The race table is ready 90 frames into the match (read 113), the countdown (G+0x60) runs 90
/// frames from there: stage 1 at 60 left (read 142), 2 at 30 (172), 3 at 5 (197). The race clock
/// runs from read 203 (+180), the boards from 204 (+181) (V r0, c_fwd, F1).
pub const READY: u32 = 90;
pub const COUNTDOWN: u32 = 90;
pub const CLOCK: u32 = 180;
pub const GO: u32 = 181;
/// From the deciding finish to the results in the level (G[0] = 40: V F1 232 → 271), and the
/// results' 40 s (0x961 VIs) to the end fade (V F1 271 → 1472).
pub const RESULTS: u32 = 39;
pub const RESULTS_FOR: u32 = 1201;
/// A dead racer is back on the track 88 frames on (V L1 378 → 466), 150 frames immune.
pub const RESPAWN: u8 = 88;
/// The death's state (clip 0x170).
pub const DEAD: u8 = 112;
/// The items (`Gives::Weapon`): the canister (the boost, 120 VIs) and the missile; at most 3 of one.
pub const CANISTER: u8 = 0x31;
pub const MISSILE: u8 = 0x33;
pub const ITEMS: u8 = 3;
pub const BOOST_VIS: u16 = 120;
/// A taken power-up is back 10 frames on (the script's 9-frame timer, then its show: V L1 541 →
/// 551).
pub const PICKUP_RESPAWN: u16 = 10;
/// Lap times are capped (VIs): a lap 0x7D00, the race 0x57E40 (C).
pub const LAP_CAP: i32 = 0x7D00;
pub const TOTAL_CAP: i32 = 0x57E40;
/// B's kick reaches a racer this near (I: w2's victim was 237 away).
pub const KICK_REACH: f32 = 250.0;
/// The missile (`func_1506C460(0x33, 310, …)`): half its speed a frame along the track, 100 above
/// it; from its 10th frame it homes on the next racer ahead within 1000; 120 frames; a hit within
/// [`MISSILE_HIT`] (I) knocks up (V w1b, w5).
pub const MISSILE_SPEED: f32 = 310.0;
pub const MISSILE_LIFE: u32 = 120;
pub const HOMING: u32 = 10;
pub const HOMING_RANGE: f32 = 1000.0;
pub const MISSILE_HIT: f32 = 120.0;
pub const MISSILE_LIFT: f32 = 100.0;
/// The table's projection: within 50 of the racer, 0.75 of the gap a step, at most 11 (C).
const NEAR: f32 = 50.0;
/// The lateral's ease toward its target a frame (I: `0x8009D9E0` unread).
const EASE: f32 = 0.02;
/// The autopilot steers at the track 0.2 nodes ahead (C: `func_15088A08(copy, 2 · 0.1)`).
const AHEAD: f32 = 0.2;
/// The autopilot's speed when the script gives none (I: the script's 150 for a finished human).
const AUTO_SPEED: f32 = 150.0;
/// A respawn node this far above its floor is over the void (I).
const OVER: f32 = 300.0;

/// A lap node: its point, its edge partner's (the node it links with +0xE = 4), and the lift its
/// +0xF gives the autopilot: 5 y velocity 75, 6 55 on either route (`func_15088A08`, C; V L1, L2
/// 735/816: 71.2 the frame after; 7/8/9 are the reversed track's, opt[0x13], which the menus' Race
/// never sets).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Node {
    pub at: [f32; 3],
    pub edge: [f32; 3],
    pub lift: Option<f32>,
}

/// The route's lap: its nodes in order from the start (node id 0x13), `race.json`'s `lap`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub nodes: Vec<Node>,
}

impl Track {
    /// From the navigation graph's bytes (the pack's `Slice::nav`: a 4-byte head, 16 per node) and
    /// a route's lap.
    pub fn new(nav: &[u8], lap: &[u8]) -> Self {
        let node = |i: u8| nav.get(4 + 16 * usize::from(i)..4 + 16 * (usize::from(i) + 1));
        let pos = |r: &[u8]| [0, 2, 4].map(|k| f32::from(i16::from_be_bytes([r[k], r[k + 1]])));
        let nodes = (lap.iter())
            .filter_map(|&i| {
                let r = node(i)?;
                let edge = (r[9..14].iter())
                    .filter_map(|&l| node(l).filter(|n| l != 0xFF && n[0xE] == 4))
                    .next()
                    .unwrap_or(r);
                let lift = match r[0xF] {
                    5 => Some(75.0),
                    6 => Some(55.0),
                    _ => None,
                };
                Some(Node {
                    at: pos(r),
                    edge: pos(edge),
                    lift,
                })
            })
            .collect();
        Self { nodes }
    }

    fn node(&self, k: i32) -> &Node {
        &self.nodes[k.rem_euclid(self.nodes.len() as i32) as usize]
    }

    /// Node `k`'s point at `lateral` (0 the node, 1 its partner; the node's height).
    fn point(&self, k: i32, lateral: f32) -> [f32; 3] {
        let n = self.node(k);
        [
            n.at[0] + (n.edge[0] - n.at[0]) * lateral,
            n.at[1],
            n.at[2] + (n.edge[2] - n.at[2]) * lateral,
        ]
    }

    /// The track at `frac` from node `count` to the next.
    pub fn at(&self, count: i16, frac: f32, lateral: f32) -> [f32; 3] {
        let (a, b) = (
            self.point(count.into(), lateral),
            self.point(i32::from(count) + 1, lateral),
        );
        [0, 1, 2].map(|k| a[k] + (b[k] - a[k]) * frac)
    }

    /// The track's direction there (the centre line's tangents at the two nodes, blended).
    pub fn angle(&self, count: i16, frac: f32) -> u16 {
        let k = i32::from(count);
        let d = |k: i32| {
            let (a, b) = (self.point(k - 1, 0.5), self.point(k + 1, 0.5));
            [b[0] - a[0], b[2] - a[2]]
        };
        let (a, b) = (d(k), d(k + 1));
        let (x, z) = (a[0] + (b[0] - a[0]) * frac, a[1] + (b[1] - a[1]) * frac);
        atan2_u16(-z, x)
    }

    /// Where a racer comes back: the middle of node `count`'s width (V w4: (3082, 701, −8672), node
    /// 21 and 50's), facing along the track turned 0xA00 toward the node's side from `lateral`'s
    /// (`func_15087EF0`, C; V L1 466/687: 57820/53878 against 58334/53419 here).
    pub fn respawn(&self, count: i16, lateral: f32) -> Placement {
        let n = self.node(count.into());
        let turn: u16 = if lateral < 0.5 {
            0xA00
        } else {
            0xA00u16.wrapping_neg()
        };
        Placement {
            index: 0,
            pos: [0, 1, 2].map(|k| (n.at[k] + n.edge[k]) * 0.5),
            facing: self.angle(count, 0.0).wrapping_add(turn),
            camera: [0.0; 3],
        }
    }

    /// The table's place of `pos` from `(count, frac)` at `lateral` (`func_15087350`'s loop: while
    /// more than [`NEAR`] off the point along the track, step 0.75 of the gap, ±0.5 node, halving
    /// when it grows; at most 11 steps). Returns the nodes passed forward (their lifts).
    fn follow(&self, count: &mut i16, frac: &mut f32, lateral: f32, pos: [f32; 3]) -> Vec<i16> {
        let mut passed = Vec::new();
        let (mut gain, mut last) = (0.75, f32::MAX);
        for _ in 0..11 {
            let (a, b) = (
                self.point((*count).into(), lateral),
                self.point(i32::from(*count) + 1, lateral),
            );
            let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
            let len = (dx * dx + dz * dz).sqrt().max(1.0);
            let p = self.at(*count, *frac, lateral);
            let d = ((pos[0] - p[0]) * dx + (pos[2] - p[2]) * dz) / len;
            if d.abs() <= NEAR {
                break;
            }
            if d.abs() > last {
                gain *= 0.5;
            }
            last = d.abs();
            *frac += (d * gain / len).clamp(-0.5, 0.5);
            if *frac >= 1.0 {
                (*frac, *count) = (*frac - 1.0, *count + 1);
                passed.push(*count);
            } else if *frac < 0.0 {
                (*frac, *count) = (*frac + 1.0, *count - 1);
            }
        }
        passed
    }
}

/// A racer's place in the race table (an entry of `*0x800872A0`, 0x84 bytes) and its board.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Ride {
    pub board: Board,
    /// The frame the board may move from (the match's start + [`GO`]; a joiner's own).
    pub go: u32,
    /// +0x24: nodes passed (−1 behind the line at the start), +8 the way to the next.
    pub count: i16,
    pub frac: f32,
    /// +0 across the track (0 on the node, 1 on its partner; 0.5 for a human racing) and +4 its
    /// target (command 0x14).
    pub lateral: f32,
    pub aim: f32,
    /// +0x28: the best lap reached (−1 before the line).
    pub lap: i8,
    /// +0x2A: the finish order (1 the first; 0 racing).
    pub order: u8,
    /// +0x29: the place, 1 the first (every frame).
    pub place: u8,
    /// s126+0x84: driven by the AI (a finished human, a bot).
    pub auto: bool,
    /// +0x48: the script's speed (command 0x12) for the autopilot.
    pub speed: u8,
    /// +0x1C the race clock, +0x18 the lap's start, +0x20 the last lap (VIs); the best lap and the
    /// race's total (`0x800E0C10`, `0x800E0C18`; −1 none).
    pub time: i32,
    pub lap_start: i32,
    pub last: i32,
    pub best: i32,
    pub total: i32,
}

impl Default for Ride {
    fn default() -> Self {
        Self {
            board: Board::default(),
            go: 0,
            count: 0,
            frac: 0.0,
            lateral: 0.5,
            aim: 0.5,
            lap: -1,
            order: 0,
            place: 0,
            auto: false,
            speed: 0,
            time: 0,
            lap_start: 0,
            last: -1,
            best: -1,
            total: -1,
        }
    }
}

impl Ride {
    /// A racer at the start: moving from `go`, the bots on the autopilot.
    pub fn new(go: u32, bot: bool) -> Self {
        Self {
            go,
            auto: bot,
            ..Self::default()
        }
    }

    /// The lap the racer is on (+0x27: nodes / per lap).
    pub fn laps(&self, per: usize) -> i16 {
        self.count / per.max(1) as i16
    }

    /// The progress the script reads (`0xF000` query 0xA): nodes · 16 + the fraction · 16.
    pub fn progress(&self) -> i32 {
        i32::from(self.count) * 16 + (self.frac * 16.0) as i32
    }
}

/// A missile in flight (its own race-table slot, `func_150538CC` kind 0xD).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Missile {
    pub owner: usize,
    pub pos: [f32; 3],
    pub count: i16,
    pub frac: f32,
    pub lateral: f32,
    /// Frames flown, and the racer it homes on (+0x221).
    pub age: u32,
    pub target: Option<usize>,
}

/// The match's Race state besides the racers'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Race {
    pub missiles: Vec<Missile>,
    /// `0x800D239A`: the next finish order (1 first).
    pub finished: u8,
    /// The end rule's frame (the script's 0x1830 returned `result`: 1, or 2 with a human out).
    pub decided: Option<u32>,
    pub result: u8,
    /// The results in the level (phase 6) from this frame, every human on the autopilot.
    pub results: Option<u32>,
}

/// What the script does to the race table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// `command(0xF000, 0x12, r, a)`: the per-frame update's speed for the autopilot.
    Drive(usize, u8),
    /// `command(0xF000, 0x14, r, v)`: the lateral target, v / 256.
    Lateral(usize, i32),
}

/// The countdown's stage at match frame `elapsed`: 0, then 1, 2, 3 at 60, 30 and 5 frames left
/// (G+0x60 from 90 at [`READY`]).
pub fn stage(elapsed: u32) -> u8 {
    let left = (READY + COUNTDOWN).saturating_sub(elapsed + 1);
    match () {
        _ if elapsed < READY || left > 60 => 0,
        _ if left > 30 => 1,
        _ if left > 5 => 2,
        _ => 3,
    }
}

impl Match {
    /// Race: the script's ops.
    pub(crate) fn race_op(&mut self, op: Op) {
        match op {
            Op::Drive(i, a) => {
                if let Some(f) = self.players.get_mut(i) {
                    f.ride.speed = a;
                }
            }
            Op::Lateral(i, v) => {
                if let Some(f) = self.players.get_mut(i) {
                    f.ride.aim = v as f32 / 256.0;
                }
            }
        }
    }

    /// Race: a power-up taken (the script's pickup loop): the same item counts up to [`ITEMS`],
    /// another replaces it.
    pub(crate) fn race_take(&mut self, i: usize, id: u8) {
        let f = &mut self.players[i];
        f.rounds = match f.weapon == Some(id) {
            true => (f.rounds + 1).min(ITEMS),
            false => 1,
        };
        f.weapon = Some(id);
    }

    /// Race: player `i`'s frame (in place of the on-foot one): dead until the respawn, else the
    /// item on Z, the board, the walls and the void, the kick, then its race-table update.
    pub(crate) fn race_player(
        &mut self,
        i: usize,
        input: &Input,
        arena: &Arena,
        world: &CollisionWorld,
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        let track = &arena.track;
        let per = track.nodes.len();
        let laps = self.settings.laps;
        let f = &mut self.players[i];
        let late = f.buttons;
        (f.late, f.buttons) = (f.buttons, input.buttons);
        f.immune = f.immune.saturating_sub(1);
        if let Action::Hurt { step, .. } = f.action {
            let step = step.saturating_add(1);
            f.action = Action::Hurt {
                reaction: DEAD,
                lethal: true,
                step,
            };
            if step >= RESPAWN {
                self.race_respawn(i, arena, world, ev);
            }
            return;
        }
        // The finish (the script's, on the lap of the frame before: `0xF000` c0x16).
        if per > 0 && f.ride.order == 0 && f.ride.laps(per) >= i16::from(laps) {
            self.finish(i);
        }
        let f = &mut self.players[i];
        let go = t >= f.ride.go;
        // The power-up on Z: a human's press; a bot's Z while it is in its pad (`func_150528C8`
        // reads s126+0x8C every frame, C; V L1 928/938: a missile fired the frame after its take).
        let pressed = input.buttons & !if f.bot { 0 } else { late };
        let mut fired = false;
        if go && pressed & BUTTON_Z != 0 && f.rounds > 0 {
            match f.weapon {
                Some(CANISTER) => f.boost = BOOST_VIS,
                Some(MISSILE) => fired = true,
                _ => {}
            }
            f.rounds -= 1;
            if f.rounds == 0 {
                f.weapon = None;
            }
        }
        let boost = f.boost > 2;
        f.boost = if boost { f.boost - 2 } else { 0 };
        // The lateral and the autopilot's point.
        let r = &mut f.ride;
        r.lateral = match r.auto {
            false => 0.5,
            true => r.lateral + ((r.aim - r.lateral) / 8.0).clamp(-EASE, EASE),
        };
        let auto = r.auto.then(|| {
            let speed = match r.speed {
                0 => AUTO_SPEED,
                a => f32::from(a),
            };
            let at = match per {
                0 => {
                    let (c, s) = cos_sin(f.body.move_angle);
                    let p = f.body.pos;
                    [p[0] + 300.0 * c, p[1], p[2] - 300.0 * s]
                }
                _ => {
                    let (n, fr) = ahead(r.count, r.frac + AHEAD);
                    track.at(n, fr, r.lateral)
                }
            };
            (speed, at)
        });
        let drive = Drive {
            go,
            auto,
            track: (per > 0).then(|| track.angle(r.count, r.frac)),
            boost,
        };
        let moved = board::step(&mut f.body, &mut f.ride.board, input, world, drive);
        (f.anim, f.body.state) = (u16::from(f.body.state), f.body.state);
        if fired {
            let r = &f.ride;
            self.race.missiles.push(Missile {
                owner: i,
                pos: f.body.pos,
                count: r.count,
                frac: r.frac,
                lateral: r.lateral,
                age: 0,
                target: None,
            });
            ev.push(Event::Shot {
                player: i,
                pos: f.body.pos,
            });
        }
        // A wall's hit or the void (immune or not: V L1, w4).
        let f = &mut self.players[i];
        let lethal = moved.void || moved.wall.is_some_and(|head_on| head_on || f.health <= 1);
        if lethal {
            return self.race_death(i, ev);
        }
        if moved.wall.is_some() {
            f.health -= 1;
            ev.push(Event::Hit {
                victim: i,
                attacker: i,
                damage: 1,
                lethal: false,
            });
        }
        // The kick's 10th frame: the nearest racer within reach is knocked up.
        if moved.kick {
            let me = f.body.pos;
            let near = (self.players.iter().enumerate())
                .filter(|&(j, v)| j != i && !self.free.contains(&j) && v.out == 0)
                .filter(|(_, v)| v.health > 0 && v.immune == 0 && v.ride.board.knock == 0)
                .map(|(j, v)| (j, dist2(v.body.pos, me)))
                .filter(|&(_, d)| d < KICK_REACH * KICK_REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((v, _)) = near {
                self.knock(v, i, ev);
            }
        }
        if per > 0 {
            self.race_table(i, track);
        }
    }

    /// Player `i`'s race-table update after its move (`func_15087350`): its place on the track,
    /// the laps and lap times, the autopilot's lifts, and the race clock.
    fn race_table(&mut self, i: usize, track: &Track) {
        let t = self.tick;
        let laps = i8::try_from(self.settings.laps).unwrap_or(i8::MAX);
        let per = track.nodes.len() as i16;
        let f = &mut self.players[i];
        let r = &mut f.ride;
        let passed = track.follow(&mut r.count, &mut r.frac, r.lateral, f.body.pos);
        let mut lift = None;
        for &c in &passed {
            lift = lift.or(track.node(c.into()).lift);
            let lap = i8::try_from(c / per).unwrap_or(i8::MAX);
            if lap <= r.lap {
                continue;
            }
            r.lap = lap;
            if !f.bot && r.order == 0 && r.time > 0 {
                if laps >= lap {
                    r.last = (r.time - r.lap_start).min(LAP_CAP);
                    if r.best <= 0 || r.last < r.best {
                        r.best = r.last;
                    }
                }
                if laps == lap {
                    r.total = r.time.min(TOTAL_CAP);
                }
            }
            r.lap_start = if laps == lap { -r.time } else { r.time };
        }
        // The autopilot's look-ahead passing a node lifts it too (each frame it does).
        let (n, _) = ahead(r.count, r.frac + AHEAD);
        if n != r.count {
            lift = lift.or(track.node(n.into()).lift);
        }
        if let Some(v) = lift.filter(|_| r.auto && t >= r.go) {
            f.body.y_vel = v;
        }
        if !f.bot && r.order == 0 && f.out == 0 && t + 1 >= r.go.max(1) {
            r.time += 2;
        }
    }

    /// The finish (`func_150880F8`): the next order; a human goes on the autopilot (not once out),
    /// everyone finished is immune.
    pub(crate) fn finish(&mut self, i: usize) {
        let order = self.race.finished.max(1);
        self.race.finished = order + 1;
        let f = &mut self.players[i];
        f.ride.order = order;
        if f.out == 0 {
            (f.ride.auto, f.immune) = (true, 0xFF);
        }
    }

    /// A missile's or a kick's hit: `v` knocked up, its health kept.
    fn knock(&mut self, v: usize, attacker: usize, ev: &mut Vec<Event>) {
        let f = &mut self.players[v];
        f.ride.board.knock(&mut f.body);
        f.ride.board.kick = 0;
        ev.push(Event::Hit {
            victim: v,
            attacker,
            damage: 0,
            lethal: false,
        });
    }

    /// A racer's death (the void, a wall): state 112 where it is, its item gone; a life counts.
    fn race_death(&mut self, i: usize, ev: &mut Vec<Event>) {
        let t = self.tick;
        let f = &mut self.players[i];
        let damage = f.health;
        f.health = 0;
        f.deaths = f.deaths.saturating_add(1);
        (f.weapon, f.rounds, f.boost) = (None, 0, 0);
        f.set(
            Action::Hurt {
                reaction: DEAD,
                lethal: true,
                step: 0,
            },
            u16::from(DEAD),
            t,
        );
        f.body.state = DEAD;
        (f.body.xz_speed, f.body.y_vel) = (0.0, 0.0);
        f.ride.board = Board::default();
        ev.push(Event::Hit {
            victim: i,
            attacker: i,
            damage,
            lethal: true,
        });
    }

    /// Back on the track [`RESPAWN`] frames after the death (the life counted there: out at 0).
    fn race_respawn(
        &mut self,
        i: usize,
        arena: &Arena,
        world: &CollisionWorld,
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        if self.lives_left(i) == 0 {
            let place = (0..self.players.len()).filter(|&j| self.in_play(j)).count();
            let f = &mut self.players[i];
            (f.out, f.out_at) = (place as u8, t);
            return;
        }
        let f = self.players[i];
        // ponytail: past a node over the void (node 8, above the chasm), the next with a floor
        // under it (I: V L1 1798, died in the chasm and back on node 9; the game's rule untraced).
        let track = &arena.track;
        let count = (0..track.nodes.len() as i16)
            .map(|k| f.ride.count + k)
            .find(|&c| {
                let p = track.respawn(c, f.ride.lateral).pos;
                (world.floor(p[0], p[2], p[1], p[1], None)).is_some_and(|fl| p[1] - fl.y < OVER)
            })
            .unwrap_or(f.ride.count);
        let at = match track.nodes.is_empty() {
            true => Placement {
                index: 0,
                pos: f.body.pos,
                facing: f.body.facing,
                camera: [0.0; 3],
            },
            false => track.respawn(count, f.ride.lateral),
        };
        let mut body = conker_sim::movement::PlayerState::spawn(at.pos, at.facing);
        (body.floor_y, body.prev_buttons) = (at.pos[1], f.buttons);
        self.players[i] = Fighter {
            body,
            health: crate::game::HEALTH,
            immune: crate::game::SPAWN_IMMUNE,
            action: Action::Free,
            anim: u16::from(board::RIDE),
            since: t,
            ride: Ride {
                board: Board::default(),
                count,
                frac: 0.0,
                ..f.ride
            },
            ..f
        };
        ev.push(Event::Respawn { player: i, at });
    }

    /// The missiles, after the racers (those fired this frame fly from the next): along the track,
    /// then at their target.
    pub(crate) fn fly_missiles(&mut self, arena: &Arena, ev: &mut Vec<Event>) {
        let track = &arena.track;
        let step = 0.5 * MISSILE_SPEED;
        let mut missiles = std::mem::take(&mut self.race.missiles);
        let mut hits = Vec::new();
        missiles.retain_mut(|m| {
            m.age += 1;
            if m.age == 1 {
                return true;
            }
            if m.age > MISSILE_LIFE + 1 {
                return false;
            }
            // From the 10th frame the next racer ahead (not immune, not knocked up), once within
            // 1000 horizontally.
            if m.age > HOMING && m.target.is_none() {
                let mine = f32::from(m.count) + m.frac;
                m.target = (self.players.iter().enumerate())
                    .filter(|&(j, f)| j != m.owner && !self.free.contains(&j) && f.out == 0)
                    .filter(|(_, f)| f.health > 0 && f.immune == 0)
                    .map(|(j, f)| (j, f32::from(f.ride.count) + f.ride.frac - mine))
                    .filter(|&(_, d)| d > 0.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(j, _)| j)
                    .filter(|&j| {
                        let p = self.players[j].body.pos;
                        let (dx, dz) = (p[0] - m.pos[0], p[2] - m.pos[2]);
                        dx * dx + dz * dz < HOMING_RANGE * HOMING_RANGE
                    });
            }
            let target = m.target.map(|j| &self.players[j]);
            match target.filter(|f| f.health > 0) {
                Some(f) => {
                    let to = [f.body.pos[0], f.body.pos[1] + 40.0, f.body.pos[2]];
                    let d = [to[0] - m.pos[0], to[1] - m.pos[1], to[2] - m.pos[2]];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    if len <= step.max(MISSILE_HIT) {
                        hits.push((m.target.unwrap_or(0), m.owner));
                        return false;
                    }
                    m.pos = [0, 1, 2].map(|k| m.pos[k] + d[k] * step / len);
                }
                None if !track.nodes.is_empty() => {
                    let (a, b) = (
                        track.at(m.count, 0.0, m.lateral),
                        track.at(m.count, 1.0, m.lateral),
                    );
                    let len = ((b[0] - a[0]).powi(2) + (b[2] - a[2]).powi(2))
                        .sqrt()
                        .max(1.0);
                    let (n, fr) = ahead(m.count, m.frac + step / len);
                    (m.count, m.frac) = (n, fr);
                    let p = track.at(n, fr, m.lateral);
                    m.pos = [p[0], p[1] + MISSILE_LIFT, p[2]];
                }
                None => {
                    m.target = None;
                }
            }
            true
        });
        self.race.missiles = missiles;
        for (v, owner) in hits {
            if self
                .players
                .get(v)
                .is_some_and(|f| f.health > 0 && f.immune == 0)
            {
                self.knock(v, owner, ev);
            }
        }
    }

    /// After the racers: the places (`func_15087350`'s sort by progress, + 30000 − 1000 · order
    /// once finished, − 10000 once out) and the end rule (the script's 0x1830: every human
    /// finished or out, or at most one racer still racing), then the results 39 frames on.
    pub(crate) fn race_end(&mut self) {
        let t = self.tick;
        let racers: Vec<usize> = (0..self.players.len())
            .filter(|i| !self.free.contains(i))
            .collect();
        // The finished by their order, then the racing by progress, then the out (I: a human out
        // and then finished by the end rule still places behind the racers).
        let score = |f: &Fighter| {
            let mut s = f32::from(f.ride.count) + f.ride.frac;
            if f.out > 0 {
                s -= 10000.0;
            } else if f.ride.order > 0 {
                s += 30000.0 - 1000.0 * f32::from(f.ride.order);
            }
            s
        };
        let mut order = racers.clone();
        order.sort_by(|&a, &b| score(&self.players[b]).total_cmp(&score(&self.players[a])));
        for (k, &i) in order.iter().enumerate() {
            self.players[i].ride.place = k as u8 + 1;
        }
        if self.race.decided.is_none() {
            let count = |p: &dyn Fn(&Fighter) -> bool| {
                racers.iter().filter(|&&i| p(&self.players[i])).count()
            };
            let humans = count(&|f| !f.bot);
            let humans_out = count(&|f| !f.bot && f.out > 0);
            let humans_in = count(&|f| !f.bot && f.out == 0 && f.ride.order > 0);
            let finished = count(&|f| f.out == 0 && f.ride.order > 0);
            let out = count(&|f| f.out > 0);
            let result = if humans_out > 0 { 2 } else { 1 };
            // I: a lone racer races on (the game's SET-UP never starts one), and bots alone race
            // on until a human joins (the game always has one).
            let ends = (humans > 0 && humans_in + humans_out >= humans)
                || (racers.len() > 1 && finished + out + 1 >= racers.len());
            if ends && !racers.is_empty() {
                (self.race.decided, self.race.result) = (Some(t), result);
                for &i in &racers {
                    let f = &self.players[i];
                    if !f.bot && f.ride.order == 0 {
                        self.finish(i);
                    }
                }
            }
        }
        if let Some(d) = self.race.decided
            && self.race.results.is_none()
            && t >= d + RESULTS
        {
            self.race.results = Some(t);
        }
    }
}

/// `(count, frac)` moved on by whole nodes.
fn ahead(count: i16, frac: f32) -> (i16, f32) {
    let n = frac.floor();
    (count + n as i16, frac - n)
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|k| (a[k] - b[k]).powi(2)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Settings;
    use crate::rng::Rng;
    use conker_sim::movement::PlayerState;

    /// An octagon of 8 nodes (the start mid-straight), 400 wide (partners outward), on a flat floor.
    fn course() -> (Arena, CollisionWorld) {
        let mut nav = vec![0, 16, 0, 0];
        let pts: [(i16, i16); 8] = [
            (0, 0),
            (2000, 0),
            (4000, -2000),
            (2000, -4000),
            (0, -4000),
            (-2000, -4000),
            (-4000, -2000),
            (-2000, 0),
        ];
        let out: [(i16, i16); 8] = [
            (0, 1),
            (0, 1),
            (1, 0),
            (0, -1),
            (0, -1),
            (0, -1),
            (-1, 0),
            (0, 1),
        ];
        for (k, &(x, z)) in pts.iter().enumerate() {
            let mut r = [0u8; 16];
            r[0..2].copy_from_slice(&x.to_be_bytes());
            r[4..6].copy_from_slice(&z.to_be_bytes());
            r[9] = 8 + k as u8;
            nav.extend(r);
        }
        for (k, &(x, z)) in pts.iter().enumerate() {
            let mut r = [0u8; 16];
            r[0..2].copy_from_slice(&(x + 400 * out[k].0).to_be_bytes());
            r[4..6].copy_from_slice(&(z + 400 * out[k].1).to_be_bytes());
            r[9..14].fill(0xFF);
            r[0xE] = 4;
            nav.extend(r);
        }
        let track = Track::new(&nav, &[0, 1, 2, 3, 4, 5, 6, 7]);
        let big = 20_000.0;
        let world = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ]);
        let arena = Arena {
            track,
            ..Arena::default()
        };
        (arena, world)
    }

    fn race(n: usize, bots: usize, laps: u8) -> Match {
        let players = (0..n)
            .map(|i| {
                let at = Placement {
                    index: i,
                    pos: [-300.0 - 100.0 * i as f32, 0.0, 200.0],
                    facing: 0,
                    camera: [0.0; 3],
                };
                Fighter {
                    bot: i >= n - bots,
                    ride: Ride::new(GO, i >= n - bots),
                    ..Fighter::new(&at, i as u8, 0)
                }
            })
            .collect();
        Match::new(&Arena::default(), players, Rng { state: 1 }, 0).with(Settings {
            race: true,
            laps,
            time: None,
            ..Settings::default()
        })
    }

    fn input(x: i8, y: i8, buttons: u16) -> Input {
        Input {
            stick_x: x,
            stick_y: y,
            buttons,
            camera_yaw: 0,
        }
    }

    /// V r0: the beeps' stages at reads 142, 172, 197 (match frames 119, 149, 174).
    #[test]
    fn countdown_as_r0() {
        let first = |s: u8| (0..300).find(|&e| stage(e) == s);
        assert_eq!(
            (first(1), first(2), first(3)),
            (Some(119), Some(149), Some(174))
        );
        assert_eq!(stage(89), 0);
    }

    /// The track: the projection follows a racer node by node; behind the line it counts −1.
    #[test]
    fn the_table_follows() {
        let (arena, _) = course();
        let t = &arena.track;
        let (mut c, mut f) = (0i16, 0.0f32);
        t.follow(&mut c, &mut f, 0.5, [-300.0, 0.0, 200.0]);
        assert_eq!(c, -1);
        let passed = t.follow(&mut c, &mut f, 0.5, [2100.0, 0.0, 200.0]);
        assert_eq!((c, passed), (1, vec![0, 1]));
        assert!(f < 0.1, "{f}");
        assert_eq!(t.angle(0, 0.0), 0);
        let r = t.respawn(1, 0.5);
        assert_eq!(r.pos, [2000.0, 0.0, 200.0]);
    }

    /// The race from the line: nobody moves before go; the human's first crossing records its
    /// time as the best lap (V F1: 56 VIs, 28 frames after the clock starts); it finishes on the
    /// lap's count, goes on the autopilot, and the end rule decides; the results 39 frames on,
    /// the fade 1201 after (V F1 232 → 271 → 1472).
    #[test]
    fn a_lap_as_f1() {
        let (arena, world) = course();
        let mut m = race(1, 0, 1);
        let fwd = [input(0, 82, 0)];
        for _ in 0..GO {
            m.step(&arena, &fwd, &world);
        }
        assert_eq!(m.players[0].body.pos[0], -300.0);
        assert_eq!(m.players[0].ride.count, -1);
        let mut crossed = None;
        while crossed.is_none() && m.tick < 1000 {
            m.step(&arena, &fwd, &world);
            if m.players[0].ride.lap == 0 {
                crossed = Some(m.tick);
            }
        }
        let r = m.players[0].ride;
        assert_eq!(
            (r.count, r.best),
            (0, 2 * (crossed.unwrap() as i32 - 1 - CLOCK as i32))
        );
        // Round again (put back behind the line, a lap on).
        let f = &mut m.players[0];
        (f.ride.count, f.ride.frac) = (7, 0.85);
        (f.body.pos[0], f.body.pos[2], f.body.move_angle) = (-300.0, 200.0, 0);
        while m.race.decided.is_none() && m.tick < 3000 {
            m.step(&arena, &fwd, &world);
        }
        let r = m.players[0].ride;
        assert_eq!((r.order, r.auto, m.players[0].immune), (1, true, 0xFF));
        assert!(r.total > r.best && r.lap == 1, "{r:?}");
        let d = m.race.decided.unwrap();
        while m.phase() != crate::game::Phase::Fading {
            m.step(&arena, &fwd, &world);
        }
        assert_eq!(m.race.results, Some(d + RESULTS));
        assert_eq!(m.tick, d + RESULTS + RESULTS_FOR);
    }

    /// Bots race on the autopilot round the course at the script's speed and finish in order;
    /// bots alone (a server before its first human) race on undecided.
    #[test]
    fn bots_race_and_finish() {
        let (arena, world) = course();
        let mut alone = race(2, 2, 2);
        alone.step(&arena, &[Input::default(); 2], &world);
        assert_eq!(alone.race.decided, None);
        let mut m = race(3, 2, 2);
        m.players[1].ride.speed = 180;
        m.players[2].ride.speed = 150;
        let idle = [Input::default(); 3];
        while m.race.decided.is_none() && m.tick < 4000 {
            m.step(&arena, &idle, &world);
        }
        let order: Vec<u8> = m.players.iter().map(|f| f.ride.order).collect();
        assert_eq!(order, [3, 1, 2], "{:?}", m.players[1].ride);
        assert_eq!(m.race.result, 1);
        assert_eq!(m.places(), [3, 1, 2]);
    }

    /// The void kills; 88 frames on the racer is back mid-track on its node, 150 immune, its item
    /// gone; at 0 lives it is out.
    #[test]
    fn death_and_respawn() {
        let (arena, world) = course();
        let mut m = race(2, 0, 5);
        m.players[0].weapon = Some(MISSILE);
        m.players[0].rounds = 3;
        m.players[0].ride.count = 2;
        m.players[0].body = PlayerState::spawn([30_000.0, 0.0, 0.0], 0);
        m.tick = GO;
        let idle = [Input::default(); 2];
        let ev = m.step(&arena, &idle, &world);
        assert!(ev.iter().any(|e| matches!(
            e,
            Event::Hit {
                victim: 0,
                lethal: true,
                ..
            }
        )));
        assert_eq!((m.players[0].body.state, m.players[0].weapon), (DEAD, None));
        for _ in 0..RESPAWN - 1 {
            m.step(&arena, &idle, &world);
        }
        assert_eq!(m.players[0].health, 0);
        let ev = m.step(&arena, &idle, &world);
        assert!(
            ev.iter()
                .any(|e| matches!(e, Event::Respawn { player: 0, .. }))
        );
        let f = m.players[0];
        assert_eq!((f.health, f.immune, f.deaths), (6, 150, 1));
        assert_eq!(f.body.pos, [4200.0, 0.0, -2000.0]);
    }

    /// A canister boosts at 254 for 60 frames; a missile flies the track and knocks the racer ahead
    /// up (state 84), its health kept; B's kick on its 10th frame knocks a racer within reach.
    #[test]
    fn power_ups_and_the_kick() {
        let (arena, world) = course();
        let mut m = race(2, 0, 5);
        m.tick = GO;
        m.race_take(0, CANISTER);
        m.race_take(0, CANISTER);
        assert_eq!(
            (m.players[0].weapon, m.players[0].rounds),
            (Some(CANISTER), 2)
        );
        m.race_take(0, MISSILE);
        assert_eq!(m.players[0].rounds, 1);
        m.race_take(0, CANISTER);
        let z = [input(0, 0, BUTTON_Z), Input::default()];
        m.step(&arena, &z, &world);
        assert_eq!(m.players[0].body.xz_speed, board::BOOST);
        let idle = [Input::default(); 2];
        for _ in 0..59 {
            m.step(&arena, &idle, &world);
        }
        assert_eq!(m.players[0].body.xz_speed, board::BOOST - 1.0);
        // The missile: player 1 ahead on the track.
        let mut m = race(2, 0, 5);
        m.tick = GO;
        m.players[1].body = PlayerState::spawn([2500.0, 0.0, 200.0], 0);
        m.players[1].immune = 0;
        m.players[0].immune = 0;
        m.race_take(0, MISSILE);
        let mut hit = None;
        for k in 0..60 {
            let i = if k == 0 { z } else { idle };
            let ev = m.step(&arena, &i, &world);
            if ev.iter().any(|e| {
                matches!(
                    e,
                    Event::Hit {
                        victim: 1,
                        attacker: 0,
                        ..
                    }
                )
            }) {
                hit = Some(k);
            }
        }
        assert!(hit.is_some());
        assert_eq!(m.players[1].health, 6);
        // The kick.
        let mut m = race(2, 0, 5);
        m.tick = GO;
        m.players[1].body = PlayerState::spawn([-200.0, 0.0, 100.0], 0);
        m.players[1].immune = 0;
        let b = [input(0, 0, crate::game::BUTTON_B), Input::default()];
        let mut knocked = None;
        for k in 1..=12 {
            let ev = m.step(&arena, if k == 1 { &b } else { &idle }, &world);
            if ev.iter().any(|e| matches!(e, Event::Hit { victim: 1, .. })) {
                knocked = Some(k);
            }
        }
        assert_eq!(knocked, Some(board::KICK_HIT));
        assert_eq!(m.players[1].body.state, board::KICK);
    }

    /// A bot's Z stays in its pad (V L1: the bot whose last press was the canister's Z fired each
    /// missile the frame after its take, reads 928 and 938): held, a bot uses every power-up the
    /// frame it has one, while a human holding Z uses only the one it pressed for.
    #[test]
    fn a_bot_holding_z_uses_each_power_up() {
        let (arena, world) = course();
        let mut m = race(2, 1, 5);
        m.tick = GO;
        let z = [input(0, 0, BUTTON_Z); 2];
        for k in 0..3 {
            m.race_take(0, MISSILE);
            m.race_take(1, MISSILE);
            m.step(&arena, &z, &world);
            let left: Vec<u8> = m.players.iter().map(|f| f.rounds).collect();
            assert_eq!(left, [u8::from(k > 0) * k, 0], "take {k}");
        }
        assert_eq!(m.race.missiles.iter().filter(|s| s.owner == 1).count(), 3);
        assert_eq!(m.race.missiles.iter().filter(|s| s.owner == 0).count(), 1);
    }

    /// A human out of lives with bots racing on ends as Deathmatch's (V b1).
    #[test]
    fn a_human_out_ends_as_b1() {
        let (arena, world) = course();
        let mut m = race(2, 1, 5);
        m.players[0].deaths = 3;
        m.players[0].health = 0;
        m.players[0].action = Action::Hurt {
            reaction: DEAD,
            lethal: true,
            step: RESPAWN - 1,
        };
        m.tick = GO;
        let idle = [Input::default(); 2];
        m.step(&arena, &idle, &world);
        assert!(m.players[0].out > 0);
        assert_eq!(m.ended_at, Some(GO + 1));
        // Finished by the end rule, still behind the bot racing.
        assert_eq!((m.players[0].ride.order, m.places()), (1, vec![2, 1]));
    }
}
