//! Tank (scene 0x24): the tank's hull and turret, a movement of its own (`func_150ED748`, the tank's
//! update: the target speed and the turn; `func_1505A3A8` the speed; `func_150599C8` the facing's turn;
//! the generic mover's step along the floor). `../lab/research/mp-original/tank.md` §1; V t1, c1, p1, f1.
//! The gun's pitch aims at the others, which is the match's (conker-rules' `tank`).

use crate::collision::CollisionWorld;
use crate::movement::{
    BUTTON_A, BUTTON_C_LEFT, BUTTON_C_RIGHT, GROUND_Y_VEL, Input, Other, PlayerState, cos_sin,
    deadzone, follow_floor, integrate_y, push_apart, rise_along, sweep_walls,
};

/// `0x800A15AC`: the target speed per stick unit (48 at the full stick's 75; V t1).
pub const DRIVE: f32 = 0.64;
/// The speed a frame toward the target: +1.4, 0.014 within 1 of it; letting go of forward brakes 2.0,
/// of reverse 3.4 (both) (V t1 31-114, 521-600). The fire clip's mover takes 5 both ways (the clip's
/// 0x32 / 10: V c1 62-68, 92-94), as a bot's always does ([`Drive::Stick`]'s `rates`).
pub const ACCEL: f32 = 1.4;
pub const BRAKE: f32 = 2.0;
pub const CLIP_RATE: u8 = 0x32;
/// The facing turns at most this a frame (V t1 441-499: 1024 turning in place).
pub const TURN_CAP: i32 = 1024;
/// The turret's turn on C-right (clockwise from above) and C-left, degrees a frame (V t1).
pub const TURRET_RATE: f32 = 3.0;
/// The gun's pitch, degrees (C).
pub const PITCH_MIN: f32 = -20.0;
pub const PITCH_MAX: f32 = 80.0;
/// A tank falls by 6 a frame (V c1 41-48: y velocity −10, −16, −22 …).
pub const GRAVITY: f32 = 6.0;
/// Boosting, A forces the pad's y to 0x50 (C; V p1 372: target 102.4 = 80 · 0.64 · 2).
pub const BOOST_STICK: i8 = 80;
/// A hit's knock-back: speed 50 along the shell, each frame less 0.1, moved half of, then × 0.9 (V f1
/// 44-57: 24.95, then 44.91, 40.33 …).
pub const KNOCK: f32 = 50.0;

/// A tank's body against the others (I: radius 78 fits p1 401, where two tanks 147 apart first push).
pub fn body(pos: [f32; 3]) -> Other {
    Other {
        pos,
        radius: 78,
        lift: 33,
        y_scale: 0.5,
    }
}

/// What moves the hull this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    /// The stick: `boost` with power-up 0x01 (A held: full forward at twice the speed scale), `scope`
    /// R with 0x02 (stopped, no turn), `fired` the fire's frame (the speed holds, V c1 61). `rates`:
    /// the mover's accel and brake ×10 (obj+0x247/+0x248) in place of the stick's [`ACCEL`] and
    /// [`BRAKE`]: the fire clip's [`CLIP_RATE`] both ways, and a bot's every frame: the tank's update
    /// sets 0x32 each before the AI's (C `func_150ED748`), whose class script's op 0x67 sets its own
    /// for that frame (C `func_15077364`; V h4, h5: a bot's first step from rest 0.4 and into a stop
    /// 0.8, script 4's and 5's `67 8a 04 08`, else 5).
    Stick {
        boost: bool,
        scope: bool,
        rates: Option<[u8; 2]>,
        fired: bool,
    },
    /// A hit's knock-back along the move angle, no stick ([`KNOCK`]).
    Knock,
}

/// `func_1505A3A8`: the speed toward `target` by `a` (×0.01 within 1 of it) or down by `d`; against
/// the motion both add up.
pub fn speed(v: f32, target: f32, a: f32, d: f32) -> f32 {
    let (mut a, mut d) = (a, d);
    if target < 0.0 {
        d = if v > 0.0 { d + a } else { a };
    } else if v < 0.0 {
        a += d;
    }
    if target < v {
        (v - d).max(target)
    } else {
        if target - 1.0 < v {
            a *= 0.01;
        }
        (v + a).min(target)
    }
}

