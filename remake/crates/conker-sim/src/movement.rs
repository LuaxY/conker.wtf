//! Conker's movement in the original game's units (1 unit = 1 collision unit, 30 Hz frames, u16 angles where
//! 65536 = 360° and angle `a` points along `(cos a, -sin a)` in XZ).
//!
//! Rules re-derived from the oracle traces (`lab/oracle`, README "Player-object fields and movement rules",
//! plus the `move_*` scenarios) and, for the floor, from the game's code (see [`crate::collision`]).
//! Tags: **V** reproduced on traces, **I** inferred or fitted (thresholds the traces only bracket).
//! `tests/oracle.rs` replays 28 traces with the trace's camera and with the camera in the loop: every row
//! within 0.001 units (the jump arc's 0.0008), no state difference, outside the frames it lists as not
//! simulated here (the grenade's push on its thrower as it appears, a hit). Walls are the game's own
//! routine ([`CollisionWorld::sweep`]), off inside the level's zones ([`walls_on`]). The match rules
//! feed what they decide through [`PlayerState`]'s `armed`, `lock`, `timer`, `aiming` and `scope_yaw`,
//! the other bodies through [`step_among`], and the clip played through [`animate`] (root motion).
//!
//! One ground frame, in the game's order (`func_1504CB98`): stick (deadzone, target speed and camera-relative
//! direction) → turn, accelerate or brake, pick the state → step along the floor plane → keep falling
//! (`y_vel −= 5; y += y_vel/2`) → the wall routine from last frame's position → floor query at the pushed
//! position and snap (or start falling) → a push against the motion gives state 122. Airborne: steer → step →
//! `y += y_vel/2 − 0.275·g; y_vel −= g` → walls → land on the floor.
//!
//! Math is limited to `+ - * / sqrt` and integer ops so native and wasm agree bit for bit.

use crate::collision::{Body, CollisionWorld, Floor, Sweep};
use crate::root::Root;
use crate::trig::AcosTable;
use conker_camera::heading_from_facing;

/// N64 button bits (`OSContPad.button`): A jumps, Z crouches, C-left and C-right strafe.
pub const BUTTON_A: u16 = 0x8000;
pub const BUTTON_Z: u16 = 0x2000;
pub const BUTTON_C_LEFT: u16 = 0x0002;
pub const BUTTON_C_RIGHT: u16 = 0x0001;
/// C-up, C-down, C-left, C-right (0x0008, 0x0004, 0x0002, 0x0001).
pub const C_BUTTONS: u16 = 0x000F;
/// [`PlayerState::armed`] while a grenade is in hand (s126+0x78 = 0x26 from the pull to the throw).
pub const GRENADE: u8 = 0x26;

/// Movement states (object byte +0x85, also the animation id).
pub mod state {
    pub const CREEP: u8 = 0;
    pub const WALK: u8 = 1;
    pub const RUN: u8 = 2;
    pub const IDLE: u8 = 15;
    pub const CROUCH: u8 = 22;
    pub const CROUCH_WALK: u8 = 27;
    pub const SKID: u8 = 33;
    pub const JUMP: u8 = 50;
    pub const LAND: u8 = 54;
    pub const RUN_JUMP: u8 = 55;
    pub const FALL: u8 = 56;
    pub const PUSH_WALL: u8 = 122;
    /// The run while scared ([`super::PlayerState::scared`]).
    pub const SCARED_RUN: u8 = 43;
    /// A with Z held ([`super::HIGH_JUMP_Y_VEL`]).
    pub const HIGH_JUMP: u8 = 235;
    /// Walking in water shallower than the player (obj+0x118 above it, `func_1504C0E8`: speed 75 %).
    pub const WADE: u8 = 139;
    /// In the water ([`super::PlayerState::swimming`], [`crate::swim`]): floating at the surface,
    /// deeper (on the way in), the stroke at the surface and deeper, holding an object.
    pub const SWIM: u8 = 204;
    pub const SWIM_DEEP: u8 = 209;
    pub const STROKE: u8 = 34;
    pub const STROKE_DEEP: u8 = 206;
    pub const SWIM_HOLD: u8 = 203;
}

/// Top running speed and the target speed per game stick unit (V).
pub const RUN_SPEED: f32 = 35.0;
pub const STICK_SPEED: f32 = 0.6;
/// Deceleration per frame without stick (V).
pub const BRAKE: f32 = 2.5;
/// Gravity while A is held after a take-off, after release, and when falling or grounded (V).
pub const GRAVITY_HELD: f32 = 4.0;
pub const GRAVITY_JUMP: f32 = 6.2;
pub const GRAVITY_FALL: f32 = 5.0;
/// y velocity stored while grounded (V).
pub const GROUND_Y_VEL: f32 = -4.0;
/// Take-off: the first airborne step acts as if y_vel were this, and stores it minus gravity (V, the stored
/// 54.9999847 is not 55).
pub const JUMP_Y_VEL: f32 = 58.999_985;
/// Speed while pushing a wall (V).
pub const WALL_SPEED: f32 = 14.0;
/// The ground follows a floor at most this far below (C: func_1504CB98's 15.0 at 0x1504FE00, 30 when the
/// position is below the floor).
pub const SNAP_DOWN: f32 = 15.0;
/// A stop skids when the speed after this frame's brake is above this with no drive left (C:
/// func_1504C0E8's 18.0; V: 20.4 walks to a stop, 22 skids).
pub const SKID_SPEED: f32 = 18.0;
/// Frames of the skid (V, the +0x83 countdown); the first 8 are locked ([`PlayerState::lock`], C).
pub const SKID_FRAMES: u8 = 17;
/// Landing holds (func_1505A9AC, C; V): a soft landing (from a jump, with no stick) holds 16 frames, shown
/// as 15 rows; a landing after walking off an edge holds 13 (12 rows) with the first 5 locked.
pub const LANDING_FRAMES: u8 = 15;
pub const HARD_LANDING_FRAMES: u8 = 12;
/// Kept for API stability; the game's hard landing is the fall that began by walking off an edge, whatever
/// the speed (C: obj+0x100 bit 0x10, set with state 56).
pub const HARD_LANDING_Y_VEL: f32 = -80.0;
/// Crouching (Z): the crouch-walk speed limit (V: the f32 below 14, 0x415FFFFF, which is 35 at 40 percent)
/// and the slide's deceleration (V; C: 3.2 at 0x80099258); a slide becomes a crouch-walk below
/// `CROUCH_WALK_BELOW` (C: func_1504C0E8's 3.0; V: 2.9999969 walks, 6.2 slides).
pub const CROUCH_SPEED: f32 = f32::from_bits(0x415F_FFFF);
pub const CROUCH_BRAKE: f32 = 3.2;
pub const CROUCH_WALK_BELOW: f32 = 3.0;
/// The crouch-walk's speed percentage (C: func_1504C0E8 sets obj+0xAA = 40).
pub const CROUCH_PERCENT: u8 = 40;
/// Stick reversal (C: func_1504CB98 at 0x1504E9C4): a stick `REVERSE_STEPS` or more 256-steps against the
/// motion brakes 2 more a frame (with the stop's 2.5: `REVERSE_BRAKE`) from speed `REVERSE_RESTART` up,
/// and above `REVERSE_MIN` below that restarts at speed 0 with a 12-step turn.
pub const REVERSE_STEPS: u8 = 0x51;
pub const REVERSE_ANGLE: i32 = 0x5100;
pub const REVERSE_BRAKE: f32 = 4.5;
pub const REVERSE_RESTART: f32 = 7.0;
pub const REVERSE_MIN: f32 = 3.0;
/// Air control stops once falling faster than 10 (C: 0x1504E96C, [`turn_steps`]).
pub const AIR_TURN_MIN_Y_VEL: f32 = -10.0;
/// Frames of the fall clip after walking off an edge (V: ledge rows 409-423).
pub const FALL_FRAMES: u16 = 15;
/// Strafing (C-left/C-right): the target speed reads 34 and the speed aims at 65 percent of it, f32
/// 22.0999985 (C: obj+0xAA = 0x41; V: camera_buttons 173-203).
pub const STRAFE_TARGET: f32 = 34.0;
pub const STRAFE_SHARE: f32 = 0.65;
pub const STRAFE_PERCENT: u8 = 65;
/// The upper body's twist limit ([`PlayerState::twist`], C: `func_15063A38`'s ±0x2328).
pub const TWIST_MAX: i16 = 9000;
/// The drive while looking through the scope (C: `0x8009920C` at 0x1504D4C4; V: obj+0x44 = 0.1).
pub const SCOPE_DRIVE: f32 = 0.1;
/// The high jump, state 235 (`func_1505F298`'s A with Z held, C; V: a human from a crouch, `q2za`, and
/// a bot pushing a wall, `q2s5w`, in `../lab/research/mp-original/bots-fidelity-scratch`): the press
/// frame is a plain ground frame, then the state holds (obj+0x83 = 0x64) and the stick is locked
/// (obj+0x89 = 0x17), so the speed brakes 2.5 a frame while the walls still slide the player; the 10th
/// frame launches at y velocity 76 with gravity 2, 9 from the 4th to the 7th frame after it; no drive
/// while it still rises faster than 6, then the stick's; it lands hard (54 for 12 frames). +356 from a
/// standing start.
pub const HIGH_JUMP_Y_VEL: f32 = 76.0;
const HIGH_JUMP_HOLD: u8 = 0x64;
const HIGH_JUMP_LOCK: u8 = 0x17;
const HIGH_JUMP_WAIT: u8 = 9;

/// The high jump's gravity for its airborne frame `n` (0 the launch's).
fn high_jump_gravity(n: u16) -> f32 {
    if (3..=6).contains(&n) { 9.0 } else { 2.0 }
}

/// One frame of input. The stick is raw (before the game's deadzone).
///
/// `camera_yaw` is the angle the camera-relative stick adds to its own direction: the camera's
/// [`stick_yaw`](conker_camera::CameraState::stick_yaw) as left by the previous frame, `trunc(cam+0x37C ·
/// 65536/360)` (`D_800CC280`, func_1504A730), all 16 bits (C, V). The move angle then turns by whole
/// 256-steps of the difference, so the spawn camera's 511 makes forward aim at 16895 and the move angle,
/// at 16896, step down to 16640 (the "−256" of the oracle README). Wave 2 carried the eye → look-at angle
/// + 90° here and used only its top byte.
///
/// The modern controls ([`action::MODERN`]) add `actions` and `aim_pitch`; `camera_yaw` then doubles
/// as the aim's yaw: the aim looks along the facing `camera_yaw + 0x4000`, where a forward stick moves
/// ([`aim_yaw`]). Bots and the oracle traces leave both at 0, which changes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "net",
    derive(serde::Serialize, serde::Deserialize, bevy::reflect::Reflect)
)]
pub struct Input {
    pub stick_x: i8,
    pub stick_y: i8,
    pub buttons: u16,
    pub camera_yaw: u16,
    /// [`action`] bits, held this frame.
    pub actions: u8,
    /// Under [`action::MODERN`]: the aim's pitch in the game's angle units (0x4000 = 90°), up positive,
    /// read within ±[`AIM_PITCH_MAX`] ([`aim_pitch`]).
    pub aim_pitch: i16,
}

/// [`Input::actions`]: the modern controls, one bit each while held. conker-rules' `pad()` turns them
/// into the N64 bits by the player's own state. Never in spare `buttons` bits: the recorded oracle
/// traces set L and the D-pad. Holster rides B (its meaning everywhere: draw/holster, the flag's
/// put-down, the bag's throw) and walk the stick (the client's half stick), so neither needs a bit.
pub mod action {
    pub const JUMP: u8 = 0x01;
    pub const CROUCH: u8 = 0x02;
    pub const FIRE: u8 = 0x04;
    pub const RELOAD: u8 = 0x08;
    /// The aim over the left shoulder (clear: the right); it moves the aim ray's start ([`super::aim_ray`]).
    pub const SHOULDER: u8 = 0x10;
    /// The weapon handling (C2c): Interact takes an on-foot weapon's pickup and gets on a mounted gun;
    /// the grenade, with a weapon drawn, holsters it first and draws it again after the throw.
    pub const INTERACT: u8 = 0x20;
    pub const GRENADE: u8 = 0x40;
    /// The modern scheme, which gates everything new: `pad()`, the aim ray, [`super::aim_facing`] and the
    /// move-while-aiming speeds. Clear: the N64 controls, bit for bit.
    pub const MODERN: u8 = 0x80;
}

/// The modern aim's camera arm, which the client's orbit camera mirrors: it pivots `AIM_PIVOT_HEIGHT`
/// above the feet (the eye the bots' aim rays start from), the camera sits `AIM_ARM` behind the pivot
/// along the look and `AIM_SHOULDER` to its right (left with [`action::SHOULDER`]), looking along the
/// aim's yaw and pitch. Its collision only slides the camera along that look line, so the line never
/// moves and the server rebuilds the ray without the client's camera ([`aim_ray`]). `right` is
/// forward × up, `(sin yaw, 0, cos yaw)` for the facing's `(cos yaw, 0, −sin yaw)`.
pub const AIM_PIVOT_HEIGHT: f32 = 67.5;
pub const AIM_ARM: f32 = 160.0;
pub const AIM_SHOULDER: f32 = 20.0;
/// How far short of a wall at the shoulder the aim ray starts ([`aim_ray`]; the client's arm keeps its eye
/// as far off a wall behind).
pub const AIM_WALL_GAP: f32 = 8.0;
/// The aim's pitch within ±80° (14563 units), the bots' scope's (`conker_camera::Scope::steer`).
pub const AIM_PITCH_MAX: i16 = 14563;
/// Modern speeds: R's aim moves at 0.75 of the stick's target, the scope at 0.5; hip-fire at full.
pub const AIM_SHARE: f32 = 0.75;
pub const SCOPE_SHARE: f32 = 0.5;

/// The facing the modern aim looks along: `camera_yaw + 0x4000`, where a forward stick moves.
pub fn aim_yaw(input: &Input) -> u16 {
    input.camera_yaw.wrapping_add(0x4000)
}

/// [`Input::aim_pitch`] within ±[`AIM_PITCH_MAX`].
pub fn aim_pitch(input: &Input) -> i16 {
    input.aim_pitch.clamp(-AIM_PITCH_MAX, AIM_PITCH_MAX)
}

/// The modern aim ray of a player standing at `pos`: its start and unit direction. It runs along the
/// camera's look line from the point of it nearest the weapon (Lyra's `CameraTowardsFocus`), so walls
/// between the camera and the player are never hit. The pivot stands in for the weapon (Lyra takes the
/// pawn's location): with the camera at `pivot − AIM_ARM·dir + side·right` and `right` ⟂ `dir`, that
/// point is `pivot + side·right`, whatever the arm's length. It may start inside the shooter's own body,
/// which every hit test along it leaves out. A wall between the pivot and that point (the shoulder against
/// it) pulls the start back toward the pivot, [`AIM_WALL_GAP`] short of the wall, so the ray never starts
/// past it; the client's camera takes the same start (`world`'s own line query, as its arm's).
pub fn aim_ray(pos: [f32; 3], input: &Input, world: &CollisionWorld) -> ([f32; 3], [f32; 3]) {
    let (c, s) = cos_sin(aim_yaw(input));
    let (pc, ps) = cos_sin(aim_pitch(input) as u16);
    let sign = match input.actions & action::SHOULDER {
        0 => 1.0,
        _ => -1.0,
    };
    let pivot = [pos[0], pos[1] + AIM_PIVOT_HEIGHT, pos[2]];
    let side = sign * aim_reach(pivot, [s, 0.0, c], sign, world);
    let start = [pos[0] + side * s, pivot[1], pos[2] + side * c];
    (start, [c * pc, ps, -s * pc])
}

