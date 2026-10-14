//! Race (scene 0x33): the hoverboard's movement (`func_1504CB98`'s board branch, s126+0x4E = 2; the
//! hover spring `func_1504CA60`; the track-angle clamp `func_15087E54`). `../lab/research/mp-original/
//! race.md` §1; V c_fwd, c_back, c_steer, c_a, c_b, w5. The race table (the track, laps, the
//! autopilot's point) and what a wall or the void does to the racer are the match's (conker-rules'
//! `race`).

use crate::collision::{Body, CollisionWorld, Sweep};
use crate::movement::{
    BUTTON_A, Input, PlayerState, angle_diff, atan2_u16, cos_sin, deadzone, rise_along,
};

/// The target speed: 135 + clamp(0.6 · stick y, −35, 45), 100-180 (C; V c_back, c_fwd).
pub const BASE: f32 = 135.0;
pub const STICK: f32 = 0.6;
/// A knocked racer's target is 60 lower (75 at rest) and its steering a third (C; V w5).
pub const KNOCKED: f32 = 60.0;
/// The steering: the move angle turns `stick x · 100 / 9` a frame, 833 at the full stick (V c_steer),
/// halved more than [`HIGH`] above the floor.
pub const STEER: i32 = 100;
pub const HIGH: f32 = 100.0;
/// The move angle stays within this of the track's direction (`func_15087E54`, C; V c_steer 214).
pub const CLAMP: u16 = 0x2400;
/// The hover: gravity [`UP`] above the floor + [`LIFT`], [`DOWN`] (a lift) below; falling, the
/// y velocity is kept above −18 within 30 of the floor, else −80 (C; V c_fwd 24-40).
pub const LIFT: f32 = 20.0;
pub const UP: f32 = 3.8;
pub const DOWN: f32 = -3.5;
/// A's jump (V c_a 241: 76 once the frame's gravity is off) and its landing's bounce (C; V c_a 325).
pub const JUMP: f32 = 76.0;
pub const BOUNCE: f32 = 45.0;
/// A hit (a missile's, a kick's): y velocity 50 under gravity 2, no hover, for [`KNOCK_FRAMES`] (the
/// first counts down in the hit's next frame; state 84 the 29 before the last), 3.5 slower (V w5
/// 234-264).
pub const KNOCK_Y: f32 = 50.0;
pub const KNOCK_GRAVITY: f32 = 2.0;
pub const KNOCK_FRAMES: u8 = 30;
pub const KNOCK_SPEED: f32 = 3.5;
/// A bot's hit also runs its class script 0xF (`func_1505C1E4`; V L1 234, 288, L2 1911, 2949): y
/// velocity 55 (op 6), the speed kept, state 84 [`Board::held`] until script 0xBC sets it riding
/// again (61 frames on, [`Board::class`]), the autopilot turning at most [`KNOCKED_TURN`] meanwhile
/// (V L1 288-337), a kick allowed once the knock's 30 frames are over (V L1 305).
pub const BOT_KNOCK_Y: f32 = 55.0;
pub const KNOCKED_TURN: i32 = 256;
pub const RIDE_AGAIN: f32 = 75.0;
/// B's kick: state 84 for 29 frames, the hop (y velocity 25) on the 12th (V c_b 241-269, 252); the
/// match tests its victim on the 10th (V w2 226).
pub const KICK_FRAMES: u8 = 29;
pub const KICK_HIT: u8 = 10;
pub const KICK_HOP: u8 = 12;
pub const HOP: f32 = 25.0;
/// A canister's boost holds the speed at 254 (V w3).
pub const BOOST: f32 = 254.0;
/// An autopilot racer's canister speed (V L1, L2: the bots at 229 whatever their target; I: why).
pub const BOOST_AUTO: f32 = 229.0;
/// A wall above speed 30 stuns for 12 frames (no steering, target 0) and takes 30% of the speed; a
/// wall's damage waits 20 frames for the next (C).
pub const STUN: u8 = 12;
pub const STUN_ABOVE: f32 = 30.0;
pub const STUN_SPEED: f32 = 0.7;
pub const WALL_COOLDOWN: u8 = 20;
/// Riding (state 5) and kicking or knocked up (84) (V).
pub const RIDE: u8 = 5;
pub const KICK: u8 = 84;
/// B (conker-rules' `BUTTON_B`).
pub const BUTTON_B: u16 = 0x4000;
/// The autopilot turns at most the full stick's rate (I: the game's bots steer with the AI's
/// stick).
pub const AUTO_TURN: i32 = 833;

