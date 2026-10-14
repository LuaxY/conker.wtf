//! The original game's multiplayer third-person camera (camera mode 8, the follow camera, and R's aim
//! camera, mode 0x100), one tick at a time. No engine dependency: the simulation steps it (the stick is
//! camera-relative, [`CameraState::stick_yaw`]), the client renders from [`CameraState::view_eye`] to
//! [`CameraState::view_at`].
//!
//! Ported from the recompiled game (`func_15122C5C` and the functions it calls in modes 8 and 0x100;
//! names follow DevOldSchool/conkers-bfd-decomp, MIT, e2400b27dda6) and checked frame by frame against
//! the oracle traces (`tests/oracle.rs`: eye and look-at equal to the bit on all 12 camera traces and
//! the armed ones, the level's answers fed from the game). Per tick:
//! - the look-at point takes the player's x/z and springs its height toward the floor plus 0.75 of the
//!   head height (plus part of the height above the floor while airborne);
//! - the yaw is recomputed from the old eye to the new look-at (moving drags the camera round), then
//!   springs toward straight behind the player's heading: the swing-back that makes a held sideways
//!   stick run a circle;
//! - the distance from the look-at springs back to 247 and the eye height above the player (or above the
//!   floor under the camera, if higher) to 100; the eye stays 40 above the floor under it;
//! - C buttons held: the yaw first keeps its pre-move direction and only follows 75 % of the drag
//!   (`func_151220D0`, `func_15122980`), so strafing does not spin the camera;
//! - walls, outside the level's camera volumes ([`volume`]): a ray from above the player toward the eye
//!   shortens the distance (`func_15123A54`), the eye moves through the level like an object
//!   (`func_1512BB10`), both answered by a [`CameraWorld`];
//! - R (`func_1512317C`, `func_15120158`): zooms in to 90 behind at look-at height while the view rises
//!   25 (1 s whatever R does, [`Aim`]), then aims with the stick while R stays held, then springs back.
//!   With a weapon drawn ([`PlayerView::weapon`]) the distance and lift are the weapon's
//!   ([`aim_reach`]), the stick is slower, and a gun aims along the player's facing
//!   ([`aims_with_body`]); the sniper rifle and three others look through a first-person [`Scope`].
//!   R is refused (or an aim released) per [`PlayerView::refuses_aim`];
//! - an eye within 42 of the look-at turns the stick's yaw 90 degrees for a while
//!   ([`CameraState::stick_override`]);
//! - the level's camera hints ([`volume::CameraHint`]): one cuts behind the player at ledge read 410
//!   ([`CameraState::hint_cut`], set by the simulation from [`volume::CameraLevel::hint_cut`]).
//!
//! Not ported: ceilings (`cam+0x364` from action-2 volumes, none on scene 0x2D; the cut when the
//! floor under the eye is above `obj+0x17C`, which is about 20000 in every trace), 3-VI frames.

pub mod math;
pub mod volume;

use math::{DEG_TO_RAD, angle_diff, cos, sin, spring, spring_angle, wrap360, yaw_of};

/// Seconds per tick as the camera's springs see it: `cam+0x7B4` = VIs per frame (2, at the 30 Hz tick)
/// times `D_800A34D0` (1/60).
pub const DT: f32 = 2.0 * f32::from_bits(0x3C88_8889);
/// Horizontal eye distance and eye height above the player (zoom level 1 of `D_800A34B0`).
pub const DISTANCE: f32 = 247.0;
pub const HEIGHT: f32 = 100.0;
/// Head height of a standing player (`struct126+0x114`); the look-at sits at 0.75 of it.
pub const HEAD_STANDING: f32 = 90.0;
/// N64 button bits the camera reads.
pub const C_BUTTONS: u16 = 0x000F;
pub const C_DOWN: u16 = 0x0004;
pub const C_UP: u16 = 0x0008;
pub const R: u16 = 0x0010;
/// The unarmed aim camera (`func_151216F8`, `cam+0x23E` = 0): eye distance, look-at lift.
pub const AIM_DISTANCE: f32 = 90.0;
pub const AIM_LIFT: f32 = 25.0;
/// The flamethrower's weapon id (`struct126+0x78`).
pub const FLAMETHROWER: u8 = 0x15;

/// `func_151216F8`: the aim camera's eye distance and look-at lift for a drawn weapon (C; V on the
/// camera4 traces and the combat_ctlw ones): unarmed 90 and 25, the flamethrower 110 and 25, every
/// other Deathmatch weapon 75 and 5.
///
/// ponytail: the single-player ids (0x34, 0x3B, 2, 0xA, 0x13, 0x12, 0xF, 0x38) and their scene
/// checks are not ported; add them with a mode that draws those weapons.
pub fn aim_reach(weapon: u8) -> (f32, f32) {
    match weapon {
        0 => (AIM_DISTANCE, AIM_LIFT),
        FLAMETHROWER => (110.0, 25.0),
        _ => (75.0, 5.0),
    }
}

/// Weapons aimed by turning the player (the guns, the flamethrower, a grenade in hand): R's aim yaw
/// follows the facing less the upper body's twist, and the aim may start while stunned (the lists of
/// `func_1511FC60`, `func_15120158` and `func_1512317C`).
pub fn aims_with_body(weapon: u8) -> bool {
    matches!(
        weapon,
        0x09 | 0x15 | 0x26 | 0x37 | 0x38 | 0x39 | 0x3A | 0x3B
    )
}

/// Weapons R looks through in first person instead of the aim camera (`func_15126378`'s list; in
/// Deathmatch the sniper rifle 0x24, 0x23, 0x14 and 0x16): [`Scope`].
pub fn scoped(weapon: u8) -> bool {
    matches!(
        weapon,
        0x0C | 0x14 | 0x16 | 0x18 | 0x1B | 0x1F | 0x23 | 0x24 | 0x29 | 0x3F | 0x41
    )
}

/// Weapons the follow camera trails stiffer, behind the facing less the twist (`func_15121C80`'s
/// list: in multiplayer the twin guns and the pistol).
fn trails_aim(weapon: u8) -> bool {
    matches!(weapon, 0x09 | 0x12 | 0x37 | 0x38 | 0x39 | 0x3B)
}

/// VIs of the zoom-in before the stick aims (`cam+0x84A`), and the VIs per tick it counts down by.
const AIM_TIMER: i16 = 60;
const VIS: i16 = 2;

/// What the camera reads from its player each tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerView {
    /// Position (`obj+0x14`).
    pub pos: [f32; 3],
    /// Height of the floor under the player (`obj+0x180`); equals `pos[1]` on the ground.
    pub floor_y: f32,
    /// Heading in degrees (`obj+0x40`): [`heading_from_facing`] of the facing, except while pushing a
    /// wall (state 122), where the game turns it toward the wall (V: -11.08 degrees in the traces).
    pub heading: f32,
    /// Head height (`struct126+0x114`): 90 standing, 40 crouch-walking (state 27).
    pub head: f32,
    /// Strafing (`struct126+0x1B3`; I: C-left or C-right held): the swing-back spring stiffens.
    pub strafing: bool,
    /// R may not start the aim camera, an aim on is released and the scope closes, whatever the
    /// other inputs say ([`refuses_aim`](Self::refuses_aim)). The simulation sets it at health 0;
    /// [`dead`](Self::dead) and the other explicit inputs below say the same per condition.
    pub aim_blocked: bool,
    /// How far the player has sunk into the floor (`obj+0xCC`, negative: down; -6 on surface kind 0xA):
    /// the look-at height adds it (C, `func_15124C38`).
    pub sink: i16,
    /// The facing (`obj+0x7A`): R's aim with a gun follows it ([`aims_with_body`]).
    pub facing: u16,
    /// The drawn weapon's id (`struct126+0x78`, 0 for none; `0x26` a grenade in hand): the camera
    /// latches it as `cam+0x23E` ([`CameraState::weapon`]).
    pub weapon: u8,
    /// The upper body's twist while aiming a gun (`struct126+0x12`, u16 angle units, V: ±9000 at
    /// most, 750 per frame in the camera4 traces).
    pub twist: i16,
    /// Reeling from a hit (`obj+0x104` != 0): R may not start aiming, except with
    /// [`aims_with_body`] weapons (`func_1512317C`, C), nor look through a scope.
    pub stunned: bool,
    /// The drawn weapon is in the hand (`struct126+0x11A` = 2; V: 11 frames into 0x14's draw, 17
    /// into the sniper's): a [`scoped`] weapon's scope may open.
    pub in_hand: bool,
    /// Health 0 (`obj+0x1CA`): R is refused and the scope closes (C).
    pub dead: bool,
    /// Horizontal speed (`obj+0x3C`): R is refused above 15 while [`kind`](Self::kind) is 1 (C).
    pub speed: f32,
    /// `struct126+0x4E & 0xF` (C only: what it means is not known; 0 in every trace).
    pub kind: u8,
    /// `struct126+0x1AC` set: R is refused and the scope closes (C only: what it means is not known;
    /// 0 in every trace).
    pub busy: bool,
    /// `obj+0x65` set: the 1-based index of an object the player is attached to (C: the scope's
    /// release reads it as one; I: riding something). R is refused: the camera's `cam+0x5F0` bits
    /// 0x100 and 0x200 that would let it through stay clear in multiplayer (V camera4). 0 in every
    /// trace.
    pub attached: bool,
    /// On a mounted gun (weapon 0x1B, with [`attached`](Self::attached)): its origin (gun+0x2C). The
    /// scope's eye is then the gun's ([`gun_eye`]), turned with the player's facing (the gun's yaw).
    pub gun: Option<[f32; 3]>,
    /// Swimming (`obj+0xAD` = 1), with the water's height over the player (`obj+0x118`): the look-at
    /// and the eye go by the water, not the sea floor ([`CameraWorld::water_below`]).
    pub swim: Option<f32>,
}

impl PlayerView {
    /// A standing player facing `facing` (u16 angle, 65536 = 360 degrees).
    pub fn new(pos: [f32; 3], floor_y: f32, facing: u16) -> Self {
        Self {
            pos,
            floor_y,
            heading: heading_from_facing(facing),
            head: HEAD_STANDING,
            strafing: false,
            aim_blocked: false,
            sink: 0,
            facing,
            weapon: 0,
            twist: 0,
            stunned: false,
            in_hand: false,
            dead: false,
            speed: 0.0,
            kind: 0,
            busy: false,
            attached: false,
            gun: None,
            swim: None,
        }
    }

