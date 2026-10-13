//! War: Raptor (scenario 2, scene 0x3F): the cavemen (team 0, the UGAS: class 0xB, model 136, the
//! club) against the raptors (team 1, class 9, model 83). `../lab/research/mp-original/raptor.md`;
//! V on its oracle runs (r1-r12, b1, b2) unless tagged.
//!
//! The raptor plays its own frame ([`Match::raptor_player`], its gait conker-sim's `raptor`): Z or
//! B bites (the standing bite kills every enemy in reach and leaves a body in the jaws; at full
//! speed the running bite knocks one up for 3), or drops the body it holds. The script (0xBE's
//! team branch, conker-bots) does the rest through [`Op`]: the eggs appear, a caveman's touch takes
//! one (the match's, below), a hit or a death resets it, the fire takes it (+2) and cooks it; a
//! raptor in the nest with a body feeds the baby (+1). Every kill is +1 for the killer's team.

use conker_sim::collision::CollisionWorld;
use conker_sim::movement::{
    self, BUTTON_Z, Input, Other, PlayerState, Side, angle_diff, atan2_u16,
};
use conker_sim::raptor::{self as gait, Drive, clip};

use crate::colors::Pose;
use crate::damage::{self, DamageType, in_reach};
use crate::game::{Action, Arena, BUTTON_B, Event, Match, Seen};

/// The raptor's class (s126+0x6E; `Combat::class`).
pub const CLASS: u8 = 9;
/// A caveman's class (0xB, model 136).
pub const CAVEMAN: u8 = 0xB;
/// The score target's default (opt[18], 1-40).
pub const SCORE: u8 = 20;
/// The teams: 0 the cavemen, 1 the raptors.
pub const CAVEMEN: u8 = 0;
pub const RAPTORS: u8 = 1;

/// The egg a caveman carries (`Fighter::flag`; Colors' flags are 0 and 1, Heist's bag 2).
pub const EGG: u8 = 3;
/// The eggs' scene uids (handles `0x2000 |` uid) and records (model 171).
pub const EGG_UID: usize = 0x14;
pub const EGGS: [[f32; 3]; 3] = [
    [-7008.0, 209.0, -3044.0],
    [-7249.0, 209.0, -3095.0],
    [-7193.0, 209.0, -2844.0],
];
/// The baby's uid (model 165, in the nest).
pub const BABY_UID: usize = 0x11;
/// A carried egg: 40 ahead of the carrier, 38 up (V r4).
pub const HELD: [f32; 2] = [40.0, 38.0];
/// The carrier's clips: the stand, walk and run (V r4); the grab is the flag's ([`Pose::Grab`]),
/// the throw at the fire [`Pose::EggThrow`].
pub const EGG_IDLE: u16 = 0xD8;
pub const EGG_WALK: u16 = 0xD7;
pub const EGG_RUN: u16 = 0x2E4;
/// The cooked eggs: props 0xFF − n (placements 0-3), shown in turn where an egg splats, at y 156.
pub const COOKED: usize = 4;
pub const COOKED_Y: f32 = 156.0;
/// The baby eats a fed body for 156 frames (V r3: behaviour 44 from 152 to 308).
pub const EATS: u32 = 156;
/// A dropped body goes 39 frames after the drop (V r6).
pub const DROPPED: u32 = 39;

/// The standing bite (clip 49, 28 frames): its area test from frame 14 for 4 frames (V r2, r5, r6:
/// 14; I the window, the pack's 4), centre 224 ahead, radius 84 (entry 4: distance 80 × 0.7 /
/// 0.125 / 2, radius 30 × 4 × 0.7); before it the raptor turns toward an enemy ahead by at most
/// 1100 a frame (V r5 at 45°; I the cone and range). The running bite (clip 53, 11 frames, only
/// at full speed): frame 10, centre 140, radius 154 (entry 6; V r8).
pub const BITE_AT: u32 = 14;
pub const BITE_WINDOW: u32 = 4;
pub const BITE_REACH: (f32, f32) = (224.0, 84.0);
pub const RUN_BITE_AT: u32 = 10;
pub const RUN_BITE_REACH: (f32, f32) = (140.0, 154.0);
pub const LOCK_TURN: i32 = 1100;
const LOCK_CONE: i32 = 0x2AAA;
const LOCK_RANGE: f32 = 500.0;
/// A raptor's death: clip 47, 48 from 59 frames in, the respawn at 119 (V r7 451 → 570).
pub const DEAD_FROM: u8 = 59;
pub const DOWN_AT: u8 = 119;

