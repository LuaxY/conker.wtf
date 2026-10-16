//! The modern controls' camera and input (docs/CONTROLS_RESEARCH.md §3, §5; lane C2b): an over-the-shoulder
//! orbit camera that the mouse and the pad's right stick turn every frame ([`look`]), and the input
//! that goes out with [`action::MODERN`] ([`input`]): `camera_yaw` = the aim's facing − 0x4000, the
//! aim's pitch, the actions from the bindings.
//!
//! The camera mirrors conker-sim's aim arm exactly ([`arm`]): it pivots `AIM_PIVOT_HEIGHT` above the
//! feet, sits `AIM_ARM` behind along the look and `AIM_SHOULDER` to the right (left with the
//! shoulder swap), and a wall only slides it along its look line, so the server rebuilds the aim ray
//! from the input alone. Aiming narrows the view ([`AIM_ZOOM`]); through a scope the view is the
//! scope's eye looking along the aim, its zoom the N64 scope's (the wheel steps it). The N64 follow
//! camera keeps running underneath (its aim gate and scope are the rules'); Tank, Race and Raptor
//! keep their own cameras and the N64 input. On a mounted gun the look turns the gun (conker-rules'
//! `modern_aim` turns it from the input's aim) and the view is its sight's eye along the aim
//! ([`gun_eye`]), its pitch within the gun's ([`GUN_PITCH`]).

use crate::bindings::{Action, Bindings, Controls, Devices};
use crate::outro::team_mode;
use crate::settings::{self, Mode, Settings};
use crate::{BUTTON_B, BUTTON_R, C_DOWN, C_UP, FollowCamera, STICK_MAX, STICK_WALK};
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use conker_rules::net::Fighting;
use conker_rules::projectile::raycast;
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::state::{CROUCH, CROUCH_WALK};
use conker_sim::movement::{
    AIM_ARM, AIM_PITCH_MAX, AIM_PIVOT_HEIGHT, Input, action, aim_reach, cos_sin,
};
use conker_sim::net::{Collision, Combat, MatchView, Player};

/// Degrees a mouse count turns at sensitivity 1 (CS's `m_yaw`, `m_pitch`).
pub const MOUSE_DEGREES: f32 = 0.022;
/// Game angle units a degree.
pub const UNITS: f32 = 65536.0 / 360.0;
/// The pad's look at full deflection and sensitivity 1: degrees a second, yaw and pitch.
pub const PAD_RATE: [f32; 2] = [450.0, 270.0];
/// The look stick's outer zone (past it, full deflection) and response curve exponent.
pub const OUTER: f32 = 0.95;
pub const CURVE: f32 = 2.0;
/// Aiming narrows the view by this much (the tangent of its half angle divided by it).
pub const AIM_ZOOM: f32 = 1.5;
/// The zoom's blend time constant, seconds.
const ZOOM_TIME: f32 = 0.06;
/// The shoulder swap's ease across, a time constant (seconds): the drawn eye only; the ray (the
/// server's) swaps at once.
pub const SWAP_TIME: f32 = 0.05;
/// How far short of a wall the arm stops (the view's near plane is 4).
const ARM_MARGIN: f32 = 8.0;
/// The pad's shared Reload / Interact button (X, §7): let go sooner, a Reload; held this long
/// (seconds), Interact while held (CoD's).
pub const HOLD: f32 = 0.25;
/// The look stick's acceleration ramp: held at its edge this long (seconds), its speed ramps over
/// [`RAMP_TIME`] up to [`RAMP_GAIN`] times.
pub const RAMP_DELAY: f32 = 0.3;
pub const RAMP_TIME: f32 = 0.25;
pub const RAMP_GAIN: f32 = 1.75;
/// The pad's look over an enemy's body (aim assist: a slowdown only, never for the mouse), and
/// how far it looks for one (the crosshair's wall range).
pub const ASSIST: f32 = 0.4;
const ASSIST_RANGE: f32 = 20_000.0;
/// On a mounted gun the aim's pitch (up positive) stays within the gun's: conker-rules' `modern_aim`
/// holds the gun's scope pitch (positive down) within the N64 gun scope's −60..50, so 50° down, 60°
/// up; the sight then stops where the gun does.
pub const GUN_PITCH: [f32; 2] = [-50.0 * UNITS, 60.0 * UNITS];
/// The actions a press shorter than a tick still sends once ([`Orbit::taps`]).
const TAPPED: [(Action, u8); 5] = [
    (Action::Jump, action::JUMP),
    (Action::Fire, action::FIRE),
    (Action::Reload, action::RELOAD),
    (Action::Interact, action::INTERACT),
    (Action::Grenade, action::GRENADE),
];

/// The modern controls' state.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct Orbit {
    /// This client plays the modern controls: a window's player, or a modern script.
    pub on: bool,
    /// The aim's facing (game angle units, kept fractional for small mouse steps) and its pitch (up
    /// positive, within ±`AIM_PITCH_MAX`).
    pub yaw: f32,
    pub pitch: f32,
    /// Over the left shoulder (the shoulder swap toggles it).
    pub left: bool,
    /// The toggle modes' latches: aim, crouch, walk, each for the keyboard and mouse and the pad.
    pub latched: [[bool; 2]; 3],
    /// The last input sent aimed (R): the view zooms.
    pub aiming: bool,
    /// The view's zoom, 0 to 1 ([`AIM_ZOOM`]).
    pub zoom: f32,
    /// The respawn count ([`FollowCamera::lives`]) the camera last turned behind the player for.
    pub lives: Option<u16>,
    /// The drawn shoulder (1 right, −1 left), easing toward [`Orbit::left`]'s ([`SWAP_TIME`]); none
    /// until the camera first draws (it starts there).
    pub side: Option<f32>,
    /// The view this camera drew last (eye, look), for the radar and the listener; none when another
    /// camera drew it (Tank, Race, the jump onto a mounted gun, the debug camera).
    pub view: Option<(Vec3, Vec3)>,
    /// Actions pressed since the tick last read the input: a press shorter than a tick is sent
    /// once ([`TAPPED`]); the pad's X tap ([`HOLD`]) as Reload.
    pub taps: u8,
    /// How long the pad's shared Reload / Interact button has been held (seconds).
    pub x_held: Option<f32>,
    /// How long the look stick has been held at its edge (seconds; [`ramp`]).
    pub edge: f32,
}

