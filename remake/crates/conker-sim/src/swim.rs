//! Swimming (War: Beach's sea, scene 0x2B): the player object's water flag obj+0xAD and its frame
//! (`func_1504CB98`'s branch for it, the float and the exit `func_1504B0FC`, the current
//! `func_150611E8`, the strokes and the jump out `func_1505F298`; C, V on the oracle's `sea` and
//! `swim1` runs, `lab/research/mp-original/beach-environment.md` §8 and `tests/sea.rs`).
//!
//! - In: at a frame's end, falling slower than 5 or standing, more than 60 under the water's surface
//!   (obj+0x118, [`CollisionWorld::water`]): half the speed, the move angle onto the facing, y velocity
//!   0.4 of it against gravity −3, state 209.
//! - The float: up to 8 a frame toward the surface, a rise that grows by 0.05 a frame, and above
//!   `water − 46` 0.65 of the excess off: the player settles 41.67 under the surface (state 204).
//! - The current: the drift velocity (obj+0x16C/0x170) goes `speed · 0.0025` of the way to half the
//!   speed of the current's segment along it, and adds to every step; out of the water it fades by
//!   0.15 a frame.
//! - Strokes: the stick drives at 40 % (the speed gains 2 a frame, brakes 1), the move angle turns
//!   at most 4 steps of 256 a frame, the facing follows it; above speed 2 the stroke (state 34).
//! - Out: less than 10 over the floor, the player stands up (speed 15, gravity 4, locked 6 frames,
//!   the wade or the hard landing after a fall); A within 90 of the surface jumps out (y velocity 46,
//!   gravity 3.3 while A is held, the speed 15 braking 0.175 a frame for 12 frames, then 0.875).
//!
//! Not ported (presentation or single player): the swim's pitch (obj+0xB8), breath and the dive
//! (s126+0x21 is set in multiplayer, s126+0x20 bit 1 on Beach: no drowning, B does nothing, V), the
//! entry's splash (`func_15083568`, off in multiplayer: C).

// The MIPS compares (`c.ule.s`, `c.ult.s`, ...) treat NaN their own way; negated comparisons keep that.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use crate::collision::{CollisionWorld, NO_FLOOR};
use crate::movement::{
    BRAKE, Choice, Other, PlayerState, Stick, accelerate_by, air, cos_sin, integrate_y, push_apart,
    root_or_speed, scaled, state, sweep_walls, turn_facing, turn_move,
};
use conker_camera::heading_from_facing;

/// How far under the surface the player starts swimming (`func_1504CB98`, `water − 60`).
pub const DEPTH: f32 = 60.0;
/// The swim's speed percentage (obj+0xAA = 0x28) and its turn, in steps of 256 (s126+0x3C = 4).
const PERCENT: u8 = 40;
const TURN: i32 = 4 * 256;

/// The drift's fade out of the water (`func_15051558`, 0x80099310).
fn fade(p: &mut PlayerState) {
    for v in &mut p.drift {
        *v += (0.0 - *v) * 0.15;
    }
}

/// The step of a frame with the drift (`func_15059C84`, C): `0.5 · dist` along `angle` plus the drift,
/// kept along the floor's plane when `normal` is given (on the ground) with the same horizontal length.
pub(crate) fn drift_step(p: &mut PlayerState, dist: f32, angle: u16, normal: Option<[f32; 3]>) {
    let (c, s) = cos_sin(angle);
    let d = 0.5 * dist;
    let (dx, dz) = (d * c + p.drift[0], -(d * s) + p.drift[1]);
    let Some([nx, ny, nz]) = normal.filter(|n| n[1] != 0.0) else {
        p.pos[0] += dx;
        p.pos[2] += dz;
        return;
    };
    let rise = (-nx * dx - nz * dz) / ny;
    let h2 = dx * dx + dz * dz;
    let full = (h2 + rise * rise).sqrt();
    if full == 0.0 {
        return;
    }
    let h = h2.sqrt();
    p.pos[0] += dx * h / full;
    p.pos[1] += rise * h / full;
    p.pos[2] += dz * h / full;
}

