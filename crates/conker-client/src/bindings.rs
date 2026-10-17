//! The controls as data (docs/CONTROLS_RESEARCH.md §5, §7): named [`Action`]s, each with two keyboard
//! or mouse slots (primary, alternate: keys, mouse buttons 1-5, the wheel) and one pad slot. Esc and
//! the pad's Menu (Start) are the menu's and never bind. Until the modern scheme's MODERN bit
//! (round 2), each action sends today's N64 bit ([`n64_bit`]) and [`device_input`] builds today's
//! N64 [`Input`], plus the frame's look (mouse counts, right stick) for round 2.

use crate::{BUTTON_B, BUTTON_R, C_DOWN, C_LEFT, C_RIGHT, C_UP, STICK_MAX, STICK_WALK};
use bevy::ecs::system::SystemParam;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::reflect::FromReflect;
use bevy::reflect::enums::DynamicEnum;
use conker_sim::movement::{BUTTON_A, BUTTON_Z, Input};
use std::collections::{BTreeMap, HashMap};

/// Everything a player can bind. Move is four actions (a slot holds one key); the sticks and the
/// mouse's look are not slots (round 2's Southpaw setting swaps the sticks).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    MoveForward,
    MoveBack,
    MoveLeft,
    MoveRight,
    Fire,
    Aim,
    ScopeZoomIn,
    ScopeZoomOut,
    Jump,
    Crouch,
    Reload,
    Holster,
    Interact,
    Grenade,
    ShoulderSwap,
    Walk,
    Scoreboard,
    PushToTalk,
    ChatAll,
    ChatTeam,
    Mute,
    TextHud,
    DebugCamera,
    Diagnostics,
}

impl Action {
    pub const ALL: [Action; 24] = {
        use Action::*;
        [
            MoveForward,
            MoveBack,
            MoveLeft,
            MoveRight,
            Fire,
            Aim,
            ScopeZoomIn,
            ScopeZoomOut,
            Jump,
            Crouch,
            Reload,
            Holster,
            Interact,
            Grenade,
            ShoulderSwap,
            Walk,
            Scoreboard,
            PushToTalk,
            ChatAll,
            ChatTeam,
            Mute,
            TextHud,
            DebugCamera,
            Diagnostics,
        ]
    };

    /// The name settings store it under.
    pub fn name(self) -> String {
        format!("{self:?}")
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.name() == name)
    }
}

/// A keyboard or mouse slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    Key(KeyCode),
    Mouse(MouseButton),
    WheelUp,
    WheelDown,
}

/// A unit variant of a reflected enum by name (`KeyW`, `Back`, `RightTrigger2`).
pub(crate) fn variant<T: FromReflect>(name: &str) -> Option<T> {
    T::from_reflect(&DynamicEnum::new(name, ()))
}

impl Bind {
    /// As settings store it: `KeyW`, `MouseLeft`, `WheelUp`.
    pub fn name(self) -> String {
        match self {
            Bind::Key(k) => format!("{k:?}"),
            Bind::Mouse(b) => format!("Mouse{b:?}"),
            b => format!("{b:?}"),
        }
    }

    /// A bindable slot by [`Bind::name`]: not Esc, mouse buttons 1-5 only.
    pub fn named(name: &str) -> Option<Self> {
        let bind = match name {
            "WheelUp" => Bind::WheelUp,
            "WheelDown" => Bind::WheelDown,
            _ => match name.strip_prefix("Mouse") {
                Some(b) => Bind::Mouse(variant(b)?),
                None => Bind::Key(variant(name)?),
            },
        };
        let other = matches!(bind, Bind::Mouse(MouseButton::Other(_)));
        (bind != Bind::Key(KeyCode::Escape) && !other).then_some(bind)
    }
}

/// A pad slot by name (`South`, `RightTrigger2`): not Start (the menu's).
pub fn pad_named(name: &str) -> Option<GamepadButton> {
    variant(name).filter(|b| !matches!(b, GamepadButton::Start | GamepadButton::Other(_)))
}

/// An action's slots: keyboard/mouse primary and alternate, and the pad's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Slots {
    pub keys: [Option<Bind>; 2],
    pub pad: Option<GamepadButton>,
}