/// An egg (handle `0x2000 | EGG_UID + k`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Egg {
    pub shown: bool,
    pub carrier: Option<usize>,
    pub pos: [f32; 3],
}

/// Raptor's objects: the eggs, the cooked eggs shown, the baby's last meal, the bodies let go.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Raptor {
    pub eggs: [Egg; 3],
    pub cooked: [Option<[f32; 3]>; COOKED],
    /// The tick of the last nest drop (the baby eats for [`EATS`]).
    pub fed: Option<u32>,
    /// Bodies dropped (Z or B): where, facing, and the drop's tick, for [`DROPPED`] frames.
    pub drops: Vec<([f32; 3], u16, u32)>,
}

/// What script 0xBE does to Raptor's objects (conker-bots' natives), before the step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// `obj(0x2014 + k)`: egg k appears on its record; `obj2(egg, 1)`: it goes (a carrier lets go).
    Egg(usize, bool),
    /// `command(p, 7, egg)`: the carrier lets go (a hit or a death resets the egg).
    LetGo(usize),
    /// `command(p, 0x10, deg << 16)`: the player faces `deg` (degrees, the game's yaw).
    Face(usize, u16),
    /// `command(p, 1, 1, 0x10)`: the carrier throws its egg into the fire (clip 0xD6).
    Throw(usize),
    /// `command(p, 0x14, 0x28)`: a raptor drops its body in the nest (behaviour 40, clip 50).
    Nest(usize),
    /// `command(0x30FF − n, 2, x, MAX, z)`: cooked egg n shown at (x, 156, z).
    Cooked(usize, [f32; 2]),
}

/// The egg's touch: a living caveman's body over it, on the ground (as the bag's, radius 29; I).
pub fn touches(egg: [f32; 3], player: &PlayerState, side: Side) -> bool {
    let [x, y, z] = player.pos;
    let low = y + 0.5 * (movement::GROUND_Y_VEL - movement::GRAVITY_FALL);
    !player.airborne && crate::heist::body(egg).overlaps(&Other::player(side, [x, low, z]))
}

/// Where a carried egg is.
pub fn held(pos: [f32; 3], facing: u16) -> [f32; 3] {
    let (c, s) = conker_sim::movement::cos_sin(facing);
    let [ahead, up] = HELD;
    [pos[0] + ahead * c, pos[1] + up, pos[2] - ahead * s]
}

/// The egg carrier's clip by its ground speed (as the bag's thresholds).
pub fn carry_clip(speed: f32) -> u16 {
    match speed {
        s if s < 1.0 => EGG_IDLE,
        s if s > 14.0 => EGG_RUN,
        _ => EGG_WALK,
    }
}

/// A hurt raptor's clip and its length by the hit's reaction: the club's 54 (13 frames), the
/// hazard's launch 23 (to the landing), the bolts' and knives' 12 (17) (V r7, r12, r9, r10).
fn hurt_clip(reaction: u8) -> (u16, u32) {
    match reaction {
        0x25 | 0x26 => (clip::HURT_CLUB, 13),
        0x2B => (clip::LAUNCH, 1),
        _ => (clip::HURT, 17),
    }
}