/// The facing's turn toward `to`, at most [`TURN_CAP`]; a clockwise gap counts one less
/// (`func_150599C8`; V t1 721-780: 62·v − 1 a frame turning right).
fn turn(facing: u16, to: u16) -> u16 {
    let d = to.wrapping_sub(facing) as i16 as i32;
    let step = if d < 0 { -d - 1 } else { d }.min(TURN_CAP);
    facing.wrapping_add((step * d.signum()) as u16)
}

/// The turn's rate per stick unit at speed `v` with the speed scale `scale`: `max(10, (s16)(18 −
/// v·0.32/scale)·0.5 + 9)` (C; the boost's doubled scale keeps it at twice the speed).
fn rate(v: f32, scale: f32) -> i32 {
    let s = (18.0 - v * (0.32 / scale)) as i32 as i16;
    ((f32::from(s) * 0.5 + 9.0) as i32 as i16).max(10).into()
}

/// One frame of a tank's hull: the drive's speed and turn (from the speed at the frame's start), then
/// half the speed along the facing on the floor's plane, the others' push, the walls and the floor
/// (falling by [`GRAVITY`]); and the turret on C-right/C-left. The move angle is the facing.
pub fn step(
    p: &mut PlayerState,
    input: &Input,
    world: &CollisionWorld,
    others: &[Other],
    drive: Drive,
) {
    let old = p.pos;
    let dist = match drive {
        Drive::Stick {
            boost,
            scope,
            rates,
            fired,
        } => {
            let boosting = boost && input.buttons & BUTTON_A != 0;
            let scale = if boosting { 2.0 * DRIVE } else { DRIVE };
            let y = if boosting {
                BOOST_STICK
            } else {
                deadzone(input.stick_y)
            };
            let x = if scope { 0 } else { deadzone(input.stick_x) };
            let to = (p.facing).wrapping_sub((i32::from(x) * rate(p.xz_speed, scale)) as u16);
            p.target_speed = if scope { 0.0 } else { f32::from(y) * scale };
            let [a, d] = rates.map_or([ACCEL, BRAKE], |r| r.map(|v| f32::from(v) / 10.0));
            p.xz_speed = match () {
                _ if scope => 0.0,
                _ if fired => p.xz_speed,
                _ => speed(p.xz_speed, p.target_speed, a, d),
            };
            p.facing = turn(p.facing, to);
            p.move_angle = p.facing;
            // `turret` is the game's s126+0x1BC negated (counter-clockwise from above, as the
            // facing): C-right adds 3 to 0x1BC, and the shells fly at facing − 0x1BC (V b1).
            if input.buttons & BUTTON_C_RIGHT != 0 {
                p.turret -= TURRET_RATE;
            }
            if input.buttons & BUTTON_C_LEFT != 0 {
                p.turret += TURRET_RATE;
            }
            p.turret = p.turret.rem_euclid(360.0);
            p.xz_speed
        }
        Drive::Knock => {
            let v = (p.xz_speed - 0.1).max(0.0);
            p.xz_speed = 0.9 * v;
            v
        }
    };
    let (c, s) = cos_sin(p.move_angle);
    if p.airborne {
        p.pos[0] += 0.5 * dist * c;
        p.pos[2] -= 0.5 * dist * s;
        p.gravity = GRAVITY;
        integrate_y(p);
        push_apart(&mut p.pos, body([0.0; 3]), others);
        p.y_vel = sweep_walls(p, world, old, p.y_vel);
        let floor = world.floor(p.pos[0], p.pos[2], p.pos[1], old[1], None);
        if let Some(f) = floor.filter(|f| p.y_vel < 0.0 && p.pos[1] <= f.y) {
            (p.pos[1], p.floor_y, p.floor) = (f.y, f.y, Some(f));
            (p.airborne, p.y_vel) = (false, GROUND_Y_VEL);
        }
        return;
    }
    // The step's length along the floor's plane is the speed's half (V t1 55+, c1 89-90).
    let rise = (p.floor).map_or(0.0, |f| rise_along(world.tris[f.tri as usize].normal, c, s));
    let d = 0.5 * dist / (1.0 + rise * rise).sqrt();
    p.pos[0] += d * c;
    p.pos[1] += d * rise;
    p.pos[2] -= d * s;
    let stepped_y = p.pos[1];
    let y_vel = p.y_vel - GRAVITY;
    p.pos[1] += 0.5 * y_vel;
    push_apart(&mut p.pos, body([0.0; 3]), others);
    let pre = p.pos;
    let y_vel = sweep_walls(p, world, old, y_vel);
    follow_floor(p, world, old, stepped_y, pre, y_vel);
    if p.airborne {
        (p.gravity, p.state) = (GRAVITY, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> CollisionWorld {
        let big = 20_000.0;
        CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ])
    }

    fn stick() -> Drive {
        Drive::Stick {
            boost: false,
            scope: false,
            rates: None,
            fired: false,
        }
    }

    fn input(x: i8, y: i8, buttons: u16) -> Input {
        Input {
            stick_x: x,
            stick_y: y,
            buttons,
            camera_yaw: 0,
            ..Default::default()
        }
    }

    /// V t1 30-114: forward (raw 82: target 48) gains 1.4 a frame to 47.6, then 0.014 to 48, moving
    /// half the speed along the facing; let go, it brakes 2.0 a frame to 0.
    #[test]
    fn drives_and_brakes_as_t1() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        tank_frames(&mut p, &world, input(0, 82, 0), 1);
        assert_eq!((p.target_speed, p.xz_speed), (48.0, 1.4));
        assert!((p.pos[0] - 0.7).abs() < 1e-4, "{:?}", p.pos);
        tank_frames(&mut p, &world, input(0, 82, 0), 33);
        assert!((p.xz_speed - 47.6).abs() < 1e-3, "{}", p.xz_speed);
        tank_frames(&mut p, &world, input(0, 82, 0), 1);
        assert!((p.xz_speed - 47.614).abs() < 1e-3, "{}", p.xz_speed);
        tank_frames(&mut p, &world, input(0, 82, 0), 30);
        assert_eq!(p.xz_speed, 48.0);
        let x = p.pos[0];
        tank_frames(&mut p, &world, input(0, 0, 0), 1);
        assert_eq!(p.xz_speed, 46.0);
        assert!((p.pos[0] - x - 23.0).abs() < 1e-3);
        tank_frames(&mut p, &world, input(0, 0, 0), 30);
        assert_eq!(p.xz_speed, 0.0);
    }

    /// V t1 521-600: reverse gains 1.4 a frame down to −48; let go, 3.4 a frame back to −0.4, then
    /// 0.034.
    #[test]
    fn reverses_as_t1() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        tank_frames(&mut p, &world, input(0, -82, 0), 40);
        assert_eq!(p.xz_speed, -48.0);
        assert!(p.pos[0] < 0.0);
        tank_frames(&mut p, &world, input(0, 0, 0), 14);
        assert!((p.xz_speed + 0.4).abs() < 1e-3, "{}", p.xz_speed);
        tank_frames(&mut p, &world, input(0, 0, 0), 1);
        assert!((p.xz_speed + 0.366).abs() < 1e-3, "{}", p.xz_speed);
    }

    /// V t1 441-499 (pad x 75 at rest: −1024 a frame) and 721-780 (stick 69/69, pad 62: 1024 at
    /// speed 0-2.8, 991 at 4.2-7.0, 619 from 29.4: 62·v − 1).
    #[test]
    fn turns_as_t1() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        tank_frames(&mut p, &world, input(82, 0, 0), 1);
        assert_eq!(p.facing, 0u16.wrapping_sub(1024));
        assert_eq!(p.xz_speed, 0.0);
        let mut p = PlayerState::spawn([0.0; 3], 0);
        let mut turns = Vec::new();
        for _ in 0..25 {
            let f = p.facing;
            tank_frames(&mut p, &world, input(69, 69, 0), 1);
            turns.push(f.wrapping_sub(p.facing));
        }
        assert_eq!(turns[..3], [1024; 3]);
        assert_eq!(turns[3..6], [991; 3]);
        assert_eq!(turns[22], 619);
        assert_eq!(p.move_angle, p.facing);
    }

    /// V p1 370-420: boosting, A drives at 102.4 whatever the stick, gaining 1.4 a frame.
    #[test]
    fn boost_doubles_the_target() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        let drive = Drive::Stick {
            boost: true,
            scope: false,
            rates: None,
            fired: false,
        };
        step(&mut p, &input(0, 82, BUTTON_A), &world, &[], drive);
        assert!((p.target_speed - 102.4).abs() < 1e-4);
        assert_eq!(p.xz_speed, 1.4);
    }

    /// V c1 60-68: the fire's frame holds the speed, the clip then gains 5 a frame.
    #[test]
    fn the_fire_clip_kicks() {
        let world = flat();
        let mut p = PlayerState {
            xz_speed: 14.0,
            ..PlayerState::spawn([0.0; 3], 0)
        };
        let clip = |fired| Drive::Stick {
            boost: false,
            scope: false,
            rates: Some([CLIP_RATE; 2]),
            fired,
        };
        step(&mut p, &input(0, 82, 0), &world, &[], clip(true));
        assert_eq!(p.xz_speed, 14.0);
        step(&mut p, &input(0, 82, 0), &world, &[], clip(false));
        assert_eq!(p.xz_speed, 19.0);
    }

    /// V h4 3424-3427, 364: a bot stopping from 10 brakes 0.8 on its stop script's frame (op 0x67's
    /// 8), then 5 (0x32); from rest its path script's frame gains 0.4.
    #[test]
    fn a_bots_mover_takes_its_scripts_rates() {
        let world = flat();
        let mut p = PlayerState {
            xz_speed: 10.0,
            ..PlayerState::spawn([0.0; 3], 0)
        };
        let bot = |rates| Drive::Stick {
            boost: false,
            scope: false,
            rates: Some(rates),
            fired: false,
        };
        let mut speeds = Vec::new();
        for rates in [[4, 8], [CLIP_RATE; 2], [CLIP_RATE; 2]] {
            step(&mut p, &input(0, 0, 0), &world, &[], bot(rates));
            speeds.push(p.xz_speed);
        }
        assert_eq!(speeds, [9.2, 4.2, 0.0]);
        step(&mut p, &input(0, 82, 0), &world, &[], bot([4, 8]));
        assert_eq!(p.xz_speed, 0.4);
    }

    /// V f1 44-57: the knock moves 24.95, then the speed is 44.91.
    #[test]
    fn knock_decays_as_f1() {
        let world = flat();
        let mut p = PlayerState {
            xz_speed: KNOCK,
            ..PlayerState::spawn([0.0; 3], 0)
        };
        step(&mut p, &input(0, 0, 0), &world, &[], Drive::Knock);
        assert!((p.pos[0] - 24.95).abs() < 1e-3);
        assert!((p.xz_speed - 44.91).abs() < 1e-3);
    }

    /// Two tanks push apart; the turret turns 3° a frame, C-right clockwise, wrapping.
    #[test]
    fn tanks_push_and_turrets_turn() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        step(
            &mut p,
            &input(0, 0, BUTTON_C_RIGHT),
            &world,
            &[body([100.0, 0.0, 0.0])],
            stick(),
        );
        assert!(p.pos[0] < 0.0, "{:?}", p.pos);
        assert_eq!(p.turret, 357.0);
        step(&mut p, &input(0, 0, BUTTON_C_LEFT), &world, &[], stick());
        assert_eq!(p.turret, 0.0);
    }

    fn tank_frames(p: &mut PlayerState, world: &CollisionWorld, i: Input, n: usize) {
        for _ in 0..n {
            step(p, &i, world, &[], stick());
        }
    }
}