impl Slots {
    /// As settings store it: `[primary, alternate, pad]`.
    pub fn names(&self) -> [Option<String>; 3] {
        let [a, b] = self.keys.map(|k| k.map(Bind::name));
        [a, b, self.pad.map(|p| format!("{p:?}"))]
    }

    /// From [`Slots::names`]; None if a name is not a bindable input.
    pub fn named(names: &[Option<String>; 3]) -> Option<Self> {
        /// An empty slot, or a bindable input (None: neither).
        fn slot<T>(name: &Option<String>, parse: fn(&str) -> Option<T>) -> Option<Option<T>> {
            name.as_deref().map_or(Some(None), |n| parse(n).map(Some))
        }
        Some(Self {
            keys: [slot(&names[0], Bind::named)?, slot(&names[1], Bind::named)?],
            pad: slot(&names[2], pad_named)?,
        })
    }
}

/// Every action's slots, in [`Action::ALL`]'s order.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Bindings(pub [Slots; Action::ALL.len()]);

impl Default for Bindings {
    /// §7's tables (Crouch on C, Walk on Left Shift; the debug camera on F8, the diagnostics on the
    /// key under Esc, whatever it types).
    fn default() -> Self {
        use Action::*;
        use GamepadButton as P;
        use KeyCode as K;
        let slots = |a: Action| {
            let (k, alt, pad): (Bind, Option<Bind>, Option<P>) = match a {
                MoveForward => (Bind::Key(K::KeyW), Some(Bind::Key(K::ArrowUp)), None),
                MoveBack => (Bind::Key(K::KeyS), Some(Bind::Key(K::ArrowDown)), None),
                MoveLeft => (Bind::Key(K::KeyA), Some(Bind::Key(K::ArrowLeft)), None),
                MoveRight => (Bind::Key(K::KeyD), Some(Bind::Key(K::ArrowRight)), None),
                Fire => (Bind::Mouse(MouseButton::Left), None, Some(P::RightTrigger2)),
                Aim => (Bind::Mouse(MouseButton::Right), None, Some(P::LeftTrigger2)),
                ScopeZoomIn => (Bind::WheelUp, None, Some(P::DPadUp)),
                ScopeZoomOut => (Bind::WheelDown, None, Some(P::DPadDown)),
                Jump => (Bind::Key(K::Space), None, Some(P::South)),
                Crouch => (Bind::Key(K::KeyC), None, Some(P::East)),
                // The pad's X: a tap reloads, a hold interacts (round 2 tells them apart).
                Reload => (Bind::Key(K::KeyR), None, Some(P::West)),
                Interact => (Bind::Key(K::KeyF), Some(Bind::Key(K::KeyE)), Some(P::West)),
                Holster => (Bind::Key(K::KeyQ), None, Some(P::North)),
                Grenade => (Bind::Key(K::KeyG), None, Some(P::LeftTrigger)),
                ShoulderSwap => (
                    Bind::Key(K::KeyX),
                    Some(Bind::Mouse(MouseButton::Middle)),
                    Some(P::RightTrigger),
                ),
                Walk => (Bind::Key(K::ShiftLeft), None, Some(P::LeftThumb)),
                Scoreboard => (Bind::Key(K::Tab), None, Some(P::Select)),
                PushToTalk => (Bind::Key(K::KeyV), None, None),
                // The page's chat box (no pad: no typing on it).
                ChatAll => (Bind::Key(K::KeyT), None, None),
                ChatTeam => (Bind::Key(K::KeyY), None, None),
                Mute => (Bind::Key(K::KeyM), None, None),
                TextHud => (Bind::Key(K::KeyH), None, None),
                DebugCamera => (Bind::Key(K::F8), None, None),
                Diagnostics => (Bind::Key(K::Backquote), None, None),
            };
            Slots {
                keys: [Some(k), alt],
                pad,
            }
        };
        Self(Action::ALL.map(slots))
    }
}

impl Bindings {
    pub fn get(&self, a: Action) -> &Slots {
        &self.0[a as usize]
    }

    pub fn set(&mut self, a: Action, slots: Slots) {
        self.0[a as usize] = slots;
    }