impl Match {
    /// A raptor's frame: its hurt and death clips, the bites and the drop, else the stick.
    pub(crate) fn raptor_player(
        &mut self,
        i: usize,
        input: &Input,
        arena: &Arena,
        world: &CollisionWorld,
        seen: Seen,
        ev: &mut Vec<Event>,
    ) {
        let t = self.tick;
        let others: Vec<Other> = (self.players.iter().enumerate())
            .filter(|&(j, _)| j != i && !self.free.contains(&j))
            .map(|(_, p)| p.other())
            .collect();
        let target = self.lock_target(i, seen);
        let f = &mut self.players[i];
        let late = f.buttons;
        (f.late, f.buttons) = (f.buttons, input.buttons);
        let pressed = input.buttons & !late;
        let still = Input {
            stick_x: 0,
            stick_y: 0,
            buttons: 0,
            ..*input
        };
        if let Action::Hurt {
            reaction,
            lethal,
            step,
        } = f.action
        {
            let step = step.saturating_add(1);
            f.action = Action::Hurt {
                reaction,
                lethal,
                step,
            };
            // No knock-back (I): the raptor stands, or falls from the hazard's hop.
            f.body.xz_speed = 0.0;
            gait::step(&mut f.body, &still, world, &others, Drive::Hold);
            if lethal {
                f.immune = 255;
                f.anim = if step < DEAD_FROM {
                    clip::DEATH
                } else {
                    clip::DEAD
                };
                if step >= DOWN_AT {
                    self.down(i, arena, ev);
                    let f = &mut self.players[i];
                    if f.out == 0 {
                        f.set(Action::Pose(Pose::Hatch), clip::RESPAWN, t);
                    }
                }
                return;
            }
            let (anim, frames) = hurt_clip(reaction);
            f.anim = anim;
            if u32::from(step) >= frames && !f.body.airborne {
                f.set(Action::Free, clip::IDLE, t);
                f.stun = 0;
            }
            return;
        }
        f.stun = 0;
        f.immune = f.immune.saturating_sub(1);
        let n = t - f.since;
        if let Action::Pose(pose) = f.action
            && n >= pose.clip().1
        {
            f.set(Action::Free, clip::IDLE, t);
        }
        // Z or B on the ground, free: the drop with a body, else a bite (running at full speed).
        let jumping = f.body.state == clip::JUMP as u8;
        if f.action == Action::Free
            && !f.body.airborne
            && !jumping
            && pressed & (BUTTON_Z | BUTTON_B) != 0
        {
            let pose = match () {
                _ if f.prey => Pose::Drop,
                _ if f.body.xz_speed >= gait::TOP => Pose::RunBite,
                _ => Pose::Bite,
            };
            let (anim, frames, _) = pose.clip();
            f.set(Action::Pose(pose), anim, t);
            f.ready = t + frames;
            if pose == Pose::Drop {
                f.prey = false;
                let at = gait_jaw(&f.body);
                self.raptor.drops.push((at, f.body.facing, t));
            }
        }
        let f = &mut self.players[i];
        let n = t - f.since;
        let drive = match f.action {
            Action::Pose(Pose::RunBite) => Drive::Lunge(n as u8),
            Action::Pose(_) => Drive::Hold,
            _ => Drive::Stick,
        };
        let free = f.action == Action::Free;
        gait::step(
            &mut f.body,
            if free { input } else { &still },
            world,
            &others,
            drive,
        );
        if free {
            f.anim = u16::from(f.body.state);
            return;
        }
        // The standing bite turns toward an enemy ahead before it hits; then both bites test.
        let pose = match f.action {
            Action::Pose(p) => p,
            _ => return,
        };
        if pose == Pose::Bite
            && n < BITE_AT
            && let Some(to) = target
        {
            let d = angle_diff(to, f.body.facing).clamp(-LOCK_TURN, LOCK_TURN);
            f.body.facing = f.body.facing.wrapping_add(d as u16);
            f.body.move_angle = f.body.facing;
        }
        let (kind, reach, window) = match pose {
            Pose::Bite => (damage::BITE, BITE_REACH, BITE_AT..BITE_AT + BITE_WINDOW),
            Pose::RunBite => (
                damage::RUN_BITE,
                RUN_BITE_REACH,
                RUN_BITE_AT..RUN_BITE_AT + 1,
            ),
            _ => return,
        };
        if window.contains(&n) && t < f.ready && self.bite(i, &kind, reach, seen, ev) {
            // The attack ends with its hit; a kill leaves a body in the jaws (V r2: +0x2E4 on the
            // hit's frame).
            let killed = kind.reaction == 0x46;
            let f = &mut self.players[i];
            f.ready = t;
            f.prey |= killed;
        }
    }

