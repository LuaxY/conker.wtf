//! War: Raptor (scene 0x3F): the raptor's own movement, hand-written after its behaviour scripts
//! (`[0x0F, 83]`, which drive humans too): the run, the brake, the turn, the jump and the running
//! bite's lunge. `lab/research/mp-original/raptor.md` §2.2; V r1 (a human raptor's run, jump and
//! turn) and r8 (the running bite) unless tagged. The bites' hits, the body and the hurt clips are
//! the match's (conker-rules' `raptor`).

use crate::collision::CollisionWorld;
use crate::movement::{
    BUTTON_A, GROUND_Y_VEL, Input, Other, PlayerState, angle_diff, cos_sin, deadzone, follow_floor,
    push_apart, rise_along, sweep_walls, target_angle,
};

/// The raptor's clips (obj+0x84), which its state (+0x85) mirrors while it moves.
pub mod clip {
    pub const IDLE: u16 = 0;
    /// Braking below [`super::SLOW`] with the stick let go.
    pub const SLOW: u16 = 1;
    pub const RUN: u16 = 10;
    pub const JUMP: u16 = 51;
    /// The standing bite (28 frames) and the running one (11).
    pub const BITE: u16 = 49;
    pub const RUN_BITE: u16 = 53;
    /// Z or B with a body: the drop (39 frames); the nest drop (the script's behaviour 40, 45).
    pub const DROP: u16 = 8;
    pub const NEST: u16 = 50;
    /// Hurt by the club (13 frames), by a bolt or a knife (17), launched by the hazard (33).
    pub const HURT_CLUB: u16 = 54;
    pub const HURT: u16 = 12;
    pub const LAUNCH: u16 = 23;
    /// The death (47, then 48 from 58 frames in) and the respawn's (28 frames).
    pub const DEATH: u16 = 47;
    pub const DEAD: u16 = 48;
    pub const RESPAWN: u16 = 19;
}

/// The full stick's target speed (the deadzoned stick's 75), gained 1 a frame.
pub const TOP: f32 = 35.0;
pub const ACCEL: f32 = 1.0;
/// Let go, the speed brakes 1.8 a frame while that leaves [`SLOW`] or more, then 1.1, to 0 under
/// 1 (I the last).
pub const BRAKE: f32 = 1.8;
pub const BRAKE_SLOW: f32 = 1.1;
pub const SLOW: f32 = 15.0;
/// The facing turns toward the stick at most 1024 a frame up to speed 21, 768 at [`TOP`] (V; I the
/// line between: r1's rates there are the camera's gap, not the cap).
pub const TURN: i32 = 1024;
pub const TURN_TOP: i32 = 768;
const TURN_FROM: f32 = 21.0;
/// The jump: y velocity 70 at the take-off with gravity 5; at 15 it drops to 12 and the gravity
/// to 2; at −12 the gravity is 8 (V r1 121-155, 681-715; the 15 → 12 step is a fit, I its cause).
/// The clip lasts [`JUMP_FRAMES`] from the take-off; in the air the speed keeps on toward the last
/// target and the facing holds; landed, the speed holds [`LANDING`] more frames, then stops until
/// the clip ends.
pub const JUMP_Y_VEL: f32 = 70.0;
pub const JUMP_FRAMES: u16 = 56;
pub const LANDING: u8 = 3;
/// The running bite (clip 53, only at [`TOP`]): the speed drops 2.5 a frame, and the clip moves
/// the raptor half the speed its first two frames, then ~15.4 and ~53.6 a frame (its root, V r8
/// 101-111).
pub const RUN_BITE_BRAKE: f32 = 2.5;
pub const LUNGE: [f32; 11] = [
    0.0, 0.0, 15.4, 15.4, 15.4, 15.4, 15.4, 53.6, 53.6, 53.6, 53.6,
];