    /// The defaults with settings' overrides (by action name); an unknown action or input keeps
    /// the default.
    pub fn with(overrides: &BTreeMap<String, [Option<String>; 3]>) -> Self {
        let mut out = Self::default();
        for (name, names) in overrides {
            match (Action::named(name), Slots::named(names)) {
                (Some(a), Some(slots)) => out.set(a, slots),
                _ => warn!("settings: binding {name} {names:?} ignored"),
            }
        }
        out
    }

    /// What settings store: the actions bound otherwise than by default.
    pub fn overrides(&self) -> BTreeMap<String, [Option<String>; 3]> {
        let default = Self::default();
        (Action::ALL.into_iter())
            .filter(|&a| self.get(a) != default.get(a))
            .map(|a| (a.name(), self.get(a).names()))
            .collect()
    }

    /// Whether `a` is held this frame (the wheel: turned since the last read).
    pub fn pressed(&self, a: Action, d: &Devices) -> bool {
        let s = self.get(a);
        s.keys.iter().flatten().any(|b| match *b {
            Bind::Key(k) => d.keys.is_some_and(|i| i.pressed(k)),
            Bind::Mouse(m) => d.mouse.is_some_and(|i| i.pressed(m)),
            Bind::WheelUp => d.wheel > 0.0,
            Bind::WheelDown => d.wheel < 0.0,
        }) || s.pad.is_some_and(|p| d.pad.is_some_and(|g| g.pressed(p)))
    }

    /// Whether `a` was pressed this frame (keys, mouse buttons and the pad).
    pub fn just_pressed(&self, a: Action, d: &Devices) -> bool {
        let s = self.get(a);
        s.keys.iter().flatten().any(|b| match *b {
            Bind::Key(k) => d.keys.is_some_and(|i| i.just_pressed(k)),
            Bind::Mouse(m) => d.mouse.is_some_and(|i| i.just_pressed(m)),
            _ => false,
        }) || s
            .pad
            .is_some_and(|p| d.pad.is_some_and(|g| g.just_pressed(p)))
    }
}

/// What one read of the devices sees.
#[derive(Clone, Copy, Default)]
pub struct Devices<'a> {
    pub keys: Option<&'a ButtonInput<KeyCode>>,
    pub mouse: Option<&'a ButtonInput<MouseButton>>,
    pub pad: Option<&'a Gamepad>,
    /// The wheel since the last read (+: up; its sign only).
    pub wheel: f32,
    /// The mouse's counts this frame (y down).
    pub motion: Vec2,
}

/// The bindings and the devices, for systems that read actions.
#[derive(SystemParam)]
pub struct Controls<'w, 's> {
    pub bindings: Res<'w, Bindings>,
    keys: Option<Res<'w, ButtonInput<KeyCode>>>,
    mouse: Option<Res<'w, ButtonInput<MouseButton>>>,
    pads: Query<'w, 's, &'static Gamepad>,
    windows: Query<'w, 's, &'static Window>,
    paused: Option<Res<'w, crate::pointer::Paused>>,
}

impl Controls<'_, '_> {
    /// The devices, the first pad only while a window is focused (with two windows open, one pad
    /// drove both); none while the Esc menu holds the input ([`crate::pointer::Paused`]).
    pub fn devices(&self, wheel: f32, motion: Vec2) -> Devices<'_> {
        if self.paused.as_ref().is_some_and(|p| p.0) {
            return Devices::default();
        }
        let focused = self.windows.iter().any(|w| w.focused);
        Devices {
            keys: self.keys.as_deref(),
            mouse: self.mouse.as_deref(),
            pad: self.pads.iter().next().filter(|_| focused),
            wheel,
            motion,
        }
    }

    pub fn pressed(&self, a: Action) -> bool {
        self.bindings.pressed(a, &self.devices(0.0, Vec2::ZERO))
    }

    pub fn just_pressed(&self, a: Action) -> bool {
        self.bindings
            .just_pressed(a, &self.devices(0.0, Vec2::ZERO))
    }
}