/// The floor and water query at the frame's end (`func_1510E8BC`): the full query unless `prev`, last
/// frame's floor, still holds the point, and never over water (obj+0x118 set); the water height only
/// comes with a full query.
pub(crate) fn query(
    p: &mut PlayerState,
    world: &CollisionWorld,
    y: f32,
    ref_y: f32,
    prev: Option<u32>,
) {
    let prev = prev.filter(|_| p.water == NO_FLOOR);
    if !prev.is_some_and(|t| world.holds(t, p.pos[0], p.pos[2])) {
        p.water = world.water(p.pos[0], p.pos[2]);
    }
    let floor = world.floor(p.pos[0], p.pos[2], y, ref_y, prev);
    p.floor = floor;
    p.floor_y = floor.map_or(NO_FLOOR, |f| f.y);
    if !p.swimming {
        fade(p);
    }
}

/// The check at the end of a ground or air frame (`func_1504CB98`, C; V sea 111, swim1 245 and 287):
/// whether the player went into the water.
pub(crate) fn enter(p: &mut PlayerState) -> bool {
    if p.swimming || p.water == NO_FLOOR || !(p.y_vel < 5.0) || !(p.pos[1] < p.water - DEPTH) {
        return false;
    }
    p.swimming = true;
    (p.airborne, p.jumped, p.water_jump, p.jump_held) = (false, false, false, false);
    p.xz_speed *= 0.5;
    p.move_angle = p.facing;
    p.gravity = -3.0;
    p.y_vel *= f32::from_bits(0x3ECC_CCCD); // 0.4
    (p.state, p.lock) = (state::SWIM_DEEP, 0);
    true
}

/// Out of the water (`func_15052408`): speed 15, gravity 4, the drive locked for 6 frames.
fn out(p: &mut PlayerState) {
    (p.swimming, p.airborne, p.air_frames) = (false, true, 0);
    (p.jumped, p.timer, p.lock) = (false, 0, 6);
    p.gravity = 4.0;
    p.xz_speed = 15.0;
}

/// The wade's clip (24 ticks) at `e / 18 + 0.2` a frame, `e = 0.4 · speed + 0.6 · drive`
/// (`func_1504C0E8`, C), does not loop: its last tick (`func_1507BDB0` → `func_1507F640`) stands the
/// player idle for a frame, the move angle onto the facing and the percentage cleared (V swim1 236, 418,
/// 514, 528: the clock at 23.15 to 23.94, never ending under 22.36).
pub(crate) fn wade(p: &mut PlayerState, drive: f32) {
    if p.state != state::WADE {
        p.wade = 0.0;
        return;
    }
    p.wade += (p.xz_speed * 0.4 + drive * 0.6) / 18.0 + 0.2;
    if p.wade >= 23.0 {
        (p.state, p.wade, p.timer, p.speed_percent) = (state::IDLE, 0.0, 0, 0);
        p.move_angle = p.facing;
        if p.lock >= 0xFA {
            p.lock = 0;
        }
    }
}