    /// `func_15125490` (C): swimming 100 or more off the surface (a dive; the multiplayer float stays
    /// within 50 of it).
    fn deep(&self) -> bool {
        self.swim
            .is_some_and(|w| (self.pos[1] - w).abs() as i32 >= 100)
    }

    /// `func_1512317C`'s test (C): R may not start the aim camera (and an aim on is released) while
    /// holding `weapon`. Not ported, because they are clear in every multiplayer trace (V camera4):
    /// the camera's own flags (`cam+0x2C & 0x80`, `cam+0x84 & 0x1000000`, `cam+0x5F0`), the
    /// airborne exception `struct126+0x9C`. A swimmer is not airborne, but a [`deep`](Self::deep) one
    /// is refused (`obj+0xAD`, `func_15125490`).
    pub fn refuses_aim(&self, weapon: u8) -> bool {
        let airborne = self.pos[1] != self.floor_y && self.swim.is_none();
        self.aim_blocked
            || self.deep()
            || self.dead
            || self.busy
            || self.attached
            || (airborne && weapon != 3)
            || (self.stunned && !aims_with_body(weapon))
            || (self.kind == 1 && 15.0 < self.speed)
    }
}

/// The mounted gun's weapon id (`struct126+0x78` while on one).
pub const GUN: u8 = 0x1B;

/// +0x2E4 for the gun scope's `pitch` (degrees, positive down): ×10 within ±180 (`func_150BDB70`, C).
pub fn gun_pitch10(pitch: f32) -> i32 {
    (((pitch + 180.0).rem_euclid(360.0) - 180.0) * 10.0) as i32
}

/// The mounted gun's tilt (degrees, nose down) for `pitch10`: the whole gun turns about its origin by
/// a sixteenth of the pitch as a game angle, floored to the sine table's 32 (V m2 1400-1520: the seat
/// stays 95.75 from the origin and turns by −32 units at pitch 3.9, −544 at 50 and 704 at −60, on the
/// read the pitch changes); the barrels' joint takes the rest (I: the eye then fits within 0.4).
pub fn gun_tilt(pitch10: i32) -> f32 {
    let units = (pitch10 as f32 * (65536.0 / 3600.0)) as i32 >> 4;
    (units & !31) as f32 * (360.0 / 65536.0)
}

/// The mounted gun's eye (`func_1512623C`: its joint 4's matrix × (0, 116, 130), C), for the gun at
/// `origin` turned `yaw` (u16 angle) with its barrels pitched `pitch` degrees (positive down): joint 4,
/// the barrels' pivot, sits 61.42 ahead and 52.265 up (the model's (0, 104.53, 122.84) at scale 0.5),
/// the eye (0, 58, 65) from it. V m2: level, origin + (126.42 ahead, 110.27 up), to the hundredth;
/// pitched, within 4 (I: the game's eye also lags the gun's turn a frame, its last matrix).
pub fn gun_eye(origin: [f32; 3], yaw: u16, pitch: f32) -> [f32; 3] {
    let t = gun_tilt(gun_pitch10(pitch));
    let (pc, ps) = (cos((pitch - t) * DEG_TO_RAD), sin((pitch - t) * DEG_TO_RAD));
    let (f, u) = (
        61.42 + 65.0 * pc + 58.0 * ps,
        52.265 + 58.0 * pc - 65.0 * ps,
    );
    let (tc, ts) = (cos(t * DEG_TO_RAD), sin(t * DEG_TO_RAD));
    let (ahead, up) = (f * tc + u * ts, u * tc - f * ts);
    let a = f32::from(yaw) * (360.0 / 65536.0) * DEG_TO_RAD;
    [
        origin[0] + ahead * cos(a),
        origin[1] + up,
        origin[2] - ahead * sin(a),
    ]
}

/// `obj+0x40` as the game derives it from the facing (V: equal to the bit on every traced frame
/// outside wall pushes).
pub fn heading_from_facing(facing: u16) -> f32 {
    f32::from(facing) * (360.0 / 65536.0) + 90.0
}

/// A hit of the game's line query against the level collision (`func_150AC9C0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// Distance from the ray's origin to `point`.
    pub dist: f32,
    pub point: [f32; 3],
    /// The hit triangle's vertices as the collision stores them (integers).
    pub tri: [[i16; 3]; 3],
}

/// The level queries of one camera tick, in the game's terms. The simulation answers them from its
/// collision; the oracle tests answer them with the game's own recorded results.
pub trait CameraWorld {
    /// The floor height under a point (`func_1510E950`, as for the player: the point's height is its
    /// current height, `eye_y_at_tick_start` the height at the start of the frame), None if there is
    /// none. The camera asks under the eye: with the floor it found last ([`CameraState::floor`],
    /// `cam+0x35C`) as the height (`func_1512A390`), or with the eye's own height after the sweep moved
    /// it (`func_1512BB10`).
    fn floor_below(&mut self, eye: [f32; 3], eye_y_at_tick_start: f32) -> Option<f32>;
    /// `func_150AC9C0` (hand-written MIPS, a walk of the collision grid): the nearest hit along `dir`
    /// from `origin`, up to `len` units; it normalizes `dir` itself. The camera passes a unit `dir`
    /// for its forward ray and the eye offset (length = the distance) for the reverse one.
    fn ray(&mut self, origin: [f32; 3], dir: [f32; 3], len: f32) -> Option<RayHit>;
    /// `func_15044380` moving the eye like an object (type 0x2D) from `from` to `to`, its collision
    /// size `[cam+0x95C, cam+0x960]` truncated to integers by `func_1507C3E0` (where the player has
    /// radius and height). None if it hit nothing (the eye reaches `to`), else where it stopped.
    fn sweep(&mut self, from: [f32; 3], to: [f32; 3], size: [f32; 2]) -> Option<[f32; 3]>;
    /// [`sweep`](Self::sweep) as the camera calls it: `long_step` is set on a cut or while aiming (the
    /// mover's 60-unit step cap is off, `D_800CBDD4`). Defaults to `sweep`.
    fn sweep_eye(
        &mut self,
        from: [f32; 3],
        to: [f32; 3],
        size: [f32; 2],
        long_step: bool,
    ) -> Option<[f32; 3]> {
        let _ = long_step;
        self.sweep(from, to, size)
    }
    /// `func_150A104C`: whether last tick's view eye ([`CameraState::view_eye`]) is inside the level's
    /// camera volumes (`cam+0x84` bit 0x40000): answer with [`volume::in_volume`] (or
    /// [`volume::CameraLevel`], which also gives the zones where the sweep skips the placements). Inside,
    /// the wall ray and the sweep are off.
    fn in_volume(&mut self, eye: [f32; 3]) -> bool;
    /// The water surface over a point (the floor query's `obj+0x118` output, here `cam+0x360` under
    /// the eye), None where there is none. Defaults to none.
    fn water_below(&mut self, at: [f32; 3]) -> Option<f32> {
        let _ = at;
        None
    }
}

/// [`CameraState::step`]'s world: a floor query only, the camera always inside a volume (no wall ray, no
/// sweep), as in wave 2.
struct FloorOnly<F>(F);

impl<F: FnMut([f32; 3], f32) -> Option<f32>> CameraWorld for FloorOnly<F> {
    fn floor_below(&mut self, eye: [f32; 3], start_y: f32) -> Option<f32> {
        (self.0)(eye, start_y)
    }
    fn ray(&mut self, _: [f32; 3], _: [f32; 3], _: f32) -> Option<RayHit> {
        None
    }
    fn sweep(&mut self, _: [f32; 3], _: [f32; 3], _: [f32; 2]) -> Option<[f32; 3]> {
        None
    }
    fn in_volume(&mut self, _: [f32; 3]) -> bool {
        true
    }
}

/// R's aim camera (camera mode 0x100, `func_15120158`): its state, mostly the block at
/// `cam+0x740`. Phases (`cam+0x73C`): 1 zooming in for 60 VIs whatever R does, 2 aiming with the stick
/// while R stays held, 3 the tick R is seen released (back to the follow camera's targets), 0 done: the
/// next tick returns to mode 8.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aim {
    /// Mode 0x100 is on (`cam+0x2C`).
    pub on: bool,
    pub phase: u8,
    /// VIs left of the zoom-in (`cam+0x84A`).
    pub timer: i16,
    /// The yaw it aims at and that yaw's spring velocity (`+0x34`, `+0x18`).
    pub yaw: f32,
    pub yaw_vel: f32,
    /// Pitch target, pitch and velocity (`+0x38`, `+0x3C`, `+0x1C`), degrees: the stick's y.
    pub pitch_target: f32,
    pub pitch: f32,
    pub pitch_vel: f32,
    /// Eye distance (`+0x44`), and the distance to the look-at below which (`+0x4E` = -1, `closer`) or
    /// above which the yaw starts turning toward [`yaw`](Self::yaw) while zooming in (`+0x48`).
    pub dist: f32,
    pub near: f32,
    pub closer: bool,
    /// `+0x4D`: zooming in turns the yaw.
    pub turn: bool,
    /// The follow camera's yaw, distance and height targets and base when R was pressed (`+0x20`,
    /// `+0x24`, `+0x28`, `+0x30`).
    pub saved: [f32; 4],
    /// The volume flag when R was pressed: the mode's pop restores it (`func_15123934` saves
    /// `cam+0x84`, `func_151239CC` restores it) for the rest of that tick.
    pub volume: bool,
}

/// The first-person scope (`func_15126378`, C; V on camera4 w23, w24, w14, w16 and the combat_ctlw
/// probes): while R is held with a [`scoped`] weapon in the hand, on the ground and not reeling, the
/// eye sits in the player's head (0.75 of its height) and looks along [`yaw`](Self::yaw) and
/// [`pitch`](Self::pitch), which the stick turns (faster the wider the view); C-up and C-down zoom.
/// The camera's yaw is the eye-to-look-at direction (`yaw - 90`, so the stick reads the look; V:
/// `cam+0x37C` on every row of the four scoped camera4 traces); the rest of the camera stands still
/// meanwhile, and letting go cuts back behind the player. The player brakes to a stop as in R's aim
/// (V camera4_w24 501-542: the stick reads centred) while the game turns its move angle toward the
/// look in 256-steps (a movement rule, not ported). Mostly `struct126+0x16C..0x197`, which the game keeps with
/// the player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scope {
    /// Looking through it (`struct126+0x197` = 2).
    pub on: bool,
    /// Look yaw and pitch in degrees (`+0x16C`, `+0x170`): the eye looks along
    /// `(cos yaw cos pitch, -sin pitch, -sin yaw cos pitch)`, so yaw = heading - 90 looks ahead. The
    /// stick moves their targets (`+0x174`, `+0x178`), springs follow (velocities `+0x17C`, `+0x180`).
    pub yaw: f32,
    pub pitch: f32,
    pub yaw_target: f32,
    pub pitch_target: f32,
    pub yaw_vel: f32,
    pub pitch_vel: f32,
    /// The zoom: degrees added to the view's horizontal and vertical angles (`+0x184`, `+0x188`, the
    /// view's `cam+0x19C`, `+0x1A0`; 0 unzoomed, -38.7 and -23.5 on opening, as far as -77.4 with the
    /// sniper rifle). Kept for the next scope with the same weapon.
    pub zoom: [f32; 2],
    /// The weapon of the last scope (`+0x190`), and whether the next one looks ahead again (`+0x194`:
    /// set by a new weapon or by the stick moving outside the scope).
    pub weapon: u8,
    pub reset: bool,
}