/// Today's N64 bit for an action, until round 2's MODERN: Fire and Crouch are Z (fire a drawn
/// weapon, else crouch), Jump and Reload A (jump, else reload a drawn gun), Holster B, Aim R,
/// Grenade C-down, the scope's zoom C-up / C-down. Interact (pickups are automatic), the shoulder,
/// Walk (a half stick) and the screens' actions send none.
pub fn n64_bit(a: Action) -> u16 {
    use Action::*;
    match a {
        Fire | Crouch => BUTTON_Z,
        Jump | Reload => BUTTON_A,
        Holster => BUTTON_B,
        Aim => BUTTON_R,
        Grenade | ScopeZoomOut => C_DOWN,
        ScopeZoomIn => C_UP,
        _ => 0,
    }
}

/// One read of the devices: today's N64 pad, and the look for round 2.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Device {
    /// Stick and buttons; the camera's yaw is the caller's.
    pub input: Input,
    /// The mouse's counts this frame (y down; raw: no acceleration or smoothing).
    pub mouse: Vec2,
    /// The pad's right stick.
    pub look: Vec2,
}

/// The devices as an N64 pad through `bindings`:
/// - The Move actions push the stick full (Walk: half, the oracle's walk), else the pad's left
///   stick does.
/// - Each held action sends its [`n64_bit`]; the zoom actions only while Aim is held (they work in
///   the scope; unscoped, C-down would throw a grenade).
/// - The pad's right stick pushes the C buttons (left / right: strafe, down: grenade), as before.
///
/// The C buttons do not turn the camera in the original's multiplayer (held, C-left/right stop its
/// swing-back and strafe), so no key or mouse turns it either.
pub fn device_input(bindings: &Bindings, d: &Devices) -> Device {
    use Action::*;
    let held = |a| bindings.pressed(a, d);
    let axis = |pos, neg| held(pos) as i8 as f32 - held(neg) as i8 as f32;
    let mut stick = Vec2::new(axis(MoveRight, MoveLeft), axis(MoveForward, MoveBack));
    let scale = if held(Walk) { STICK_WALK } else { STICK_MAX };
    let aiming = held(Aim);
    let mut buttons = (Action::ALL.into_iter())
        .filter(|&a| held(a) && (aiming || !matches!(a, ScopeZoomIn | ScopeZoomOut)))
        .fold(0, |b, a| b | n64_bit(a));
    let look = d.pad.map_or(Vec2::ZERO, |p| p.right_stick());
    if let Some(pad) = d.pad {
        if stick == Vec2::ZERO {
            stick = pad.left_stick();
        }
        for (on, bit) in [
            (look.x < -0.5, C_LEFT),
            (look.x > 0.5, C_RIGHT),
            (look.y > 0.5, C_UP),
            (look.y < -0.5, C_DOWN),
        ] {
            if on {
                buttons |= bit;
            }
        }
    }
    let q = |v: f32| (v.clamp(-1.0, 1.0) * scale).round() as i8;
    Device {
        input: Input {
            stick_x: q(stick.x),
            stick_y: q(stick.y),
            buttons,
            ..Default::default()
        },
        mouse: d.motion,
        look,
    }
}

/// The wheel's notches since the input last read it (a frame's turn is one: the browser's pixels
/// and a native mouse's lines don't compare), and the reads they still hold: the input is read at
/// the tick (30 Hz), slower than frames, so a frame's turn would be lost. Each notch holds its
/// direction for the setting's reads ([`crate::settings::Mouse::wheel_zoom`]); one the other way
/// drops what is left.
// ponytail: a trackpad turns every frame of a swipe and its momentum, so it queues a long hold
// (the zoom stops at its end; turning back cancels it); a cap or per-unit notch sizes if it bites.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Wheel {
    notches: i32,
    held: i32,
}

impl Wheel {
    /// This read's wheel (+1 up, -1 down, 0), each notch held for `ticks` reads.
    pub fn read(&mut self, ticks: i32) -> f32 {
        let n = std::mem::take(&mut self.notches);
        if n != 0 {
            if n.signum() != self.held.signum() {
                self.held = 0;
            }
            self.held += n * ticks;
        }
        let sign = self.held.signum();
        self.held -= sign;
        sign as f32
    }
}

/// Not while the Esc menu holds the input (its list scrolls with the wheel).
fn gather_wheel(
    scroll: Res<AccumulatedMouseScroll>,
    paused: Option<Res<crate::pointer::Paused>>,
    mut wheel: ResMut<Wheel>,
) {
    if scroll.delta.y != 0.0 && !paused.is_some_and(|p| p.0) {
        wheel.notches += scroll.delta.y.signum() as i32;
    }
}