    /// The bite's area: every enemy within `reach` (centre ahead, radius) is hit (V b2 2129: three
    /// cavemen at once). Returns whether one was.
    fn bite(
        &mut self,
        i: usize,
        kind: &DamageType,
        (ahead, r): (f32, f32),
        seen: Seen,
        ev: &mut Vec<Event>,
    ) -> bool {
        let me = self.players[i].body;
        let (c, s) = conker_sim::movement::cos_sin(me.facing);
        let centre = [me.pos[0] + ahead * c, me.pos[1], me.pos[2] - ahead * s];
        let mut hit = false;
        for v in 0..self.players.len() {
            if v == i || self.free.contains(&v) || !self.enemies(i, v) {
                continue;
            }
            let p = self.seen_pos(seen, i, v);
            if in_reach(centre, r, p, self.players[v].hitbox()) {
                hit |= self.hurt(v, kind, true, false, me.facing, Some(i), ev);
            }
        }
        hit
    }

    /// The direction to the nearest living enemy ahead of raptor `i` (its bite's lock-on).
    fn lock_target(&self, i: usize, seen: Seen) -> Option<u16> {
        let me = &self.players[i].body;
        (0..self.players.len())
            .filter(|&v| v != i && !self.free.contains(&v) && self.enemies(i, v))
            .filter(|&v| self.players[v].health > 0)
            .filter_map(|v| {
                let p = self.seen_pos(seen, i, v);
                let (dx, dz) = (p[0] - me.pos[0], p[2] - me.pos[2]);
                let d = dx.hypot(dz);
                let a = atan2_u16(-dz, dx);
                (d < LOCK_RANGE && angle_diff(a, me.facing).abs() < LOCK_CONE).then_some((d, a))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, a)| a)
    }

    /// The cavemen take the eggs they touch (raptors never do), as Colors' flags are touched.
    pub(crate) fn touch_eggs(&mut self) {
        let t = self.tick;
        for k in 0..self.raptor.eggs.len() {
            let egg = self.raptor.eggs[k];
            if !egg.shown || egg.carrier.is_some() {
                continue;
            }
            let toucher = self.players.iter().enumerate().position(|(i, f)| {
                !self.free.contains(&i)
                    && f.health > 0
                    && f.class != CLASS
                    && f.flag.is_none()
                    && f.stun == 0
                    && f.body.lock == 0
                    && matches!(
                        f.action,
                        Action::Free
                            | Action::Fire { .. }
                            | Action::Aim { .. }
                            | Action::Reload { .. }
                    )
                    && touches(egg.pos, &f.body, f.side)
            });
            if let Some(i) = toucher {
                let f = &mut self.players[i];
                (f.flag, f.drawn) = (Some(EGG), false);
                let (anim, ..) = Pose::Grab.clip();
                f.set(Action::Pose(Pose::Grab), anim, t);
                self.raptor.eggs[k].carrier = Some(i);
            }
        }
    }

    /// The carried eggs follow their carriers' hands; the dropped bodies go.
    pub(crate) fn hold_eggs(&mut self) {
        for egg in &mut self.raptor.eggs {
            if let Some(c) = egg.carrier.and_then(|c| self.players.get(c)) {
                egg.pos = held(c.body.pos, c.body.facing);
            }
        }
        let t = self.tick;
        self.raptor.drops.retain(|d| t < d.2 + DROPPED);
    }

    /// Player `i` lets go of the egg it carries, which stays where it was (the script then resets
    /// or splats it).
    pub(crate) fn let_go_egg(&mut self, i: usize) {
        for egg in &mut self.raptor.eggs {
            if egg.carrier == Some(i) {
                egg.carrier = None;
            }
        }
    }