impl Orbit {
    /// Behind the player (`facing`) at its first tick and after each respawn (`lives`), level, the
    /// toggles let go.
    pub fn face(&mut self, facing: u16, lives: u16) {
        if self.lives != Some(lives) {
            (self.lives, self.yaw, self.pitch) = (Some(lives), f32::from(facing), 0.0);
            self.latched = Default::default();
        }
    }

    /// Turn by `d` (game units: yaw, pitch up).
    pub fn turn(&mut self, d: Vec2) {
        self.yaw = (self.yaw + d.x).rem_euclid(65536.0);
        let max = f32::from(AIM_PITCH_MAX);
        self.pitch = (self.pitch + d.y).clamp(-max, max);
    }

    /// On a mounted gun: the pitch within the gun's ([`GUN_PITCH`]).
    pub fn hold_to_gun(&mut self) {
        self.pitch = self.pitch.clamp(GUN_PITCH[0], GUN_PITCH[1]);
    }

    /// The aim's facing as the wire has it.
    pub fn aim_yaw(&self) -> u16 {
        (self.yaw.round() as i32).rem_euclid(65536) as u16
    }

    /// [`Input::camera_yaw`]: the aim looks along `camera_yaw + 0x4000` (conker-sim's `aim_yaw`).
    pub fn camera_yaw(&self) -> u16 {
        self.aim_yaw().wrapping_sub(0x4000)
    }

    pub fn aim_pitch(&self) -> i16 {
        self.pitch.round() as i16
    }

    /// The look's unit direction and the arm's right, from the wire's angles (conker-sim's
    /// `aim_ray`, bit for bit).
    pub fn look(&self) -> (Vec3, Vec3) {
        let (c, s) = cos_sin(self.aim_yaw());
        let (pc, ps) = cos_sin(self.aim_pitch() as u16);
        (Vec3::new(c * pc, ps, -s * pc), Vec3::new(s, 0.0, c))
    }
}

/// The camera of a player standing at `feet` (drawn), looking along `orbit`: the arm's start (the
/// point of the look line nearest the pivot, where the aim ray starts) and the eye, `AIM_ARM` back
/// along the look line, or short of the first wall on it.
pub fn arm(orbit: &Orbit, feet: Vec3, world: &CollisionWorld) -> (Vec3, Vec3) {
    arm_at(orbit, feet, world, if orbit.left { -1.0 } else { 1.0 })
}

/// [`arm`] over shoulder `side` (1 right, −1 left; between them while the swap eases across).
pub fn arm_at(orbit: &Orbit, feet: Vec3, world: &CollisionWorld, side: f32) -> (Vec3, Vec3) {
    let (dir, right) = orbit.look();
    // The aim ray's start (conker-sim's `aim_ray`): short of a wall at the shoulder.
    let pivot = feet + Vec3::Y * AIM_PIVOT_HEIGHT;
    let reach = aim_reach(pivot.to_array(), right.to_array(), side.signum(), world);
    let start = pivot + right * (side * reach);
    let wall = world.ray(start.to_array(), (-dir).to_array(), AIM_ARM + ARM_MARGIN);
    let back = wall.map_or(AIM_ARM, |h| (h.dist - ARM_MARGIN).clamp(0.0, AIM_ARM));
    (start, start - dir * back)
}

/// On the mounted gun at `at` (its origin), its sight's eye: the gun's (`conker_camera::gun_eye_kicked`,
/// kicked as last drawn) turned and pitched by the aim, as conker-rules turns the gun from the input
/// and its shot's sight takes it; the view looks along the aim from there.
pub fn gun_eye(orbit: &Orbit, at: [f32; 3], kick: [f32; 3]) -> Vec3 {
    let pitch = -f32::from(orbit.aim_pitch()) / UNITS;
    Vec3::from_array(conker_camera::gun_eye_kicked(
        at,
        orbit.aim_yaw(),
        pitch,
        kick,
    ))
}

/// Mouse counts (y down; raw, no acceleration or smoothing) as a turn in game units: CS's scale,
/// 0.022° a count × the sensitivity × `scale` (aiming, [`aim_scale`]).
pub fn mouse_turn(counts: Vec2, m: &settings::Mouse, scale: f32) -> Vec2 {
    let k = MOUSE_DEGREES * m.sensitivity * scale * UNITS;
    let up = if m.invert_y { counts.y } else { -counts.y };
    Vec2::new(-counts.x * k, up * k)
}

/// The look stick's share of full speed: nothing within the radial dead zone `dead`, full past
/// [`OUTER`], squared ([`CURVE`]) between.
pub fn stick_share(v: Vec2, dead: f32) -> Vec2 {
    let len = v.length();
    if len <= dead {
        return Vec2::ZERO;
    }
    let t = ((len - dead) / (OUTER - dead).max(f32::EPSILON)).min(1.0);
    v / len * t.powf(CURVE)
}

/// The pad's right stick (y up) held `dt` seconds as a turn in game units ([`PAD_RATE`] × the
/// sensitivity × `scale`).
pub fn stick_turn(v: Vec2, p: &settings::Pad, scale: f32, dt: f32) -> Vec2 {
    let s = stick_share(v, p.dead_zone[1]);
    let up = if p.invert_y { -s.y } else { s.y };
    let k = p.sensitivity * scale * dt * UNITS;
    Vec2::new(-s.x * PAD_RATE[0] * k, up * PAD_RATE[1] * k)
}

/// The move stick: nothing within the radial dead zone `dead`, full past [`OUTER`], linear
/// between (the stick's speed curve is the movement's own).
pub fn move_share(v: Vec2, dead: f32) -> Vec2 {
    let len = v.length();
    if len <= dead {
        return Vec2::ZERO;
    }
    v / len * ((len - dead) / (OUTER - dead).max(f32::EPSILON)).min(1.0)
}