impl Scope {
    /// One frame of a bot's scope (`func_151279A0`, C), which has no camera: it opens level along
    /// `heading - 90`, then the AI's virtual stick (s126+0x85, +0x86) moves the targets 0.025 degrees a
    /// unit (stick right turns right, up looks down), the pitch within ±80, the springs as a human's.
    pub fn steer(&mut self, heading: f32, stick: [i8; 2]) {
        if !self.on {
            let yaw = wrap360(heading - 90.0);
            *self = Self {
                on: true,
                yaw,
                yaw_target: yaw,
                ..Self::default()
            };
        }
        let k = 0.025;
        self.yaw_target = wrap360(self.yaw_target - f32::from(stick[0]) * k);
        self.pitch_target = (self.pitch_target + f32::from(stick[1]) * k).clamp(-80.0, 80.0);
        spring_angle(
            &mut self.yaw,
            self.yaw_target,
            &mut self.yaw_vel,
            6.0,
            8.0,
            DT,
        );
        spring_angle(
            &mut self.pitch,
            self.pitch_target,
            &mut self.pitch_vel,
            6.0,
            8.0,
            DT,
        );
    }
}

/// How far weapon `weapon`'s scope zooms in, the least [`Scope::zoom`]`[0]` (`func_15126378`, C):
/// the sniper rifle 0x24 all the way to its limit `-(VIEW_ANGLES[0] - 5)`, 0x1B half way, the others
/// three quarters.
pub fn zoom_limit(weapon: u8) -> f32 {
    let far = -(VIEW_ANGLES[0] - 5.0);
    match weapon {
        0x24 => far,
        0x1B => far * 0.5,
        _ => far * 0.75,
    }
}

/// The view's angles the zoom narrows (`*0x800BE628 + 0x6C`, `+0x70`; I: horizontal and vertical
/// field of view in degrees).
pub const VIEW_ANGLES: [f32; 2] = [f32::from_bits(0x42A4_D4FE), 50.0];

/// The camera's whole state (field offsets in the game's camera struct). Plain data, so it can be
/// rolled back and replicated with the player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    /// Eye (`+0x2EC`) and look-at point (`+0x2E0`) as the camera logic moves them; the view adds
    /// [`look_offset`](Self::look_offset) ([`view_eye`](Self::view_eye), [`view_at`](Self::view_at)).
    pub eye: [f32; 3],
    pub at: [f32; 3],
    /// Yaw in degrees (`+0x37C`): the eye sits at `at + d (sin yaw, cos yaw)` horizontally.
    pub yaw: f32,
    /// Spring velocities: yaw (`+0x7C8`), distance (`+0x258`), eye height (`+0x25C`), look-at height
    /// (`+0x2A0`).
    pub yaw_vel: f32,
    pub dist_vel: f32,
    pub height_vel: f32,
    pub at_vel: f32,
    /// Floor under the eye (`+0x35C`) and the height the eye keeps 100 above (`+0x354`).
    pub floor: f32,
    pub base: f32,
    /// Share of the drag the yaw follows while C is held, and its velocity (`+0x8DC`, `+0x8D8`).
    pub strafe_blend: [f32; 2],
    /// Swing-back spring constants while strafing and their velocities (`+0x96C`, `+0x98C`, `+0x970`,
    /// `+0x990`).
    pub strafe_spring: [f32; 4],
    /// Player position last tick (`+0x2B0`).
    pub prev_pos: [f32; 3],
    /// The next step snaps behind the player, look-at included (`+0x23C` set before the tick: at
    /// spawn). The step clears it.
    pub cut: bool,
    /// A level camera hint cuts during the next step ([`volume::CameraLevel::hint_cut`], e.g. ledge
    /// read 410; `func_1512E8E0` → `func_1512FE00` → `func_15128774`): the eye jumps onto the player,
    /// then the same snap, after the look-at has sprung as usual (V). Ignored while R aims. Every step
    /// clears it.
    pub hint_cut: bool,
    /// The eye at the start of the last step (`+0x304`): where the sweep starts.
    pub prev_eye: [f32; 3],
    /// This tick's [`CameraWorld::in_volume`] (`+0x84` bit 0x40000).
    pub volume: bool,
    /// The wall ray's result (`+0x600`, `+0x604`): a wall shortened the distance to `wall_dist`. A hit
    /// also stiffens the next tick's swing-back spring.
    pub wall_hit: bool,
    pub wall_dist: f32,
    /// The sweep's collision size and its spring velocities (`+0x95C`, `+0x960`; `+0x954`, `+0x958`).
    pub sweep_size: [f32; 2],
    pub sweep_size_vel: [f32; 2],
    /// Horizontal eye distance and eye height above the base the springs aim at (`+0x374`, `+0x348`):
    /// 247 and 100 in the follow camera, set by the aim camera.
    pub dist_target: f32,
    pub height_target: f32,
    /// Height added to the eye and look-at for the view (`+0x18C`), its target (`+0x190`: 25 while
    /// aiming) and velocity (`+0x7D4`), sprung at the end of each tick (`func_1512C490`).
    pub look_offset: f32,
    pub look_offset_target: f32,
    pub look_offset_vel: f32,
    /// Last tick's buttons: R's press is an edge (`cam+0x36A`).
    pub held: u16,
    pub aim: Aim,
    /// The yaw the stick reads instead of [`yaw`](Self::yaw) after the eye came within 42 of the
    /// look-at (`+0x384`, [`NO_OVERRIDE`] when off), easing back to the yaw at the rate
    /// `override_blend` (`+0x5CC`) grows; `heading_rel` is the heading minus the yaw minus 180,
    /// in [0, 360) (`+0x390`), which picks the side.
    pub stick_override: f32,
    pub override_blend: f32,
    pub heading_rel: f32,
    /// The weapon the camera works with (`cam+0x23E`): [`PlayerView::weapon`], latched at the start
    /// of each step except while R's aim releases (phases 3 and 0).
    pub weapon: u8,
    pub scope: Scope,
}

/// [`CameraState::stick_override`] when the stick reads the yaw (`D_800991A8`).
pub const NO_OVERRIDE: f32 = 999.0;

impl Default for CameraState {
    fn default() -> Self {
        Self {
            eye: [0.0; 3],
            at: [0.0; 3],
            yaw: 0.0,
            yaw_vel: 0.0,
            dist_vel: 0.0,
            height_vel: 0.0,
            at_vel: 0.0,
            floor: 0.0,
            base: 0.0,
            strafe_blend: [0.0; 2],
            strafe_spring: [0.0; 4],
            prev_pos: [0.0; 3],
            cut: false,
            hint_cut: false,
            prev_eye: [0.0; 3],
            volume: false,
            wall_hit: false,
            wall_dist: 0.0,
            sweep_size: [0.0; 2],
            sweep_size_vel: [0.0; 2],
            dist_target: DISTANCE,
            height_target: HEIGHT,
            look_offset: 0.0,
            look_offset_target: 0.0,
            look_offset_vel: 0.0,
            held: 0,
            aim: Aim::default(),
            stick_override: NO_OVERRIDE,
            override_blend: 1.0,
            heading_rel: 0.0,
            weapon: 0,
            scope: Scope::default(),
        }
    }
}

impl CameraState {
    /// The camera on a player's first frame: the spawn record's camera eye looking at the player's
    /// position; the first [`step`](Self::step) cuts to 247 behind and 100 above (V: oracle reads 23-24).
    pub fn spawn(eye: [f32; 3], player: &PlayerView) -> Self {
        let at = player.pos;
        Self {
            eye,
            at,
            yaw: yaw_of(at[0] - eye[0], at[2] - eye[2]),
            floor: player.floor_y,
            base: player.pos[1],
            prev_pos: player.pos,
            cut: true,
            prev_eye: eye,
            ..Self::default()
        }
    }

    /// One tick without walls: [`step_with`](Self::step_with) on a world that only answers the floor
    /// query and keeps the camera inside a volume (no wall ray, no sweep). `floor_below(eye,
    /// eye_y_at_tick_start)` is [`CameraWorld::floor_below`].
    pub fn step(
        &mut self,
        p: &PlayerView,
        buttons: u16,
        floor_below: impl FnMut([f32; 3], f32) -> Option<f32>,
    ) {
        self.step_with(p, buttons, &mut FloorOnly(floor_below));
    }

    /// One tick (`func_15122C5C`) with the stick centred: [`step_pad`](Self::step_pad).
    pub fn step_with(&mut self, p: &PlayerView, buttons: u16, world: &mut impl CameraWorld) {
        self.step_pad(p, buttons, [0, 0], world);
    }

