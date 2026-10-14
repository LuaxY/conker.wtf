//! The modern controls' camera and input (CONTROLS_RESEARCH.md §3, §5; lane C2b): an over-the-shoulder
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
//! keep their own cameras and the N64 input, and a mounted gun its sight.

use crate::bindings::{Action, Bindings, Controls, Devices};
use crate::settings::{self, Mode, Settings};
use crate::{BUTTON_B, BUTTON_R, C_DOWN, C_UP, FollowCamera, STICK_MAX, STICK_WALK};
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use conker_rules::net::Fighting;
use conker_sim::collision::CollisionWorld;
use conker_sim::movement::{
    AIM_ARM, AIM_PITCH_MAX, AIM_PIVOT_HEIGHT, AIM_SHOULDER, Input, action, cos_sin,
};
use conker_sim::net::{Collision, Player};

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
/// How far short of a wall the arm stops (the view's near plane is 4).
const ARM_MARGIN: f32 = 8.0;

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
    let (dir, right) = orbit.look();
    let side = if orbit.left {
        -AIM_SHOULDER
    } else {
        AIM_SHOULDER
    };
    let start = feet + Vec3::Y * AIM_PIVOT_HEIGHT + right * side;
    let wall = world.ray(start.to_array(), (-dir).to_array(), AIM_ARM + ARM_MARGIN);
    let back = wall.map_or(AIM_ARM, |h| (h.dist - ARM_MARGIN).clamp(0.0, AIM_ARM));
    (start, start - dir * back)
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

/// The modern input from the devices (its `camera_yaw` and `aim_pitch` are the caller's):
/// - The Move actions push the stick full, else the pad's left stick does; Walk halves it.
/// - Aim sends R (the N64 aim camera and scope still key on it), Holster B; through a scope
///   (`scoped`), the zoom actions C-up and C-down. The C buttons do nothing else: the right stick
///   looks, and the grenade is its action.
/// - Jump, Crouch, Fire, Reload, Interact and Grenade send their actions, the left shoulder
///   SHOULDER, and MODERN always (conker-rules' `pad()` turns them into the N64 bits).
pub fn input(b: &Bindings, d: &Devices, o: &Orbit, s: &Settings, scoped: bool) -> Input {
    use Action::*;
    let held = |a| b.pressed(a, d);
    let axis = |pos, neg| held(pos) as i8 as f32 - held(neg) as i8 as f32;
    let mut stick = Vec2::new(axis(MoveRight, MoveLeft), axis(MoveForward, MoveBack));
    if stick == Vec2::ZERO
        && let Some(pad) = d.pad
    {
        stick = pad.left_stick();
    }
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
        (held(Reload), action::RELOAD),
        (held(Interact), action::INTERACT),
        (held(Grenade), action::GRENADE),
        (o.left, action::SHOULDER),
    ];
    let or = |list: &[(bool, u16)]| list.iter().filter(|x| x.0).fold(0, |a, x| a | x.1);
    let actions = acts
        .iter()
        .filter(|x| x.0)
        .fold(action::MODERN, |a, x| a | x.1);
    Input {
        stick_x: q(stick.x),
        stick_y: q(stick.y),
        buttons: or(&bits),
        actions,
        ..Default::default()
    }
}

/// Every frame, before the tick reads it: the mouse's counts and the right stick turn the camera,
/// the toggles and the shoulder swap take this frame's presses. Not while the debug camera flies,
/// a screen or the lost pointer lock holds the input, nor in the modes with their own camera or on
/// a mounted gun; natively only with the cursor grabbed (a click); scripts turn it themselves.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn look(
    controls: Controls,
    settings: Res<Settings>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    time: Res<Time<Real>>,
    mut orbit: ResMut<Orbit>,
    own: Query<(&Player, &FollowCamera, Option<&Fighting>)>,
    (free, team, paused): (
        Query<&FreeCameraState>,
        Res<crate::teams::Screen>,
        Res<crate::pointer::Paused>,
    ),
    cursor: Query<&bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    let Ok((p, cam, fighting)) = own.single() else {
        return;
    };
    orbit.face(p.facing, cam.lives);
    let seated = fighting.is_some_and(|f| f.state.gun.is_some());
    let grabbed = cfg!(target_family = "wasm")
        || cursor
            .iter()
            .any(|c| c.grab_mode == bevy::window::CursorGrabMode::Locked);
    if free.iter().any(|c| c.enabled) || team.open || paused.0 || cam.tank.is_some() || seated {
        return;
    }
    let d = controls.devices(0.0, Vec2::ZERO);
    press(&controls.bindings, &d, &mut orbit, &settings);
    let scope = cam.follow.cam.scope;
    let scope = scope.on.then_some(&scope);
    let aiming = on(&controls.bindings, &d, &orbit, &settings, 0);
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
        let scale = if aiming {
            aim_scale(fov, settings.pad.aim_multiplier)
        } else {
            1.0
        };
        turn += stick_turn(pad.right_stick(), &settings.pad, scale, time.delta_secs());
    }
    orbit.turn(turn);
}

/// The view (after the follow camera's, before the outros' and Beach's, which win): the arm's
/// eye on the drawn body (between ticks, as the body is drawn), zoomed while aiming; through a
/// scope, the follow camera's eye (the head) looking along the aim, at the scope's zoom.
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
    let Ok((p, cam, fighting)) = player.single() else {
        return;
    };
    let seated = fighting.is_some_and(|f| f.state.gun.is_some());
    if !orbit.on || free.enabled || cam.tank.is_some() || seated {
        return;
    }
    let k = 1.0 - (-time.delta_secs() / ZOOM_TIME).exp();
    orbit.zoom += (f32::from(u8::from(orbit.aiming)) - orbit.zoom) * k;
    let f = fixed.overstep_fraction().clamp(0.0, 1.0);
    let drawn = cam.drawn.between(cam.drawn.frame(p, &cam.follow), f);
    let scope = cam.follow.cam.scope;
    let (eye, fov) = match scope.on {
        true => (drawn.eye, crate::render::fov(&scope)),
        false => (arm(&orbit, drawn.body, &world.0).1, aim_fov(orbit.zoom)),
    };
    *transform = Transform::from_translation(eye).looking_to(orbit.look().0, Vec3::Y);
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = fov.to_radians();
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_sim::movement::aim_ray;

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
        let (start, dir) = aim_ray(feet.to_array(), &input);
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
        // The other shoulder: the start 40 across.
        o.left = false;
        assert!((arm(&o, feet, &wall).0 - s - Vec3::Z * 40.0).length() < 1e-3);
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
}