/// The pad's move and look sticks (Southpaw swaps them), and the settings with their dead zones
/// in that order (the setting's are the physical left and right sticks').
pub fn sticks(pad: &Gamepad, p: &settings::Pad) -> ([Vec2; 2], settings::Pad) {
    let [l, r] = [pad.left_stick(), pad.right_stick()];
    match p.southpaw {
        true => (
            [r, l],
            settings::Pad {
                dead_zone: [p.dead_zone[1], p.dead_zone[0]],
                ..*p
            },
        ),
        false => ([l, r], *p),
    }
}

/// The look stick's acceleration after `edge` seconds at its edge: 1, then from [`RAMP_DELAY`] up
/// to [`RAMP_GAIN`] over [`RAMP_TIME`].
pub fn ramp(edge: f32) -> f32 {
    let t = ((edge - RAMP_DELAY) / RAMP_TIME).clamp(0.0, 1.0);
    1.0 + (RAMP_GAIN - 1.0) * t
}

/// Aim assist: the look ray from `start` along `dir` meets one of the `bodies` (position, hit
/// cylinder, crouched) within `len` (the first wall's distance).
pub fn over_body(
    start: Vec3,
    dir: Vec3,
    len: f32,
    mut bodies: impl Iterator<Item = ([f32; 3], conker_rules::damage::Hitbox, bool)>,
) -> bool {
    use conker_rules::projectile::hits_body;
    let (s, d) = (start.to_array(), dir.to_array());
    bodies.any(|(at, h, crouched)| hits_body(s, d, len, at, h, crouched).is_some())
}

/// The view's vertical angle, degrees: the follow camera's, narrowed by `zoom` of [`AIM_ZOOM`].
pub fn aim_fov(zoom: f32) -> f32 {
    let base = (conker_camera::VIEW_ANGLES[1] / 2.0).to_radians().tan();
    let k = 1.0 + (AIM_ZOOM - 1.0) * zoom;
    2.0 * (base / k).atan().to_degrees()
}

/// The look's speed while aiming or scoped, FOV-normalised: `multiplier` × how much the view
/// narrowed (the tangents' ratio), so the crosshair sweeps the screen as fast as unaimed.
pub fn aim_scale(fov: f32, multiplier: f32) -> f32 {
    let half = |deg: f32| (deg / 2.0).to_radians().tan();
    multiplier * half(fov) / half(conker_camera::VIEW_ANGLES[1])
}

/// The view's angle while aiming (`scope`: the N64 scope's, else [`aim_fov`] fully zoomed).
fn aimed_fov(scope: Option<&conker_camera::Scope>) -> f32 {
    scope.map_or(aim_fov(1.0), crate::render::fov)
}

/// `d` split by device: the keyboard and mouse, and the pad.
fn split<'a>(d: &Devices<'a>) -> [Devices<'a>; 2] {
    [
        Devices { pad: None, ..*d },
        Devices {
            keys: None,
            mouse: None,
            wheel: 0.0,
            ..*d
        },
    ]
}

/// The hold/toggle actions: aim, crouch and walk, with each device's mode.
const MODED: [Action; 3] = [Action::Aim, Action::Crouch, Action::Walk];

fn modes(s: &Settings, k: usize) -> [Mode; 2] {
    match k {
        0 => [s.mouse.aim, s.pad.aim],
        1 => [s.mouse.crouch, s.pad.crouch],
        _ => [s.mouse.walk, s.pad.walk],
    }
}

/// Whether moded action `k` ([`MODED`]) is on: held, or latched on a device in toggle mode.
fn on(b: &Bindings, d: &Devices, o: &Orbit, s: &Settings, k: usize) -> bool {
    let (devices, modes) = (split(d), modes(s, k));
    (0..2).any(|i| match modes[i] {
        Mode::Hold => b.pressed(MODED[k], &devices[i]),
        Mode::Toggle => o.latched[k][i],
    })
}

/// A frame's presses: each toggle-mode action flips its device's latch; the shoulder swap.
pub fn press(b: &Bindings, d: &Devices, o: &mut Orbit, s: &Settings) {
    let devices = split(d);
    for (k, &action) in MODED.iter().enumerate() {
        for (i, device) in devices.iter().enumerate() {
            if modes(s, k)[i] == Mode::Toggle && b.just_pressed(action, device) {
                o.latched[k][i] ^= true;
            }
        }
    }
    if b.just_pressed(Action::ShoulderSwap, d) {
        o.left ^= true;
    }
}

/// The pad button Reload and Interact share (X by default), told apart by how long it is held.
pub fn shared_x(b: &Bindings) -> Option<GamepadButton> {
    let pad = b.get(Action::Reload).pad;
    pad.filter(|_| b.get(Action::Interact).pad == pad)
}

/// A frame's presses for the next tick (`dt` seconds): the [`TAPPED`] actions pressed, and the
/// shared X: let go before [`HOLD`] a Reload, else Interact while held ([`input`]).
pub fn hold(b: &Bindings, d: &Devices, o: &mut Orbit, dt: f32) {
    let x = shared_x(b);
    let [keys, pad] = split(d);
    for (a, bit) in TAPPED {
        let split = x.is_some() && matches!(a, Action::Reload | Action::Interact);
        if b.just_pressed(a, if split { &keys } else { d }) {
            o.taps |= bit;
        }
    }
    let down = x.is_some_and(|x| pad.pad.is_some_and(|p| p.pressed(x)));
    match (down, o.x_held) {
        (true, t) => o.x_held = Some(t.unwrap_or(0.0) + dt),
        (false, Some(t)) => {
            if t < HOLD {
                o.taps |= action::RELOAD;
            }
            o.x_held = None;
        }
        (false, None) => {}
    }
}