/// The board's own counters (s126+0x50 knock, the kick's frame, obj+0x81 jumped, s126+0x44 the
/// wall's cooldown) and a bot's class script 0xF (`held`, obj+0x232).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub struct Board {
    pub knock: u8,
    pub kick: u8,
    pub jumped: bool,
    pub wall: u8,
    pub held: bool,
}

impl Board {
    /// Knocked up: a missile's hit or a kick's (after the frame's movement, V w5 234).
    pub fn knock(&mut self, p: &mut PlayerState) {
        self.knock = KNOCK_FRAMES;
        (p.y_vel, p.gravity) = (KNOCK_Y, KNOCK_GRAVITY);
        p.xz_speed -= KNOCK_SPEED;
    }

    /// A bot's hit: the knock at [`BOT_KNOCK_Y`], held.
    pub fn bot_knock(&mut self, p: &mut PlayerState) {
        (self.knock, self.held) = (KNOCK_FRAMES, true);
        (p.y_vel, p.gravity) = (BOT_KNOCK_Y, KNOCK_GRAVITY);
    }

    /// Script 0xBC's command 0x14 (the class script): 0xD rides again at most [`RIDE_AGAIN`] (V L1
    /// 348), 0x1F (its kick's, V L1 305) ends the hold too.
    pub fn class(&mut self, p: &mut PlayerState, script: u8) {
        if self.held && script != 0xF {
            self.held = false;
            if script == 0xD {
                p.xz_speed = p.xz_speed.min(RIDE_AGAIN);
            }
        }
    }

    /// State 5 or 84.
    pub fn state(&self) -> u8 {
        if self.kick > 0 || self.knock > 0 || self.held {
            KICK
        } else {
            RIDE
        }
    }
}

/// What drives the board this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Drive {
    /// After go (before it the board hovers on the line).
    pub go: bool,
    /// The autopilot (bots, finished humans): its target speed and the point it steers at; `None`,
    /// the stick.
    pub auto: Option<(f32, [f32; 3])>,
    /// The track's direction here, for [`CLAMP`].
    pub track: Option<u16>,
    /// A canister's boost runs.
    pub boost: bool,
}

/// What the frame did that the match rules on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    /// A wall hit past the cooldown: `true` head-on (`0x5E01 ≤ move − push < 0xA200`).
    pub wall: Option<bool>,
    /// No floor under the racer.
    pub void: bool,
    /// The kick's 10th frame: the match looks for its victim.
    pub kick: bool,
}

/// The speed toward `target` by 1 a frame (I: the decrease).
fn toward(v: f32, target: f32) -> f32 {
    if v < target {
        (v + 1.0).min(target)
    } else {
        (v - 1.0).max(target)
    }
}

/// The autopilot's speed toward `target`: 1 a frame under 100, 2 from there, and down at most 25
/// (V L7e: 0 → 94 at the start by 1, 100 → 200 in 50 frames, 229 → 100 in 5 after a canister,
/// 182 → 100 in 4; ponytail: the rule read off the rows, the code's untraced).
fn toward_auto(v: f32, target: f32) -> f32 {
    match v < target {
        true => (v + if v < 100.0 { 1.0 } else { 2.0 }).min(target),
        false => (v - 25.0).max(target),
    }
}

/// Keeps `angle` within [`CLAMP`] of `track` (`func_15087E54`).
pub fn clamp(angle: u16, track: u16) -> u16 {
    let d = track.wrapping_sub(angle);
    if d & 0x8000 != 0 {
        if d < 0xDBFF {
            return track.wrapping_add(CLAMP);
        }
    } else if d > CLAMP {
        return track.wrapping_sub(CLAMP);
    }
    angle
}