    /// One tick (`func_15122C5C`). `buttons` are the player's held N64 buttons, `stick` its stick after
    /// the deadzone (the `pad_x`/`pad_y` of the traces; only the aim camera reads it).
    ///
    /// Order within a game tick (V, oracle rows): the player's stick is resolved with the camera of the
    /// previous tick ([`stick_yaw`](Self::stick_yaw)), the player moves, then the camera steps with the
    /// moved player: the volume test on last tick's eye, the look-at and its height spring, R's mode
    /// switch, then the mode (8: the yaw swing-back, the floor under the eye, the distance spring with
    /// the wall ray, the eye height; 0x100: [`Aim`]), the eye sweep, the final yaw, the view offset.
    pub fn step_pad(
        &mut self,
        p: &PlayerView,
        buttons: u16,
        stick: [i8; 2],
        world: &mut impl CameraWorld,
    ) {
        let hint = std::mem::take(&mut self.hint_cut);
        if self.aim.phase != 3 && !(self.aim.on && self.aim.phase == 0) {
            self.weapon = p.weapon;
        }
        self.volume = world.in_volume(self.view_eye());
        self.prev_eye = self.eye;
        let (old_at, start_y) = (self.at, self.eye[1]);
        let pressed = buttons & !self.held;
        self.held = buttons;
        if self.scope_step(p, buttons, stick) {
            self.prev_pos = p.pos;
            self.spring_look_offset();
            return;
        }
        let moved = p.pos != self.prev_pos;
        self.prev_pos = p.pos;
        self.at[0] = p.pos[0];
        self.at[2] = p.pos[2];
        if self.cut {
            self.at[1] = self.look_at_height(p);
            self.at_vel = 0.0;
            return self.snap(p, start_y, world);
        }
        self.yaw = self.yaw_to(self.at);
        if buttons & C_BUTTONS != 0 {
            let (from_old, to_new) = (self.yaw_to(old_at), self.yaw);
            if buttons & C_DOWN == 0 {
                self.yaw = from_old;
            }
            let [b, bv] = &mut self.strafe_blend;
            spring(b, if moved { 0.75 } else { 0.0 }, bv, 1.0, 2.0, DT);
            self.yaw += angle_diff(from_old, to_new) * *b;
            self.place_eye(p, world);
            self.spring_height(p);
            self.find_floor(p, start_y, world);
        }
        let target = self.look_at_height(p);
        let (k, damp) = if self.aim.on { (5.0, 9.0) } else { (4.0, 8.0) };
        let (k, damp) = if target < old_at[1] {
            (k + k, damp + damp)
        } else {
            (k, damp)
        };
        spring(&mut self.at[1], target, &mut self.at_vel, k, damp, DT);
        // func_1512317C: R pressed starts the aim camera; it ends the tick after its phase 0. While R
        // is up or the player blocked, the rest of the tick reads R released.
        let mut buttons = buttons;
        if buttons & R != 0 && !p.refuses_aim(self.weapon) {
            if pressed & R != 0 && !self.aim.on {
                let (dx, dz) = (self.eye[0] - old_at[0], self.eye[2] - old_at[2]);
                self.start_aim(p, (dx * dx + dz * dz).sqrt());
            }
        } else {
            if self.aim.on && self.aim.phase == 0 {
                (self.aim.on, self.volume) = (false, self.aim.volume);
                (self.dist_target, self.height_target) = (DISTANCE, HEIGHT); // func_15124B18
            }
            buttons &= !R;
        }
        // func_1512E8E0 (skipped while R aims): a hint's cut (func_15128774) first puts the eye on the
        // player, its floor under it.
        if hint && !self.aim.on {
            (self.eye, self.prev_eye, self.floor) = (p.pos, p.pos, p.floor_y);
            self.cut = true;
            return self.snap(p, p.pos[1], world);
        }
        if self.aim.on {
            self.aim_step(p, buttons, stick, world);
        } else {
            self.follow(p, start_y, world);
        }
        self.yaw = self.yaw_to(self.at); // func_15125594, before func_1512C068
        self.near_eye(p, world);
        self.sweep(p, world);
        self.end_tick(p);
        self.spring_look_offset();
    }

    /// `func_1512C490`: the view offset springs, stiffer while aiming or armed.
    fn spring_look_offset(&mut self) {
        let (k, damp) = if self.aim.on || self.weapon != 0 {
            (4.0, 6.0)
        } else {
            (0.75, f32::from_bits(0x3F8C_CCCD)) // 1.1
        };
        let target = self.look_offset_target;
        spring(
            &mut self.look_offset,
            target,
            &mut self.look_offset_vel,
            k,
            damp,
            DT,
        );
    }

    /// `func_15126378` ([`Scope`]); true while the scope is the view this tick (the rest of the
    /// camera then stands still but the yaw). Letting go sets [`cut`](Self::cut) for the rest of the
    /// tick.
    fn scope_step(&mut self, p: &PlayerView, buttons: u16, stick: [i8; 2]) -> bool {
        const NEG_DEG: f32 = f32::from_bits(0xBC8E_FA35); // -pi/180
        let far = -(VIEW_ANGLES[0] - 5.0); // spBC: the zoom's limit
        let aspect = VIEW_ANGLES[1] / VIEW_ANGLES[0];
        let zoom_step = VIEW_ANGLES[0] * f32::from_bits(0x3CA3_D70A); // 0.02
        let w = self.weapon;
        let s = &mut self.scope;
        if !s.on && stick != [0, 0] {
            s.reset = true;
        }
        // The mounted gun's scope holds R itself and opens on the seat: its in-hand test is waived
        // and its ground test is the gun's (`obj+0x28 == 0 || attached+0x28 == 0`).
        let gun = w == GUN;
        let buttons = if gun { buttons | R } else { buttons };
        let grounded = p.pos[1] == p.floor_y;
        let in_hand = p.in_hand || matches!(w, GUN | 0x1F | 0x29);
        if !s.on
            && scoped(w)
            && buttons & R != 0
            && in_hand
            && (grounded || p.attached)
            && !p.stunned
        {
            // It ends R's aim camera (func_151239CC with slot 1).
            if self.aim.on {
                (self.volume, self.dist_target, self.height_target) =
                    (self.aim.volume, DISTANCE, HEIGHT);
            }
            (self.aim.on, self.aim.phase) = (false, 0);
            s.on = true;
            if w != s.weapon {
                let share = match w {
                    0x1B => 1.0,
                    0x16 => 0.75,
                    _ => 0.5,
                };
                (s.weapon, s.reset, s.zoom[0]) = (w, true, (0.0 - far) * share + far);
            }
            // The gun's scope always opens along the facing, level (V m2 84).
            if s.reset || gun {
                s.yaw_target = wrap360(p.heading - 90.0);
                (s.pitch_target, s.pitch, s.yaw_vel, s.pitch_vel) = (0.0, 0.0, 0.0, 0.0);
                (s.reset, s.yaw) = (false, s.yaw_target);
            }
            s.zoom[1] = s.zoom[0] * aspect;
        }
        if !s.on {
            return false;
        }
        // func_15127EB8: let go (or no longer able) cuts back to the follow camera.
        let lost = p.aim_blocked || p.dead || p.busy || (p.stunned && !p.attached && !grounded);
        if buttons & R == 0 || w == 0 || lost {
            (s.on, self.cut) = (false, true);
            return false;
        }
        let eye = match p.gun.filter(|_| gun) {
            Some(origin) => gun_eye(origin, p.facing, s.pitch),
            None => [p.pos[0], p.pos[1] + p.head * 0.75, p.pos[2]],
        };
        let open = (s.zoom[0] / far).clamp(0.0, 1.0);
        // The gun turns slower, a quarter (yaw) and half (pitch) of reach 19 to 31, at most 3 degrees a
        // frame, its pitch within [-60, 50] (C; V m2: full stick 3 and 3, h1: half 0.8006 and 1.6011).
        let (near, wide, pitches) = match gun {
            true => (19.0, 31.0, (-60.0, 50.0)),
            false => (25.0, 90.0, (-80.0, 80.0)),
        };
        let reach = 1.0 / ((wide - near) * open + near);
        let reach = reach * reach;
        let turn = |v: i8, share: f32| {
            let v = f32::from(v);
            let step = (if v < 0.0 { -1.0 } else { 1.0 }) * (v * v * reach);
            if gun {
                (step * share).clamp(-3.0, 3.0)
            } else {
                step
            }
        };
        s.yaw_target = wrap360(s.yaw_target - turn(stick[0], 0.25));
        s.pitch_target = (s.pitch_target + turn(stick[1], 0.5)).clamp(pitches.0, pitches.1);
        spring_angle(&mut s.yaw, s.yaw_target, &mut s.yaw_vel, 6.0, 8.0, DT);
        spring_angle(&mut s.pitch, s.pitch_target, &mut s.pitch_vel, 6.0, 8.0, DT);
        if w != 0x16 && w != 0x0C && buttons & 0xC != 0xC {
            if buttons & C_UP != 0 && far < s.zoom[0] {
                s.zoom[0] -= zoom_step;
                s.zoom[1] -= zoom_step * aspect;
            } else if buttons & C_DOWN != 0 && s.zoom[0] < 0.0 {
                s.zoom[0] += zoom_step;
                s.zoom[1] += zoom_step * aspect;
            }
        }
        let wide = zoom_limit(w);
        s.zoom = [
            s.zoom[0].clamp(wide, 0.0),
            s.zoom[1].clamp(wide * aspect, 0.0),
        ];
        let (pc, ps) = (cos(s.pitch * NEG_DEG), sin(s.pitch * NEG_DEG));
        let (yc, ys) = (cos(s.yaw * NEG_DEG) * pc, sin(s.yaw * NEG_DEG) * pc);
        self.eye = eye;
        self.at = [eye[0] + yc, eye[1] + ps, eye[2] + ys];
        self.yaw = self.yaw_to(self.at);
        true
    }

    /// Mode 8 after the look-at (`func_15121C80`): the swing-back, then the eye. With the twin guns or
    /// the pistol drawn it is stiffer and aims behind the facing less the twist (C).
    fn follow(&mut self, p: &PlayerView, start_y: f32, world: &mut impl CameraWorld) {
        // A wall hit on the last ray stiffens the swing-back.
        let (k, damp) = if p.strafing {
            let [k, kv, d, dv] = &mut self.strafe_spring;
            spring(k, 5.0, kv, 1.0, 2.0, DT);
            spring(d, 8.0, dv, 1.0, 2.0, DT);
            (*k, *d)
        } else if self.wall_hit {
            (4.0, 6.0)
        } else if trails_aim(self.weapon) {
            (2.0, 6.0)
        } else {
            (1.0, f32::from_bits(0x4033_3333)) // 2.8
        };
        let mut target = p.heading - 180.0;
        if !p.strafing && trails_aim(self.weapon) {
            target -= f32::from(p.twist) * ANGLE;
        }
        spring_angle(
            &mut self.yaw,
            wrap360(target),
            &mut self.yaw_vel,
            k,
            damp,
            DT,
        );
        self.find_floor(p, start_y, world);
        self.place_eye(p, world);
        self.spring_height(p);
    }

    /// The final yaw (`func_15125594`), the heading's side (`func_15123070`), the override
    /// (`func_1512C150`).
    fn end_tick(&mut self, p: &PlayerView) {
        self.yaw = self.yaw_to(self.at);
        let mut rel = p.heading - self.yaw - 180.0;
        while rel < 0.0 {
            rel += 360.0;
        }
        self.heading_rel = rel;
        self.ease_override();
    }