    /// One of the script's [`Op`]s.
    pub(crate) fn raptor_op(&mut self, op: Op) {
        let t = self.tick;
        match op {
            Op::Egg(k, shown) => {
                if let Some(c) = self.raptor.eggs.get(k).and_then(|e| e.carrier)
                    && let Some(f) = self.players.get_mut(c)
                {
                    f.flag = None;
                }
                if let Some(e) = self.raptor.eggs.get_mut(k) {
                    *e = Egg {
                        shown,
                        carrier: None,
                        pos: if shown { EGGS[k] } else { e.pos },
                    };
                }
            }
            Op::LetGo(i) => {
                if let Some(f) = self.players.get_mut(i).filter(|f| f.flag == Some(EGG)) {
                    f.flag = None;
                }
                self.let_go_egg(i);
            }
            Op::Face(i, deg) => {
                if let Some(f) = self.players.get_mut(i) {
                    let yaw = (u32::from(deg % 360) * 65536 / 360) as u16;
                    (f.body.facing, f.body.move_angle) = (yaw, yaw);
                }
            }
            Op::Throw(i) => {
                self.let_go_egg(i);
                if let Some(f) = self.players.get_mut(i).filter(|f| f.health > 0) {
                    f.flag = None;
                    let (anim, ..) = Pose::EggThrow.clip();
                    f.set(Action::Pose(Pose::EggThrow), anim, t);
                }
            }
            Op::Nest(i) => {
                if let Some(f) = self.players.get_mut(i).filter(|f| f.prey && f.health > 0) {
                    f.prey = false;
                    let (anim, frames, _) = Pose::Nest.clip();
                    f.set(Action::Pose(Pose::Nest), anim, t);
                    f.ready = t + frames;
                    self.raptor.fed = Some(t);
                }
            }
            Op::Cooked(n, [x, z]) => {
                if let Some(c) = self.raptor.cooked.get_mut(n) {
                    *c = Some([x, COOKED_Y, z]);
                }
            }
        }
    }
}