/// How far out from the `pivot` toward the shoulder `sign` (1: along `right`, −1: against it) the aim
/// ray starts ([`aim_ray`]): [`AIM_SHOULDER`], or [`AIM_WALL_GAP`] short of a wall nearer.
pub fn aim_reach(pivot: [f32; 3], right: [f32; 3], sign: f32, world: &CollisionWorld) -> f32 {
    let out = [sign * right[0], 0.0, sign * right[2]];
    (world.ray(pivot, out, AIM_SHOULDER + AIM_WALL_GAP)).map_or(AIM_SHOULDER, |h| {
        (h.dist - AIM_WALL_GAP).clamp(0.0, AIM_SHOULDER)
    })
}

/// Under [`action::MODERN`], the facing the aim holds this frame ([`aim_yaw`]), if it does: while R
/// aims, through the scope, or while FIRE is held with a weapon drawn; not swimming. The stick then
/// moves the player under it (R's aim no longer centres the stick).
pub fn aim_facing(p: &PlayerState, input: &Input) -> Option<u16> {
    let aims = input.buttons & conker_camera::R != 0
        || p.scope_yaw.is_some()
        || (input.actions & action::FIRE != 0 && p.armed != 0 && p.armed != GRENADE);
    (input.actions & action::MODERN != 0 && aims && !p.swimming).then(|| aim_yaw(input))
}

/// Everything the movement step reads and writes, named after the game's player object.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "net",
    derive(serde::Serialize, serde::Deserialize, bevy::reflect::Reflect)
)]
pub struct PlayerState {
    /// +0x14: position, y up.
    pub pos: [f32; 3],
    /// +0x20, +0x24.
    pub y_vel: f32,
    pub gravity: f32,
    /// +0x3C, +0x44.
    pub xz_speed: f32,
    pub target_speed: f32,
    /// +0x76, +0x7A.
    pub move_angle: u16,
    pub facing: u16,
    /// +0x85.
    pub state: u8,
    pub airborne: bool,
    /// +0x180, and the floor triangle (+0x19E) / surface (+0x184) under the player.
    pub floor_y: f32,
    pub floor: Option<Floor>,
    /// Countdown of the current state (skid, landing).
    pub timer: u8,
    pub hard_landing: bool,
    /// Braking against a reversed stick (the skid clip plays until the restart reaches speed 4).
    pub reversing: bool,
    /// Frames spent in the current airborne state (the jump and fall clips end on it).
    pub air_frames: u16,
    /// A has been held since the take-off (gravity 4).
    pub jump_held: bool,
    pub prev_buttons: u16,
    /// +0x40: the heading in degrees the camera follows: the facing's ([`heading_from_facing`]), except
    /// that pushing a wall (state 122) turns it to face the wall square on.
    pub heading: f32,
    /// +0x80: the facing may still turn. Cleared once it reaches the move angle; on the ground the game
    /// sets it again every frame (`+0x7F`, the ground frame count, is 9 or more), in the air only landing
    /// does, so a jump's facing stops turning once it has caught up with the move angle (C, V: camera_jump
    /// 161-163).
    pub turning: bool,
    /// R's aim camera is in one of its phases (zooming in, aiming, zooming out) as the camera left it last
    /// tick ([`crate::follow::Follow::tick`]): the stick reads centred, so a run brakes into a skid and the
    /// facing holds (V: camera_aim 391-408, camera_buttons 534-565; the player moves again on the tick
    /// the camera spends in phase 0 before it pops, camera_buttons 565).
    pub aiming: bool,
    /// Last frame was a strafe ([`STRAFE_TARGET`]; ground frames only).
    pub prev_strafe: bool,
    /// +0xCC: how far the player has sunk into the floor ([`SURFACES`]); the camera's look-at and the
    /// body's joints show it (conker-rules' `parts`), the position stays on the floor.
    pub sink: i16,
    /// +0xAA: the floor's speed percentage ([`SURFACES`]), 0 for none.
    pub speed_percent: u8,
    /// +0x89: while set the stick gives no drive (no turn, the speed brakes), counted down by one a frame
    /// below 0xFA (`func_1504BC38`). The movement sets it for a skid's first 8 frames and a hard landing's
    /// first 5; the match rules set it for actions that hold the player (C, V: the grenade throw locks 30
    /// counting down, then 255 until the throw ends, `combat_buttons` 175-215).
    pub lock: u8,
    /// +0x81 is set: the player took off with A (it stays set until the landing). A walk off an edge leaves
    /// it clear, and the ground rules (turn cap, state choice) keep running in that fall (C, V: anim_ledge
    /// 409-424).
    pub jumped: bool,
    /// s126+0x78: the weapon drawn (its id), 0 for none. Set by the match rules; the movement takes the
    /// weapon's walk (its speed percentage, no skid, no wall-push state) and stops into idle at once
    /// (`func_15064B94`, C; V: combat_e_w22 816).
    pub armed: u8,
    /// s126+0x4A: frames left scared by an enemy's drawn weapon. Set by the match rules; unarmed,
    /// the gait at 5 and above is the scared run, its rate `e/12` (`func_1504C0E8`, C; V
    /// combat5_sg_e2 766-791: 0x2B where the run would be, then the skid).
    pub scared: u8,
    /// s126+0x37: the crouch-walk is allowed (set below speed 3 with Z held, cleared above 4 without).
    pub crouch_ok: bool,
    /// obj+0x13C ≥ 100: the player holds an object (War: Colors' flag). Set by the match rules; the
    /// drive is 0.8 of the stick's and the ground speed gains 0.75 a frame (`func_1504CB98` at
    /// 0x1504E3A0 and 0x1504EE64, C; V bots1: carriers top out at 28 = 0.8 · 35, gaining 0.75 then
    /// 0.0075; e1: +0.75 a frame).
    pub holding: bool,
    /// War: Heist's bag held (with [`Self::holding`]): the drive 0.9 more, 0.72 of the stick's (V
    /// h1, bots1: carriers top out at 25.2 = 0.72 · 35, gaining 0.75 a frame; I: the factor's
    /// source, taken as the floor's 90 % on the holding 0.8).
    pub heavy: bool,
    /// The animation clock and the root's advance ([`animate`], [`crate::root`]).
    pub root: Root,
    /// s126+0x12: the upper body's twist while a gun is aimed ([`twist`]), u16 angle units within
    /// ±[`TWIST_MAX`]. The camera aims along the facing less it (`conker_camera::PlayerView::twist`).
    pub twist: i16,
    /// s126+0x16C while the first-person scope is open (s126+0x197): the look's yaw in degrees
    /// (`conker_camera::Scope::yaw`), which the facing follows. Set after each camera step, as
    /// [`aiming`](Self::aiming) is: `cam.scope.on.then_some(cam.scope.yaw)`.
    pub scope_yaw: Option<f32>,
    /// obj+0xF4 bit 0x10000: the wall routine moved the player in x or z this frame (`func_15058898`,
    /// cleared at the frame's start by `func_1504A730`; C, V `q2f4`). The bots' scripts read it (query
    /// 0xC3) to press Z+A against a wall: the high jump.
    pub walled: bool,
    /// Tank: −s126+0x1BC, the turret's yaw in degrees off the hull's facing (0..360, as the facing
    /// turns: the shells fly at facing + `turret`), and s126+0x1B8,
    /// the gun's pitch in degrees (−20..80, up positive); 0 on foot ([`crate::tank`]).
    pub turret: f32,
    pub pitch: f32,
    /// obj+0xAD: in the water ([`crate::swim`]); the frame is the swim's, `airborne` stays false.
    pub swimming: bool,
    /// obj+0x118: the water's height over the player at the last full floor query
    /// ([`CollisionWorld::water`]), [`crate::collision::NO_FLOOR`] for none.
    pub water: f32,
    /// obj+0x16C, obj+0x170: the current's drift, added to every step (x, z).
    pub drift: [f32; 2],
    /// s126+0x1C: the float's rise.
    pub rise: f32,
    /// obj+0xAE: frames left of a jump out of the water's slow brake.
    pub jump_out: u8,
    /// obj+0x81 = 2: the jump was out of the water (gravity 3.3 while A is held; the air brakes
    /// with the stick released).
    pub water_jump: bool,
    /// obj+0x7F is 0: no frame on the floor since a frame ended more than 20 over it. A landing under
    /// speed 9 then puts the move angle on the facing (`func_1505A9AC`, C; V swim1 208).
    pub aloft: bool,
    /// The wade's clip clock (obj+0xB4 in state 139, [`crate::swim::wade`]).
    pub wade: f32,
}

/// Per surface kind (bits 0-4 of the floor's word), the game's table at `0x80099140` (C, `func_1504BA38`;
/// conker-rules' `surface::SINK`, checked against the ROM there): the depth the player sinks to, its
/// change with speed, and the speed percentage. On scene 0x2D only kind 0xA has an entry (V: wall_jump
/// 470-476, speed 35 → 32.5 → 31.5 and the look-at 6 lower; the oracle's surface_a run).
pub const SURFACES: [[i8; 3]; 20] = {
    let mut t = [[0; 3]; 20];
    t[0x07] = [45, -5, 50];
    t[0x0A] = [-6, 0, 90];
    t[0x0B] = [-6, 0, 90];
    t[0x0C] = [44, -6, 40];
    t[0x0E] = [-100, 0, 0];
    t[0x0F] = [44, -4, 80];
    t[0x12] = [44, -4, 80];
    t[0x13] = [-100, 0, 30];
    t
};

/// `func_1504BA38`, at the start of a frame, on last frame's floor: within 10 of it, the kind's speed
/// percentage (kept while one is set) and the depth the sink heads for (0 higher up). `sink`: the
/// world's table, else [`SURFACES`].
fn surface(p: &mut PlayerState, sink: Option<&[[i8; 3]; 32]>) -> i16 {
    if 10.0 < p.pos[1] - p.floor_y {
        return 0;
    }
    let kind = p.floor.map_or(0, |f| f.surface & 0x1F) as usize;
    let [depth, per_speed, percent] = match sink {
        Some(table) => table[kind],
        None => SURFACES.get(kind).copied().unwrap_or_default(),
    };
    if percent != 0 && p.speed_percent == 0 {
        p.speed_percent = percent as u8;
    }
    if per_speed == 0 {
        return depth.into();
    }
    let scale = f32::from(per_speed) * f32::from_bits(0x3DCC_CCCD); // 0.1
    (scale * (f32::from(depth) - p.xz_speed)) as i32 as i16
}

/// `func_1505250C`: the sink steps toward its target by a twelfth of the gap plus one, not past it.
fn step_sink(sink: i16, target: i16) -> i16 {
    let (cc, ce) = (i32::from(sink), i32::from(target));
    if cc < ce {
        ((cc + (ce - cc) / 12 + 1) as i16).min(target)
    } else if ce < cc {
        ((cc + (ce - cc) / 12 - 1) as i16).max(target)
    } else {
        sink
    }
}

impl PlayerState {
    /// Standing still at `pos` facing `facing`, as a spawned player.
    pub fn spawn(pos: [f32; 3], facing: u16) -> Self {
        Self {
            pos,
            y_vel: GROUND_Y_VEL,
            gravity: GRAVITY_FALL,
            xz_speed: 0.0,
            target_speed: 0.0,
            move_angle: facing,
            facing,
            state: state::IDLE,
            airborne: false,
            floor_y: pos[1],
            floor: None,
            timer: 0,
            hard_landing: false,
            reversing: false,
            air_frames: 0,
            jump_held: false,
            prev_buttons: 0,
            heading: heading_from_facing(facing),
            turning: true,
            aiming: false,
            prev_strafe: false,
            sink: 0,
            speed_percent: 0,
            lock: 0,
            jumped: false,
            armed: 0,
            scared: 0,
            crouch_ok: false,
            holding: false,
            heavy: false,
            root: Root {
                id: u16::MAX,
                ..Root::default()
            },
            twist: 0,
            scope_yaw: None,
            walled: false,
            turret: 0.0,
            pitch: 0.0,
            swimming: false,
            water: crate::collision::NO_FLOOR,
            drift: [0.0; 2],
            rise: 0.0,
            jump_out: 0,
            water_jump: false,
            aloft: false,
            wade: 0.0,
        }
    }
}

/// The game's stick deadzone (`func_1501BBB8`): |raw| < 8 reads 0, else raw - 7·sign (V).
pub fn deadzone(raw: i8) -> i8 {
    match raw {
        -7..=7 => 0,
        r if r > 0 => r - 7,
        r => r + 7,
    }
}

/// Signed difference `to - from` of two u16 angles, in -32768..=32767.
pub fn angle_diff(to: u16, from: u16) -> i32 {
    to.wrapping_sub(from) as i16 as i32
}

/// `(cos a, sin a)` for a u16 angle (polynomials, |error| < 1e-6).
pub fn cos_sin(a: u16) -> (f32, f32) {
    // Reduce to an octant: x in [-1/8, 1/8] turns around the nearest quarter.
    let quarter = ((a as u32 + 0x2000) >> 14) & 3;
    let r =
        (a.wrapping_sub((quarter as u16) << 14) as i16) as f32 * (core::f32::consts::TAU / 65536.0);
    let r2 = r * r;
    let s = r * (1.0 - r2 / 6.0 * (1.0 - r2 / 20.0 * (1.0 - r2 / 42.0)));
    let c = 1.0 - r2 / 2.0 * (1.0 - r2 / 12.0 * (1.0 - r2 / 30.0 * (1.0 - r2 / 56.0)));
    match quarter {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    }
}

/// `atan2(y, x)` as a u16 angle (65536 = a turn), from basic float ops (|error| < 0.2 units).
pub fn atan2_u16(y: f32, x: f32) -> u16 {
    // atan(t) for t in [0, 1] (|error| < 1e-5 rad), unfolded to the full circle.
    let atan01 = |t: f32| {
        let t2 = t * t;
        t * (0.999_866
            + t2 * (-0.330_299_5 + t2 * (0.180_141 + t2 * (-0.085_133 + t2 * 0.020_835_1))))
    };
    let (ax, ay) = (if x < 0.0 { -x } else { x }, if y < 0.0 { -y } else { y });
    if ax == 0.0 && ay == 0.0 {
        return 0;
    }
    let mut a = if ax >= ay {
        atan01(ay / ax)
    } else {
        core::f32::consts::FRAC_PI_2 - atan01(ax / ay)
    };
    if x < 0.0 {
        a = core::f32::consts::PI - a;
    }
    if y < 0.0 {
        a = core::f32::consts::TAU - a;
    }
    (a * (65536.0 / core::f32::consts::TAU)) as u32 as u16
}

/// The stick direction in 1/256 turns (0 = right, 64 = up), rounded down (I: the game's atan table).
pub fn stick_angle8(x: i8, y: i8) -> u8 {
    (atan2_u16(y as f32, x as f32) >> 8) as u8
}

/// The stick's direction as the game reads it (`func_1505A630` on the pad after the deadzone: 0 = right,
/// 0x4000 = up), from the game's arctangent table when the world has it.
pub fn stick_angle(x: i8, y: i8, acos: Option<&AcosTable>) -> u16 {
    match acos {
        Some(t) => t.angle(x as f32, y as f32),
        None => atan2_u16(y as f32, x as f32),
    }
}

/// The u16 angle of an XZ direction, in the game's convention `(cos a, -sin a)`.
fn angle_of(dx: f32, dz: f32) -> u16 {
    atan2_u16(-dz, dx)
}