/// A raptor's body against the others: radius 124, centre 126 up, vertical scale 124/203 (V: b1's
/// obj+0xE4/0xE8/0xEC; cavemen gather ~145-150 from it).
pub fn body(pos: [f32; 3]) -> Other {
    Other {
        pos,
        radius: 124,
        lift: 126,
        y_scale: f32::from_bits(0x3F1C_5FD8),
    }
}

/// What moves the raptor this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    /// The stick: run, brake, turn; A takes off from the ground.
    Stick,
    /// Held in a clip (a bite, a drop, a hurt, the respawn): no stick, the speed brakes.
    Hold,
    /// Frame `n` of the running bite.
    Lunge(u8),
}

/// The speed's next value toward `target`.
fn speed(v: f32, target: f32) -> f32 {
    if target > v {
        return (v + ACCEL).min(target);
    }
    let next = match v - BRAKE {
        n if n >= SLOW => n,
        _ => v - BRAKE_SLOW,
    };
    if next < 1.0 && target < 1.0 {
        0.0
    } else {
        next.max(target)
    }
}

/// The turn's cap at speed `v`.
fn turn_cap(v: f32) -> i32 {
    let k = ((v - TURN_FROM) / (TOP - TURN_FROM)).clamp(0.0, 1.0);
    TURN - (k * (TURN - TURN_TOP) as f32) as i32
}

/// The y velocity and gravity after a frame in the air (the regimes of [`JUMP_Y_VEL`]).
fn fall(y_vel: f32, gravity: f32) -> (f32, f32) {
    match () {
        _ if gravity == 5.0 && y_vel <= 15.0 => (y_vel - 3.0, 2.0),
        _ if gravity < 8.0 && y_vel <= -12.0 => (y_vel - 8.0, 8.0),
        _ => (y_vel - gravity, gravity),
    }
}