/// The deflection off a wall that pushed toward `away` (`func_1504CB98` at m2c 1530): pushes from
/// within ±0x1E00 of behind leave the angle.
fn deflect(move_angle: u16, away: u16) -> u16 {
    let d = move_angle.wrapping_sub(away);
    if !(0x1E01..0xE200).contains(&d) {
        return move_angle;
    }
    let t = d.wrapping_sub(0x8000);
    let v = match t & 0x8000 != 0 {
        true => (t >> 2).wrapping_add(0x9800),
        false => (t >> 2) + 0x2800,
    };
    away.wrapping_sub(v)
}

/// One frame of a racer's board.
pub fn step(
    p: &mut PlayerState,
    b: &mut Board,
    input: &Input,
    world: &CollisionWorld,
    drive: Drive,
) -> Moved {
    let mut out = Moved::default();
    let old = p.pos;
    let pressed = input.buttons & !p.prev_buttons;
    p.prev_buttons = input.buttons;
    let height = p.pos[1] - p.floor_y;
    // The speed and the turn.
    let (x, y) = (deadzone(input.stick_x), deadzone(input.stick_y));
    let mut lean = (f32::from(y) * STICK).clamp(-35.0, 45.0);
    let mut steer = i32::from(x) * STEER;
    let turn = if b.held { KNOCKED_TURN } else { AUTO_TURN };
    if b.knock > 0 {
        b.knock -= 1;
        steer /= 3;
        lean -= KNOCKED;
    } else if height > HIGH {
        steer /= 2;
    }
    let mut target = BASE + lean;
    match drive.auto {
        _ if !drive.go => target = 0.0,
        Some((speed, at)) => {
            target = speed;
            let to = atan2_u16(p.pos[2] - at[2], at[0] - p.pos[0]);
            let d = angle_diff(to, p.move_angle).clamp(-turn, turn);
            p.move_angle = p.move_angle.wrapping_add(d as u16);
        }
        None if p.lock == 0 => p.move_angle = p.move_angle.wrapping_sub((steer / 9) as u16),
        None => target = 0.0,
    }
    p.lock = p.lock.saturating_sub(1);
    if let Some(t) = drive.track.filter(|_| drive.go) {
        p.move_angle = clamp(p.move_angle, t);
    }
    p.facing = p.move_angle;
    p.target_speed = target;
    p.xz_speed = match () {
        _ if !drive.go => 0.0,
        _ if drive.boost && drive.auto.is_some() => BOOST_AUTO,
        _ if drive.boost => BOOST,
        _ if drive.auto.is_some() => toward_auto(p.xz_speed, target),
        _ => toward(p.xz_speed, target),
    };
    // Touching the floor, the step follows its plane, as on foot (V L1 5228-5235: 35 a frame up a
    // 45° slope at speed 100, 50 once hovering again).
    let (c, s) = cos_sin(p.move_angle);
    let rise = match p.floor {
        Some(f) if p.pos[1] <= p.floor_y => rise_along(world.tris[f.tri as usize].normal, c, s),
        _ => 0.0,
    };
    let d = 0.5 * p.xz_speed / (1.0 + rise * rise).sqrt();
    p.pos[0] += d * c;
    p.pos[1] += d * rise;
    p.pos[2] -= d * s;
    // Up and down: A's jump, the frame's gravity (the autopilot's lifts over the gaps are the race
    // table's).
    if drive.go && pressed & BUTTON_A != 0 && !b.jumped && height <= HIGH {
        (p.y_vel, p.gravity, b.jumped) = (JUMP + UP, UP, true);
    }
    p.pos[1] += 0.5 * p.y_vel - 0.275 * p.gravity;
    p.y_vel -= p.gravity;
    // The walls: a deflection, a stun, and past the cooldown a hit for the match.
    let pre = p.pos;
    // The wall routine with the board's long step (`D_800CBDD4`, no 60-unit cap; C), not inside the
    // camera volumes (as on foot).
    if !world.camera.as_ref().is_some_and(|c| c.in_volume(old)) {
        let long = Sweep {
            long_step: true,
            ..Sweep::default()
        };
        let swept = world.sweep(old, p.pos, &Body::player(old[1], p.pos[1]), &long, p.y_vel);
        (p.pos, p.y_vel) = (swept.pos, swept.y_vel);
    }
    b.wall = b.wall.saturating_sub(1);
    // obj+0xF4 bit 0x10000, which script 0xBC reads to blow up a bot off trigger 0xF (query 0xC3).
    p.walled = (p.pos[0], p.pos[2]) != (pre[0], pre[2]);
    if p.walled {
        let away = atan2_u16(pre[2] - p.pos[2], p.pos[0] - pre[0]);
        let d = p.move_angle.wrapping_sub(away);
        if (0x1E01..0xE200).contains(&d) {
            p.move_angle = deflect(p.move_angle, away);
            p.facing = p.move_angle;
            if b.wall == 0 {
                out.wall = Some((0x5E01..0xA200).contains(&d));
                b.wall = WALL_COOLDOWN;
            }
            if p.xz_speed > STUN_ABOVE {
                p.lock = STUN;
                p.xz_speed *= STUN_SPEED;
            }
        }
    }
    let Some(f) = world.floor(p.pos[0], p.pos[2], p.pos[1], old[1], None) else {
        out.void = true;
        return out;
    };
    (p.floor, p.floor_y, p.airborne) = (Some(f), f.y, false);
    if p.pos[1] < f.y {
        p.pos[1] = f.y;
        // I: the oracle's rows show −6 on the landing frame (c_fwd 209).
        p.y_vel = p.y_vel.max(-6.0);
    }
    // The hover spring (not while knocked up).
    // The cap and the landing go by the height at the frame's start, the spring's pull by the
    // y now (V c_fwd 206-207: −19.5 kept 30.8 up, −23.3 held at −18 from 21.9).
    if b.knock == 0 {
        let (mut cap, mut rest) = (if height > 30.0 { 80.0 } else { 18.0 }, f.y + LIFT);
        if b.jumped {
            (cap, rest) = (80.0, rest - 30.0);
            if height < 5.0 && p.y_vel < 0.0 {
                (b.jumped, p.y_vel) = (false, BOUNCE);
            }
        }
        p.gravity = if p.pos[1] > rest { UP } else { DOWN };
        p.y_vel = p.y_vel.clamp(-cap, 80.0);
    }
    // B's kick (not while one runs or knocked up).
    if b.kick > 0 {
        b.kick += 1;
        if b.kick == KICK_HOP {
            p.y_vel = HOP;
        }
        if b.kick > KICK_FRAMES {
            b.kick = 0;
        }
    } else if drive.go && pressed & BUTTON_B != 0 && b.knock == 0 {
        b.kick = 1;
    }
    out.kick = b.kick == KICK_HIT;
    p.state = b.state();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::CollisionWorld;

    const FLOOR: f32 = 618.378_9;

    fn flat() -> CollisionWorld {
        let (big, y) = (20_000.0, FLOOR);
        CollisionWorld::new([
            ([[-big, y, -big], [-big, y, big], [big, y, big]], 5),
            ([[-big, y, -big], [big, y, big], [big, y, -big]], 5),
        ])
    }

    fn racer() -> PlayerState {
        let mut p = PlayerState::spawn([0.0, FLOOR, 0.0], 6144);
        (p.floor_y, p.gravity) = (FLOOR, DOWN);
        p
    }

    fn input(x: i8, y: i8, buttons: u16) -> Input {
        Input {
            stick_x: x,
            stick_y: y,
            buttons,
            camera_yaw: 0,
        }
    }

    fn go() -> Drive {
        Drive {
            go: true,
            ..Drive::default()
        }
    }

    /// V c_fwd 204-215, c_back, c_a: the speed gains 1 a frame toward 180 (full forward), 135 idle,
    /// 100 full back; half of it moves along the move angle.
    #[test]
    fn speeds_as_c_fwd_and_c_back() {
        let world = flat();
        for (y, target) in [(82, 180.0), (0, 135.0), (-82, 100.0)] {
            let (mut p, mut b) = (racer(), Board::default());
            step(&mut p, &mut b, &input(0, y, 0), &world, go());
            assert_eq!((p.target_speed, p.xz_speed), (target, 1.0));
            let (c, _) = cos_sin(6144);
            assert!((p.pos[0] - 0.5 * c).abs() < 1e-4, "{:?}", p.pos);
            for _ in 0..250 {
                step(&mut p, &mut b, &input(0, y, 0), &world, go());
            }
            assert_eq!(p.xz_speed, target);
        }
        // Before go nothing moves but the hover.
        let (mut p, mut b) = (racer(), Board::default());
        step(&mut p, &mut b, &input(0, 82, 0), &world, Drive::default());
        assert_eq!((p.xz_speed, p.pos[0]), (0.0, 0.0));
    }

    /// V c_fwd 24-40: from the floor the spring lifts (−3.5) to 20 above it, then pulls (3.8): the
    /// rows' y velocities −2.5, 1, 4.5 … 18.5, 14.7 … and the peak 665.7; falling back, −23.3 is
    /// held at −18 within 30 of the floor (V 207).
    #[test]
    fn hovers_as_c_fwd() {
        let world = flat();
        let (mut p, mut b) = (racer(), Board::default());
        p.y_vel = -6.0;
        let mut vels = Vec::new();
        let mut peak = 0.0f32;
        for _ in 0..20 {
            step(&mut p, &mut b, &input(0, 0, 0), &world, Drive::default());
            vels.push(p.y_vel);
            peak = peak.max(p.pos[1]);
        }
        assert!(
            (vels[0] + 2.5).abs() < 1e-3 && (vels[1] - 1.0).abs() < 1e-3,
            "{vels:?}"
        );
        assert!((vels[7] - 14.7).abs() < 1e-3, "{vels:?}");
        assert!((peak - 665.7).abs() < 1.0, "{peak}");
        assert!(vels.contains(&-18.0), "{vels:?}");
    }

    /// V c_steer 204-214: full stick turns 833 a frame until 0x2400 off the track's 6409 (62729).
    #[test]
    fn steers_and_clamps_as_c_steer() {
        let world = flat();
        let (mut p, mut b) = (racer(), Board::default());
        let drive = Drive {
            track: Some(6409),
            ..go()
        };
        step(&mut p, &mut b, &input(82, 0, 0), &world, drive);
        assert_eq!(p.move_angle, 6144 - 833);
        for _ in 0..12 {
            step(&mut p, &mut b, &input(82, 0, 0), &world, drive);
        }
        assert_eq!(p.move_angle, 62729);
    }

    /// V c_a 241: A jumps (76 once the frame's gravity is off, 79.8 moved); held, it does not jump
    /// again; landing bounces at 45.
    #[test]
    fn jumps_as_c_a() {
        let world = flat();
        let (mut p, mut b) = (racer(), Board::default());
        p.gravity = UP;
        let y = p.pos[1];
        step(&mut p, &mut b, &input(0, 0, BUTTON_A), &world, go());
        assert!((p.y_vel - 76.0).abs() < 1e-4, "{}", p.y_vel);
        assert!((p.pos[1] - y - (0.5 * 79.8 - 0.275 * 3.8)).abs() < 1e-3);
        let mut bounced = false;
        for _ in 0..60 {
            step(&mut p, &mut b, &input(0, 0, BUTTON_A), &world, go());
            bounced |= p.y_vel == BOUNCE;
            assert!(p.y_vel >= -80.0);
        }
        assert!(bounced && !b.jumped);
    }

    /// V c_b 241-269: B kicks for 29 frames (state 84), the hop 25 on the 12th, the hit's test on
    /// the 10th; the speed goes on.
    #[test]
    fn kicks_as_c_b() {
        let world = flat();
        let (mut p, mut b) = (racer(), Board::default());
        let mut states = Vec::new();
        let mut hit = None;
        for k in 1..=32 {
            let buttons = if k <= 2 { BUTTON_B } else { 0 };
            let m = step(&mut p, &mut b, &input(0, 0, buttons), &world, go());
            states.push(p.state);
            if m.kick {
                hit = Some(k);
            }
            if k == KICK_HOP {
                assert_eq!(p.y_vel, HOP);
            }
        }
        assert_eq!(states.iter().filter(|&&s| s == KICK).count(), 29);
        assert_eq!((states[0], states[29]), (KICK, RIDE));
        assert_eq!(hit, Some(10));
        assert_eq!(p.xz_speed, 32.0);
    }

    /// V w5 234-265: knocked up, 50 under gravity 2 with no hover for 29 frames (the rise about
    /// 310), 3.5 slower, then a target of 75 until the counter runs out.
    #[test]
    fn knocked_up_as_w5() {
        let world = flat();
        let (mut p, mut b) = (racer(), Board::default());
        p.xz_speed = 31.0;
        b.knock(&mut p);
        let y = p.pos[1];
        let mut states = Vec::new();
        for _ in 0..30 {
            step(&mut p, &mut b, &input(0, 0, 0), &world, go());
            states.push((p.state, p.gravity, p.target_speed));
        }
        assert_eq!(states[0], (KICK, KNOCK_GRAVITY, 75.0));
        assert_eq!(states[28].0, KICK);
        assert_eq!(states[29], (RIDE, UP, 75.0));
        assert!((p.xz_speed - 57.5).abs() < 1e-3, "{}", p.xz_speed);
        assert!(p.pos[1] - y > 290.0, "{}", p.pos[1] - y);
    }

    /// V L1 288-348, L2 1911: a bot's hit rises at 55 with the hover off for the knock's 30 frames,
    /// is held in state 84 (turning at most 256) until the script's class 0xD, then rides at most 75.
    #[test]
    fn bot_knocked_up_as_l1() {
        let world = flat();
        let (mut p, mut b) = (racer(), Board::default());
        p.xz_speed = 100.0;
        b.bot_knock(&mut p);
        let drive = Drive {
            auto: Some((100.0, [-1000.0, FLOOR, 0.0])),
            ..go()
        };
        let angle = p.move_angle;
        step(&mut p, &mut b, &input(0, 0, 0), &world, drive);
        assert_eq!((p.y_vel, p.state), (BOT_KNOCK_Y - KNOCK_GRAVITY, KICK));
        assert_eq!(angle_diff(p.move_angle, angle).abs(), KNOCKED_TURN);
        for _ in 0..40 {
            step(&mut p, &mut b, &input(0, 0, 0), &world, drive);
        }
        assert_eq!(
            (b.knock, p.state, p.gravity == KNOCK_GRAVITY),
            (0, KICK, false)
        );
        b.class(&mut p, 0xD);
        step(&mut p, &mut b, &input(0, 0, 0), &world, drive);
        assert_eq!((p.state, p.xz_speed), (RIDE, RIDE_AGAIN + 1.0));
    }

    /// A wall: deflected, stunned above 30 (×0.7, 12 frames without the stick); the next damage
    /// waits 20 frames; no floor is the void.
    #[test]
    fn walls_and_the_void() {
        assert_eq!(clamp(0, 0x4000), 0x4000 - CLAMP);
        assert_eq!(clamp(0x8000, 0x4000), 0x4000 + CLAMP);
        assert_eq!(deflect(0x8000, 0), 0u16.wrapping_sub(0x2800));
        assert_eq!(deflect(0x4000, 0), 0x3800);
        assert_eq!(deflect(0, 0), 0);
        let mut p = racer();
        p.pos = [1e6, FLOOR, 0.0];
        let m = step(
            &mut p,
            &mut Board::default(),
            &input(0, 0, 0),
            &flat(),
            go(),
        );
        assert!(m.void);
    }
}