/// The move angle the stick asks for: the camera's stick yaw + the stick's direction, all 16 bits (C, V:
/// func_1504CB98; the camera agent's 2,154 steering rows). Without the game's table (see [`stick_angle`]).
pub fn target_angle(input: &Input, sx: i8, sy: i8) -> u16 {
    input.camera_yaw.wrapping_add(stick_angle(sx, sy, None))
}

/// Largest move-angle turn this frame in 256-steps (C: func_1504CB98 at 0x1504E8DC; V at speeds 0-5, 8-12,
/// 14, 35): 25 below speed 5, else `trunc((64 − speed) · 0.25)`; after a jump (obj+0x81)
/// `trunc((51 − speed) / 13)` from speed 5 on, and none while falling faster than 10 (V: camera_jump's
/// turns through y_vel −9, none from −13).
fn turn_steps(speed: f32, air: bool, y_vel: f32) -> i32 {
    if speed < 5.0 {
        25
    } else if !air {
        ((64.0 - speed) * 0.25) as i32
    } else if y_vel < -10.0 {
        0
    } else {
        ((51.0 - speed) / 13.0) as i32
    }
}

/// Turns the move angle toward `target` by whole 256-steps of the difference,
/// `((target − angle) as i16 >> 8) << 8`, at most `cap`: it keeps its low byte, and an exact reversal
/// turns clockwise (C, V: anim_jump_stand row 364).
pub(crate) fn turn_move(angle: u16, target: u16, cap: i32) -> u16 {
    let d = ((target.wrapping_sub(angle) as i16) >> 8) as i32 * 256;
    angle.wrapping_add(d.clamp(-cap, cap) as u16)
}

/// The move angle's turn with a drive (func_1504CB98 from 0x1504E8B4, C): toward `target` by
/// [`turn_steps`] (or `cap`), and a stick [`REVERSE_STEPS`] or more against the motion brakes 2 more a
/// frame and drops the drive from speed 7, or between 3 and 7 restarts the other way with a 12-step turn
/// at speed 0, shortening a hold to 4 (V: anim_jump_stand 353-366, pick87 133-156). Returns the drive left.
fn turn_toward(
    p: &mut PlayerState,
    target: u16,
    mut drive: f32,
    air: bool,
    cap: Option<i32>,
) -> f32 {
    if drive == 0.0 || p.move_angle == target {
        return drive;
    }
    let mut steps = turn_steps(p.xz_speed, air, p.y_vel);
    let d = (target.wrapping_sub(p.move_angle) >> 8) as u8;
    if d.min(d.wrapping_neg()) >= REVERSE_STEPS {
        if p.xz_speed >= REVERSE_RESTART {
            (drive, steps) = (0.0, 0);
            p.xz_speed -= 2.0;
        } else if p.xz_speed > REVERSE_MIN {
            (steps, p.xz_speed) = (12, 0.0);
            if p.timer >= 5 {
                p.timer = 4;
            }
        }
    }
    p.move_angle = turn_move(p.move_angle, target, cap.unwrap_or(steps) * 256);
    drive
}

/// Turns the facing toward `to` (func_150599C8 at the player's rate 12, C): at most 3072, and a clockwise
/// turn stops one unit short, also for exactly −3072 (V: e.g. 16896 → 16641 for 16640, 46082 → 46337 for
/// 46337, camera_aim 522, move_turnrun 261). The camera swings back to this facing, so a unit off flips
/// its 256-step (tests/oracle.rs camera_loop).
pub(crate) fn turn_facing(facing: u16, to: u16) -> u16 {
    let d = angle_diff(to, facing);
    let step = if d < 0 {
        (-d - 1).min(3072)
    } else {
        d.min(3072)
    };
    facing.wrapping_add((step * d.signum()) as u16)
}

/// How far the facing is from `to` in 256-steps, as `func_150599C8` returns it (a clockwise gap counts
/// one less).
fn facing_gap(facing: u16, to: u16) -> u8 {
    let d = angle_diff(to, facing);
    ((if d < 0 { -d - 1 } else { d }) >> 8) as u8
}

/// Speed after one frame of stick: +`accel` (1, or 0.75 holding an object) while that stays at or under
/// the target, then +`accel` · 0.01 up to it (V).
fn accelerate(speed: f32, target: f32, accel: f32) -> f32 {
    accelerate_by(speed, target, accel, BRAKE)
}

/// [`accelerate`] with its brake (in the water 2 and 1, [`crate::swim`]).
pub(crate) fn accelerate_by(speed: f32, target: f32, accel: f32, brake: f32) -> f32 {
    // func_1505A3A8 (C): above the target brake 2.5 down to it, else +accel, or +accel · 0.01
    // (0x800994D0) within 1 of it.
    if target < speed {
        let s = speed - brake;
        if s < target { target } else { s }
    } else {
        let s = speed
            + if target - 1.0 < speed {
                accel * 0.01
            } else {
                accel
            };
        if target < s { target } else { s }
    }
}

/// A multiplayer side: the team and the base a player starts and respawns in, the squirrels (the
/// SHC soldier, bank-01 model 128), the Tediz (117, the oracle's Deathmatch character) or the
/// cavemen (136, the bots of the original's Deathmatch on scene 0x3F, scenario 7: no base; V
/// `../lab/oracle/caveman_README.md`). War: Total War puts team 0 on class 4 (model 128) and team 1
/// on class 3 (117). The body a player pushes and is hit with is its class's (conker-rules'
/// `characters::body`), not its side's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "net", derive(serde::Serialize, serde::Deserialize))]
pub enum Side {
    Squirrels,
    #[default]
    Tediz,
    Cavemen,
}

/// Another body that pushes the player away when they overlap (`func_1505D6F0`): its position, and its
/// object's radius (+0xE4), centre height (+0xE8) and vertical scale (+0xEC).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Other {
    pub pos: [f32; 3],
    pub radius: i16,
    pub lift: i16,
    pub y_scale: f32,
}

impl Other {
    /// A body at `pos` of radius `r`, height `h` and centre `c` above the feet (obj+0xE4, +0xE6,
    /// +0xE8): its vertical scale is r / h (+0xEC; V: the Tedi's 23/45 at scale 0.5, the
    /// squirrel's 30/66, the raptor's 124/203; conker-rules' `characters::body` gives a class's).
    pub fn new(pos: [f32; 3], [r, h, c]: [i16; 3]) -> Self {
        Self {
            pos,
            radius: r,
            lift: c,
            y_scale: f32::from(r) / f32::from(h),
        }
    }

    /// The two bodies overlap ([`push_apart`]'s test; the game sets the touch bits of
    /// `0x800CC268` on it, `func_1505D408`).
    pub fn overlaps(&self, o: &Other) -> bool {
        let ([dx, dy, dz], sum) = self.offset(o);
        dx * dx + dy * dy + dz * dz < sum * sum
    }

    /// Within `d2`'s square root of `o` on the same distance (the scare's test, `func_1505D6F0`).
    pub fn within(&self, o: &Other, d2: f32) -> bool {
        let ([dx, dy, dz], _) = self.offset(o);
        dx * dx + dy * dy + dz * dz < d2
    }

    /// The offset from `o` to this body, its height difference between the centres times the mean
    /// vertical scale, and the radii's sum.
    fn offset(&self, o: &Other) -> ([f32; 3], f32) {
        let top = self.pos[1] + f32::from(self.lift);
        let dy = (top - (o.pos[1] + f32::from(o.lift))) * ((self.y_scale + o.y_scale) * 0.5);
        let d = [self.pos[0] - o.pos[0], dy, self.pos[2] - o.pos[2]];
        (d, f32::from(self.radius) + f32::from(o.radius))
    }
}

/// `func_1505D6F0` → `func_1505D408` (C; V: combat_e_w14b from 820): for each overlapping body, where
/// `dx² + dy² + dz² < (r + r')²` with `dy` the centres' height difference times the mean vertical scale,
/// the player moves 0.4 of the overlap away from it along that 3D direction (a distance² under 1 counts
/// as 1.2, a body straight above pushes along +x). Distances are measured from where the player stood
/// before any of this frame's pushes; equal bodies push each other (each in its own update). Public for
/// the bodies the match rules move themselves (hit, dead, burning); returns whether it moved `pos`, the
/// position of body `me` (a player's, its class's; a tank's, [`crate::tank::body`]).
pub fn push_apart(pos: &mut [f32; 3], me: Other, others: &[Other]) -> bool {
    let me = Other { pos: *pos, ..me };
    let start = *pos;
    for o in others {
        let ([mut dx, dy, dz], sum) = me.offset(o);
        let mut d2 = dx * dx + dy * dy + dz * dz;
        if d2 >= sum * sum {
            continue;
        }
        if d2 < 1.0 {
            d2 = f32::from_bits(0x3F99_999A); // 1.2
        }
        if dx == 0.0 && dz == 0.0 {
            dx = 1.0;
        }
        let d = d2.sqrt();
        let inv = 1.0 / d;
        let push = (sum - d) * f32::from_bits(0x3ECC_CCCD); // 0.4
        pos[0] += push * (dx * inv);
        pos[2] += push * (dz * inv);
        pos[1] += push * (dy * inv);
    }
    *pos != start
}

/// `func_15063A38` (C; V camera4_w09, w15, w39 with the twin guns, the flamethrower and the pistol): while
/// a gun is aimed the stick's x twists the upper body, `x·200·D_800CC264/2000` a frame (`D_800CC264`, the
/// frame time in percent, is 100 at 30 Hz: 750 at full stick); past ±[`TWIST_MAX`] the rest turns the
/// facing, and the move angle with it.
fn twist(p: &mut PlayerState, x: i8) {
    let t = (i32::from(p.twist) + i32::from(x) * 200 * 100 / 2000) as i16 as i32;
    let max = i32::from(TWIST_MAX);
    let over = t - t.clamp(-max, max);
    if over != 0 {
        p.facing = p.facing.wrapping_sub(over as u16);
        p.move_angle = p.facing;
    }
    p.twist = (t - over) as i16;
}

/// The twist's relaxation at the start of `func_1504C0E8`, every frame the stick does not twist (C; V
/// camera4_w09 542-563: −9000, −6750, −5063 … −30, −23, 0): a quarter of the way back to 0 (truncated),
/// and 0 within 29.
fn relax_twist(p: &mut PlayerState) {
    p.twist = if (-29..=29).contains(&p.twist) {
        0
    } else {
        p.twist + (0.25 * -f32::from(p.twist)) as i16
    };
}

/// What this frame's step goes by (func_15059C84): the root's advance along the facing once (`true`),
/// or the speed along the move angle.
pub(crate) fn root_or_speed(p: &mut PlayerState) -> (bool, f32, u16) {
    if std::mem::take(&mut p.root.moves) {
        (true, p.root.advance, p.facing)
    } else {
        (false, p.xz_speed, p.move_angle)
    }
}

/// One frame of the animation clock as far as root motion goes (C: `func_1505E0C4` restarts a newly
/// requested clip at `start`, `func_1502DB84` measures the root before `func_1507BDB0` advances the clock
/// by `rate`; V: anim_creep, see [`crate::root`]). Call it after each frame's movement with the animation
/// the player plays: its state for the movement's own clips (the creep), the rules' choice for actions
/// (weapon 0x16's attack 0x77, the death 0x17E), with conker-anim's rate and start for it. When that clip
/// has a root track ([`CollisionWorld::root_track`]) the next frame's step goes by the root (two frames
/// behind the clock); other clips change nothing. Without calls every clip moves by speed.
pub fn animate(p: &mut PlayerState, id: u16, rate: f32, start: f32, world: &CollisionWorld) {
    let r = &mut p.root;
    if id != r.id {
        (r.id, r.time, r.fresh) = (id, start, true);
    }
    let Some(track) = world.root_track(id) else {
        r.wrapped = false;
        return;
    };
    let z = track.z_at(r.time);
    if r.wrapped {
        r.at -= track.travel();
    }
    (r.advance, r.at) = (z - r.at, z);
    r.moves = !std::mem::take(&mut r.fresh);
    let t = r.time + rate;
    let duration = f32::from(track.duration);
    r.wrapped = t >= duration;
    r.time = if r.wrapped { t - duration } else { t };
}

/// Moves the player by a pending root advance ([`animate`]) when the match rules move the body
/// themselves this frame instead of [`step`] (hit reactions, deaths: the 0x17E death drifts along its
/// spinning facing, V: combat_e_w22 852-865, combat_e_w24 905-935), along the floor's plane on the
/// ground as [`step`] does; returns whether it moved. The pushes, walls and floor are the caller's.
pub fn root_motion(p: &mut PlayerState, world: &CollisionWorld) -> bool {
    if !std::mem::take(&mut p.root.moves) {
        return false;
    }
    let (c, s) = cos_sin(p.facing);
    let rise = match p.floor.filter(|_| !p.airborne) {
        Some(f) => rise_along(world.tris[f.tri as usize].normal, c, s),
        None => 0.0,
    };
    let d = 0.5 * p.root.advance / (1.0 + rise * rise).sqrt();
    p.pos[0] += d * c;
    p.pos[1] += d * rise;
    p.pos[2] -= d * s;
    true
}

/// One frame of the jump onto a mounted gun (conker-rules' `gun`; `func_1505327C` sets it up, C; V
/// b30 4896-4916 to 1e-3, t1 132-155): the speed the player came with brakes 2.5 a frame down to 0
/// and carries it along the move angle (the gun's yaw), then x and z go 0.2 of the way to `to` (the
/// seat's, s126+0x28/0x2C, fixed at the launch); the height by the velocity and gravity the launch
/// set, the facing turned onto the move angle (at most 3072, a clockwise turn one short), and the
/// wall routine as in the air: a ceiling stops the rise (V m2 72: 687, velocity 0).
pub fn hop(p: &mut PlayerState, to: [f32; 2], world: &CollisionWorld) {
    let old = p.pos;
    p.xz_speed = (p.xz_speed - BRAKE).max(0.0);
    let (c, s) = cos_sin(p.move_angle);
    p.pos[0] += 0.5 * p.xz_speed * c;
    p.pos[2] -= 0.5 * p.xz_speed * s;
    p.pos[0] += (to[0] - p.pos[0]) * 0.2;
    p.pos[2] += (to[1] - p.pos[2]) * 0.2;
    integrate_y(p);
    p.facing = turn_facing(p.facing, p.move_angle);
    p.heading = heading_from_facing(p.facing);
    p.y_vel = sweep_walls(p, world, old, p.y_vel);
}

/// A gunner on its mounted gun's seat (V m2, e1): there, facing and moving along the gun's `yaw`,
/// still, its y velocity the ground's.
pub fn sit(p: &mut PlayerState, seat: [f32; 3], yaw: u16) {
    p.pos = seat;
    (p.facing, p.move_angle, p.heading) = (yaw, yaw, heading_from_facing(yaw));
    (p.airborne, p.y_vel, p.xz_speed, p.target_speed) = (false, GROUND_Y_VEL, 0.0, 0.0);
}

/// The jump back off a mounted gun on A (`func_1505F298`'s seat case, C; V m2 1601-1615): a standing
/// jump at y velocity 30 and speed 30 along `angle`, A held (gravity 4 until let go, then 6.2), the
/// facing 0x1200 short of it until the landing, and obj+0x89 = 0xFF, which zeroes the drive, so the
/// air brakes the speed 0.875 a frame from the next ([`air`]). The press's own frame moves already
/// (the speed's update ran before the jump): 15 along `angle`, the height by the velocity (26 left),
/// the push out of `others` (the gun's body, the player no longer its gunner) and the walls; [`step`]
/// flies it from the next frame.
pub fn leap(p: &mut PlayerState, angle: u16, world: &CollisionWorld, me: Other, others: &[Other]) {
    p.state = state::JUMP;
    (p.airborne, p.jumped, p.jump_held, p.turning) = (true, true, true, false);
    (p.air_frames, p.lock, p.timer) = (1, 0xFF, 0);
    (p.y_vel, p.gravity, p.xz_speed, p.move_angle) = (30.0, GRAVITY_HELD, 30.0, angle);
    p.facing = angle.wrapping_sub(0x1200);
    p.heading = heading_from_facing(p.facing);
    let old = p.pos;
    let (c, s) = cos_sin(angle);
    p.pos[0] += 0.5 * p.xz_speed * c;
    p.pos[2] -= 0.5 * p.xz_speed * s;
    integrate_y(p);
    push_apart(&mut p.pos, me, others);
    p.y_vel = sweep_walls(p, world, old, p.y_vel);
}