    /// `func_1512C068` (C; V on camera_aim): an eye within 42 of the look-at runs the distance spring
    /// (and the wall ray) once more, and turns the stick's yaw 90 degrees toward the heading's side.
    fn near_eye(&mut self, p: &PlayerView, world: &mut impl CameraWorld) {
        let side = if self.heading_rel < 180.0 { 2.0 } else { -2.0 };
        let (dx, dz) = (self.eye[0] - self.at[0], self.eye[2] - self.at[2]);
        if (dx * dx + dz * dz).sqrt() < 42.0 {
            self.override_blend = 0.0;
            self.stick_override = self.yaw - 45.0 * side;
            self.place_eye(p, world);
        }
        self.ease_override();
    }

    /// `func_1512C150`: the override eases back to the yaw, off once the blend reaches 1 (0.006 a call,
    /// two calls a tick).
    fn ease_override(&mut self) {
        let blend = self.override_blend + f32::from_bits(0x3BC4_9BA6); // 0.006
        self.override_blend = if 1.0 < blend { 1.0 } else { blend };
        if self.override_blend == 1.0 {
            self.stick_override = NO_OVERRIDE;
        } else {
            let o = self.stick_override;
            self.stick_override = o + angle_diff(o, self.yaw) * self.override_blend;
        }
    }

    /// `func_1511FC60` (C; V on camera_buttons, camera4): saves the follow camera, aims straight behind
    /// the heading (with a gun: along the facing less the twist), 110 then [`aim_reach`] away at
    /// look-at height (30 below it with a weapon not aimed by the body). `dist` is last tick's eye
    /// distance (`+0x370`).
    fn start_aim(&mut self, p: &PlayerView, dist: f32) {
        const NEAR: f32 = f32::from_bits(0x3F99_999A); // 1.2
        const FAR: f32 = f32::from_bits(0x3F2A_AA9F); // 0.666666
        let start = 110.0;
        let closer = start < dist;
        self.aim = Aim {
            on: true,
            phase: 1,
            timer: AIM_TIMER,
            yaw: wrap360(if aims_with_body(self.weapon) {
                body_yaw(p)
            } else {
                p.heading - 180.0
            }),
            dist: aim_reach(self.weapon).0,
            near: start * if closer { NEAR } else { FAR },
            closer,
            turn: true,
            saved: [self.yaw, self.dist_target, self.height_target, self.base],
            volume: self.volume,
            // Not reset: the springs keep last aim's velocities (V camera4_w09 447).
            yaw_vel: self.aim.yaw_vel,
            pitch_vel: self.aim.pitch_vel,
            ..Aim::default()
        };
        (self.floor, self.base) = (self.at[1], self.at[1]);
        let w = self.weapon;
        let height = if w == 0 || w == 0xC || aims_with_body(w) {
            60.0
        } else {
            30.0
        };
        (self.dist_target, self.height_target) = (start, height);
        self.look_offset_target = aim_reach(w).1;
    }

    /// `func_15120158`, mode 0x100 (C; V on camera_buttons, camera_aim and camera4). Armed, phase 2 turns
    /// slower (0.0128 per unit and VI) on stiffer springs that also lift the view; a weapon aimed by
    /// the body takes its yaw from the facing, the stick only pitching.
    fn aim_step(
        &mut self,
        p: &PlayerView,
        held: u16,
        stick: [i8; 2],
        world: &mut impl CameraWorld,
    ) {
        // Phase 2's stick (`D_800A33AC`, armed `D_800A33A8`) and springs (`0x4089999A`, `D_800A33D0`;
        // armed `0x407A3D71`, `D_800A33C0`).
        const SENSITIVITY: f32 = f32::from_bits(0x3CCC_CCCD); // 0.025
        const ARMED_SENSITIVITY: f32 = f32::from_bits(0x3C51_B717); // 0.0128
        const K: f32 = f32::from_bits(0x4089_999A); // 4.3
        const DAMP: f32 = f32::from_bits(0x40E9_999A); // 7.3
        const ARMED_K: f32 = f32::from_bits(0x407A_3D71); // 3.91
        const ARMED_DAMP: f32 = f32::from_bits(0x4103_5C29); // 8.21
        let (dx, dz) = (self.eye[0] - self.at[0], self.eye[2] - self.at[2]);
        let dist = (dx * dx + dz * dz).sqrt(); // +0x370
        let a = &mut self.aim;
        if a.phase != 3 {
            (a.dist, self.look_offset_target) = aim_reach(self.weapon); // func_151216F8
        }
        (self.floor, self.base) = (self.at[1], self.at[1]);
        match a.phase {
            1 => {
                a.timer -= VIS;
                if a.timer <= 0 {
                    (a.phase, a.yaw) = (2, self.yaw);
                    return;
                }
                let r = a.pitch * DEG_TO_RAD;
                self.dist_target = (cos(r) * a.dist) as i32 as f32;
                self.height_target = (sin(r) * a.dist) as i32 as f32;
                if !a.turn {
                    a.phase = 2;
                } else if if a.closer {
                    dist < a.near
                } else {
                    a.near < dist
                } {
                    let (k, damp) = if self.wall_hit {
                        (10.0, 18.0)
                    } else {
                        (5.0, 9.0)
                    };
                    spring_angle(&mut self.yaw, a.yaw, &mut a.yaw_vel, k, damp, DT);
                    let (o, t, v) = (
                        &mut self.look_offset,
                        self.look_offset_target,
                        &mut self.look_offset_vel,
                    );
                    spring(o, t, v, 5.0, 9.0, DT);
                }
            }
            2 => {
                let armed = self.weapon != 0;
                let speed = if armed {
                    ARMED_SENSITIVITY
                } else {
                    SENSITIVITY
                };
                let dt = f32::from(VIS);
                if aims_with_body(self.weapon) {
                    a.yaw = body_yaw(p);
                } else {
                    a.yaw -= f32::from(stick[0]) * speed * dt;
                }
                a.pitch_target += f32::from(stick[1]) * speed * dt;
                a.yaw = wrap360(a.yaw);
                a.pitch_target = a.pitch_target.clamp(-45.0, 80.0);
                let (k, damp) = if armed {
                    (ARMED_K, ARMED_DAMP)
                } else {
                    (K, DAMP)
                };
                spring_angle(&mut self.yaw, a.yaw, &mut a.yaw_vel, k, damp, DT);
                spring_angle(&mut a.pitch, a.pitch_target, &mut a.pitch_vel, k, damp, DT);
                if armed {
                    let (o, t, v) = (
                        &mut self.look_offset,
                        self.look_offset_target,
                        &mut self.look_offset_vel,
                    );
                    spring(o, t, v, ARMED_K, ARMED_DAMP, DT);
                }
                let r = a.pitch * DEG_TO_RAD;
                (self.dist_target, self.height_target) = (cos(r) * a.dist, sin(r) * a.dist);
                // func_151218C4: R released.
                if held & R == 0 {
                    (a.phase, a.timer) = (3, AIM_TIMER);
                    (self.dist_target, self.height_target) = (200.0, 100.0);
                    self.look_offset_target = 0.0;
                }
            }
            3 => {
                // cam+0x84 bit 0x200000, set on every multiplayer frame, clears `turn`: the follow
                // camera's targets and base come back at once and the phase ends (V: one tick).
                a.timer -= VIS;
                if a.timer <= 0 {
                    a.phase = 0;
                    return;
                }
                a.turn = false;
                [_, self.dist_target, self.height_target, self.base] = a.saved;
                (self.floor, a.phase) = (self.base, 0);
            }
            _ => {}
        }
        self.place_eye(p, world);
        self.spring_height(p);
    }
    /// `func_15124C38`: where the look-at height is heading: the floor, plus the height above it scaled
    /// by that height / 300 (capped at 1) while airborne, plus the sink and 0.75 of the head height, or
    /// all of it while the last wall ray hit (not on a cut; C, V on camera_wall). Aiming: 0.85 of it
    /// armed, 0.9 unarmed while looking down more than 15 degrees, plus 30 with a weapon aimed by the
    /// body (C; V camera4).
    fn look_at_height(&self, p: &PlayerView) -> f32 {
        let share = if self.aim.on {
            if self.weapon != 0 {
                f32::from_bits(0x3F59_999A) // 0.85
            } else if self.aim.pitch < -15.0 {
                f32::from_bits(0x3F66_6666) // 0.9
            } else {
                0.75
            }
        } else if self.wall_hit && !self.cut {
            1.0
        } else {
            0.75
        };
        let mut lift = f32::from(p.sink) + p.head * share;
        let w = self.weapon;
        if self.aim.on && (aims_with_body(w) || w == 0xC || w == 0x12) {
            lift += 30.0;
        }
        if p.swim.is_some() {
            // Off the swimmer's own height, a quarter of the lift when deep (C; V sea 150-325: 67.5).
            return p.pos[1] + if p.deep() { lift * 0.25 } else { lift };
        }
        let air = p.pos[1] - p.floor_y;
        let k = (air * f32::from_bits(0x3B5A_740E)).clamp(0.0, 1.0); // 1/300
        p.floor_y + air * k + lift
    }

    /// The direction from the eye to the look-at point as a u16 angle in the player's convention
    /// (65536 = 360 degrees, angle `a` points along `(cos a, -sin a)`), truncated. Wave 2's stick model
    /// used `yaw_u16() >> 8`; the game reads [`stick_yaw`](Self::stick_yaw) instead (it agrees except
    /// near a 256 boundary).
    pub fn yaw_u16(&self) -> u16 {
        (wrap360(self.yaw + 90.0) * (65536.0 / 360.0)) as u32 as u16
    }

    /// The camera angle the camera-relative stick reads (C, V): before each player's update,
    /// `func_1504A730` stores `trunc(cam+0x37C × 65536/360)` (f32 multiply by `D_800991AC`,
    /// `trunc.w.s`) at `0x800CC280`; the movement code (`func_1504CB98`) adds the stick's own angle
    /// (`func_1505A630`: 0 = right, 0x4000 = up, full u16 precision) to get the target move angle,
    /// and the move angle then turns by whole 256-steps, `((target - angle) as i16 >> 8) << 8`, so it
    /// keeps its low byte. Call it on the camera of the previous tick.
    ///
    /// While [`stick_override`](Self::stick_override) is on (the eye came within 42 of the look-at,
    /// e.g. aiming steeply), the game reads it instead (V on camera_aim's raw `cam+0x384`).
    pub fn stick_yaw(&self) -> u16 {
        let yaw = if self.stick_override == NO_OVERRIDE {
            self.yaw
        } else {
            self.stick_override
        };
        (yaw * f32::from_bits(0x4336_0B61)) as i32 as u16
    }