/// Where a raptor's jaw holds its body (I: 150 ahead, 100 up; the pack's joint 6 is the client's).
fn gait_jaw(b: &PlayerState) -> [f32; 3] {
    let (c, s) = conker_sim::movement::cos_sin(b.facing);
    [b.pos[0] + 150.0 * c, b.pos[1] + 100.0, b.pos[2] - 150.0 * s]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Fighter, Settings};
    use crate::gas::Effect;
    use crate::spawn::{Placement, Spawn};
    use conker_sim::movement::BUTTON_A;

    fn flat() -> CollisionWorld {
        let big = 20_000.0;
        CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ])
    }

    /// A raptor (slot 0) facing +x and a caveman (slot 1) at `at`, neither immune.
    fn hunt(at: [f32; 3], score: u8) -> (Match, Arena) {
        let rec = |pos: [i16; 3]| Spawn {
            pos,
            b6: 0,
            yaw: 0,
            camera: [0.0; 3],
            b14: 0,
            b15: 0,
        };
        let arena = Arena {
            spawns: vec![rec([-3000, 0, -3000]), rec([3000, 0, 3000])],
            ..Arena::default()
        };
        let place = |pos: [f32; 3]| Placement {
            index: 0,
            pos,
            facing: 0,
            camera: [0.0; 3],
        };
        let players = vec![
            Fighter {
                immune: 0,
                class: CLASS,
                side: Side::Tediz,
                ..Fighter::new(&place([0.0; 3]), RAPTORS, 0)
            },
            Fighter {
                immune: 0,
                class: CAVEMAN,
                side: Side::Cavemen,
                ..Fighter::new(&place(at), CAVEMEN, 0)
            },
        ];
        let m = Match::new(&arena, players, crate::Rng::default(), 0).with(Settings {
            raptor: true,
            score,
            ..Settings::default()
        });
        (m, arena)
    }

    fn pad(x: i8, y: i8, buttons: u16) -> Input {
        Input {
            stick_x: x,
            stick_y: y,
            buttons,
            camera_yaw: 0xC000,
        }
    }

    /// V r2, r5, r6: Z plays clip 49 for 28 frames; 14 frames in, a caveman 144-300 ahead dies
    /// (clip 0x170), the raptor's team scores 1 and the body is in its jaws; 400 ahead or behind,
    /// nothing. B then drops it (clip 8, 39 frames).
    #[test]
    fn bites_as_r2_r5_r6() {
        let world = flat();
        for (d, dies) in [(144.0, true), (200.0, true), (251.0, true), (300.0, true)]
            .into_iter()
            .chain([(400.0, false), (-100.0, false)])
        {
            let (mut m, arena) = hunt([d, 0.0, 0.0], 0);
            m.step(&arena, &[pad(0, 0, BUTTON_Z), Input::default()], &world);
            assert_eq!(m.players[0].anim, clip::BITE);
            for n in 1..=BITE_AT {
                assert!(m.players[1].health > 0, "{d}: {n}");
                m.step(&arena, &[Input::default(); 2], &world);
            }
            let (f, v) = (&m.players[0], &m.players[1]);
            assert_eq!(v.health == 0, dies, "{d}");
            assert_eq!(
                (f.prey, m.colors.scores),
                (dies, [0, u16::from(dies)]),
                "{d}"
            );
            if !dies {
                continue;
            }
            assert_eq!((v.anim, f.kills), (damage::BITTEN, 1));
            for _ in BITE_AT + 1..28 {
                m.step(&arena, &[Input::default(); 2], &world);
            }
            assert_eq!(m.players[0].anim, clip::BITE);
            m.step(&arena, &[pad(0, 0, BUTTON_B), Input::default()], &world);
            assert_eq!((m.players[0].anim, m.players[0].prey), (clip::DROP, false));
            assert_eq!(m.raptor.drops.len(), 1);
        }
    }

    /// V r8: Z at full speed plays clip 53 for 11 frames, braking 2.5 a frame; 10 frames in, the
    /// caveman ahead takes 3 and is blown up (0xF3, 0xF4, 0xF0). Below full speed Z stands (r7).
    #[test]
    fn running_bite_as_r8() {
        let world = flat();
        let (mut m, arena) = hunt([500.0, 0.0, 0.0], 0);
        for _ in 0..40 {
            m.step(&arena, &[pad(0, 82, 0), Input::default()], &world);
        }
        let start = m.players[0].body.pos[0];
        m.players[1].body.pos[0] = start + 500.0;
        m.step(&arena, &[pad(0, 82, BUTTON_Z), Input::default()], &world);
        assert_eq!(m.players[0].anim, clip::RUN_BITE);
        assert_eq!(m.players[0].body.xz_speed, 32.5);
        for _ in 1..RUN_BITE_AT {
            m.step(&arena, &[pad(0, 82, 0), Input::default()], &world);
            assert_eq!(m.players[1].health, 6);
        }
        m.step(&arena, &[pad(0, 82, 0), Input::default()], &world);
        let v = &m.players[1];
        assert_eq!((v.health, v.anim), (3, conker_anim::combat::BLOWN_UP));
        assert!(!m.players[0].prey);
        let (mut m, arena) = hunt([300.0, 0.0, 0.0], 0);
        for _ in 0..25 {
            m.step(&arena, &[pad(0, 82, 0), Input::default()], &world);
        }
        m.step(&arena, &[pad(0, 82, BUTTON_Z), Input::default()], &world);
        assert_eq!(m.players[0].anim, clip::BITE);
    }

    /// V r7: the club takes 1 a hit (clip 54, 13 frames); the 6th kills: clip 47, 48 from 59
    /// frames on, the respawn at 119 with clip 19 for 28 frames, the body gone.
    #[test]
    fn dies_and_respawns_as_r7() {
        let world = flat();
        let (mut m, arena) = hunt([3000.0, 0.0, 0.0], 0);
        m.players[0].prey = true;
        let mut ev = Vec::new();
        for k in 1..=6u8 {
            assert!(m.hurt(0, &damage::CLUB_SWING, false, false, 0, Some(1), &mut ev));
            assert_eq!(m.players[0].health, 6 - k);
            if k < 6 {
                m.step(&arena, &[Input::default(); 2], &world);
                assert_eq!(m.players[0].anim, clip::HURT_CLUB);
                for _ in 0..14 {
                    m.step(&arena, &[Input::default(); 2], &world);
                }
                assert_eq!(m.players[0].action, Action::Free);
            }
        }
        assert_eq!(m.colors.scores, [1, 0]);
        let mut anims = Vec::new();
        for _ in 0..DOWN_AT {
            m.step(&arena, &[Input::default(); 2], &world);
            anims.push(m.players[0].anim);
        }
        assert_eq!(anims[0], clip::DEATH);
        assert_eq!(anims[usize::from(DEAD_FROM) - 1], clip::DEAD);
        let f = &m.players[0];
        assert_eq!((f.anim, f.health, f.prey), (clip::RESPAWN, 6, false));
        assert_eq!((f.class, f.team), (CLASS, RAPTORS));
    }

    /// V r4, r5, r11: an egg a caveman touches is his (the grab 0xD5), carried at the bag's
    /// speed, A and B doing nothing; the fire's throw lets it go (0xD6). A raptor never takes one,
    /// nor a pickup.
    #[test]
    fn eggs_as_r4() {
        let world = flat();
        let ground = |[x, _, z]: [f32; 3]| [x, 0.0, z];
        let (mut m, arena) = hunt(ground(EGGS[1]), 0);
        m.players[0].body.pos = ground(EGGS[0]);
        m.apply(&[0, 1].map(|k| Effect::Raptor(Op::Egg(k, true))));
        for e in &mut m.raptor.eggs {
            e.pos = ground(e.pos);
        }
        m.step(&arena, &[Input::default(); 2], &world);
        let e = m.raptor.eggs;
        assert_eq!((e[0].carrier, e[1].carrier), (None, Some(1)));
        let f = &m.players[1];
        assert_eq!(
            (f.flag, f.anim),
            (Some(EGG), conker_anim::combat::FLAG_GRAB)
        );
        for _ in 0..80 {
            m.step(&arena, &[Input::default(), pad(0, 82, BUTTON_A)], &world);
            let f = &m.players[1];
            assert!(f.body.xz_speed <= 25.2 + 1e-3 && !f.body.airborne);
        }
        let f = &m.players[1];
        assert_eq!((f.anim, f.flag), (EGG_RUN, Some(EGG)));
        let e = m.raptor.eggs[1];
        assert!((e.pos[1] - f.body.pos[1] - HELD[1]).abs() < 1e-3);
        m.step(&arena, &[Input::default(), pad(0, 0, BUTTON_B)], &world);
        assert_eq!(m.players[1].flag, Some(EGG));
        m.apply(&[Effect::Raptor(Op::Throw(1))]);
        m.step(&arena, &[Input::default(); 2], &world);
        let f = &m.players[1];
        assert_eq!((f.flag, f.anim), (None, 0xD6));
        assert_eq!(m.raptor.eggs[1].carrier, None);
        m.apply(&[Effect::Raptor(Op::Egg(1, false))]);
        assert!(!m.raptor.eggs[1].shown);
    }

    /// V b1: the score target latches on the frame after the kill that reaches it.
    #[test]
    fn a_kill_to_the_target_ends() {
        let world = flat();
        let (mut m, arena) = hunt([200.0, 0.0, 0.0], 1);
        m.step(&arena, &[pad(0, 0, BUTTON_Z), Input::default()], &world);
        for _ in 0..BITE_AT {
            assert_eq!(m.ended_at, None);
            m.step(&arena, &[Input::default(); 2], &world);
        }
        assert_eq!(m.colors.scores, [0, 1]);
        assert_eq!(m.ended_at, Some(m.tick));
        assert_eq!(m.places(), [1, 2]);
    }
}