/// Advances the player by one frame (V for the rules tagged in this module).
pub fn step(p: &mut PlayerState, input: &Input, world: &CollisionWorld) {
    step_among(p, input, world, Other::new([0.0; 3], [23, 45, 33]), &[]);
}

/// [`step`] with the other bodies around (the other players, in object order: a player updated earlier
/// this frame at its new position): the player, of body `me`, is pushed out of them before the walls
/// ([`push_apart`]).
pub fn step_among(
    p: &mut PlayerState,
    input: &Input,
    world: &CollisionWorld,
    me: Other,
    others: &[Other],
) {
    let modern = input.actions & action::MODERN != 0;
    let (x, y) = if p.aiming && !modern {
        (0, 0)
    } else {
        (deadzone(input.stick_x), deadzone(input.stick_y))
    };
    let sink_target = surface(p, world.sink.as_ref());
    let mut stick = Stick {
        x,
        y,
        on: x != 0 || y != 0,
        target: input
            .camera_yaw
            .wrapping_add(stick_angle(x, y, world.acos.as_ref())),
        speed: 0.0,
    };
    p.target_speed = if stick.on {
        (STICK_SPEED * stick.mag()).min(RUN_SPEED)
    } else {
        0.0
    };
    // Modern: the scope moves at 0.5 of the stick's target, R's aim at 0.75 (R held, not the aim
    // camera, which shuts in the air and waits for a new press).
    if modern && p.scope_yaw.is_some() {
        p.target_speed *= SCOPE_SHARE;
    } else if modern && input.buttons & conker_camera::R != 0 {
        p.target_speed *= AIM_SHARE;
    }
    let face = aim_facing(p, input);
    if let Some(f) = face {
        p.facing = f;
    }
    // The speed aims at the floor's percentage of the target (0x1504E358; obj+0x44 keeps the target).
    stick.speed = if p.speed_percent == 0 {
        p.target_speed
    } else {
        p.target_speed * (f32::from(p.speed_percent) * f32::from_bits(0x3C23_D70A)) // 0.01
    };
    let a_held = input.buttons & BUTTON_A != 0;
    let a_pressed = a_held && p.prev_buttons & BUTTON_A == 0;
    p.prev_buttons = input.buttons;
    p.walled = false;
    p.jump_out = p.jump_out.saturating_sub(1);
    let old = p.pos;

    // Z held makes A the high jump, from a crouch too (C: the jump branch tests no state; V q2za from
    // 22; I: not with a weapon drawn, where Z is the weapon's; modern: armed too, Z the crouch's).
    let high = !p.airborne
        && a_pressed
        && input.buttons & BUTTON_Z != 0
        && (p.armed == 0 || modern)
        && (can_jump(p.state) || matches!(p.state, state::CROUCH | state::CROUCH_WALK));
    if !high && !p.airborne && a_pressed && can_jump(p.state) {
        // func_1505F298 (C): a jump ends a lock and a hold; standing or running is decided on the
        // frame's speed (in `air`).
        p.state = state::RUN_JUMP;
        (p.airborne, p.jumped, p.air_frames, p.jump_held) = (true, true, 0, true);
        p.water_jump = false;
        (p.lock, p.timer, p.hard_landing) = (0, 0, false);
        p.y_vel = JUMP_Y_VEL;
    }

    // A gun aimed with R (`conker_camera::aims_with_body`): the weapon's aim state twists the upper body
    // with the stick, the real one even while the aim camera centres it for the movement (C: the
    // handler's calls of `func_15063A38` with R; V from the press frame on, camera4_w09 500; I: the
    // grenade in hand, and Z's share of the 0x2010 mask).
    let twist_x = (!p.airborne
        && !p.swimming
        && !modern
        && input.buttons & conker_camera::R != 0
        && conker_camera::aims_with_body(p.armed))
    .then(|| deadzone(input.stick_x));
    let waiting = p.state == state::HIGH_JUMP && !p.airborne;
    let choice = if p.swimming {
        crate::swim::swim(p, stick, a_pressed, a_held, world, old, me, others)
    } else if p.airborne {
        air(p, stick, a_held, world, old, me, others)
    } else {
        ground(p, input, stick, world, old, me, others, twist_x)
    };
    // Into the water at the frame's end: the state choice is the swim's.
    let entered = crate::swim::enter(p);
    if high && !p.airborne && !entered {
        // The press frame went as the old state's (V q2s5w 112: the speed and the slide unbraked).
        (p.state, p.timer, p.lock) = (state::HIGH_JUMP, HIGH_JUMP_HOLD, HIGH_JUMP_LOCK);
    } else if waiting && p.airborne {
        // Slid off an edge while waiting: the hold goes with the state (I).
        p.timer = 0;
    } else if waiting && p.timer == HIGH_JUMP_HOLD - HIGH_JUMP_WAIT {
        // The launch ends a ground frame; the rise starts on the next (V: q2s5w 121, q2za 60).
        (p.airborne, p.jumped, p.jump_held, p.hard_landing) = (true, true, false, true);
        (p.y_vel, p.gravity, p.air_frames, p.timer) = (HIGH_JUMP_Y_VEL, high_jump_gravity(0), 0, 0);
    }
    if twist_x.is_none() {
        relax_twist(p);
    }
    p.sink = step_sink(p.sink, sink_target);
    if p.scope_yaw.is_some() && matches!(p.armed, 0x23 | 0x24) {
        // 0x23's and 0x24's scope holds the state (obj+0x83, the weapon handler's write before the
        // frame's countdown; C; V camera4_w23, w24: 9 on the scoped rows, then 8 … 0 once it closes,
        // the speed percentage kept: w24 503-552). I: 0x23's hold lapses for a frame about every 30
        // (its aim clip restarting: the move angle snaps onto the facing, the percentage clears).
        p.timer = 10;
    }
    // The modern aim keeps the facing through the frame (the air turns it toward the stick).
    if let Some(f) = face {
        (p.facing, p.heading) = (f, heading_from_facing(f));
    }
    end_frame(p);
    if p.timer == 0 && (!p.airborne || matches!(choice, Choice::Out)) && !entered {
        choice.apply(p);
    }
    let drive = match choice {
        Choice::Gait { drive, .. } => drive,
        _ => 0.0,
    };
    crate::swim::wade(p, drive);
}

/// The frame's stick after the deadzone, and the move angle it asks for.
#[derive(Clone, Copy)]
pub(crate) struct Stick {
    x: i8,
    y: i8,
    pub(crate) on: bool,
    pub(crate) target: u16,
    /// The speed the stick aims at: the target speed, scaled by the floor's percentage.
    speed: f32,
}

impl Stick {
    fn mag(&self) -> f32 {
        let (x, y) = (self.x as f32, self.y as f32);
        (x * x + y * y).sqrt()
    }
}

/// States that take off on A (V: from a wall push too, a running jump that lands back into the push,
/// wall_jump; I: jumps from 54 are untraced, so A does nothing there; 22 and 27 hold Z, so A is the
/// high jump there, [`HIGH_JUMP_Y_VEL`]).
fn can_jump(s: u8) -> bool {
    matches!(
        s,
        state::IDLE
            | state::CREEP
            | state::WALK
            | state::RUN
            | state::SCARED_RUN
            | state::SKID
            | state::PUSH_WALL
    )
}

/// One ground frame; returns the drive and whether the walls pushed back for the state choice
/// ([`choose`]), unless the crouch decided the state. `twist_x`: the stick's x twists the upper body
/// ([`twist`]) after the turn and the speed, before the step.
#[allow(clippy::too_many_arguments)]
fn ground(
    p: &mut PlayerState,
    input: &Input,
    mut stick: Stick,
    world: &CollisionWorld,
    old: [f32; 3],
    me: Other,
    others: &[Other],
    twist_x: Option<i8>,
) -> Choice {
    // Z crouches only unarmed (func_1504C0E8 skips the crouch with s126+0x78 set, C); the modern
    // controls crouch armed too (the rules feed Z from the crouch action, not the trigger).
    let modern = input.actions & action::MODERN != 0;
    let z = input.buttons & BUTTON_Z != 0 && (p.armed == 0 || modern);
    let was = p.state;
    // The lock takes this frame's drive, then counts down (func_1504CB98 at 0x1504E7xx, func_1504BC38).
    let locked = p.lock != 0;
    if locked && p.lock < 0xFA {
        p.lock -= 1;
    }
    // A state hold (obj+0x83: skid, landing) keeps the state and the crouch off; the stick still drives.
    let held = p.timer != 0;
    let strafed = std::mem::take(&mut p.prev_strafe);
    let crouched = !held && z && matches!(p.state, state::CROUCH | state::CROUCH_WALK);
    if p.state == state::PUSH_WALL && z {
        // Z against a wall does not crouch: the move angle snaps to the facing and the push goes on
        // (V: wall_jump 350-361).
        p.move_angle = p.facing;
    }
    // Modern: the aim holds the facing ([`aim_facing`]) and the stick moves the player under it.
    let aimed = aim_facing(p, input).is_some();
    if let Some(yaw) = p.scope_yaw.filter(|_| !modern) {
        // Through the scope (func_1504CB98 at 0x1504D4B4, C; V camera4 w14, w16, w23, w24: 771 of 784
        // rows exact, the rest one unit off): the facing takes the look's yaw, `trunc(yaw · 182.04445)`,
        // the move angle turns toward it at a drive of 0.1 (no step below speed 1) and the facing turns
        // back toward the move angle. I: the 13 rows where the move angle also snaps onto the facing
        // (every ~30 frames, likely the aim clip's loop) are not modelled.
        p.facing = (yaw * f32::from_bits(0x4336_0B61)) as i32 as u16;
        stick.target = p.facing;
        p.target_speed = SCOPE_DRIVE;
    }
    // C-left/C-right strafe; with a weapon drawn C-up and C-down too (forward, back: the mask 0xF at
    // func_1504CB98 0x1504DD14, C; V: combat_ctlw_k6 271-273 with 0x23), but not with a grenade in hand
    // (0x26, the throw's s126+0x19B; V: combat_ctl_cdhold 85-140) nor through the scope except with 0x16
    // (C, untraced). The modern controls strafe with the stick: no C strafe.
    let mask = if modern {
        0
    } else if p.armed != 0 && p.armed != GRENADE && (p.scope_yaw.is_none() || p.armed == 0x16) {
        C_BUTTONS
    } else {
        BUTTON_C_LEFT | BUTTON_C_RIGHT
    };
    let c = input.buttons & mask;
    let (target, cap) = if c != 0 && !z {
        p.prev_strafe = true;
        strafe(p, c, stick)
    } else {
        if strafed {
            // Letting go of a strafe snaps the move angle back to the facing (s126+0x52, C; V:
            // camera_buttons 204, 264, 653).
            p.move_angle = p.facing;
        }
        (stick.target, None)
    };
    // Crouched (state 22), the speed only brakes, by 3.2; with Z held the drive goes after the turn
    // (func_1504CB98 at 0x1504EC94, C).
    let crouch = p.state == state::CROUCH;
    let speed = if locked { 0.0 } else { scaled(p) };
    let drive = locomotion(
        p,
        target,
        speed,
        cap,
        !p.prev_strafe && !aimed,
        crouch,
        crouch && z,
    );
    if let Some(x) = twist_x {
        twist(p, x);
    }
    // func_15059C84 (C): by the root's advance along the facing when the last render measured one
    // ([`animate`]), else by the speed along the move angle, and below speed 1 not at all (V: 0.77 does
    // not move, 1.0 does). Under the modern aim the root goes along the move angle (a strafe).
    let (root, dist, angle) = root_or_speed(p);
    let angle = if aimed { p.move_angle } else { angle };
    if p.drift != [0.0; 2] {
        // The current's drift adds to the step, even standing (V sea 351-356).
        let normal = p.floor.map(|f| world.tris[f.tri as usize].normal);
        crate::swim::drift_step(p, dist, angle, normal);
    } else if root || dist >= 1.0 {
        // The step follows the floor's plane: same length along the surface (V: 1e-6 on 12 sloped
        // triangles), the vertical part only shows when the floor ends (ledge row 409).
        let (c, s) = cos_sin(angle);
        let rise = p
            .floor
            .map_or(0.0, |f| rise_along(world.tris[f.tri as usize].normal, c, s));
        let d = 0.5 * dist / (1.0 + rise * rise).sqrt();
        p.pos[0] += d * c;
        p.pos[1] += d * rise;
        p.pos[2] -= d * s;
    }
    if crouched && !aimed {
        // Crouching and crouch-walking the move angle takes the facing after the step (V: move_crouch
        // 95-99, anim_crouch 384, camera_buttons 655-660 step along the turned move angle).
        p.move_angle = p.facing;
    }
    p.heading = heading_from_facing(p.facing);
    // Grounded, the game still falls a step (y_vel -4 → -9, y − 4.5) and lets the floor catch it (V: the
    // wall routine's calls, lab/oracle/trajectories/wall_*_lldb.csv).
    let stepped_y = p.pos[1];
    let y_vel = p.y_vel - GRAVITY_FALL;
    p.pos[1] += 0.5 * y_vel;
    push_apart(&mut p.pos, me, others);
    let pre = p.pos;
    let y_vel = sweep_walls(p, world, old, y_vel);
    p.walled = p.pos[0] != pre[0] || p.pos[2] != pre[2];
    follow_floor(p, world, old, stepped_y, pre, y_vel);
    let wall = !p.airborne && push_wall(p, push_angle(p, pre, world), true);
    p.turning = !p.airborne || p.facing != p.move_angle;
    if z {
        Choice::Crouch { wall, was }
    } else {
        Choice::Gait { drive, wall }
    }
}

/// What the frame's end chooses the next state from (`func_1504C0E8`), once the hold has counted down.
#[derive(Clone, Copy)]
pub(crate) enum Choice {
    /// Nothing (in the air, or a landing that holds).
    None,
    /// Just out of the water and still in the air: the gait (the wade) all the same, without drive.
    Out,
    /// The gait from the drive left this frame, or a wall push.
    Gait { drive: f32, wall: bool },
    /// Z held: a crouch or crouch-walk (a wall push first), `was` the state at the frame's start.
    Crouch { wall: bool, was: u8 },
}

impl Choice {
    fn apply(self, p: &mut PlayerState) {
        match self {
            Choice::None => {}
            Choice::Out => choose(p, 0.0, false),
            Choice::Gait { drive, wall } => choose(p, drive, wall),
            Choice::Crouch { wall: true, .. } => p.state = state::PUSH_WALL,
            Choice::Crouch { was, .. } => {
                crouch_state(p);
                if p.state == state::CROUCH && was != state::CROUCH {
                    // A crouch from standing snaps the move angle to the facing after the step (V:
                    // move_crouch row 94, 0.0017 off when snapped before it).
                    p.move_angle = p.facing;
                }
            }
        }
    }
}