/// The modern input from the devices (its `camera_yaw` and `aim_pitch` are the caller's):
/// - The Move actions push the stick full, else the pad's move stick does (its dead zone,
///   [`move_share`]; Southpaw's right); Walk halves it.
/// - Aim sends R (the N64 aim camera and scope still key on it), Holster B; through a scope
///   (`scoped`), the zoom actions C-up and C-down. The C buttons do nothing else: the right stick
///   looks, and the grenade is its action.
/// - Jump, Crouch, Fire, Reload, Interact and Grenade send their actions, the left shoulder
///   SHOULDER, and MODERN always (conker-rules' `pad()` turns them into the N64 bits); the pad's
///   shared X Interact only once held ([`hold`]: a tap is a Reload), and [`Orbit::taps`] too.
pub fn input(b: &Bindings, d: &Devices, o: &Orbit, s: &Settings, scoped: bool) -> Input {
    use Action::*;
    let held = |a| b.pressed(a, d);
    let axis = |pos, neg| held(pos) as i8 as f32 - held(neg) as i8 as f32;
    let mut stick = Vec2::new(axis(MoveRight, MoveLeft), axis(MoveForward, MoveBack));
    if stick == Vec2::ZERO
        && let Some(pad) = d.pad
    {
        let ([v, _], p) = sticks(pad, &s.pad);
        stick = move_share(v, p.dead_zone[0]);
    }
    // The pad's shared X sends Reload on a tap ([`hold`]'s), Interact once held.
    let keys = split(d)[0];
    let (reload, interact) = match shared_x(b) {
        Some(_) => (
            b.pressed(Reload, &keys),
            b.pressed(Interact, &keys) || o.x_held.is_some_and(|t| t >= HOLD),
        ),
        None => (held(Reload), held(Interact)),
    };
    let (aim, crouch, walk) = (on(b, d, o, s, 0), on(b, d, o, s, 1), on(b, d, o, s, 2));
    let scale = if walk { STICK_WALK } else { STICK_MAX };
    let q = |v: f32| (v.clamp(-1.0, 1.0) * scale).round() as i8;
    let bits = [
        (aim, BUTTON_R),
        (held(Holster), BUTTON_B),
        (scoped && aim && held(ScopeZoomIn), C_UP),
        (scoped && aim && held(ScopeZoomOut), C_DOWN),
    ];
    let acts = [
        (held(Jump), action::JUMP),
        (crouch, action::CROUCH),
        (held(Fire), action::FIRE),
        (reload, action::RELOAD),
        (interact, action::INTERACT),
        (held(Grenade), action::GRENADE),
        (o.left, action::SHOULDER),
    ];
    let or = |list: &[(bool, u16)]| list.iter().filter(|x| x.0).fold(0, |a, x| a | x.1);
    let actions = acts
        .iter()
        .filter(|x| x.0)
        .fold(action::MODERN | o.taps, |a, x| a | x.1);
    Input {
        stick_x: q(stick.x),
        stick_y: q(stick.y),
        buttons: or(&bits),
        actions,
        ..Default::default()
    }
}

/// Every frame, before the tick reads it: the mouse's counts and the look stick turn the camera
/// (the stick ramped at its edge, [`ramp`], and slowed over an enemy's body, [`ASSIST`]), the
/// toggles, the shoulder swap and the taps take this frame's presses. Not while the debug camera
/// flies, a screen or the lost pointer lock holds the input, nor in the modes with their own camera
/// or jumping onto a mounted gun (its sight then opens along the gun, level, as the N64's: V m2 84);
/// seated, it turns the gun, at the scope's speed, within its pitch ([`Orbit::hold_to_gun`]).
/// Natively only with the cursor grabbed (a click); scripts turn it themselves.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn look(
    controls: Controls,
    settings: Res<Settings>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    time: Res<Time<Real>>,
    mut orbit: ResMut<Orbit>,
    own: Query<(&Player, &Combat, &FollowCamera, Option<&Fighting>)>,
    (free, team, paused): (
        Query<&FreeCameraState>,
        Res<crate::teams::Screen>,
        Res<crate::pointer::Paused>,
    ),
    cursor: Query<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    (others, game, world): (
        Query<(&Player, &Combat), With<lightyear::prelude::Interpolated>>,
        Query<&MatchView>,
        Res<Collision>,
    ),
) {
    let Ok((p, c, cam, fighting)) = own.single() else {
        return;
    };
    orbit.face(p.facing, cam.lives);
    // On a mounted gun: seated, or jumping on along the gun's yaw (I: the N64's opening).
    let seated = fighting.and_then(|f| f.state.gun.map(|_| f.state.seated()));
    if seated == Some(false) {
        (orbit.yaw, orbit.pitch) = (f32::from(p.move_angle), 0.0);
    }
    let grabbed = cfg!(target_family = "wasm")
        || cursor
            .iter()
            .any(|c| c.grab_mode == bevy::window::CursorGrabMode::Locked);
    if free.iter().any(|c| c.enabled)
        || team.open
        || paused.0
        || cam.tank.is_some()
        || seated == Some(false)
    {
        return;
    }
    let seated = seated == Some(true);
    let d = controls.devices(0.0, Vec2::ZERO);
    press(&controls.bindings, &d, &mut orbit, &settings);
    hold(&controls.bindings, &d, &mut orbit, time.delta_secs());
    let scope = cam.follow.cam.scope;
    let scope = scope.on.then_some(&scope);
    // The gun's sight is a scope, always narrowed: its look at the scope's speed.
    let aiming = seated || on(&controls.bindings, &d, &orbit, &settings, 0);
    let fov = aimed_fov(scope);
    let mut turn = Vec2::ZERO;
    if let Some(m) = motion.filter(|_| grabbed) {
        let scale = if aiming {
            aim_scale(fov, settings.mouse.aim_multiplier)
        } else {
            1.0
        };
        turn += mouse_turn(m.delta, &settings.mouse, scale);
    }
    if let Some(pad) = d.pad {
        let ([_, v], ps) = sticks(pad, &settings.pad);
        let scale = if aiming {
            aim_scale(fov, ps.aim_multiplier)
        } else {
            1.0
        };
        let dt = time.delta_secs();
        let at_edge = stick_share(v, ps.dead_zone[1]).length() >= 1.0;
        orbit.edge = if at_edge { orbit.edge + dt } else { 0.0 };
        // Aim assist: the look ray (the server's) over a living enemy's body, short of a wall.
        let m = game.iter().next();
        let enemy = |o: &Combat| {
            o.health > 0 && o.slot != c.slot && !m.is_some_and(|m| team_mode(m) && o.team == c.team)
        };
        let (start, dir) = (arm(&orbit, Vec3::from(p.pos), &world.0).0, orbit.look().0);
        let len = raycast(&world.0, start.to_array(), dir.to_array(), ASSIST_RANGE);
        let bodies = (others.iter().filter(|(_, o)| enemy(o))).map(|(o, oc)| {
            let crouched = matches!(o.state, CROUCH | CROUCH_WALK);
            (
                o.pos,
                conker_rules::characters::body(oc.class).hitbox,
                crouched,
            )
        });
        let assist = if over_body(start, dir, len, bodies) {
            ASSIST
        } else {
            1.0
        };
        turn += stick_turn(v, &ps, scale, dt) * ramp(orbit.edge) * assist;
    }
    orbit.turn(turn);
    if seated {
        orbit.hold_to_gun();
    }
}