    fn yaw_to(&self, at: [f32; 3]) -> f32 {
        yaw_of(at[0] - self.eye[0], at[2] - self.eye[2])
    }

    /// `func_1512A390`: the floor under the eye (the player's floor if none), the height the eye keeps
    /// above (that floor or the player, whichever is higher), and 40 units of clearance. V: asking at
    /// the eye's height instead put ledge's closed-loop eye 135 off after the hint cut, the floor under
    /// the snapped eye being the plateau above the falling player.
    fn find_floor(&mut self, p: &PlayerView, start_y: f32, world: &mut impl CameraWorld) {
        // The game's query takes last tick's floor as the height, not the eye (`func_1510E8BC`).
        let at = [self.eye[0], self.floor, self.eye[2]];
        self.floor = world.floor_below(at, start_y).unwrap_or(p.floor_y);
        // The query's water under the eye (`cam+0x360`): a surface between that floor and the eye is
        // the floor (C; V sea 375-650: the eye 100 over the sea, −439, the player on the beach).
        let water = world.water_below(at);
        if let Some(w) = water.filter(|&w| self.floor < w && w < self.eye[1]) {
            self.floor = w;
        }
        let base = if self.floor < p.pos[1] {
            p.pos[1]
        } else {
            self.floor
        };
        self.base = self.floor + (base - self.floor);
        if let Some(w) = water.filter(|_| p.swim.is_some() && !p.deep()) {
            // A swimmer's eye keeps 100 over 50 above the surface (C; V sea 150-325: −389 over −539).
            // I: `cam+0x84` bit 0x4000 taken as set (a deep swimmer keeps the plain base).
            self.base = w + 50.0;
        }
        if self.eye[1] < self.floor + 40.0 {
            self.eye[1] = self.floor + 40.0;
        }
    }

    /// `func_15123A54`: the distance springs back to its target (or to a wall in the way, stiffer), the
    /// eye goes there along the yaw.
    fn place_eye(&mut self, p: &PlayerView, world: &mut impl CameraWorld) {
        let r = self.yaw * DEG_TO_RAD;
        let target = self.wall_ray(p, r, world);
        let (dx, dz) = (self.eye[0] - self.at[0], self.eye[2] - self.at[2]);
        let mut d = (dx * dx + dz * dz).sqrt();
        let (k, damp) = if self.wall_hit {
            (8.0, 10.0)
        } else if self.aim.on {
            (6.0, 10.0)
        } else {
            (6.0, 8.0)
        };
        spring(&mut d, target, &mut self.dist_vel, k, damp, DT);
        self.eye[0] = sin(r) * d + self.at[0];
        self.eye[2] = cos(r) * d + self.at[2];
    }

    /// `func_15123A54`'s wall ray (C; V on camera_wall and camera_buttons): a ray from 140 above the
    /// player along the yaw, 30 longer than the distance target. A hit closer than the target, on a
    /// triangle facing the ray (`dot > 0.325322`) and found again by the reverse ray from where the eye
    /// would be (hit points within 1 apart), or any such hit during a cut, lowers this tick's target to
    /// the hit minus 12 (16 while aiming), at least 50. Off while inside a volume, except during a cut
    /// or an aim phase. Returns the target.
    fn wall_ray(&mut self, p: &PlayerView, r: f32, world: &mut impl CameraWorld) -> f32 {
        const PI: f32 = f32::from_bits(0x4049_0FDB);
        const FACING: f32 = f32::from_bits(0x3EA6_909B);
        let target = self.dist_target;
        self.wall_hit = false;
        self.wall_dist = target;
        if self.volume && !self.cut && self.aim.phase == 0 {
            return target;
        }
        let (s, c) = (sin(r), cos(r));
        let origin = [p.pos[0], p.pos[1] + 140.0, p.pos[2]];
        let len = target + 30.0;
        let Some(hit) = world
            .ray(origin, [s, 0.0, c], len)
            .filter(|h| h.dist < target)
        else {
            return target;
        };
        let n = normal(hit.tri);
        let facing = s * n[0] + 0.0 * n[1] + c * n[2] > FACING; // false for NaN, as c.lt.s
        if !facing {
            return target;
        }
        let (ex, ez) = (sin(r + PI) * target, cos(r + PI) * target);
        let back = [p.pos[0] - ex, origin[1], p.pos[2] - ez];
        let Some(back_hit) = world.ray(back, [ex, 0.0, ez], len) else {
            return target;
        };
        let (dx, dz) = (
            back_hit.point[0] - hit.point[0],
            back_hit.point[2] - hit.point[2],
        );
        if !((dx * dx + dz * dz).sqrt() < 1.0 || self.cut) {
            return target;
        }
        let d = hit.dist - if self.aim.phase != 0 { 16.0 } else { 12.0 };
        let d = if d < 50.0 { 50.0 } else { d };
        (self.wall_hit, self.wall_dist) = (true, d);
        if d < target { d } else { target }
    }

    /// `func_1512E140`: the eye's height above the base springs to its target (100; 0 while aiming,
    /// stiffer); frozen while the player is more than 600 above its floor, unless swimming.
    fn spring_height(&mut self, p: &PlayerView) {
        if p.pos[1] - p.floor_y > 600.0 && p.swim.is_none() {
            return;
        }
        let mut h = self.eye[1] - self.base;
        let (k, damp) = if self.aim.on { (5.0, 9.0) } else { (6.0, 8.0) };
        spring(
            &mut h,
            self.height_target,
            &mut self.height_vel,
            k,
            damp,
            DT,
        );
        self.eye[1] = self.base + h;
    }

    /// `func_1512BB10` (C; V on camera_ledge): the eye moves from where it started the tick to where
    /// the camera put it, as an object of size 30×60 (20×20 while strafing, the size springing there),
    /// through the player's mover; when that collides, the eye stays 40 above the floor under it. Off
    /// while inside a volume.
    fn sweep(&mut self, p: &PlayerView, world: &mut impl CameraWorld) {
        if self.volume {
            return;
        }
        let target = if p.strafing {
            [20.0, 20.0]
        } else {
            [30.0, 60.0]
        };
        let sizes = self.sweep_size.iter_mut().zip(&mut self.sweep_size_vel);
        for ((size, v), t) in sizes.zip(target) {
            if self.cut {
                (*size, *v) = (t, 0.0);
            } else {
                spring(size, t, v, 4.0, 6.0, DT);
            }
        }
        let long_step = self.cut || self.aim.on;
        let Some(eye) = world.sweep_eye(self.prev_eye, self.eye, self.sweep_size, long_step) else {
            return;
        };
        self.eye = eye;
        if let Some(floor) = world.floor_below(eye, self.prev_eye[1])
            && self.eye[1] < floor + 40.0
        {
            self.eye[1] = floor + 40.0;
        }
    }

    /// The cut path of every function after the look-at: straight behind the heading (or closer,
    /// before a wall, the ray being on during a cut), at rest. `func_15122C5C` runs the mode three
    /// times on a cut tick (V: ledge 410's three equal rays), each from the floor under the eye so far;
    /// then the end of the frame, which it runs twice.
    fn snap(&mut self, p: &PlayerView, start_y: f32, world: &mut impl CameraWorld) {
        for _ in 0..3 {
            self.yaw = wrap360(p.heading - 180.0);
            self.find_floor(p, start_y, world);
            let r = self.yaw * DEG_TO_RAD;
            let d = self.wall_ray(p, r, world);
            self.eye = [
                sin(r) * d + self.at[0],
                self.base + self.height_target,
                cos(r) * d + self.at[2],
            ];
            self.yaw = self.yaw_to(self.at);
        }
        (self.yaw_vel, self.dist_vel, self.height_vel) = (0.0, 0.0, 0.0);
        for _ in 0..2 {
            self.near_eye(p, world);
            self.sweep(p, world);
        }
        (self.cut, self.hint_cut) = (false, false);
        self.end_tick(p);
        (self.look_offset, self.look_offset_vel) = (self.look_offset_target, 0.0);
    }

    /// The eye and look-at the view uses (`func_1512C490`): both raised by
    /// [`look_offset`](Self::look_offset). The traces' `cam_at` columns are [`view_at`](Self::view_at),
    /// their `cam_eye` columns [`eye`](Self::eye).
    /// Through the [`Scope`] the view is the eye and look-at themselves.
    pub fn view_eye(&self) -> [f32; 3] {
        [self.eye[0], self.eye[1] + self.view_lift(), self.eye[2]]
    }
    pub fn view_at(&self) -> [f32; 3] {
        [self.at[0], self.at[1] + self.view_lift(), self.at[2]]
    }
    fn view_lift(&self) -> f32 {
        if self.scope.on { 0.0 } else { self.look_offset }
    }
}