/// The state with Z held (func_1504C0E8, C; V: move_crouch, anim_crouch): below speed 3 the crouch-walk
/// is allowed (s126+0x37, until the speed passes 4 without Z); allowed and with the stick asking for more
/// than 2.6 it crouch-walks at 40 percent of the target (35 gives f32 0x415FFFFF), else it crouches.
/// Armed, only the modern controls get here ([`ground`]'s Z).
fn crouch_state(p: &mut PlayerState) {
    if p.xz_speed < 3.0 {
        p.crouch_ok = true;
    }
    if p.target_speed > f32::from_bits(0x4026_6666) && p.crouch_ok {
        p.state = state::CROUCH_WALK;
        p.speed_percent = CROUCH_PERCENT;
    } else {
        p.state = state::CROUCH;
    }
}

/// The target speed scaled by the speed percentage (obj+0xAA: the floor's, a strafe's 65, a crouch-walk's
/// 40), the drive `func_1504CB98` hands on (C, 0x1504E358).
pub(crate) fn scaled(p: &PlayerState) -> f32 {
    let drive = if p.speed_percent == 0 {
        p.target_speed
    } else {
        p.target_speed * (f32::from(p.speed_percent) * f32::from_bits(0x3C23_D70A)) // 0.01
    };
    if p.holding && p.heavy {
        drive * f32::from_bits(0x3F4C_CCCD) * 0.9
    } else if p.holding {
        drive * f32::from_bits(0x3F4C_CCCD) // 0.8, 0x8009922C
    } else {
        drive
    }
}

/// The end of every frame (`func_1504CB98` at 0x15050E74): a state hold counts down, and without one the
/// speed percentage clears (C; V: wall_jump 470-476 keeps 90 through a skid).
fn end_frame(p: &mut PlayerState) {
    if p.timer == 0 {
        p.speed_percent = 0;
    } else if p.timer != 0xFF {
        p.timer -= 1;
    }
}

/// The ground state for the next frame (`func_1504C0E8` with `func_15064B94`, C): a wall push, else the
/// gait on `e = 0.4·speed + 0.6·drive` (scared, the scared run from 5; run above 14, walk above 6 or
/// while braking above speed 1, creep above 1, else idle), and a skid when a stop from above speed 18 has no drive left (17 frames, the
/// first 8 locked). Most drawn weapons ([`armed_walk`]) walk while the stick asks for 1 or more, else
/// stand, and never skid. Idle snaps the move angle to the facing (`func_1507F640`).
pub(crate) fn choose(p: &mut PlayerState, drive: f32, wall: bool) {
    if p.xz_speed > 4.0 {
        p.crouch_ok = false;
    }
    let armed = armed_walk(p.armed);
    let e = 0.4 * p.xz_speed + 0.6 * drive;
    p.state = if wall && p.armed == 0 {
        state::PUSH_WALL
    } else if armed.is_some() && p.target_speed < 1.0 {
        state::IDLE
    } else if p.scared > 0 && p.armed == 0 && e >= 5.0 {
        state::SCARED_RUN
    } else if p.water != crate::collision::NO_FLOOR && p.pos[1] < p.water && p.xz_speed >= 1.0 {
        // Wading (V swim1 220-245: the speed 75 % of the stick's, 26.25 at a full stick).
        p.speed_percent = 75;
        state::WADE
    } else if e > 14.0 {
        state::RUN
    } else if e > 6.0 || (drive < 1.0 && p.xz_speed > 1.0) {
        state::WALK
    } else if e > 1.0 {
        state::CREEP
    } else {
        state::IDLE
    };
    if p.state == state::IDLE {
        // func_1507F640: the move angle takes the facing, and an action's lock (0xFA and up) ends.
        // Through the scope 0x14's and 0x16's aim state stands in for idle (`func_15064B94` returns
        // it; C, V camera4_w14, w16): no snap.
        if p.scope_yaw.is_none() {
            p.move_angle = p.facing;
        }
        p.speed_percent = 0;
        if p.lock >= 0xFA {
            p.lock = 0;
        }
    } else if let Some(percent) = armed {
        p.speed_percent = percent;
    } else if !wall && p.xz_speed > SKID_SPEED && drive == 0.0 {
        p.state = state::SKID;
        p.lock = 8;
        p.timer = SKID_FRAMES;
    }
}

/// The drawn weapons `func_15064B94` walks with, and their speed percentage (C): most 80, the melee
/// weapon 0x21 110, 0x16 30, 0x3B 55, the twin guns (0x09) and the pistol (0x39) none in multiplayer.
/// Others (0x14) keep the plain gait and its skid (V: combat_e_w14b 824-830).
fn armed_walk(weapon: u8) -> Option<u8> {
    match weapon {
        0x21 => Some(110),
        0x16 => Some(30),
        0x3B => Some(55),
        0x12 | 0x18 | 0x41 | 0x24 | 0x22 | 0x26 | 0x3A | 0x25 | 0x23 | 0x15 => Some(80),
        0x09 | 0x38 | 0x39 | 0x37 | 0x2D | 0x2E | 0x19 | 0x40 => Some(0),
        _ => None,
    }
}

/// Pushing into a wall on the ground (func_1504CB98 at 0x15050340, C): within ±0x2200 of straight against
/// the motion, the speed is capped at 14 above 10, and above speed 2 the push is a wall push (state 122,
/// unless crouch-walking under 10 or strafing) whose heading faces the wall (V: camera_wall's obj+0x40)
/// unless the player was airborne (obj+0x81) at the start of the frame. Returns whether it is one.
fn push_wall(p: &mut PlayerState, away: Option<u16>, turn_heading: bool) -> bool {
    let Some(away) = away.filter(|&a| against(p.move_angle, a)) else {
        return false;
    };
    let push = p.xz_speed > 10.0
        || (p.xz_speed > 2.0 && !matches!(p.state, state::CROUCH_WALK) && !p.prev_strafe);
    if !push || p.prev_strafe {
        return false;
    }
    p.xz_speed = p.xz_speed.min(WALL_SPEED);
    if p.armed != 0 {
        return false;
    }
    if turn_heading && p.timer == 0 {
        p.heading = (away as i32 - 0x4000) as f32 * HEADING_UNIT;
    }
    true
}

/// A C-button strafe on the ground (func_1504CB98 at 0x1504D7xx, C; V: camera_buttons 173-262 with the
/// stick centred, 623-652 with it held): the stick's x turns the facing by `(x · 100) >> 4` a frame, the
/// move angle aims at the facing plus the strafe table's direction for the C buttons and the stick's y
/// (forward from 6, back below −5: diagonals), the target speed reads 34 at 65 percent, and the facing no
/// longer follows the move angle. Opposite buttons stop the player. Returns the move angle aimed at and
/// the turn cap override (4 steps above speed 12 while the facing equals the move angle).
fn strafe(p: &mut PlayerState, c: u16, stick: Stick) -> (u16, Option<i32>) {
    // u16 table at 0x8009917C by C-right 1, C-left 2, C-down (back) 4, C-up (forward) 8; 0xFFFF stands.
    const DIRECTION: [u16; 16] = [
        0, 0xC000, 0x4000, 0xFFFF, 0x8000, 0xA000, 0x6000, 0x8000, 0, 0xE000, 0x2000, 0, 0xFFFF,
        0xFFFF, 0xFFFF, 0xFFFF,
    ];
    let mut bits = c as usize;
    if p.scope_yaw.is_some() {
        // Through the scope the stick only looks (C, 0x1504DD2C).
    } else if stick.y >= 6 {
        bits |= 8;
    } else if stick.y < -5 {
        bits |= 4;
    }
    if p.speed_percent == 0 || p.speed_percent >= 0x42 {
        p.speed_percent = STRAFE_PERCENT;
    }
    let cap = (p.xz_speed > 12.0 && p.facing == p.move_angle).then_some(4);
    p.facing = p
        .facing
        .wrapping_sub(((i32::from(stick.x) * 100) >> 4) as u16);
    let dir = DIRECTION[bits];
    if dir == 0xFFFF {
        p.xz_speed = 0.0;
        p.target_speed = 0.0;
        return (p.move_angle, cap);
    }
    p.target_speed = STRAFE_TARGET;
    (p.facing.wrapping_add(dir), cap)
}

/// The drive on the ground (func_1504CB98 from 0x1504E8B4, C): the move angle turns toward `target` by
/// whole 256-steps, at most [`turn_steps`] (or `cap`); a stick 0x51 steps or more against the motion
/// brakes 2 more a frame and drops the drive from speed 7, and between 3 and 7 restarts the other way
/// with a 12-step turn at speed 0 (V: anim_jump_stand 353-366, pick87 133-136). The facing follows the
/// move angle (`func_150599C8`) unless strafing, and a facing 0x5B steps or more behind it above speed 12
/// drops the drive. Then the speed accelerates toward what is left of the drive, which is returned.
fn locomotion(
    p: &mut PlayerState,
    target: u16,
    drive: f32,
    cap: Option<i32>,
    facing: bool,
    crouch: bool,
    drop: bool,
) -> f32 {
    let mut drive = turn_toward(p, target, drive, p.jumped, cap);
    if facing {
        let behind = facing_gap(p.facing, p.move_angle);
        p.facing = turn_facing(p.facing, p.move_angle);
        if behind >= 0x5B && p.xz_speed > 12.0 && !p.jumped {
            drive = 0.0;
        }
    }
    if drop {
        drive = 0.0;
    }
    p.xz_speed = if crouch {
        // No acceleration crouched, a 3.2 brake (C: 0x80099258).
        if drive < p.xz_speed {
            (p.xz_speed - CROUCH_BRAKE).max(drive)
        } else {
            p.xz_speed
        }
    } else {
        accelerate(p.xz_speed, drive, if p.holding { 0.75 } else { 1.0 })
    };
    drive
}

/// One degree per 65536/360 angle units (`0x3BB40000`, exact in f32).
const HEADING_UNIT: f32 = 360.0 / 65536.0;

/// Whether the wall routine runs for the player this frame (call it before [`step`]): `func_1504CB98`
/// skips it while `obj+0xF8` bit 0x4 is clear (C, 0x1504FEC4). `func_150829D8` sets the bit every frame
/// and `func_150A11C4` clears it inside the level's type-1 trigger zones of kind 0 (set again by kind 6,
/// the last zone containing the player deciding), tested at the frame's start: the camera volumes
/// ([`CollisionWorld::camera`]; V on every human row of the raw dumps, see `tests/oracle.rs`
/// `wall_zones_match_the_game`). Scene 0x2D's zones cover most of the floor: the walls only push where
/// none is. Without the level's camera data the routine always runs.
///
/// ponytail: the gate's other conditions (s126+0x54, s126+0x6B, obj+0x137, obj+0x65, a bot that did not
/// move) are the rules' states and are not ported.
pub fn walls_on(p: &PlayerState, world: &CollisionWorld) -> bool {
    !world.camera.as_ref().is_some_and(|c| c.in_volume(p.pos))
}

/// The game's wall routine for the player from `old` (the frame's start) to the current position, unless
/// a zone turns it off ([`walls_on`]); returns the vertical velocity (zeroed by a ceiling).
pub(crate) fn sweep_walls(
    p: &mut PlayerState,
    world: &CollisionWorld,
    old: [f32; 3],
    y_vel: f32,
) -> f32 {
    if world.camera.as_ref().is_some_and(|c| c.in_volume(old)) {
        return y_vel;
    }
    let mut body = Body::player(old[1], p.pos[1]);
    if p.swimming {
        // A swimmer's box is 90 across, not 60 (`func_1507C3E0` with obj+0xAD, C; V swim1 476).
        (body.half_width, body.radius) = (45, 45);
    }
    let swept = world.sweep(old, p.pos, &body, &Sweep::default(), y_vel);
    p.pos = swept.pos;
    swept.y_vel
}

/// The direction the walls pushed the player this frame (`func_1505A630` of the push), if they did.
pub fn push_angle(p: &PlayerState, pre: [f32; 3], world: &CollisionWorld) -> Option<u16> {
    let (dx, mdz) = (p.pos[0] - pre[0], pre[2] - p.pos[2]);
    (p.pos[0] != pre[0] || p.pos[2] != pre[2]).then(|| match &world.acos {
        Some(t) => t.angle(dx, mdz),
        None => angle_of(dx, -mdz),
    })
}

/// A push within ±0x2200 (47.8°) of straight against the motion (`0x5E01 ≤ move − away < 0xA200`).
pub fn against(move_angle: u16, away: u16) -> bool {
    (0x5E01..0xA200).contains(&move_angle.wrapping_sub(away))
}

/// The floor's rise per unit of horizontal motion along `(c, -s)`.
pub(crate) fn rise_along([nx, ny, nz]: [f32; 3], c: f32, s: f32) -> f32 {
    if ny == 0.0 {
        0.0
    } else {
        (nz * s - nx * c) / ny
    }
}

/// The floor under the (pushed) position, then snap onto it or start falling. After a push the game
/// re-queries in full (no last-triangle fast path, `func_1504CB98` at 0x15050074).
pub(crate) fn follow_floor(
    p: &mut PlayerState,
    world: &CollisionWorld,
    old: [f32; 3],
    stepped_y: f32,
    pre: [f32; 3],
    y_vel: f32,
) {
    let pushed = p.pos[0] != pre[0] || p.pos[2] != pre[2];
    // `p.floor_y` is still the frame start's (the zones' floor test).
    let prev = if pushed {
        None
    } else {
        world.last_floor(old, p.floor_y, p.floor)
    };
    // The query takes the height after the frame's fall (obj+0x18; V swim1 540: up a slope onto the
    // shelf over it, which the stepped height misses).
    crate::swim::query(p, world, p.pos[1], old[1], prev);
    match p.floor {
        Some(f) if f.y > stepped_y - SNAP_DOWN => p.pos[1] = f.y,
        _ => {
            // Walked off an edge: fall (state 56, gravity 5) from the step already taken (V: ledge row
            // 409, -4.5 from y_vel -4 → -9).
            p.airborne = true;
            p.air_frames = 0;
            p.jump_held = false;
            p.state = state::FALL;
            p.gravity = GRAVITY_FALL;
            p.y_vel = y_vel;
        }
    }
}

pub(crate) fn integrate_y(p: &mut PlayerState) {
    p.pos[1] += 0.5 * p.y_vel - 0.275 * p.gravity;
    p.y_vel -= p.gravity;
}