/// One frame of a raptor. Returns whether it took off this frame.
pub fn step(
    p: &mut PlayerState,
    input: &Input,
    world: &CollisionWorld,
    others: &[Other],
    drive: Drive,
) -> bool {
    let old = p.pos;
    let pressed = input.buttons & !p.prev_buttons;
    p.prev_buttons = input.buttons;
    let jumping = p.state == clip::JUMP as u8;
    let mut took_off = false;
    let dist = match drive {
        _ if jumping => {
            p.air_frames += 1;
            if p.airborne {
                p.xz_speed = speed(p.xz_speed, p.target_speed.max(p.xz_speed));
            } else if p.timer > 0 {
                p.timer -= 1;
            } else {
                (p.xz_speed, p.target_speed) = (0.0, 0.0);
            }
            if p.air_frames >= JUMP_FRAMES && !p.airborne {
                p.state = clip::IDLE as u8;
            }
            p.xz_speed
        }
        Drive::Stick => {
            let (x, y) = (deadzone(input.stick_x), deadzone(input.stick_y));
            let mag = (f32::from(x).hypot(f32::from(y))).min(75.0);
            p.target_speed = mag * (TOP / 75.0);
            p.xz_speed = speed(p.xz_speed, p.target_speed);
            if mag > 0.0 {
                let d = angle_diff(target_angle(input, x, y), p.facing);
                let cap = turn_cap(p.xz_speed);
                p.facing = p.facing.wrapping_add(d.clamp(-cap, cap) as u16);
            }
            p.state = match () {
                _ if p.target_speed > 0.0 || p.xz_speed >= SLOW => clip::RUN,
                _ if p.xz_speed > 0.0 => clip::SLOW,
                _ => clip::IDLE,
            } as u8;
            if pressed & BUTTON_A != 0 && !p.airborne {
                (p.state, p.air_frames, p.airborne) = (clip::JUMP as u8, 0, true);
                (p.y_vel, p.gravity, p.floor) = (JUMP_Y_VEL, 5.0, None);
                took_off = true;
            }
            p.xz_speed
        }
        Drive::Hold => {
            p.target_speed = 0.0;
            p.xz_speed = speed(p.xz_speed, 0.0);
            p.xz_speed
        }
        Drive::Lunge(n) => {
            p.target_speed = 0.0;
            p.xz_speed = (p.xz_speed - RUN_BITE_BRAKE).max(0.0);
            match LUNGE.get(usize::from(n)) {
                Some(&d) if d > 0.0 => 2.0 * d,
                _ => p.xz_speed,
            }
        }
    };
    p.move_angle = p.facing;
    let (c, s) = cos_sin(p.move_angle);
    if p.airborne {
        p.pos[0] += 0.5 * dist * c;
        p.pos[2] -= 0.5 * dist * s;
        if !took_off {
            p.pos[1] += 0.5 * p.y_vel - 0.275 * p.gravity;
            (p.y_vel, p.gravity) = fall(p.y_vel, p.gravity);
        }
        push_apart(&mut p.pos, body([0.0; 3]), others);
        p.y_vel = sweep_walls(p, world, old, p.y_vel);
        let floor = world.floor(p.pos[0], p.pos[2], p.pos[1], old[1], None);
        if let Some(f) = floor.filter(|f| p.y_vel < 0.0 && p.pos[1] <= f.y) {
            (p.pos[1], p.floor_y, p.floor) = (f.y, f.y, Some(f));
            (p.airborne, p.y_vel, p.timer) = (false, GROUND_Y_VEL, LANDING);
        }
        return took_off;
    }
    // On the ground: half the speed along the floor's plane (as the tank's).
    let rise = (p.floor).map_or(0.0, |f| rise_along(world.tris[f.tri as usize].normal, c, s));
    let d = 0.5 * dist / (1.0 + rise * rise).sqrt();
    p.pos[0] += d * c;
    p.pos[1] += d * rise;
    p.pos[2] -= d * s;
    let stepped_y = p.pos[1];
    push_apart(&mut p.pos, body([0.0; 3]), others);
    let pre = p.pos;
    let y_vel = sweep_walls(p, world, old, GROUND_Y_VEL);
    let state = p.state;
    follow_floor(p, world, old, stepped_y, pre, y_vel);
    if p.airborne {
        // Off an edge: the fall's gravity (I), the clip kept.
        (p.gravity, p.state) = (8.0, state);
    }
    false
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

    fn input(x: i8, y: i8, buttons: u16) -> Input {
        Input {
            stick_x: x,
            stick_y: y,
            buttons,
            camera_yaw: 0xC000,
            ..Default::default()
        }
    }

    fn frames(p: &mut PlayerState, world: &CollisionWorld, i: Input, drive: Drive, n: usize) {
        for _ in 0..n {
            step(p, &i, world, &[], drive);
        }
    }

    /// V r1 30-119: the full stick gains 1 a frame to 35, moving half the speed; let go, it brakes
    /// 1.8 a frame to 15.2 (state 10), then 1.1 (state 1) to 2.0, then stops.
    #[test]
    fn runs_and_brakes_as_r1() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        frames(&mut p, &world, input(0, 82, 0), Drive::Stick, 1);
        assert_eq!((p.target_speed, p.xz_speed, p.state), (35.0, 1.0, 10));
        assert!((p.pos[0] - 0.5).abs() < 1e-4, "{:?}", p.pos);
        frames(&mut p, &world, input(0, 82, 0), Drive::Stick, 40);
        assert_eq!(p.xz_speed, 35.0);
        let x = p.pos[0];
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 1);
        assert!((p.xz_speed - 33.2).abs() < 1e-4);
        assert!((p.pos[0] - x - 16.6).abs() < 1e-3);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 10);
        assert!((p.xz_speed - 15.2).abs() < 1e-3, "{}", p.xz_speed);
        assert_eq!(p.state, 10);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 1);
        assert!((p.xz_speed - 14.1).abs() < 1e-3);
        assert_eq!(p.state, 1);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 11);
        assert!((p.xz_speed - 2.0).abs() < 1e-3, "{}", p.xz_speed);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 1);
        assert_eq!((p.xz_speed, p.state), (0.0, 0));
    }

    /// V r1 440-500: the stick to the side turns 1024 a frame to speed 21 and 768 at 35.
    #[test]
    fn turns_as_r1() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        let mut turns = Vec::new();
        for _ in 0..40 {
            let f = p.facing;
            // The camera behind the raptor, the stick to the right: always a quarter turn off.
            let i = Input {
                camera_yaw: p.facing.wrapping_sub(0x4000),
                ..input(82, 0, 0)
            };
            step(&mut p, &i, &world, &[], Drive::Stick);
            turns.push(f.wrapping_sub(p.facing));
        }
        assert_eq!(turns[..20], [1024; 20]);
        assert_eq!(turns[36..], [768; 4]);
    }

    /// V r1 120-177 (standing) and 680-737 (at speed): y velocity 70, 65 … 15, then 12, 10 … −12,
    /// −20, −28 … to the floor; the clip 56 frames; in the air the speed keeps on, landed it holds
    /// 4 frames, then 0.
    #[test]
    fn jumps_as_r1() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 1);
        assert!(step(
            &mut p,
            &input(0, 0, BUTTON_A),
            &world,
            &[],
            Drive::Stick
        ));
        assert_eq!(
            (p.y_vel, p.gravity, p.state, p.pos[1]),
            (70.0, 5.0, 51, 0.0)
        );
        let mut ys = Vec::new();
        let mut landed = 0;
        for k in 1..60 {
            step(&mut p, &input(0, 0, 0), &world, &[], Drive::Stick);
            ys.push((p.pos[1], p.y_vel, p.gravity));
            if landed == 0 && !p.airborne {
                landed = k;
            }
        }
        assert!((ys[0].0 - 33.625).abs() < 1e-3, "{:?}", ys[0]);
        assert_eq!((ys[11].1, ys[11].2), (12.0, 2.0), "{:?}", &ys[9..13]);
        assert_eq!((ys[24].1, ys[24].2), (-20.0, 8.0), "{:?}", &ys[22..26]);
        assert!((ys[11].0 - 238.5).abs() < 1e-2, "{:?}", ys[11]);
        assert_eq!(landed, 34);
        // At speed: landed on frame 34 at 35, stopped on 38, the clip's 56 from the take-off.
        let mut p = PlayerState::spawn([0.0; 3], 0);
        frames(&mut p, &world, input(0, 82, 0), Drive::Stick, 10);
        step(&mut p, &input(0, 82, BUTTON_A), &world, &[], Drive::Stick);
        assert_eq!(p.xz_speed, 11.0);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 34);
        assert!(!p.airborne && p.state == 51);
        assert_eq!(p.xz_speed, 35.0);
        frames(&mut p, &world, input(0, 0, 0), Drive::Stick, 4);
        assert_eq!(p.xz_speed, 0.0);
        frames(&mut p, &world, input(0, 82, 0), Drive::Stick, 17);
        assert_eq!((p.state, p.xz_speed), (51, 0.0));
        frames(&mut p, &world, input(0, 82, 0), Drive::Stick, 2);
        assert_eq!((p.state, p.xz_speed), (10, 1.0));
    }

    /// V r8 101-111: the running bite from 35 brakes 2.5 a frame, moving 16.25, 15, then 15.4 five
    /// times and 53.6 four.
    #[test]
    fn lunges_as_r8() {
        let world = flat();
        let mut p = PlayerState::spawn([0.0; 3], 0);
        frames(&mut p, &world, input(0, 82, 0), Drive::Stick, 40);
        let mut moved = Vec::new();
        for n in 0..11 {
            let x = p.pos[0];
            step(&mut p, &input(0, 82, 0), &world, &[], Drive::Lunge(n));
            moved.push(((p.pos[0] - x) * 100.0).round() / 100.0);
        }
        assert_eq!(moved[..3], [16.25, 15.0, 15.4]);
        assert_eq!(moved[7..], [53.6; 4]);
        assert_eq!(p.xz_speed, 7.5);
    }
}