/// The view (after the follow camera's, before the outros' and Beach's, which win): the arm's
/// eye on the drawn body (between ticks, as the body is drawn), zoomed while aiming; through a
/// scope, the follow camera's eye (the head) looking along the aim, at the scope's zoom; on a
/// mounted gun, its sight's eye ([`gun_eye`]) along the aim at its scope's zoom (the follow camera
/// draws the jump onto it).
#[allow(clippy::type_complexity)]
pub fn camera(
    fixed: Res<Time<Fixed>>,
    time: Res<Time<Real>>,
    world: Res<Collision>,
    mut orbit: ResMut<Orbit>,
    player: Query<(&Player, &FollowCamera, Option<&Fighting>)>,
    camera: Single<(&mut Transform, &mut Projection, &FreeCameraState)>,
) {
    let (mut transform, mut projection, free) = camera.into_inner();
    orbit.view = None;
    let Ok((p, cam, fighting)) = player.single() else {
        return;
    };
    let gun = fighting.and_then(|f| f.state.gun.map(|m| (m.at, f.state.seated())));
    if !orbit.on || free.enabled || cam.tank.is_some() || gun.is_some_and(|g| !g.1) {
        return;
    }
    let k = 1.0 - (-time.delta_secs() / ZOOM_TIME).exp();
    orbit.zoom += (f32::from(u8::from(orbit.aiming)) - orbit.zoom) * k;
    let side = ease_side(orbit.side, orbit.left, time.delta_secs());
    orbit.side = Some(side);
    let f = fixed.overstep_fraction().clamp(0.0, 1.0);
    let drawn = cam.drawn.between(cam.drawn.frame(p, &cam.follow), f);
    let scope = cam.follow.cam.scope;
    let (eye, fov) = match (gun, scope.on) {
        (Some((at, _)), _) => (
            gun_eye(&orbit, at, cam.gun.drawn),
            crate::render::fov(&scope),
        ),
        (None, true) => (drawn.eye, crate::render::fov(&scope)),
        (None, false) => (
            arm_at(&orbit, drawn.body, &world.0, side).1,
            aim_fov(orbit.zoom),
        ),
    };
    let look = orbit.look().0;
    *transform = Transform::from_translation(eye).looking_to(look, Vec3::Y);
    orbit.view = Some((eye, look));
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = fov.to_radians();
    }
}

/// The drawn shoulder `dt` seconds on: from `side` (none: already there) toward the left's −1 or
/// the right's 1 ([`SWAP_TIME`]).
pub fn ease_side(side: Option<f32>, left: bool, dt: f32) -> f32 {
    let to = if left { -1.0 } else { 1.0 };
    let from = side.unwrap_or(to);
    from + (to - from) * (1.0 - (-dt / SWAP_TIME).exp())
}

/// The pad's vibration for the local player's shots (`fired`: its shot count rose) and the hits it
/// took (`hurt`: its health fell): (strong, weak) motors and how long, seconds.
pub fn rumble(fired: bool, hurt: bool) -> Option<(GamepadRumbleIntensity, f32)> {
    match (hurt, fired) {
        (true, _) => Some((GamepadRumbleIntensity::strong_motor(0.6), 0.18)),
        (false, true) => Some((GamepadRumbleIntensity::weak_motor(0.3), 0.06)),
        _ => None,
    }
}

/// The pad's vibration ([`rumble`]) when the Vibration setting is on: the first pad, as the
/// controls read it. gilrs plays it natively where its platform has force feedback (not on macOS,
/// nor in the browser: its web backend has none).
fn vibrate(
    settings: Res<Settings>,
    own: Query<(&Combat, &Fighting), With<FollowCamera>>,
    pads: Query<Entity, With<Gamepad>>,
    mut last: Local<Option<(u16, u8)>>,
    mut out: MessageWriter<GamepadRumbleRequest>,
) {
    let Ok((c, f)) = own.single() else {
        return;
    };
    let now = (f.state.shots, c.health);
    let was = last.replace(now);
    let (Some((shots, health)), Some(gamepad)) = (was, pads.iter().next()) else {
        return;
    };
    if let Some((intensity, secs)) =
        rumble(now.0 > shots, now.1 < health).filter(|_| settings.pad.vibration)
    {
        out.write(GamepadRumbleRequest::Add {
            duration: std::time::Duration::from_secs_f32(secs),
            intensity,
            gamepad,
        });
    }
}