pub(crate) fn air(
    p: &mut PlayerState,
    stick: Stick,
    a_held: bool,
    world: &CollisionWorld,
    old: [f32; 3],
    me: Other,
    others: &[Other],
) -> Choice {
    p.prev_strafe = false;
    p.jump_held &= a_held;
    if matches!(p.state, state::JUMP | state::RUN_JUMP) || p.jump_held {
        p.gravity = if p.jump_held && p.water_jump {
            f32::from_bits(0x4053_3333) // 3.3, out of the water (func_1505F298)
        } else if p.jump_held {
            GRAVITY_HELD
        } else {
            GRAVITY_JUMP
        };
    }
    let locked = p.lock != 0;
    if locked && p.lock < 0xFA {
        p.lock -= 1;
    }
    // obj+0x28, the height above the floor at the start of the frame.
    let height = p.pos[1] - p.floor_y;
    // The take-off frame still drives like the ground: obj+0x81 is set after the turn and the speed
    // (V: camera_jump row 234, -1792 at speed 35). A walk off an edge never sets it.
    let takeoff = p.air_frames == 0 && matches!(p.state, state::JUMP | state::RUN_JUMP);
    let jumped = p.jumped && !takeoff;
    // The high jump's rise: no drive, no speed (V q2s5w 118-141; I: the test, which the traces only
    // bracket between 7.99994 and 5.99994 after the frame's gravity).
    let rising = p.state == state::HIGH_JUMP && p.y_vel - p.gravity > 6.0;
    if rising {
        p.xz_speed = 0.0;
    }
    // The ground's turn block with the air's cap ([`turn_steps`]).
    let drive = if stick.on && !locked && !rising {
        scaled(p)
    } else {
        0.0
    };
    let mut drive = turn_toward(p, stick.target, drive, jumped, None);
    // The facing (func_150599C8 from func_1504CB98 at 0x1504ED60, rate 12: at most 3072, a clockwise
    // turn stops one short): toward the stick's own target, all 16 bits, once 50 or more above the floor
    // (0x1504EC64) with a drive, else toward the move angle; only while `turning` (C; V: camera_jump's
    // 126 calls).
    let to = if drive != 0.0 && height >= 50.0 {
        stick.target
    } else {
        p.move_angle
    };
    let behind = facing_gap(p.facing, to);
    if p.turning {
        p.facing = turn_facing(p.facing, to);
    }
    p.turning &= p.facing != p.move_angle;
    if behind >= 0x5B && p.xz_speed > 12.0 && !jumped {
        drive = 0.0;
    }
    // A released stick keeps a jump's speed, unless a lock zeroes the drive (`func_1504CB98`: obj+0x89
    // set, C; V m2 1602-1614, the jump off a mounted gun brakes 0.875 a frame) or the jump was out of
    // the water (V swim1 271-287: 0.175 a frame while obj+0xAE counts down, then 0.875).
    if stick.on || !jumped || locked || p.water_jump {
        p.xz_speed = accelerate_air(p.xz_speed, drive, jumped, behind, p.jump_out != 0);
    }
    if takeoff {
        // A standing jump below speed 9 after this frame's drive (func_1505F298, C).
        p.state = if p.xz_speed < 9.0 {
            state::JUMP
        } else {
            state::RUN_JUMP
        };
    }
    let (_, dist, angle) = root_or_speed(p);
    if p.drift != [0.0; 2] {
        crate::swim::drift_step(p, dist, angle, None);
    } else {
        let (c, s) = cos_sin(angle);
        let d = 0.5 * dist;
        p.pos[0] += d * c;
        p.pos[2] -= d * s;
    }
    integrate_y(p);
    push_apart(&mut p.pos, me, others);
    p.heading = heading_from_facing(p.facing);
    let pre = p.pos;
    p.y_vel = sweep_walls(p, world, old, p.y_vel);
    let pushed = push_angle(p, pre, world);
    p.walled = pushed.is_some();
    if pushed.is_some_and(|away| against(p.move_angle, away)) && p.xz_speed > WALL_SPEED {
        p.xz_speed = WALL_SPEED;
    }
    p.air_frames += 1;
    if p.state == state::HIGH_JUMP {
        // Set for the next frame, as the trace shows it (V q2s5w 124-127: 9).
        p.gravity = high_jump_gravity(p.air_frames);
    }
    // The standing-jump clip lasts 24 frames, the fall clip 15 (then the run clip plays in the air) (V).
    if p.state == state::JUMP && p.air_frames >= 25 {
        p.state = state::FALL;
        p.air_frames = 0;
    } else if p.state == state::FALL && !p.jump_held && p.air_frames >= FALL_FRAMES {
        p.state = state::RUN;
    }
    let prev = if pushed.is_some() {
        None
    } else {
        world.last_floor(old, p.floor_y, p.floor)
    };
    crate::swim::query(p, world, p.pos[1], old[1], prev);
    let Some(f) = p.floor.filter(|f| p.pos[1] <= f.y) else {
        // A walk off an edge more than 60 above the floor is a fall (obj+0x100 bit 0x10, set with state
        // 56 by func_1504C0E8): its landing is the hard one (C).
        p.hard_landing |= !p.jumped && p.pos[1] - p.floor_y > 60.0;
        p.aloft |= p.pos[1] - p.floor_y > 20.0;
        return Choice::None;
    };
    p.pos[1] = f.y;
    if std::mem::take(&mut p.aloft) && p.xz_speed < 9.0 {
        p.move_angle = p.facing;
    }
    p.airborne = false;
    p.turning = true;
    p.y_vel = GROUND_Y_VEL;
    p.gravity = GRAVITY_FALL;
    p.jump_held = false;
    p.jumped = false;
    p.water_jump = false;
    // A landing sets obj+0x89: the hard one's 5 below, else none (C; V swim1 498: the exit's 6 ends).
    p.lock = 0;
    // func_1505A9AC (C; V: ledge 425-437, anim_jump_stand 74-88, camera_jump 263): a fall lands hard
    // (13-frame hold, the first 5 locked), a jump with the stick released lands soft (16), and with the
    // stick held the gait goes on (a wall in front pushes straight back into state 122, V: wall_jump 271,
    // 327). Each hold counts this frame (12 and 15 rows).
    if p.hard_landing {
        (p.state, p.timer, p.lock, p.xz_speed) = (state::LAND, HARD_LANDING_FRAMES + 1, 5, 0.0);
    } else if p.target_speed == 0.0 {
        (p.state, p.timer, p.xz_speed) = (state::LAND, LANDING_FRAMES + 1, 0.0);
    } else {
        let wall = push_wall(p, pushed, false);
        return Choice::Gait { drive, wall };
    }
    p.hard_landing = false;
    Choice::None
}