/// What each pressed key types (`KeyCode` is the physical key: on AZERTY, KeyW types "z"), for
/// prompts that show the player's own key.
#[derive(Resource, Clone, Debug, Default)]
pub struct KeyLabels(pub HashMap<KeyCode, Key>);

/// The player last pressed something on the pad (true) or the keyboard and mouse: the prompts show
/// that device's input.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct UsingPad(pub bool);

fn note_device(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pads: Query<&Gamepad>,
    mut using: ResMut<UsingPad>,
) {
    let pad = pads.iter().any(|p| {
        p.digital().get_just_pressed().next().is_some()
            || p.left_stick().length() > 0.5
            || p.right_stick().length() > 0.5
    });
    let desk =
        keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some();
    if pad != desk {
        using.set_if_neq(UsingPad(pad));
    }
}

fn note_labels(mut keys: MessageReader<KeyboardInput>, mut labels: ResMut<KeyLabels>) {
    for k in keys.read() {
        if labels.0.get(&k.key_code) != Some(&k.logical_key) {
            labels.0.insert(k.key_code, k.logical_key.clone());
        }
    }
}

/// `bindings` (the settings'), and natively and in the browser the wheel latch, key labels and the
/// device in use.
pub fn build(app: &mut App, bindings: Bindings, windowed: bool) {
    app.insert_resource(bindings);
    app.init_resource::<Wheel>();
    if windowed {
        app.init_resource::<KeyLabels>();
        app.init_resource::<UsingPad>();
        app.add_systems(
            PreUpdate,
            (gather_wheel, note_labels, note_device).after(bevy::input::InputSystems),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_round_trip_by_name_and_esc_and_start_never_bind() {
        for b in [
            Bind::Key(KeyCode::KeyW),
            Bind::Key(KeyCode::ShiftLeft),
            Bind::Key(KeyCode::F8),
            Bind::Mouse(MouseButton::Left),
            Bind::Mouse(MouseButton::Back),
            Bind::Mouse(MouseButton::Forward),
            Bind::WheelUp,
            Bind::WheelDown,
        ] {
            assert_eq!(Bind::named(&b.name()), Some(b), "{}", b.name());
        }
        assert_eq!(Bind::Mouse(MouseButton::Back).name(), "MouseBack");
        assert_eq!(
            pad_named("RightTrigger2"),
            Some(GamepadButton::RightTrigger2)
        );
        assert_eq!(Bind::named("Escape"), None);
        assert_eq!(pad_named("Start"), None);
        assert_eq!(Bind::named("KeyNope"), None);
        let d = Bindings::default();
        for a in Action::ALL {
            assert_eq!(Action::named(&a.name()), Some(a));
            assert_eq!(Slots::named(&d.get(a).names()), Some(*d.get(a)));
        }
    }

    #[test]
    fn only_overrides_are_stored_and_bad_ones_keep_the_default() {
        let mut b = Bindings::default();
        assert!(b.overrides().is_empty());
        let jump = Slots {
            keys: [Some(Bind::Mouse(MouseButton::Forward)), None],
            pad: Some(GamepadButton::North),
        };
        b.set(Action::Jump, jump);
        let stored = b.overrides();
        assert_eq!(
            stored.into_iter().collect::<Vec<_>>(),
            vec![(
                "Jump".to_string(),
                [Some("MouseForward".into()), None, Some("North".into())]
            )]
        );
        assert_eq!(Bindings::with(&b.overrides()), b);
        let bad = BTreeMap::from([
            ("Gone".to_string(), [None, None, None]),
            ("Fire".to_string(), [Some("Escape".into()), None, None]),
        ]);
        assert_eq!(Bindings::with(&bad), Bindings::default());
    }

    fn read(keys: &[KeyCode], mouse: &[MouseButton], wheel: f32) -> Input {
        let (mut k, mut m) = (ButtonInput::default(), ButtonInput::default());
        keys.iter().for_each(|&c| k.press(c));
        mouse.iter().for_each(|&b| m.press(b));
        let d = Devices {
            keys: Some(&k),
            mouse: Some(&m),
            wheel,
            ..Devices::default()
        };
        device_input(&Bindings::default(), &d).input
    }

    #[test]
    fn the_defaults_send_todays_n64_pad() {
        use KeyCode::*;
        let i = read(&[KeyW, KeyD, Space], &[], 0.0);
        assert_eq!((i.buttons, i.stick_x, i.stick_y), (BUTTON_A, 80, 80));
        // Left Shift walks (half stick); C crouches (Z); Ctrl and Alt do nothing.
        let i = read(&[KeyW, ShiftLeft, KeyC, ControlLeft, AltLeft], &[], 0.0);
        assert_eq!((i.buttons, i.stick_x, i.stick_y), (BUTTON_Z, 0, 41));
        assert_eq!(read(&[], &[], 0.0), Input::default());
        let bits =
            |keys: &[KeyCode], mouse: &[MouseButton], wheel| read(keys, mouse, wheel).buttons;
        assert_eq!(bits(&[], &[MouseButton::Left], 0.0), BUTTON_Z);
        assert_eq!(bits(&[], &[MouseButton::Right], 0.0), BUTTON_R);
        for (key, bit) in [
            (KeyR, BUTTON_A),
            (KeyQ, BUTTON_B),
            (KeyG, C_DOWN),
            (KeyF, 0),
            (KeyE, 0),
            (KeyV, 0),
            (KeyT, 0),
            (KeyY, 0),
            (KeyX, 0),
            (KeyM, 0),
            (Tab, 0),
            (F8, 0),
            (Backquote, 0),
        ] {
            assert_eq!(bits(&[key], &[], 0.0), bit, "{key:?}");
        }
        // The wheel zooms the scope while aiming only (unaimed C-down throws a grenade).
        assert_eq!(bits(&[], &[], 1.0), 0);
        assert_eq!(bits(&[], &[], -1.0), 0);
        assert_eq!(bits(&[], &[MouseButton::Right], 1.0), BUTTON_R | C_UP);
        assert_eq!(bits(&[], &[MouseButton::Right], -2.0), BUTTON_R | C_DOWN);
    }

    #[test]
    fn rebound_actions_send_their_bit_and_the_mouse_is_looked_through() {
        let mut b = Bindings::default();
        b.set(
            Action::Fire,
            Slots {
                keys: [Some(Bind::Mouse(MouseButton::Back)), None],
                pad: None,
            },
        );
        let mut m = ButtonInput::default();
        m.press(MouseButton::Back);
        let d = Devices {
            mouse: Some(&m),
            motion: Vec2::new(3.0, -2.0),
            ..Devices::default()
        };
        let out = device_input(&b, &d);
        assert_eq!(out.input.buttons, BUTTON_Z);
        assert_eq!((out.mouse, out.look), (Vec2::new(3.0, -2.0), Vec2::ZERO));
        m.release(MouseButton::Back);
        m.press(MouseButton::Left);
        let d = Devices {
            mouse: Some(&m),
            ..Devices::default()
        };
        assert_eq!(device_input(&b, &d).input.buttons, 0);
    }

    /// A notch holds the wheel for the setting's reads; notches add up; one the other way drops
    /// the rest.
    #[test]
    fn a_wheel_notch_holds_for_its_ticks() {
        let mut w = Wheel::default();
        let reads = |w: &mut Wheel, n: usize| (0..n).map(|_| w.read(3)).collect::<Vec<_>>();
        w.notches = 1;
        assert_eq!(reads(&mut w, 4), [1.0, 1.0, 1.0, 0.0]);
        // Two notches, in two frames before a read or one read apart: six reads.
        w.notches = 2;
        assert_eq!(reads(&mut w, 7), [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]);
        w.notches = 1;
        w.read(3);
        w.notches = 1;
        assert_eq!(reads(&mut w, 6), [1.0, 1.0, 1.0, 1.0, 1.0, 0.0]);
        // Down mid-hold: the hold up is dropped.
        w.notches = 1;
        w.read(3);
        w.notches = -1;
        assert_eq!(reads(&mut w, 4), [-1.0, -1.0, -1.0, 0.0]);
    }
}