/// The unit normal of a collision triangle as `func_15123A54` computes it (`func_15048F90`,
/// `func_150AD8B0`, `func_1504917C`): `(v2 - v0) × (v1 - v0)`, scaled by 1 / its length (0 if degenerate).
fn normal(tri: [[i16; 3]; 3]) -> [f32; 3] {
    let v = tri.map(|v| v.map(f32::from));
    let e1: [f32; 3] = std::array::from_fn(|k| v[1][k] - v[0][k]);
    let e2: [f32; 3] = std::array::from_fn(|k| v[2][k] - v[0][k]);
    let n = [
        e2[1] * e1[2] - e2[2] * e1[1],
        e2[2] * e1[0] - e2[0] * e1[2],
        e2[0] * e1[1] - e2[1] * e1[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    let inv = if len == 0.0 { len } else { 1.0 / len };
    n.map(|x| x * inv)
}

/// `obj+0x7A`-style angle units to degrees (`0x3BB40000`, 360/65536).
const ANGLE: f32 = f32::from_bits(0x3BB4_0000);

/// The aim yaw of a weapon aimed by the body (`func_1511FC60`, `func_15120158`): the facing less the
/// twist, turned a quarter (the camera looks along it from behind).
fn body_yaw(p: &PlayerView) -> f32 {
    (i32::from(p.facing) - i32::from(p.twist) + 0xC000) as f32 * ANGLE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Player 1's Deathmatch spawn in the oracle (record 1 + 16 along its facing).
    fn spawn_player() -> PlayerView {
        PlayerView::new([-2267.0, -521.0, 6016.0], -521.0, 16896)
    }

    fn flat(_: [f32; 3], _: f32) -> Option<f32> {
        Some(-521.0)
    }

    fn settled() -> CameraState {
        let p = spawn_player();
        let mut cam = CameraState::spawn([-2328.0, -300.0, 6563.0], &p);
        cam.step(&p, 0, flat);
        cam
    }

    #[test]
    fn spawn_cuts_behind_the_player() {
        // Oracle read 24: eye (-2254.88037, -421, 6262.70264), look-at (-2267, -453.5, 6016).
        let cam = settled();
        assert_eq!(cam.at, [-2267.0, -453.5, 6016.0]);
        assert!(
            (cam.eye[0] - -2_254.880_4).abs() < 1e-3 && (cam.eye[2] - 6_262.702_6).abs() < 1e-3
        );
        assert_eq!(cam.eye[1], -421.0);
        assert!(!cam.cut);
        // 16895.99: one 256-step below the facing, the oracle's first push goes to 16640.
        assert_eq!(cam.yaw_u16() & 0xFF00, 16640);
        // The stick's camera angle: trunc(2.8124... × 182.04445) = 511, so forward targets 16895,
        // one unit below the facing 16896, and the move angle turns a whole step down to 16640.
        assert_eq!(cam.stick_yaw(), 511);
    }

    #[test]
    fn stick_yaw_truncates_the_scaled_yaw() {
        let cam = |yaw: f32| CameraState {
            yaw,
            ..CameraState::default()
        };
        assert_eq!(cam(0.0).stick_yaw(), 0);
        assert_eq!(cam(90.0).stick_yaw(), 16384);
        // move_release read 146: 1.41172028 x 182.04445 = 256.99 -> 256.
        assert_eq!(cam(1.411_720_3).stick_yaw(), 256);
        assert_eq!(cam(359.99).stick_yaw(), 65534);
    }

    #[test]
    fn standing_still_is_a_fixed_point() {
        let mut cam = settled();
        let before = cam;
        for _ in 0..30 {
            cam.step(&spawn_player(), 0, flat);
        }
        assert_eq!((cam.eye, cam.at), (before.eye, before.at));
    }

    #[test]
    fn swings_back_behind_a_turned_player() {
        let mut cam = settled();
        let mut p = spawn_player();
        p.heading = heading_from_facing(16896 + 8192); // turned 45 degrees left
        let mut prev = cam.yaw;
        for _ in 0..120 {
            cam.step(&p, 0, flat);
            assert!(cam.yaw >= prev - 1e-3, "monotone swing");
            prev = cam.yaw;
        }
        assert!(cam.yaw_u16().abs_diff(16896 + 8192) <= 1);
    }

    #[test]
    fn running_away_stretches_then_recovers_the_distance() {
        let mut cam = settled();
        let mut p = spawn_player();
        for _ in 0..30 {
            p.pos[2] -= 17.5; // full run north, straight away from the camera
            cam.step(&p, 0, flat);
        }
        let dist = |c: &CameraState| (c.eye[0] - c.at[0]).hypot(c.eye[2] - c.at[2]);
        assert!(
            dist(&cam) > 300.0 && dist(&cam) < 330.0,
            "trails behind: {}",
            dist(&cam)
        );
        for _ in 0..90 {
            cam.step(&p, 0, flat);
        }
        assert!((dist(&cam) - DISTANCE).abs() < 0.01);
    }

    #[test]
    fn jumping_lifts_the_look_at_by_a_share_of_the_air_height() {
        let mut p = spawn_player();
        p.pos[1] = -521.0 + 150.0;
        // Half the air height (150 / 300) counts: floor + 75 + 67.5.
        let cam = CameraState::default();
        assert_eq!(cam.look_at_height(&p), -521.0 + 75.0 + 67.5);
        p.pos[1] = -521.0 + 400.0;
        assert_eq!(cam.look_at_height(&p), -521.0 + 400.0 + 67.5);
        // After a wall hit the look-at aims at the whole head height.
        let walled = CameraState {
            wall_hit: true,
            ..cam
        };
        assert_eq!(walled.look_at_height(&p), -521.0 + 400.0 + 90.0);
    }

    #[test]
    fn a_sunk_player_lowers_the_look_at() {
        // Surface kind 0xA: 61.5 above the feet at rest instead of 67.5 (the oracle's surface_a run).
        let p = PlayerView {
            sink: -6,
            ..spawn_player()
        };
        assert_eq!(CameraState::default().look_at_height(&p), -521.0 + 61.5);
    }

    #[test]
    fn eye_keeps_clear_of_a_floor_rising_under_it() {
        let mut cam = settled();
        cam.step(&spawn_player(), 0, |_, _| Some(-421.0));
        // Pushed to the floor under it + 40 first, then the base is that floor (above the player).
        assert_eq!(cam.base, -421.0);
        assert!(cam.eye[1] >= -381.0);
        for _ in 0..200 {
            cam.step(&spawn_player(), 0, |_, _| Some(-421.0));
        }
        assert_eq!(cam.eye[1], -321.0);
    }

    #[test]
    fn strafing_keeps_the_view_direction() {
        let run = |buttons: u16| {
            let mut cam = settled();
            let mut p = spawn_player();
            p.strafing = buttons & 3 != 0;
            for _ in 0..20 {
                p.pos[0] -= 10.0; // sideways, as C-left does
                cam.step(&p, buttons, flat);
            }
            angle_diff(settled().yaw, cam.yaw).abs()
        };
        let (plain, strafe) = (run(0), run(0x0002));
        assert!(strafe < plain * 0.5, "strafe {strafe} vs plain {plain}");
    }

    fn dist(c: &CameraState) -> f32 {
        (c.eye[0] - c.at[0]).hypot(c.eye[2] - c.at[2])
    }

    #[test]
    fn an_r_tap_still_zooms_in_for_a_second() {
        let (mut cam, p) = (settled(), spawn_player());
        cam.step(&p, R, flat); // pressed for one tick only
        assert!(cam.aim.on && cam.aim.phase == 1 && cam.dist_target == 90.0);
        for _ in 0..28 {
            cam.step(&p, 0, flat);
        }
        assert_eq!(cam.aim.phase, 1);
        assert!((dist(&cam) - AIM_DISTANCE).abs() < 0.01, "{}", dist(&cam));
        assert!((cam.eye[1] - cam.at[1]).abs() < 0.01); // eye at look-at height
        assert!((cam.view_at()[1] - (cam.at[1] + AIM_LIFT)).abs() < 0.01);
        cam.step(&p, 0, flat); // 60 VIs: phase 2
        cam.step(&p, 0, flat); // R is seen released: phase 3, zooming out
        assert_eq!(
            (cam.aim.phase, cam.dist_target, cam.height_target),
            (3, 200.0, 100.0)
        );
        cam.step(&p, 0, flat);
        assert_eq!(
            (cam.aim.phase, cam.dist_target, cam.height_target),
            (0, DISTANCE, HEIGHT)
        );
        cam.step(&p, 0, flat);
        assert!(!cam.aim.on);
        for _ in 0..90 {
            cam.step(&p, 0, flat);
        }
        assert!((dist(&cam) - DISTANCE).abs() < 0.01 && cam.look_offset.abs() < 0.01);
    }

    #[test]
    fn r_is_refused_in_the_air_or_when_blocked() {
        let mut p = spawn_player();
        p.pos[1] += 10.0;
        let mut cam = settled();
        cam.step(&p, R, flat);
        assert!(!cam.aim.on);
        let mut p = spawn_player();
        p.aim_blocked = true;
        cam.step(&p, 0, flat);
        cam.step(&p, R, flat);
        assert!(!cam.aim.on);
        // Held from before is not a press.
        cam.step(&spawn_player(), R, flat);
        assert!(!cam.aim.on);
    }

    #[test]
    fn each_block_condition_refuses_r() {
        let with = |f: fn(&mut PlayerView)| {
            let mut q = spawn_player();
            f(&mut q);
            q
        };
        let refused = [
            (with(|q| q.dead = true), 0),
            (with(|q| q.busy = true), 0),
            (with(|q| q.attached = true), 0),
            (with(|q| (q.kind, q.speed) = (1, 15.5)), 0),
            (with(|q| q.stunned = true), 0x21),
            (with(|q| q.pos[1] += 10.0), 0x09), // airborne
        ];
        let allowed = [
            (spawn_player(), 0),
            (with(|q| (q.kind, q.speed) = (1, 15.0)), 0),
            (with(|q| (q.kind, q.speed) = (2, 35.0)), 0),
            (with(|q| q.stunned = true), 0x09), // a gun aims while reeling
            (with(|q| q.pos[1] += 10.0), 3),
        ];
        for (q, w) in refused {
            assert!(q.refuses_aim(w), "{q:?} {w:#x}");
            let mut cam = settled();
            cam.step(&PlayerView { weapon: w, ..q }, R, flat);
            assert!(!cam.aim.on, "{q:?} {w:#x}");
        }
        for (q, w) in allowed {
            assert!(!q.refuses_aim(w), "{q:?} {w:#x}");
        }
        // Dead or busy players lose the scope; attached ones keep it while reeling in the air.
        for (q, open) in [
            (
                PlayerView {
                    dead: true,
                    ..armed(0x24)
                },
                false,
            ),
            (
                PlayerView {
                    busy: true,
                    ..armed(0x24)
                },
                false,
            ),
            (armed(0x24), true),
        ] {
            let mut cam = settled();
            cam.step(&armed(0x24), R, flat);
            cam.step(&q, R, flat);
            assert_eq!(cam.scope.on, open, "{q:?}");
        }
    }

    #[test]
    fn aiming_turns_and_pitches_with_the_stick() {
        let (mut cam, p) = (settled(), spawn_player());
        for _ in 0..31 {
            cam.step(&p, R, flat);
        }
        assert_eq!(cam.aim.phase, 2);
        let yaw = cam.aim.yaw;
        cam.step_pad(&p, R, [75, 0], &mut FloorOnly(flat));
        assert_eq!(cam.aim.yaw, wrap360(yaw - 3.75)); // 75 x 0.025 x 2 VIs
        for _ in 0..40 {
            cam.step_pad(&p, R, [0, 75], &mut FloorOnly(flat));
        }
        assert_eq!(cam.aim.pitch_target, 80.0); // clamped
        assert!(cam.eye[1] - cam.base > 80.0 && dist(&cam) < 20.0); // high above, looking down
        // So close the stick reads 90 degrees off the view, easing back once the eye is away.
        assert_ne!(cam.stick_override, NO_OVERRIDE);
        assert_ne!(
            cam.stick_yaw(),
            CameraState {
                stick_override: NO_OVERRIDE,
                ..cam
            }
            .stick_yaw()
        );
        cam.step(&p, 0, flat); // released: back to the follow camera's targets
        for _ in 0..200 {
            cam.step(&p, 0, flat);
        }
        assert_eq!(cam.stick_override, NO_OVERRIDE);
        assert_eq!(cam.override_blend, 1.0);
    }

    #[test]
    fn leaving_aim_restores_the_volume_flag_of_the_press() {
        struct Volumes(bool);
        impl CameraWorld for Volumes {
            fn floor_below(&mut self, _: [f32; 3], _: f32) -> Option<f32> {
                Some(-521.0)
            }
            fn ray(&mut self, _: [f32; 3], _: [f32; 3], _: f32) -> Option<RayHit> {
                None
            }
            fn sweep(&mut self, _: [f32; 3], _: [f32; 3], _: [f32; 2]) -> Option<[f32; 3]> {
                None
            }
            fn in_volume(&mut self, _: [f32; 3]) -> bool {
                self.0
            }
        }
        let (mut cam, p) = (settled(), spawn_player());
        cam.step_pad(&p, R, [0, 0], &mut Volumes(false));
        let mut world = Volumes(true);
        while cam.aim.on {
            cam.step_pad(&p, 0, [0, 0], &mut world);
            if !cam.aim.on {
                assert!(!cam.volume); // the pop tick
            }
        }
        cam.step_pad(&p, 0, [0, 0], &mut world);
        assert!(cam.volume);
    }

    #[test]
    fn a_hint_cut_snaps_behind_the_player_unless_aiming() {
        let (mut cam, mut p) = (settled(), spawn_player());
        // Turned around and 100 down a drop, as off ledge's plateau at read 410.
        p.heading = heading_from_facing(16896 + 32768);
        (p.pos[1], p.floor_y) = (-621.0, -1000.0);
        cam.hint_cut = true;
        cam.step(&p, 0, |_, _| Some(-1000.0));
        assert!(!cam.hint_cut && !cam.cut);
        assert!((dist(&cam) - DISTANCE).abs() < 1e-3);
        assert!(angle_diff(cam.yaw, wrap360(p.heading - 180.0)).abs() < 1e-3);
        assert_eq!(cam.eye[1], -521.0); // 100 above the player, which is above its floor
        // While R aims the hint is dropped.
        let mut aiming = settled();
        aiming.step(&spawn_player(), R, flat);
        let mut hinted = aiming;
        hinted.hint_cut = true;
        aiming.step(&spawn_player(), R, flat);
        hinted.step(&spawn_player(), R, flat);
        assert_eq!(hinted, aiming);
    }

    /// The spawn player holding `weapon`, in hand.
    fn armed(weapon: u8) -> PlayerView {
        PlayerView {
            weapon,
            in_hand: true,
            ..spawn_player()
        }
    }

    /// Holds R `ticks` ticks with `p` and stick `stick`.
    fn aim(cam: &mut CameraState, p: &PlayerView, ticks: usize, stick: [i8; 2]) {
        for _ in 0..ticks {
            cam.step_pad(p, R, stick, &mut FloorOnly(flat));
        }
    }

    #[test]
    fn armed_aim_takes_the_weapons_reach_and_lift() {
        for (weapon, reach, lift, look) in [
            (0x09, 75.0, 5.0, 90.0 * 0.85 + 30.0), // the twin guns: aimed by the body
            (FLAMETHROWER, 110.0, 25.0, 90.0 * 0.85 + 30.0),
            (0x21, 75.0, 5.0, 90.0 * 0.85), // melee
        ] {
            let (mut cam, p) = (settled(), armed(weapon));
            aim(&mut cam, &p, 31, [0, 0]);
            assert_eq!((cam.aim.phase, cam.weapon), (2, weapon));
            assert!(
                (dist(&cam) - reach).abs() < 0.01,
                "{weapon:#x} {}",
                dist(&cam)
            );
            assert!((cam.look_offset - lift).abs() < 0.01, "{weapon:#x}");
            assert!((cam.at[1] - (-521.0 + look)).abs() < 0.01, "{weapon:#x}");
        }
    }

    #[test]
    fn a_gun_aims_along_the_facing_and_the_stick_turns_slower_armed() {
        let mut p = armed(0x09);
        let mut cam = settled();
        aim(&mut cam, &p, 31, [0, 0]);
        (p.facing, p.twist) = (20000, 9000);
        aim(&mut cam, &p, 1, [75, 0]);
        let want = (20000 - 9000 + 0xC000) as f32 * ANGLE;
        assert_eq!(cam.aim.yaw, wrap360(want)); // the stick only pitches
        let (mut cam, p) = (settled(), armed(0x21));
        aim(&mut cam, &p, 31, [0, 0]);
        let yaw = cam.aim.yaw;
        aim(&mut cam, &p, 1, [75, 0]);
        assert_eq!(
            cam.aim.yaw,
            wrap360(yaw - 75.0 * f32::from_bits(0x3C51_B717) * 2.0)
        );
    }

    #[test]
    fn stunned_players_aim_only_with_guns() {
        for (weapon, aims) in [(0, false), (0x21, false), (0x09, true)] {
            let (mut cam, p) = (
                settled(),
                PlayerView {
                    stunned: true,
                    ..armed(weapon)
                },
            );
            cam.step(&p, 0, flat);
            cam.step(&p, R, flat);
            assert_eq!(cam.aim.on, aims, "{weapon:#x}");
        }
    }

    #[test]
    fn the_twin_guns_trail_the_facing_stiffer() {
        let turned = |weapon| {
            let mut cam = settled();
            let p = PlayerView {
                heading: heading_from_facing(16896 + 8192),
                ..armed(weapon)
            };
            for _ in 0..10 {
                cam.step(&p, 0, flat);
            }
            angle_diff(cam.yaw, wrap360(p.heading - 180.0)).abs()
        };
        assert!(
            turned(0x09) < turned(0x21) * 0.8,
            "{} {}",
            turned(0x09),
            turned(0x21)
        );
    }

    #[test]
    fn the_scope_looks_from_the_head_and_cuts_back_on_release() {
        let p = armed(0x24);
        let mut cam = settled();
        let behind = cam.eye;
        cam.step(&p, R, flat);
        assert!(cam.scope.on);
        assert_eq!(cam.eye, [p.pos[0], p.pos[1] + 67.5, p.pos[2]]);
        assert_eq!(cam.view_at(), cam.at); // no view lift through the scope
        assert!(angle_diff(cam.yaw, wrap360(cam.scope.yaw - 90.0)).abs() < 1e-3); // the stick's yaw
        let look = [cam.at[0] - cam.eye[0], cam.at[2] - cam.eye[2]];
        // Ahead: the facing's direction (cos a, -sin a).
        let a = f32::from(p.facing) * (std::f32::consts::TAU / 65536.0);
        assert!((look[0] - a.cos()).abs() < 1e-3 && (look[1] + a.sin()).abs() < 1e-3);
        let yaw = cam.scope.yaw_target;
        aim(&mut cam, &p, 1, [75, 0]);
        // Half zoomed: the stick's square over 57.5 squared, per tick.
        assert_eq!(
            cam.scope.yaw_target,
            wrap360(yaw - 75.0 * 75.0 * (1.0 / 57.5) * (1.0 / 57.5))
        );
        cam.step(&p, R | C_UP, flat);
        assert!(cam.scope.zoom[0] < -38.7);
        cam.step(&p, 0, flat); // let go: cut back behind the player
        assert!(!cam.scope.on && !cam.cut);
        assert!((cam.eye[0] - behind[0]).abs() < 1e-3 && (cam.eye[2] - behind[2]).abs() < 1e-3);
        // Not in the hand yet: R aims as usual.
        let mut cam = settled();
        cam.step(
            &PlayerView {
                in_hand: false,
                ..p
            },
            R,
            flat,
        );
        assert!(!cam.scope.on && cam.aim.on);
    }

    /// The mounted gun's scope (V m2): open without R or the hand, level along the facing, from the
    /// gun's eye; full stick turns 3° a frame either way, the yaw all the way round, the pitch
    /// within [-60, 50]; half stick (34) 0.8006 and 1.6011 (V h1).
    #[test]
    fn the_gun_scope_opens_on_the_seat_and_clamps_its_pitch() {
        let p = PlayerView {
            weapon: GUN,
            in_hand: false,
            attached: true,
            gun: Some([170.0, 544.0, 5938.0]),
            facing: 15872,
            heading: heading_from_facing(15872),
            ..spawn_player()
        };
        let mut cam = settled();
        cam.step_pad(&p, 0, [0, 0], &mut FloorOnly(flat));
        assert!(cam.scope.on && cam.scope.pitch == 0.0);
        assert_eq!(cam.eye, gun_eye([170.0, 544.0, 5938.0], 15872, 0.0));
        let yaw = cam.scope.yaw_target;
        cam.step_pad(&p, 0, [34, 34], &mut FloorOnly(flat));
        assert!((wrap360(yaw - cam.scope.yaw_target) - 0.8006).abs() < 1e-3);
        assert!((cam.scope.pitch_target - 1.6011).abs() < 1e-3);
        for _ in 0..200 {
            cam.step_pad(&p, 0, [75, 75], &mut FloorOnly(flat));
        }
        assert_eq!(cam.scope.pitch_target, 50.0);
        for _ in 0..200 {
            cam.step_pad(&p, 0, [75, -75], &mut FloorOnly(flat));
        }
        assert_eq!(cam.scope.pitch_target, -60.0);
        // 400 frames at 3° (less the half-stick frame's 0.8): round and round.
        assert!((wrap360(yaw - cam.scope.yaw_target) - wrap360(0.8006 + 1200.0)).abs() < 0.01);
    }

    #[test]
    fn high_falls_freeze_the_eye_height() {
        let mut cam = settled();
        let mut p = spawn_player();
        p.floor_y = -1500.0;
        p.pos[1] = -800.0; // 700 above its floor
        let y = cam.eye[1];
        cam.step(&p, 0, |_, _| None);
        // The floor clamp still applies (none here: falls back to the player's floor).
        assert_eq!(cam.eye[1], y);
        assert_eq!(cam.floor, -1500.0);
    }
}