/// [`accelerate`] in the air after a jump (func_1505A3A8 with obj+0x81 set, C): the brake is 0.35 of the
/// ground's (and 0.2 of that while `slow`), a facing 0x2E steps or more from where it heads halves the
/// acceleration, 0x5B or more the target.
fn accelerate_air(speed: f32, target: f32, jumped: bool, behind: u8, slow: bool) -> f32 {
    if !jumped {
        return accelerate(speed, target, 1.0);
    }
    let accel = if behind >= 0x2E { 0.5 } else { 1.0 };
    let target = if behind >= 0x5B { target * 0.5 } else { target };
    if target < speed {
        let mut brake = BRAKE * f32::from_bits(0x3EB3_3333); // 0.35
        if slow {
            // obj+0xAE, a jump out of the water's first 12 frames (0x800994CC).
            brake *= f32::from_bits(0x3E4C_CCCD); // 0.2
        }
        let s = speed - brake;
        if s < target { target } else { s }
    } else {
        let s = speed
            + if target - 1.0 < speed {
                accel * 0.01
            } else {
                accel
            };
        if target < s { target } else { s }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat() -> CollisionWorld {
        CollisionWorld::new([
            (
                [
                    [-5000., 0., -5000.],
                    [5000., 0., -5000.],
                    [-5000., 0., 5000.],
                ],
                5,
            ),
            (
                [[5000., 0., -5000.], [5000., 0., 5000.], [-5000., 0., 5000.]],
                5,
            ),
        ])
    }

    fn run(p: &mut PlayerState, input: Input, frames: usize, w: &CollisionWorld) {
        (0..frames).for_each(|_| step(p, &input, w));
    }

    const NORTH: u16 = 16896; // the oracle's spawn facing
    /// The spawn camera's stick yaw (2.81 degrees): forward targets 16895, one unit below the facing.
    const CAM: u16 = 511;

    #[test]
    fn deadzone_matches_the_game() {
        assert_eq!(
            [82, 41, 20, 9, 7, 0, -8, -82].map(deadzone),
            [75, 34, 13, 2, 0, 0, -1, -75]
        );
    }

    #[test]
    fn cos_sin_is_accurate() {
        for a in (0..=65535u32).step_by(97) {
            let (c, s) = cos_sin(a as u16);
            let r = a as f64 * std::f64::consts::TAU / 65536.0;
            assert!(
                (c as f64 - r.cos()).abs() < 2e-6 && (s as f64 - r.sin()).abs() < 2e-6,
                "{a}"
            );
        }
    }

    #[test]
    fn atan2_matches_f64() {
        for (y, x) in [
            (1.0f32, 0.0f32),
            (0.3, -2.0),
            (-5.0, -1.0),
            (-0.01, 7.0),
            (246.70264, -12.119629),
        ] {
            let want = (y as f64).atan2(x as f64).rem_euclid(std::f64::consts::TAU) * 65536.0
                / std::f64::consts::TAU;
            let got = atan2_u16(y, x) as f64;
            assert!(
                (got - want.floor()).abs() <= 1.0,
                "{y} {x}: {got} vs {want}"
            );
        }
        assert_eq!(angle_of(0.0, -1.0), 16384); // -z is a quarter turn
    }

    #[test]
    fn stick_angle_quarters() {
        assert_eq!(stick_angle8(75, 0), 0);
        assert_eq!(stick_angle8(0, 75), 64);
        assert_eq!(stick_angle8(-75, 0), 128);
        assert_eq!(stick_angle8(0, -75), 192);
        assert_eq!(stick_angle8(62, 62), 32);
    }

    #[test]
    fn target_angle_is_camera_relative() {
        let input = Input {
            camera_yaw: CAM,
            ..Default::default()
        };
        assert_eq!(target_angle(&input, 0, 75), 16895);
        assert_eq!(target_angle(&input, 75, 0), 511);
        // The move angle steps down a whole 256 to 16640 (the traced "−256").
        assert_eq!(turn_move(NORTH, target_angle(&input, 0, 75), 6400), 16640);
    }

    #[test]
    fn turning_caps_and_low_byte() {
        assert_eq!(turn_move(16896, 256, 6400), 10496);
        // Whole steps of the difference, floored: the low byte stays (move_release 149: 16641 for 16385).
        assert_eq!(turn_move(16641, 16385, 6400), 16385);
        assert_eq!(turn_move(16641, 16384, 6400), 16129);
        assert_eq!(turn_move(62977, 65025, 6400), 65025);
        assert_eq!(turn_move(16896, 16896 + 32768, 6400), 16896 - 6400); // reversal: clockwise
        // func_1504CB98's caps: 25 steps below speed 5, trunc((64 - speed) / 4), in the air
        // trunc((51 - speed) / 13) and none below y_vel -10.
        assert_eq!(
            [0.0, 4.0, 5.0, 8.23, 14.0, 35.0].map(|v| turn_steps(v, false, 0.0)),
            [25, 25, 14, 13, 12, 7]
        );
        assert_eq!(
            [(4.0, -9.0), (14.0, -9.0), (35.0, -9.0), (35.0, -10.5)]
                .map(|(v, y)| turn_steps(v, true, y)),
            [25, 2, 1, 0]
        );
        assert_eq!(turn_facing(16896, 16640), 16641);
        assert_eq!(turn_facing(16896, 256), 13824);
        assert_eq!(turn_facing(7425, 6656), 6657);
        assert_eq!(turn_facing(46082, 46337), 46337); // anim_jump_stand 379: counter-clockwise lands
        assert_eq!(turn_facing(23040, 24320), 24320); // camera_swing 210
        assert_eq!(turn_facing(16641, 16640), 16641); // one short of a unit: stays
        assert_eq!(turn_facing(10752, 4096), 7680); // anim_turn 56: capped at 3072
        assert_eq!(turn_facing(5889, 2817), 2818); // move_turnrun 261: exactly -3072 stops one short
        assert_eq!(facing_gap(10752, 4096), 25);
        assert_eq!(facing_gap(10752, 10751), 0);
    }

    #[test]
    fn acceleration_walk_creeps_by_hundredths() {
        let mut s = 0.0;
        for _ in 0..20 {
            s = accelerate(s, 20.4, 1.0);
        }
        assert_eq!(s, 20.0);
        assert!((accelerate(s, 20.4, 1.0) - 20.01).abs() < 1e-6);
        assert_eq!(accelerate(34.0, 35.0, 1.0), 35.0);
    }

    /// Holding a flag (V bots1): the drive is 0.8 of a full stick's 35, the speed gains 0.75 a frame
    /// up to 27.75, then 0.0075.
    #[test]
    fn holding_drives_at_four_fifths() {
        let p = PlayerState {
            target_speed: 35.0,
            holding: true,
            ..PlayerState::spawn([0.0; 3], 0)
        };
        let drive = scaled(&p);
        assert_eq!(drive, 28.0);
        let mut s = 0.0;
        for _ in 0..37 {
            s = accelerate(s, drive, 0.75);
        }
        assert_eq!(s, 27.75);
        assert!((accelerate(s, drive, 0.75) - 27.7575).abs() < 1e-5);
    }

    /// Heist's bag (V h1, bots1): 0.72 of the stick's 35, 0.75 a frame, easing in under 25.2.
    #[test]
    fn the_bag_drives_at_25_2() {
        let p = PlayerState {
            target_speed: 35.0,
            holding: true,
            heavy: true,
            ..PlayerState::spawn([0.0; 3], 0)
        };
        let drive = scaled(&p);
        assert!((drive - 25.2).abs() < 1e-4, "{drive}");
        let mut s = 0.0;
        for _ in 0..33 {
            s = accelerate(s, drive, 0.75);
        }
        assert_eq!(s, 24.75);
        assert!(accelerate(s, drive, 0.75) < drive);
    }

    #[test]
    fn steps_follow_the_floor_plane() {
        // The ramp north of the spawn rises 177 over 688 toward -z.
        let len = (688.0f32 * 688.0 + 177.0 * 177.0).sqrt();
        let n = [0.0, 688.0 / len, 177.0 / len];
        let factor = |c: f32, s: f32| 1.0 / (1.0 + rise_along(n, c, s).powi(2)).sqrt();
        assert!((factor(0.0, 1.0) - 688.0 / len).abs() < 1e-6); // straight up: cos of the slope
        assert!((rise_along(n, 0.0, 1.0) - 177.0 / 688.0).abs() < 1e-6);
        let (c, s) = cos_sin(16640); // run_a's heading: measured 0.968484 (rows 128-143)
        assert!((factor(c, s) - 0.968_482).abs() < 2e-6);
        assert_eq!(factor(1.0, 0.0), 1.0); // along the contour
    }

    #[test]
    fn state_choice_follows_func_1504c0e8() {
        fn pick(p: &mut PlayerState, speed: f32, drive: f32) -> u8 {
            (p.xz_speed, p.target_speed, p.timer, p.lock) = (speed, drive, 0, 0);
            choose(p, drive, false);
            p.state
        }
        let p = &mut PlayerState::spawn([0.0; 3], NORTH);
        // e = 0.4 speed + 0.6 drive: run above 14, walk above 6 or braking above speed 1, creep above 1.
        assert_eq!(pick(p, 4.0, 20.4), state::WALK);
        assert_eq!(pick(p, 5.0, 20.4), state::RUN);
        assert_eq!(pick(p, 10.8, 10.8), state::WALK);
        assert_eq!(pick(p, 3.0, 7.8), state::CREEP);
        assert_eq!(pick(p, 4.0, 7.8), state::WALK);
        assert_eq!(pick(p, 1.5, 0.0), state::WALK);
        assert_eq!(pick(p, 1.0, 0.0), state::IDLE);
        assert_eq!(pick(p, 17.9, 0.0), state::WALK); // a walk at 20.4 brakes to 17.9: no skid
        assert_eq!(pick(p, 18.5, 0.0), state::SKID); // above 18 it skids, the first 8 frames locked
        assert_eq!((p.timer, p.lock), (SKID_FRAMES, 8));
        // Scared: the scared run from e 5 (V combat5_sg_e2 766: speed 1, drive 35), the skid kept.
        p.scared = 20;
        assert_eq!(pick(p, 1.0, 35.0), state::SCARED_RUN);
        assert_eq!(pick(p, 4.0, 7.8), state::SCARED_RUN);
        assert_eq!(pick(p, 3.0, 7.8), state::SCARED_RUN); // e 5.88, a creep unscared
        assert_eq!(pick(p, 1.5, 6.0), state::CREEP); // e 4.2
        assert_eq!(pick(p, 26.0, 0.0), state::SKID);
        p.scared = 0;
        // Armed: no skid, and the stick's release stands at once with the move angle on the facing
        // (combat_e_w22 816).
        p.armed = 0x22;
        (p.move_angle, p.facing) = (25089, 25090);
        assert_eq!(pick(p, 15.0, 19.2), state::RUN);
        assert_eq!(p.speed_percent, 80);
        assert_eq!(pick(p, 12.5, 0.0), state::IDLE);
        assert_eq!(p.move_angle, 25090);
    }

    #[test]
    fn run_then_skid_to_idle() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0, 0.0, 0.0], NORTH);
        let fwd = Input {
            stick_y: 82,
            camera_yaw: CAM,
            ..Default::default()
        };
        run(&mut p, fwd, 40, &w);
        assert_eq!(
            (p.state, p.xz_speed, p.move_angle),
            (state::RUN, 35.0, 16640)
        );
        // 1 + 2 + ... + 35 then 5 frames at 35, half a speed unit per frame.
        let path = (1..=35).sum::<i32>() as f32 / 2.0 + 5.0 * 17.5;
        let dist = (p.pos[0] * p.pos[0] + p.pos[2] * p.pos[2]).sqrt();
        assert!(
            (dist - path).abs() < 0.01 && p.pos[2] < -400.0,
            "{:?}",
            p.pos
        );
        run(
            &mut p,
            Input {
                camera_yaw: CAM,
                ..Default::default()
            },
            1,
            &w,
        );
        assert_eq!((p.state, p.xz_speed, p.timer), (state::SKID, 32.5, 17));
        run(&mut p, Input::default(), 16, &w); // 17 rows of 33 in run_a (144-160), idle at 161
        assert_eq!((p.state, p.timer), (state::SKID, 1));
        run(&mut p, Input::default(), 1, &w);
        assert_eq!((p.state, p.xz_speed), (state::IDLE, 0.0));
    }

    #[test]
    fn walk_release_stops_without_moving_on_the_idle_frame() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        p.state = state::WALK;
        p.xz_speed = 2.9;
        run(&mut p, Input::default(), 1, &w);
        assert_eq!((p.state, p.pos), (state::IDLE, [0.0; 3]));
        assert!((p.xz_speed - 0.4).abs() < 1e-6);
    }

    #[test]
    fn jump_arc_matches_the_trace() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        let a = Input {
            buttons: BUTTON_A,
            ..Default::default()
        };
        run(&mut p, a, 1, &w);
        assert_eq!(p.state, state::JUMP);
        assert!((p.pos[1] - 28.4).abs() < 1e-4 && (p.y_vel - 55.0).abs() < 1e-4);
        run(&mut p, Input::default(), 1, &w); // A released: gravity 6.2
        assert!((p.y_vel - 48.8).abs() < 1e-4, "{}", p.y_vel);
        let mut frames = 2;
        while p.airborne {
            run(&mut p, Input::default(), 1, &w);
            frames += 1;
        }
        assert_eq!(
            (p.state, p.pos[1], p.y_vel),
            (state::LAND, 0.0, GROUND_Y_VEL)
        );
        assert_eq!(frames, 20); // anim_jump_stand: take-off at read 54, lands in row 74
    }

    /// Forward on the stick with the camera north of the player (forward = 16640).
    fn fwd(stick_y: i8) -> Input {
        Input {
            stick_y,
            camera_yaw: CAM,
            ..Default::default()
        }
    }

    #[test]
    fn soft_landing_holds_fifteen_frames_without_a_lock() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        run(
            &mut p,
            Input {
                buttons: BUTTON_A,
                ..Default::default()
            },
            1,
            &w,
        );
        while p.airborne {
            run(&mut p, Input::default(), 1, &w);
        }
        assert_eq!((p.state, p.timer, p.lock), (state::LAND, LANDING_FRAMES, 0));
        // The hold keeps the state; the stick drives at once (func_1505A9AC leaves obj+0x89 at 0, C).
        run(&mut p, fwd(82), 14, &w);
        assert_eq!((p.state, p.xz_speed), (state::LAND, 14.0));
        run(&mut p, fwd(82), 1, &w);
        assert_eq!((p.state, p.xz_speed), (state::RUN, 15.0));
    }

    #[test]
    fn crouch_slide_then_crouch_walk() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        let z = |stick_y: i8| Input {
            buttons: BUTTON_Z,
            ..fwd(stick_y)
        };
        run(&mut p, z(0), 1, &w);
        assert_eq!(p.state, state::CROUCH); // Z while still
        run(&mut p, z(82), 1, &w);
        assert_eq!((p.state, p.xz_speed), (state::CROUCH_WALK, 0.0)); // turns first (V: anim_crouch 114)
        run(&mut p, z(82), 20, &w);
        assert!((p.xz_speed - 13.07).abs() < 1e-4, "{}", p.xz_speed); // +1 to 13, then +0.01 (V)
        run(&mut p, z(41), 200, &w);
        assert!((p.xz_speed - 8.16).abs() < 1e-4, "{}", p.xz_speed); // half stick: 0.24 x 34 (V)
        // From a run: Z slides at the run's speed, then -3.2 per frame, crouch-walking below 5.
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        run(&mut p, fwd(82), 40, &w);
        run(&mut p, z(82), 1, &w);
        assert_eq!((p.state, p.xz_speed), (state::CROUCH, 35.0));
        run(&mut p, z(82), 10, &w);
        assert_eq!(p.state, state::CROUCH_WALK);
        run(&mut p, fwd(82), 1, &w); // Z released with the stick held: one walk frame
        assert_eq!(p.state, state::WALK);
    }

    #[test]
    fn reversed_stick_brakes_then_restarts() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        run(&mut p, fwd(41), 30, &w);
        let speed = p.xz_speed;
        run(&mut p, fwd(-82), 3, &w);
        assert_eq!(p.state, state::WALK);
        assert!((p.xz_speed - (speed - 3.0 * REVERSE_BRAKE)).abs() < 1e-4);
        let angle = p.move_angle;
        run(&mut p, fwd(-82), 1, &w);
        assert_eq!((p.xz_speed, p.state), (1.0, state::RUN));
        assert_eq!(angle_diff(p.move_angle, angle), -3072);
        // From a full run the same reversal skids: 8 locked frames brake 2.5, then 4.5 again.
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        run(&mut p, fwd(82), 40, &w);
        run(&mut p, fwd(-82), 1, &w);
        assert_eq!(
            (p.state, p.xz_speed, p.lock, p.timer),
            (state::SKID, 30.5, 8, SKID_FRAMES)
        );
        run(&mut p, fwd(-82), 9, &w);
        assert_eq!((p.state, p.xz_speed), (state::SKID, 6.0)); // anim_jump_stand 354-363
        run(&mut p, fwd(-82), 1, &w);
        assert_eq!((p.xz_speed, p.timer), (1.0, 3)); // the restart shortens the hold to 4
    }

    #[test]
    fn walking_off_an_edge_falls_then_lands_hard() {
        // A platform at y = 1000 ending at z = -50, flat ground at 0 beyond it.
        let w = CollisionWorld::new([
            (
                [
                    [-500., 1000., -50.],
                    [500., 1000., -50.],
                    [-500., 1000., 500.],
                ],
                5,
            ),
            (
                [
                    [500., 1000., -50.],
                    [500., 1000., 500.],
                    [-500., 1000., 500.],
                ],
                5,
            ),
            (
                [
                    [-5000., 0., -5000.],
                    [5000., 0., -5000.],
                    [-5000., 0., 5000.],
                ],
                5,
            ),
        ]);
        let mut p = PlayerState::spawn([0.0, 1000.0, 0.0], NORTH);
        let mut frames = 0;
        while !p.airborne {
            run(&mut p, fwd(82), 1, &w);
            frames += 1;
            assert!(frames < 30);
        }
        assert_eq!((p.state, p.y_vel), (state::FALL, -9.0));
        assert!((p.pos[1] - 995.5).abs() < 1e-3, "{}", p.pos[1]); // half the updated y_vel (V)
        run(&mut p, fwd(82), FALL_FRAMES as usize, &w);
        assert_eq!(p.state, state::RUN); // the fall clip ended, still airborne
        while p.airborne {
            run(&mut p, fwd(82), 1, &w);
        }
        assert_eq!(
            (p.state, p.timer, p.lock, p.xz_speed),
            (state::LAND, HARD_LANDING_FRAMES, 5, 0.0)
        );
        run(&mut p, fwd(82), 5, &w);
        assert_eq!(p.xz_speed, 0.0); // locked for 5 frames (anim_ledge 425-430)
        run(&mut p, fwd(82), 2, &w);
        assert_eq!((p.state, p.xz_speed), (state::LAND, 2.0)); // then accelerating while landing
        run(&mut p, fwd(82), 5, &w);
        assert_eq!(p.state, state::RUN);
    }

    #[test]
    fn air_control_stops_once_falling() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        (p.state, p.airborne, p.jumped, p.air_frames) = (state::RUN_JUMP, true, true, 5);
        (p.xz_speed, p.target_speed) = (35.0, 35.0);
        p.pos[1] = 500.0;
        p.y_vel = -9.0;
        let right = Input {
            stick_x: 82,
            camera_yaw: CAM,
            ..Default::default()
        };
        run(&mut p, right, 1, &w);
        assert_eq!(angle_diff(p.move_angle, NORTH), -256);
        run(&mut p, right, 1, &w); // y_vel is now -15
        assert_eq!(angle_diff(p.move_angle, NORTH), -256);
    }

    #[test]
    fn pushing_a_wall_slides_at_fifteen_and_caps_at_fourteen() {
        // The floor and a wall at x = 100 facing -x.
        let w = CollisionWorld::new([
            (
                [
                    [-5000., 0., -5000.],
                    [5000., 0., -5000.],
                    [-5000., 0., 5000.],
                ],
                5,
            ),
            (
                [[5000., 0., -5000.], [5000., 0., 5000.], [-5000., 0., 5000.]],
                5,
            ),
            (
                [
                    [100., -10., -5000.],
                    [100., -10., 5000.],
                    [100., 500., -5000.],
                ],
                5,
            ),
            (
                [
                    [100., 500., -5000.],
                    [100., -10., 5000.],
                    [100., 500., 5000.],
                ],
                5,
            ),
        ]);
        let mut p = PlayerState::spawn([0.0; 3], 1280);
        p.state = state::RUN;
        p.xz_speed = 35.0;
        p.move_angle = 1280; // 7°: into the wall (V: move_wall2)
        let into = Input {
            stick_y: 82,
            camera_yaw: 1280u16.wrapping_sub(16384), // forward = 1280
            ..Default::default()
        };
        run(&mut p, into, 10, &w);
        assert_eq!((p.state, p.xz_speed), (state::PUSH_WALL, WALL_SPEED));
        assert!((p.pos[0] - 70.0).abs() < 1e-4, "{:?}", p.pos); // 30 from the wall
        // The heading faces the wall: 0 (+x) is 90 degrees.
        assert_eq!(p.heading, 90.0);
        let z = p.pos[2];
        run(&mut p, into, 1, &w);
        let (_, s) = cos_sin(1280);
        assert!((z - p.pos[2] - 7.5 * s).abs() < 1e-3); // the slide uses the accelerated 15 (V: 0.918)
    }

    #[test]
    fn airborne_facing_follows_the_stick_above_fifty() {
        // camera_jump 145-147: a running jump, the stick forward, the camera's angle 285 → 281.
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], 16640);
        (p.state, p.airborne, p.xz_speed, p.y_vel) = (state::RUN_JUMP, true, 35.0, 30.0);
        (p.pos[1], p.facing, p.air_frames) = (100.0, 16641, 2);
        let fwd = |camera_yaw| Input {
            stick_y: 82,
            camera_yaw,
            ..Default::default()
        };
        step(&mut p, &fwd(285), &w);
        assert_eq!((p.move_angle, p.facing), (16640, 16669)); // toward 16669, the move angle keeps
        step(&mut p, &fwd(281), &w);
        assert_eq!(p.facing, 16666); // 16665: a clockwise turn stops one short
        // Below 50 the facing heads for the move angle; once there it stays for the rest of the jump.
        (p.pos[1], p.y_vel) = (140.0, -60.0);
        p.floor_y = 100.0;
        step(&mut p, &fwd(281), &w);
        assert_eq!(p.facing, 16641);
        (p.facing, p.pos[1], p.y_vel) = (16640, 40.0, 0.0);
        step(&mut p, &fwd(281), &w);
        assert!(!p.turning);
        (p.pos[1], p.floor_y) = (300.0, 0.0);
        step(&mut p, &fwd(281), &w);
        assert_eq!(p.facing, 16640);
    }

    #[test]
    fn aiming_reads_the_stick_centred() {
        // camera_aim 391-392: R while running brakes into a skid, the stick ignored.
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        let fwd = Input {
            stick_y: 82,
            camera_yaw: CAM,
            ..Default::default()
        };
        run(&mut p, fwd, 40, &w);
        assert_eq!((p.state, p.xz_speed), (state::RUN, 35.0));
        p.aiming = true;
        step(&mut p, &fwd, &w);
        assert_eq!(
            (p.state, p.xz_speed, p.target_speed),
            (state::SKID, 32.5, 0.0)
        );
    }

    #[test]
    fn c_left_strafes_and_lets_go_into_a_skid() {
        // camera_buttons 173-205.
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        let c_left = Input {
            buttons: BUTTON_C_LEFT,
            camera_yaw: CAM,
            ..Default::default()
        };
        let mut seen = Vec::new();
        for _ in 0..3 {
            step(&mut p, &c_left, &w);
            seen.push((p.move_angle, p.state, p.xz_speed));
        }
        assert_eq!(
            seen,
            [
                (23296, state::WALK, 1.0),
                (29696, state::RUN, 2.0),
                (33280, state::RUN, 3.0)
            ]
        );
        assert_eq!((p.facing, p.target_speed), (NORTH, STRAFE_TARGET));
        run(&mut p, c_left, 27, &w);
        assert!((p.xz_speed - 22.08).abs() < 1e-4); // +1 to 22, then +0.01 toward 22.1
        step(&mut p, &Input::default(), &w);
        assert_eq!((p.move_angle, p.state), (NORTH, state::SKID));
    }

    #[test]
    fn kind_0xa_slows_to_ninety_percent_and_sinks() {
        // wall_jump 470-472 (35 → 32.5 → 31.5) and the surface_a run (the sink steps to -6).
        let w = CollisionWorld::new([(
            [
                [-5000., 0., -5000.],
                [5000., 0., -5000.],
                [-5000., 0., 5000.],
            ],
            0xA,
        )]);
        let mut p = PlayerState::spawn([-1000.0, 0.0, 1000.0], NORTH);
        let fwd = Input {
            stick_y: 82,
            camera_yaw: CAM,
            ..Default::default()
        };
        step(&mut p, &Input::default(), &w); // finds the floor
        let mut sinks = Vec::new();
        for _ in 0..7 {
            step(&mut p, &Input::default(), &w);
            sinks.push(p.sink);
        }
        assert_eq!(sinks, [-1, -2, -3, -4, -5, -6, -6]);
        (p.xz_speed, p.state) = (35.0, state::RUN);
        step(&mut p, &fwd, &w);
        assert_eq!((p.xz_speed, p.target_speed), (32.5, 35.0));
        step(&mut p, &fwd, &w);
        assert_eq!(p.xz_speed, 31.5);
        assert_eq!(p.pos[1], 0.0); // the sink does not move the player
    }

    #[test]
    fn a_gun_aimed_twists_then_turns_the_facing() {
        // camera4_w09 281-312 (twin guns, R held): 750 a frame up to 9000, then the facing and move
        // angle turn by the rest; let go of R, the twist relaxes by a quarter a frame.
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], 10753);
        // The aim camera is on (since R's press): the movement reads the stick centred.
        (p.armed, p.aiming) = (0x09, true);
        let aim = |x| Input {
            stick_x: x,
            buttons: conker_camera::R,
            ..Default::default()
        };
        for _ in 0..12 {
            step(&mut p, &aim(82), &w);
        }
        assert_eq!((p.twist, p.facing), (9000, 10753));
        step(&mut p, &aim(82), &w);
        assert_eq!((p.twist, p.facing, p.move_angle), (9000, 10003, 10003));
        step(&mut p, &aim(-82), &w);
        assert_eq!((p.twist, p.facing), (8250, 10003));
        let mut relaxed = Vec::new();
        for _ in 0..3 {
            step(&mut p, &Input::default(), &w);
            relaxed.push(p.twist);
        }
        assert_eq!(relaxed, [6188, 4641, 3481]);
        p.twist = -23; // within 29: straight to 0 (camera4_w09 563)
        step(&mut p, &Input::default(), &w);
        assert_eq!(p.twist, 0);
    }

    #[test]
    fn the_scope_turns_the_facing_toward_the_look() {
        // camera4_w23 282-283: the facing takes trunc(171.477249 · 182.04445) = 31216, the move angle
        // steps a whole 256 toward it, and the facing turns back one short of the move angle.
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], 31233);
        (p.armed, p.scope_yaw) = (0x23, Some(171.477_25));
        step(&mut p, &Input::default(), &w);
        assert_eq!((p.move_angle, p.facing), (30977, 30978));
        assert_eq!((p.target_speed, p.timer), (SCOPE_DRIVE, 9)); // 0x23's scope holds the state
    }

    /// A wall 10 to the right of the pivot (an aim along +x) pulls the aim ray's start back to 2
    /// (`AIM_WALL_GAP` short); the left shoulder, clear, keeps its 20.
    #[test]
    fn a_wall_at_the_shoulder_pulls_the_aim_rays_start_in() {
        let big = 5000.0;
        let wall = CollisionWorld::new([
            (
                [[-big, -big, 10.0], [big, -big, 10.0], [-big, big, 10.0]],
                5,
            ),
            ([[big, -big, 10.0], [big, big, 10.0], [-big, big, 10.0]], 5),
        ]);
        let aim = |actions| Input {
            camera_yaw: 0xC000,
            actions: action::MODERN | actions,
            ..Input::default()
        };
        let (start, dir) = aim_ray([0.0; 3], &aim(0), &wall);
        assert_eq!(dir, [1.0, 0.0, 0.0]);
        assert!((start[2] - (10.0 - AIM_WALL_GAP)).abs() < 1e-4, "{start:?}");
        assert_eq!(start[1], AIM_PIVOT_HEIGHT);
        assert_eq!(aim_ray([0.0; 3], &aim(0), &flat()).0[2], AIM_SHOULDER);
        let left = aim_ray([0.0; 3], &aim(action::SHOULDER), &wall).0;
        assert_eq!(left[2], -AIM_SHOULDER);
    }

    #[test]
    fn the_modern_aim_strafes_at_three_quarters_and_the_scope_at_half() {
        // MODERN: R's aim no longer centres the stick; the aim holds the facing on camera_yaw + 0x4000
        // and a right stick strafes under it, at 0.75 of the run while R aims, 0.5 through the scope.
        let w = flat();
        let right = Input {
            stick_x: 82,
            buttons: conker_camera::R,
            camera_yaw: CAM,
            actions: action::MODERN,
            ..Default::default()
        };
        let (c, s) = cos_sin(aim_yaw(&right));
        for (armed, scope, share) in [(0x09, None, AIM_SHARE), (0x14, Some(0.0), SCOPE_SHARE)] {
            let mut p = PlayerState::spawn([0.0; 3], NORTH);
            (p.armed, p.aiming, p.scope_yaw) = (armed, true, scope);
            run(&mut p, right, 60, &w);
            assert_eq!(
                (p.facing, p.target_speed),
                (aim_yaw(&right), RUN_SPEED * share)
            );
            assert!(
                angle_diff(p.move_angle, CAM).abs() < 256,
                "{}",
                p.move_angle
            );
            assert!(
                (p.xz_speed - RUN_SPEED * share).abs() < 0.2,
                "{armed:#x}: {}",
                p.xz_speed
            );
            // Sideways: hardly anything along the aim.
            let along = p.pos[0] * c - p.pos[2] * s;
            assert!(along.abs() < 0.05 * p.pos[0].hypot(p.pos[2]), "{:?}", p.pos);
        }
        // Without MODERN, R's aim still centres the stick.
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        (p.armed, p.aiming) = (0x09, true);
        run(
            &mut p,
            Input {
                actions: 0,
                ..right
            },
            20,
            &w,
        );
        assert_eq!(p.xz_speed, 0.0);
    }

    #[test]
    fn modern_hip_fire_faces_the_aim_at_full_speed() {
        // FIRE held with a weapon drawn keeps the facing on the aim while the stick runs sideways at
        // the full run; let go, the facing turns back to the run.
        let w = flat();
        let fire = Input {
            stick_x: 82,
            camera_yaw: CAM,
            actions: action::MODERN | action::FIRE,
            ..Default::default()
        };
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        p.armed = 0x09;
        run(&mut p, fire, 60, &w);
        assert_eq!((p.facing, p.target_speed), (aim_yaw(&fire), RUN_SPEED));
        assert!((p.xz_speed - RUN_SPEED).abs() < 0.2, "{}", p.xz_speed);
        assert!(
            angle_diff(p.move_angle, CAM).abs() < 256,
            "{}",
            p.move_angle
        );
        let run_only = Input {
            actions: action::MODERN,
            ..fire
        };
        run(&mut p, run_only, 10, &w);
        assert!(
            angle_diff(p.facing, p.move_angle).abs() < 256,
            "{}",
            p.facing
        );
    }

    #[test]
    fn modern_crouches_and_high_jumps_armed_and_leaves_the_c_strafe() {
        // C2c: under MODERN the C buttons strafe no more (the stick does), and a drawn weapon
        // crouches on Z (the rules feed it from CROUCH) and high-jumps with A; without MODERN it does
        // neither.
        let w = flat();
        let c_left = Input {
            buttons: BUTTON_C_LEFT,
            camera_yaw: CAM,
            actions: action::MODERN,
            ..Default::default()
        };
        let mut p = PlayerState::spawn([0.0; 3], NORTH);
        p.armed = 0x09;
        run(&mut p, c_left, 10, &w);
        assert_eq!((p.pos, p.xz_speed), ([0.0; 3], 0.0));
        for actions in [action::MODERN, 0] {
            let z = Input {
                buttons: BUTTON_Z,
                camera_yaw: CAM,
                actions,
                ..Default::default()
            };
            let mut p = PlayerState::spawn([0.0; 3], NORTH);
            p.armed = 0x09;
            run(&mut p, z, 5, &w);
            let crouched = p.state == state::CROUCH;
            step(
                &mut p,
                &Input {
                    buttons: BUTTON_Z | BUTTON_A,
                    ..z
                },
                &w,
            );
            assert_eq!(
                (crouched, p.state == state::HIGH_JUMP),
                (actions != 0, actions != 0)
            );
        }
    }

    #[test]
    fn bodies_scale_as_the_oracles() {
        // obj+0xEC = radius / height in f32 (V: the Tedi's, the squirrel's, the raptor's RAM).
        let y = |b| Other::new([0.0; 3], b).y_scale.to_bits();
        assert_eq!(y([23, 45, 33]), 0x3F02_D82E);
        assert_eq!(y([30, 66, 33]), 0x3EE8_BA2F);
        assert_eq!(y([124, 203, 126]), 0x3F1C_5FD8);
    }

    #[test]
    fn overlapping_players_push_apart() {
        // Two player bodies 10 apart on x (radii 23 + 23): 0.4 of the 36 overlap along the line. The
        // push comes after the grounded frame's half step down (y − 4.5). Two squirrels (30 + 30):
        // 0.4 of 50.
        let w = flat();
        for (body, x) in [([23, 45, 33], -14.4), ([30, 66, 33], -20.0)] {
            let mut p = PlayerState::spawn([0.0; 3], NORTH);
            let other = Other::new([10.0, -4.5, 0.0], body);
            step_among(&mut p, &Input::default(), &w, other, &[other]);
            assert!((p.pos[0] - x).abs() < 1e-4, "{body:?}: {:?}", p.pos);
            assert_eq!(p.pos[2], 0.0);
        }
    }

    #[test]
    fn zones_turn_the_wall_routine_off() {
        // One type-1 trigger record of kind 0: a box 200 wide and 300 high around the origin.
        let mut r = [0u8; 0x34];
        r[6..12].copy_from_slice(&[0, 100, 1, 44, 0, 100]);
        r[0x15] = 1 << 2 | 2;
        let mut w = flat();
        w.camera = Some(conker_camera::volume::CameraLevel::new(0, &r, &[]));
        let mut p = PlayerState::spawn([0.0, 1.0, 0.0], NORTH);
        assert!(!walls_on(&p, &w));
        p.pos[0] = 150.0;
        assert!(walls_on(&p, &w));
        w.camera = None;
        p.pos[0] = 0.0;
        assert!(walls_on(&p, &w));
    }

    /// Z+A from a crouch (V q2za rows 51-79, `bots-fidelity-scratch`; the game's floor is at 17.03):
    /// 235 holds still 9 frames, launches at 76 (gravity 2, then 9 for 4 frames, then 2), rises
    /// 356 and lands hard.
    #[test]
    fn z_and_a_high_jump() {
        let w = flat();
        let mut p = PlayerState::spawn([0.0; 3], 50176);
        let z = Input {
            buttons: BUTTON_Z,
            ..Input::default()
        };
        run(&mut p, z, 10, &w);
        assert_eq!(p.state, state::CROUCH);
        let za = Input {
            buttons: BUTTON_Z | BUTTON_A,
            ..z
        };
        run(&mut p, za, 1, &w);
        assert_eq!((p.state, p.timer, p.lock), (state::HIGH_JUMP, 99, 0x17));
        let mut rows = Vec::new(); // reads 52..
        for _ in 0..80 {
            run(&mut p, z, 1, &w);
            rows.push((p.state, p.pos[1], p.y_vel));
        }
        let at = |read: usize| rows[read - 52];
        assert_eq!(at(59), (state::HIGH_JUMP, 0.0, GROUND_Y_VEL));
        assert_eq!(at(60), (state::HIGH_JUMP, 0.0, 76.0));
        for (read, y, vy) in [
            (61, 37.45, 74.0),
            (64, 141.875, 61.0),
            (67, 212.45, 34.0),
            (79, 343.8497, 10.0),
        ] {
            let (s, sy, svy) = at(read);
            assert!(s == state::HIGH_JUMP && (sy - y).abs() < 1e-3 && (svy - vy).abs() < 1e-3);
        }
        let top = rows.iter().map(|r| r.1).fold(0.0, f32::max);
        assert!((top - 356.1).abs() < 0.1, "{top}");
        let land = rows.iter().position(|r| r.0 == state::LAND).unwrap();
        assert!(
            rows[land..land + 12]
                .iter()
                .all(|r| r == &(state::LAND, 0.0, GROUND_Y_VEL))
        );
        assert_eq!(rows[land + 12].0, state::CROUCH);
    }

    /// The wall routine moving the player sets `walled` for that frame only.
    #[test]
    fn a_wall_slide_is_walled() {
        // A wall along x = 50 facing -x, 300 high.
        let w = CollisionWorld::new([
            (
                [
                    [-5000., 0., -5000.],
                    [5000., 0., -5000.],
                    [-5000., 0., 5000.],
                ],
                5,
            ),
            (
                [[5000., 0., -5000.], [5000., 0., 5000.], [-5000., 0., 5000.]],
                5,
            ),
            ([[50., 0., -500.], [50., 300., 500.], [50., 300., -500.]], 5),
            ([[50., 0., -500.], [50., 0., 500.], [50., 300., 500.]], 5),
        ]);
        let mut p = PlayerState::spawn([0.0; 3], 0);
        let east = Input {
            stick_y: 82,
            camera_yaw: 0xC000, // forward is +x
            ..Input::default()
        };
        let mut walled = Vec::new();
        for _ in 0..12 {
            run(&mut p, east, 1, &w);
            walled.push(p.walled);
        }
        assert!(!walled[0] && walled[11], "{walled:?} {:?}", p.pos);
        run(&mut p, Input::default(), 30, &w);
        assert!(!p.walled);
    }

    /// The jump onto a mounted gun ([`hop`]): b30's bot 4 came at 34.9 along the gun's yaw and
    /// brakes 2.5 a frame while it homes on the gun's origin (its seat, the oracle's gun undrawn);
    /// t1's player stood and homes on the seat, turning 3072 a frame. x and z as V's to 1e-3.
    #[test]
    fn hop_keeps_the_speed_and_homes() {
        let w = CollisionWorld::default();
        let mut p = PlayerState::spawn([-337.06015, -387.691406, -3703.08057], 47872);
        (p.y_vel, p.gravity, p.airborne) = (44.0, 3.2, true);
        (p.xz_speed, p.move_angle) = (34.8999329, 48128);
        let b30 = [
            (32.3999329, -335.31842, -3673.76709),
            (29.8999329, -333.827026, -3651.31128),
            (27.3999329, -332.535889, -3634.3418),
            (24.8999329, -331.404968, -3621.76123),
            (22.3999329, -330.402191, -3612.69214),
            (19.8999329, -329.501984, -3606.43213),
            (17.3999329, -328.683777, -3602.41943),
            (14.8999329, -327.931213, -3600.20435),
            (12.3999329, -327.23114, -3599.42725),
            (9.89993286, -326.573059, -3599.80103),
            (7.39993286, -325.948578, -3601.09521),
            (4.89993286, -325.350983, -3603.12549),
            (2.39993286, -324.774872, -3605.74512),
            (0.0, -324.21991, -3608.79614),
            (0.0, -323.77594, -3611.23682),
        ];
        for (k, (speed, x, z)) in b30.into_iter().enumerate() {
            hop(&mut p, [-322.0, -3621.0], &w);
            let e = (p.pos[0] - x).abs().max((p.pos[2] - z).abs());
            assert!(
                p.xz_speed == speed && e < 1e-3,
                "{k} {:?} {}",
                p.pos,
                p.xz_speed
            );
        }
        let mut p = PlayerState::spawn([-233.445694, -387.371094, -3621.0], 17152);
        (p.y_vel, p.gravity, p.airborne, p.move_angle) = (44.0, 3.2, true, 48128);
        let seat = [-315.828827, -3683.65723];
        let mut n = 0;
        for (t, facing, x, z) in [
            (132, 20224, -249.922318, -3633.53149),
            (133, 23296, -263.103607, -3643.55664),
            (134, 26368, -273.648651, -3651.57666),
            (141, 47872, -306.983002, -3676.92944),
            (142, 48128, -308.752167, -3678.2749),
            (155, 48128, -315.439819, -3683.36157),
        ] {
            while n < t - 131 {
                hop(&mut p, seat, &w);
                n += 1;
            }
            let e = (p.pos[0] - x).abs().max((p.pos[2] - z).abs());
            assert!(
                p.facing == facing && e < 1e-3,
                "{t} {:?} {}",
                p.pos,
                p.facing
            );
        }
    }
}