/// Natively and in the browser: [`look`] (a window's player only; a modern script turns the
/// camera at its ticks) and [`camera`] ([`crate::render`] chains it after the follow camera).
pub fn build(app: &mut App, on: bool, windowed: bool) {
    app.insert_resource(Orbit {
        on,
        ..Default::default()
    });
    if on && windowed {
        app.add_systems(PreUpdate, look.after(bevy::input::InputSystems));
        app.add_systems(Update, vibrate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_sim::movement::{AIM_SHOULDER, aim_ray};

    #[test]
    fn the_mouse_turns_right_and_looks_down_as_cs() {
        let m = settings::Mouse::default();
        // 100 counts right at 1.5: 3.3°, to the right (the game's angle falls), none up.
        let d = mouse_turn(Vec2::new(100.0, 0.0), &m, 1.0);
        assert!((d.x + 3.3 * UNITS).abs() < 0.01 && d.y == 0.0, "{d}");
        // Pulled toward the player (y down): looks down; inverted, up.
        assert!(mouse_turn(Vec2::new(0.0, 10.0), &m, 1.0).y < 0.0);
        let inverted = settings::Mouse {
            invert_y: true,
            ..m
        };
        assert!(mouse_turn(Vec2::new(0.0, 10.0), &inverted, 1.0).y > 0.0);
        // Facing +x (0), turning right faces +z: the facing's (cos a, −sin a) with a below 0.
        let mut o = Orbit::default();
        o.turn(d);
        assert!(o.look().0.z > 0.0 && o.look().0.x > 0.99);
        // The pitch stops at ±80°.
        o.turn(Vec2::new(0.0, 1e6));
        assert_eq!(o.aim_pitch(), AIM_PITCH_MAX);
    }

    /// Fix F1-B: on a mounted gun the look is the gun's aim, mouse up looking up (no N64 stick
    /// inversion): its pitch stops where conker-rules stops the gun's (−60..50 positive down: 60° up,
    /// 50° down) and the sight's eye is the gun's, turned and pitched as the rules turn it.
    #[test]
    fn the_seated_look_stops_at_the_guns_limits_and_sights_from_its_eye() {
        let m = settings::Mouse::default();
        let mut o = Orbit::default();
        let degrees = |o: &Orbit| f32::from(o.aim_pitch()) / UNITS;
        o.turn(mouse_turn(Vec2::new(0.0, -1e5), &m, 1.0));
        o.hold_to_gun();
        assert!((degrees(&o) - 60.0).abs() < 0.01, "{}", degrees(&o));
        o.turn(mouse_turn(Vec2::new(0.0, 1e5), &m, 1.0));
        o.hold_to_gun();
        assert!((degrees(&o) + 50.0).abs() < 0.01, "{}", degrees(&o));
        // Facing 0x4000 (−z), 22.5° up: the rules' scope pitch −22.5; the look climbs.
        (o.yaw, o.pitch) = (16384.0, 4096.0);
        let at = [170.0, 560.0, 5990.0];
        let eye = conker_camera::gun_eye(at, 0x4000, -22.5);
        assert_eq!(gun_eye(&o, at, [0.0; 3]).to_array(), eye);
        let look = o.look().0;
        assert!(
            look.y > 0.38 && look.z < -0.92 && look.x.abs() < 1e-3,
            "{look}"
        );
    }

    #[test]
    fn the_look_stick_has_a_radial_dead_zone_an_outer_zone_and_a_curve() {
        assert_eq!(stick_share(Vec2::new(0.07, 0.07), 0.1), Vec2::ZERO);
        assert_eq!(stick_share(Vec2::new(0.0, 0.96), 0.1), Vec2::Y);
        let half = stick_share(Vec2::new(0.525, 0.0), 0.1).x;
        assert!((half - 0.25).abs() < 1e-5, "{half}");
        // A second at full right, sensitivity 1: 450° to the right.
        let p = settings::Pad::default();
        let d = stick_turn(Vec2::X, &p, 1.0, 1.0);
        assert!((d.x + 450.0 * UNITS).abs() < 0.5 && d.y == 0.0, "{d}");
    }

    #[test]
    fn aiming_narrows_the_view_and_slows_the_look_as_much() {
        assert!((aim_fov(0.0) - conker_camera::VIEW_ANGLES[1]).abs() < 1e-4);
        let fov = aim_fov(1.0);
        assert!((aim_scale(fov, 1.0) - 1.0 / AIM_ZOOM).abs() < 1e-5);
        assert!((aim_scale(fov, 2.0) - 2.0 / AIM_ZOOM).abs() < 1e-5);
    }

    /// The camera's look line is the server's aim ray (conker-sim's `aim_ray` from the input alone),
    /// the eye on it `AIM_ARM` back; a wall behind slides the eye along that line, never off it.
    #[test]
    fn the_arm_is_the_aim_ray_and_a_wall_only_shortens_it() {
        let mut o = Orbit {
            yaw: 25345.4,
            pitch: -2000.0,
            left: true,
            ..Default::default()
        };
        let feet = Vec3::new(-123.6, -293.8, 112.0);
        let input = Input {
            camera_yaw: o.camera_yaw(),
            aim_pitch: o.aim_pitch(),
            actions: action::MODERN | action::SHOULDER,
            ..Default::default()
        };
        let (start, dir) = aim_ray(feet.to_array(), &input, &CollisionWorld::default());
        let (s, eye) = arm(&o, feet, &CollisionWorld::default());
        assert_eq!((s.to_array(), o.look().0.to_array()), (start, dir));
        assert!((s - eye - o.look().0 * AIM_ARM).length() < 1e-3);
        // A wall about 100 behind the start (whole units: the level's), across the look: the eye 8
        // short of it, on the line.
        o.pitch = 0.0;
        o.yaw = 0.0;
        let (s, _) = arm(&o, feet, &CollisionWorld::default());
        let x = (s.x - 100.0).floor();
        let big = 5000.0;
        let wall = CollisionWorld::new([
            ([[x, -big, -big], [x, big, -big], [x, -big, big]], 5),
            ([[x, big, -big], [x, big, big], [x, -big, big]], 5),
        ]);
        let (s2, eye) = arm(&o, feet, &wall);
        assert_eq!(s2, s);
        assert!(
            (s - eye - Vec3::X * (s.x - x - 8.0)).length() < 1e-3,
            "{eye} {s}"
        );
        // The other shoulder: the start 2 · AIM_SHOULDER across.
        o.left = false;
        assert!((arm(&o, feet, &wall).0 - s - Vec3::Z * 2.0 * AIM_SHOULDER).length() < 1e-3);
    }

    /// A wall at the right shoulder pulls the arm's start in where the server's aim ray starts (8
    /// short of it, conker-sim's `aim_ray`), so the eye's line stays the ray's.
    #[test]
    fn a_wall_at_the_shoulder_pulls_the_arm_in_as_the_aim_ray() {
        let o = Orbit::default();
        let big = 5000.0;
        let wall = CollisionWorld::new([
            (
                [[-big, -big, 10.0], [big, -big, 10.0], [-big, big, 10.0]],
                5,
            ),
            ([[big, -big, 10.0], [big, big, 10.0], [-big, big, 10.0]], 5),
        ]);
        let input = Input {
            camera_yaw: o.camera_yaw(),
            aim_pitch: o.aim_pitch(),
            actions: action::MODERN,
            ..Default::default()
        };
        let (start, _) = aim_ray([0.0; 3], &input, &wall);
        let (s, eye) = arm(&o, Vec3::ZERO, &wall);
        assert_eq!(s.to_array(), start);
        assert!(
            (s.z - 2.0).abs() < 1e-3 && (eye.z - 2.0).abs() < 1e-3,
            "{s} {eye}"
        );
    }

    fn read(keys: &[KeyCode], mouse: &[MouseButton], wheel: f32, o: &Orbit, s: &Settings) -> Input {
        let (mut k, mut m) = (ButtonInput::default(), ButtonInput::default());
        keys.iter().for_each(|&c| k.press(c));
        mouse.iter().for_each(|&b| m.press(b));
        let d = Devices {
            keys: Some(&k),
            mouse: Some(&m),
            wheel,
            ..Devices::default()
        };
        input(&Bindings::default(), &d, o, s, true)
    }

    #[test]
    fn the_defaults_send_the_modern_actions() {
        use KeyCode::*;
        let (o, s) = (Orbit::default(), Settings::default());
        let i = read(
            &[KeyA, KeyW, Space, KeyR],
            &[MouseButton::Left],
            0.0,
            &o,
            &s,
        );
        assert_eq!((i.stick_x, i.stick_y, i.buttons), (-80, 80, 0));
        let want = action::MODERN | action::JUMP | action::FIRE | action::RELOAD;
        assert_eq!(i.actions, want);
        // Crouch, Interact (F or E), Grenade, the shoulder; Holster is B, Walk the half stick.
        let i = read(&[KeyC, KeyE, KeyG, KeyQ, ShiftLeft, KeyS], &[], 0.0, &o, &s);
        let want = action::MODERN | action::CROUCH | action::INTERACT | action::GRENADE;
        assert_eq!((i.actions, i.buttons, i.stick_y), (want, BUTTON_B, -41));
        let left = Orbit { left: true, ..o };
        assert_eq!(
            read(&[], &[], 0.0, &left, &s).actions,
            action::MODERN | action::SHOULDER
        );
        // Aim is R; the wheel zooms the scope only while aiming through it.
        let aim = [MouseButton::Right];
        assert_eq!(read(&[], &aim, 1.0, &o, &s).buttons, BUTTON_R | C_UP);
        assert_eq!(read(&[], &aim, -1.0, &o, &s).buttons, BUTTON_R | C_DOWN);
        assert_eq!(read(&[], &[], 1.0, &o, &s).buttons, 0);
        let mut k = ButtonInput::default();
        k.press(KeyCode::KeyG);
        let d = Devices {
            keys: Some(&k),
            ..Devices::default()
        };
        assert_eq!(input(&Bindings::default(), &d, &o, &s, false).buttons, 0);
    }

    #[test]
    fn toggle_modes_latch_on_a_press() {
        let mut s = Settings::default();
        (s.mouse.aim, s.mouse.crouch) = (Mode::Toggle, Mode::Toggle);
        let mut o = Orbit::default();
        let b = Bindings::default();
        let mut m = ButtonInput::default();
        m.press(MouseButton::Right);
        let d = Devices {
            mouse: Some(&m),
            ..Devices::default()
        };
        press(&b, &d, &mut o, &s);
        assert_eq!(o.latched[0], [true, false]);
        m.clear();
        m.release(MouseButton::Right);
        let up = Devices {
            mouse: Some(&m),
            ..Devices::default()
        };
        // Let go: still aiming, till the next press.
        assert_eq!(input(&b, &up, &o, &s, false).buttons, BUTTON_R);
        // A respawn lets go of the toggles and faces the spawn's way.
        o.face(1234, 1);
        assert_eq!(
            (o.latched, o.yaw, o.lives),
            (Default::default(), 1234.0, Some(1))
        );
    }

    /// §7: the pad's X reloads on a tap and interacts once held (CoD's); R and F stay immediate; a
    /// press let go between two ticks (a grenade's LB tap) is still sent once.
    #[test]
    fn the_pads_x_taps_reload_and_holds_interact_and_short_taps_reach_a_tick() {
        let (b, s) = (Bindings::default(), Settings::default());
        let mut o = Orbit::default();
        let mut pad = Gamepad::default();
        let x = GamepadButton::West;
        let both = action::RELOAD | action::INTERACT;
        let frame = |pad: &Gamepad, o: &mut Orbit, dt| {
            let d = Devices {
                pad: Some(pad),
                ..Devices::default()
            };
            hold(&b, &d, o, dt);
            input(&b, &d, o, &s, false).actions
        };
        assert_eq!(shared_x(&b), Some(x));
        pad.digital_mut().press(x);
        assert_eq!(frame(&pad, &mut o, 0.1) & both, 0, "pressed: not yet");
        pad.digital_mut().clear();
        pad.digital_mut().release(x);
        assert_eq!(frame(&pad, &mut o, 0.016) & both, action::RELOAD, "a tap");
        o.taps = 0; // the tick sent it
        pad.digital_mut().clear();
        assert_eq!(frame(&pad, &mut o, 0.016) & both, 0, "once");
        // Held past HOLD: Interact while held, no Reload when let go.
        pad.digital_mut().press(x);
        assert_eq!(frame(&pad, &mut o, 0.2) & both, 0);
        pad.digital_mut().clear();
        assert_eq!(frame(&pad, &mut o, 0.1) & both, action::INTERACT);
        pad.digital_mut().release(x);
        pad.digital_mut().clear();
        assert_eq!(frame(&pad, &mut o, 0.016) & both, 0);
        // LB's grenade pressed and let go before the tick read it: sent once.
        let lb = GamepadButton::LeftTrigger;
        pad.digital_mut().press(lb);
        frame(&pad, &mut o, 0.016);
        pad.digital_mut().release(lb);
        pad.digital_mut().clear();
        assert_ne!(frame(&pad, &mut o, 0.016) & action::GRENADE, 0);
        // Bound apart, X is Reload alone, at once.
        let mut apart = b.clone();
        let mut interact = *apart.get(Action::Interact);
        interact.pad = Some(GamepadButton::North);
        apart.set(Action::Interact, interact);
        assert_eq!(shared_x(&apart), None);
        pad.digital_mut().press(x);
        let d = Devices {
            pad: Some(&pad),
            ..Devices::default()
        };
        let a = input(&apart, &d, &Orbit::default(), &s, false).actions;
        assert_eq!(a & both, action::RELOAD);
    }

    /// The shoulder swap: the ray (the server's) swaps at once, the drawn eye eases across, on the
    /// same look (the crosshair stays centred on a parallel line).
    #[test]
    fn the_shoulder_swap_eases_the_eye_across_and_the_ray_swaps_at_once() {
        let w = CollisionWorld::default();
        let mut o = Orbit::default();
        let (right, _) = arm(&o, Vec3::ZERO, &w);
        o.left = true;
        let (left, _) = arm(&o, Vec3::ZERO, &w);
        assert!((right - left - Vec3::Z * 2.0 * AIM_SHOULDER).length() < 1e-4);
        assert_eq!(
            ease_side(None, true, 0.016),
            -1.0,
            "the first frame is there"
        );
        let mut side = 1.0;
        side = ease_side(Some(side), true, 1.0 / 60.0);
        assert!(side > 0.0 && side < 1.0, "{side}");
        let (start, eye) = arm_at(&o, Vec3::ZERO, &w, side);
        assert!(start.z < right.z && start.z > left.z);
        assert!((start - eye).normalize().dot(o.look().0) > 0.9999);
        for _ in 0..14 {
            side = ease_side(Some(side), true, 1.0 / 60.0);
        }
        assert!(
            (side + 1.0).abs() < 0.02,
            "across in a quarter second: {side}"
        );
    }

    /// §7's pad feel: the move stick's radial dead zone (linear), Southpaw, the look's ramp at its
    /// edge, aim assist's slowdown over an enemy's body (not past a wall), the rumbles.
    #[test]
    fn the_pad_ramps_at_its_edge_slows_over_an_enemy_and_rumbles() {
        assert_eq!(move_share(Vec2::new(0.06, 0.06), 0.1), Vec2::ZERO);
        assert_eq!(move_share(Vec2::new(0.0, 0.96), 0.1), Vec2::Y);
        assert!((move_share(Vec2::new(0.525, 0.0), 0.1).x - 0.5).abs() < 1e-5);
        let mut pad = Gamepad::default();
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.5);
        pad.analog_mut().set(GamepadAxis::RightStickY, -0.7);
        let p = settings::Pad {
            dead_zone: [0.1, 0.2],
            ..Default::default()
        };
        let ([mv, lk], q) = sticks(&pad, &p);
        assert_eq!(
            (mv, lk, q.dead_zone),
            (Vec2::new(0.5, 0.0), Vec2::new(0.0, -0.7), [0.1, 0.2])
        );
        let south = settings::Pad {
            southpaw: true,
            ..p
        };
        let ([mv, lk], q) = sticks(&pad, &south);
        assert_eq!(
            (mv, lk, q.dead_zone),
            (Vec2::new(0.0, -0.7), Vec2::new(0.5, 0.0), [0.2, 0.1])
        );
        // The ramp: none for 0.3 s at the edge, then up to 1.75× over 0.25 s.
        assert_eq!((ramp(0.0), ramp(RAMP_DELAY)), (1.0, 1.0));
        assert!((ramp(RAMP_DELAY + RAMP_TIME / 2.0) - 1.375).abs() < 1e-5);
        assert_eq!(ramp(5.0), RAMP_GAIN);
        // Aim assist: a Tedi 300 ahead on the look; 100 to the side, or behind a wall at 200, not.
        let h = conker_rules::characters::body(0).hitbox;
        let (start, dir) = (Vec3::new(0.0, 67.5, 0.0), Vec3::X);
        let body = |z: f32| std::iter::once(([300.0, 0.0, z], h, false));
        assert!(over_body(start, dir, 20_000.0, body(0.0)));
        assert!(!over_body(start, dir, 20_000.0, body(100.0)));
        assert!(!over_body(start, dir, 200.0, body(0.0)));
        // Crouched, a look over its lowered top passes.
        let low = Vec3::new(0.0, h.centre + h.half - 10.0, 0.0);
        assert!(over_body(low, dir, 20_000.0, body(0.0)));
        assert!(!over_body(
            low,
            dir,
            20_000.0,
            std::iter::once(([300.0, 0.0, 0.0], h, true))
        ));
        // Rumbles: a hit taken (strong), a shot (weak), else none.
        assert!(rumble(true, true).is_some_and(|r| r.0.strong_motor > 0.0));
        assert!(
            rumble(true, false).is_some_and(|r| r.0.weak_motor > 0.0 && r.0.strong_motor == 0.0)
        );
        assert_eq!(rumble(false, false), None);
    }
}