/// One frame in the water. `stick` as the ground reads it; `a_pressed`: A went down this frame.
#[allow(clippy::too_many_arguments)]
pub(crate) fn swim(
    p: &mut PlayerState,
    stick: Stick,
    a_pressed: bool,
    a_held: bool,
    world: &CollisionWorld,
    old: [f32; 3],
    me: Other,
    others: &[Other],
) -> Choice {
    // obj+0x28: the height over the floor at the frame's start.
    let height = old[1] - p.floor_y;
    let base = p.water - DEPTH;
    // The float: up to 8 a frame while under 200 above the band's bottom (`water − 230`).
    let below = 2.0 * (p.pos[1] - (base - 170.0));
    if below < 400.0 {
        p.pos[1] += if below + 8.0 > 400.0 {
            400.0 - below
        } else {
            8.0
        };
    }
    let mut current = None;
    if p.water == NO_FLOOR {
        out(p);
    } else {
        if let Some((angle, speed)) = world.current(p.pos[0], p.pos[2]) {
            let f = (speed * f32::from_bits(0x3B23_D70A)).min(1.0); // 0.0025
            let (c, s) = cos_sin(angle);
            let d = 0.5 * speed;
            p.drift[0] += (d * c - p.drift[0]) * f;
            p.drift[1] += (-(d * s) - p.drift[1]) * f;
            // s126+0x68: a fast current's direction, which a jump out against it loses its speed to.
            current = (speed > 20.0).then_some(angle | 1);
        }
        if height < 10.0 && !(p.pos[1] < base + 20.0) {
            out(p);
        } else {
            if (p.y_vel != 0.0 || p.gravity != 0.0) && p.gravity < 0.0 && p.y_vel > 0.0 {
                (p.gravity, p.y_vel) = (0.0, 0.0);
            }
            if base - 175.0 < p.pos[1] {
                if p.rise < 2.5 {
                    p.rise += f32::from_bits(0x3D4C_CCCD); // 0.05
                }
            } else {
                p.rise = 0.0;
            }
            p.pos[1] += p.rise;
            let over = p.pos[1] - (base + 14.0);
            if over > 0.0 {
                p.pos[1] -= over * f32::from_bits(0x3F26_6666); // 0.65
                p.rise = 0.0;
            }
        }
    }
    if !p.swimming {
        // Standing up: the rest of the frame is a fall's, and the gait is chosen in the air (V sea
        // 335-338: the wade, the brake to 12.5).
        let choice = air(p, stick, a_held, world, old, me, others);
        return if p.airborne { Choice::Out } else { choice };
    }

    // The drive at the swim's 40 %, the turn and the speed (`func_1505A3A8`: +2, −1 in water).
    p.speed_percent = PERCENT;
    let locked = p.lock != 0;
    if locked && p.lock < 0xFA {
        p.lock -= 1;
    }
    let drive = if locked || !stick.on { 0.0 } else { scaled(p) };
    if drive != 0.0 {
        p.move_angle = turn_move(p.move_angle, stick.target, TURN);
    }
    p.facing = turn_facing(p.facing, p.move_angle);
    p.xz_speed = accelerate_by(p.xz_speed, drive, 2.0, BRAKE * f32::from_bits(0x3ECC_CCCD));

    // `func_1505F298`: A near the surface jumps out, else the float or a stroke.
    if a_pressed && base - 30.0 < p.pos[1] {
        out(p);
        let to = if stick.on { stick.target } else { p.facing };
        (p.move_angle, p.facing) = (to, to);
        if current.is_some_and(|c: u16| to.wrapping_sub(c).wrapping_add(0x4000) & 0x8000 != 0) {
            (p.xz_speed, p.lock) = (0.0, 0xFF);
        }
        (p.airborne, p.jumped, p.water_jump, p.jump_held) = (true, true, true, true);
        (p.jump_out, p.air_frames, p.state) = (12, 1, state::JUMP);
        (p.y_vel, p.gravity) = (46.0, f32::from_bits(0x4053_3333)); // 3.3
    } else {
        p.state = if p.xz_speed > 2.0 {
            p.timer = 0;
            if base < p.pos[1] {
                state::STROKE
            } else {
                state::STROKE_DEEP
            }
        } else if p.water - p.pos[1] < 70.0 {
            state::SWIM
        } else {
            state::SWIM_DEEP
        };
        if p.holding {
            p.state = state::SWIM_HOLD;
        }
    }
    p.turning = p.facing != p.move_angle;

    // The step with the drift, the height (`func_1505A770`), the bodies, the walls, the floor.
    let (root, dist, angle) = root_or_speed(p);
    if root || !(0.0..1.0).contains(&dist) || p.drift != [0.0; 2] {
        drift_step(p, dist, angle, None);
    }
    if height != 0.0 || p.y_vel > 0.0 {
        integrate_y(p);
    } else {
        p.y_vel -= p.gravity;
        p.pos[1] += p.y_vel * 0.5;
    }
    push_apart(&mut p.pos, me, others);
    p.heading = heading_from_facing(p.facing);
    let pre = p.pos;
    p.y_vel = sweep_walls(p, world, old, p.y_vel);
    p.walled = p.pos[0] != pre[0] || p.pos[2] != pre[2];
    query(p, world, p.pos[1], old[1], None);
    // `func_1505A9AC` (C): the sea floor holds a swimmer that sinks to it (the exit follows from
    // there); more than 20 over it clears obj+0x7F (V swim1 498: the landing after the exit snaps).
    if p.pos[1] < p.floor_y {
        p.pos[1] = p.floor_y;
    }
    p.aloft |= p.pos[1] - p.floor_y > 20.0;
    Choice::None
}
