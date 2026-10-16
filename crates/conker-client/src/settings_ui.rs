//! The settings screen's model (docs/CONTROLS_RESEARCH.md §5, §6), drawn and driven by the Esc menu
//! ([`crate::menu`]): its tabs and rows (grouped Movement / Combat / Camera / UI), each v1 setting
//! with its range and step, the rebinding capture ([`Capture`]: [`TIMEOUT`], Esc cancels, Backspace
//! clears, the opening press let go first, a trigger past [`TRIGGER`]) and its conflicts (Swap /
//! Replace / Cancel), and the bindings' labels (the AZERTY cache, the pad's glyphs).
//!
//! The 20 actions all play in every mode (Tank and Race read the same Fire, Jump…), so they share
//! one context: a key bound twice is a conflict, but for the pad's X, Reload's tap and Interact's
//! hold by design.

use crate::bindings::{Action, Bind, Bindings, KeyLabels};
use crate::settings::{Colour, Mode, Prompts, Settings};
use bevy::input::keyboard::Key;
use bevy::prelude::*;

/// The prompt while a slot waits for its input (the font has no dash but `-`).
pub const PROMPT: &str = "PRESS A KEY OR BUTTON - ESC CANCELS, BACKSPACE CLEARS";
/// Seconds a capture waits.
pub const TIMEOUT: f64 = 5.0;
/// A trigger counts pushed past this (Bevy's own press is at 0.75).
pub const TRIGGER: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Keys,
    Pad,
    Look,
    Gameplay,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Keys, Tab::Pad, Tab::Look, Tab::Gameplay];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Keys => "KEYBOARD & MOUSE",
            Tab::Pad => "CONTROLLER",
            Tab::Look => "MOUSE/LOOK",
            Tab::Gameplay => "GAMEPLAY",
        }
    }
}

/// A line of a tab: a group's title, an action's slots or a setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Group(&'static str),
    Bind(Action),
    Value(Field),
}

/// The actions by group, each once.
pub const GROUPS: [(&str, &[Action]); 4] = {
    use Action::*;
    [
        (
            "MOVEMENT",
            &[
                MoveForward,
                MoveBack,
                MoveLeft,
                MoveRight,
                Jump,
                Crouch,
                Walk,
            ],
        ),
        (
            "COMBAT",
            &[
                Fire,
                Aim,
                ScopeZoomIn,
                ScopeZoomOut,
                Reload,
                Holster,
                Interact,
                Grenade,
            ],
        ),
        ("CAMERA", &[ShoulderSwap, DebugCamera]),
        ("UI", &[Scoreboard, Mute, TextHud, Diagnostics]),
    ]
};

/// A tab's settings in a group (after the group's bindings on the binding tabs).
fn fields(tab: Tab, group: &str) -> &'static [Field] {
    use Field::*;
    match (tab, group) {
        (Tab::Pad, "MOVEMENT") => &[StickLayout, LeftDeadZone],
        (Tab::Pad, "CAMERA") => &[RightDeadZone],
        (Tab::Pad, "UI") => &[ButtonPrompts, Vibration],
        (Tab::Look, "CAMERA") => &[
            MouseSensitivity,
            MouseAim,
            MouseInvert,
            WheelZoom,
            PadSensitivity,
            PadAim,
            PadInvert,
        ],
        (Tab::Gameplay, "MOVEMENT") => &[CrouchKeys, CrouchPad, WalkKeys, WalkPad],
        (Tab::Gameplay, "COMBAT") => &[AimKeys, AimPad],
        (Tab::Gameplay, "UI") => &[CrosshairColour, CrosshairSize],
        _ => &[],
    }
}

/// A tab's rows: Keyboard & Mouse and Controller bind every action (two slots, one), with the
/// pad's settings on Controller; Mouse/Look and Gameplay hold the other settings.
pub fn rows(tab: Tab) -> Vec<Row> {
    let binds = matches!(tab, Tab::Keys | Tab::Pad);
    let mut out = Vec::new();
    for (group, actions) in GROUPS {
        let fields = fields(tab, group);
        if !binds && fields.is_empty() {
            continue;
        }
        out.push(Row::Group(group));
        if binds {
            out.extend(actions.iter().map(|&a| Row::Bind(a)));
        }
        out.extend(fields.iter().map(|&f| Row::Value(f)));
    }
    out
}

/// A v1 setting of §5's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    MouseSensitivity,
    MouseAim,
    MouseInvert,
    WheelZoom,
    PadSensitivity,
    PadAim,
    PadInvert,
    StickLayout,
    LeftDeadZone,
    RightDeadZone,
    Vibration,
    ButtonPrompts,
    AimKeys,
    AimPad,
    CrouchKeys,
    CrouchPad,
    WalkKeys,
    WalkPad,
    CrosshairColour,
    CrosshairSize,
}

/// A setting's value in [`Settings`].
enum Value<'a> {
    /// The value, its range and step.
    Number(&'a mut f32, [f32; 3]),
    /// Off and on's words.
    Toggle(&'a mut bool, [&'static str; 2]),
    Mode(&'a mut Mode),
    Prompts(&'a mut Prompts),
    Colour(&'a mut Colour),
}

/// The choice `dir` steps from `v` among `all`, wrapping.
fn cycle<T: Copy + PartialEq>(all: &[T], v: T, dir: i32) -> T {
    let i = all.iter().position(|x| *x == v).unwrap_or(0) as i32;
    all[(i + dir).rem_euclid(all.len() as i32) as usize]
}

impl Field {
    pub fn label(self) -> &'static str {
        use Field::*;
        match self {
            MouseSensitivity => "MOUSE SENSITIVITY",
            MouseAim => "MOUSE AIM MULTIPLIER",
            MouseInvert => "MOUSE INVERT Y",
            WheelZoom => "WHEEL ZOOM SPEED",
            PadSensitivity => "STICK SENSITIVITY",
            PadAim => "STICK AIM MULTIPLIER",
            PadInvert => "STICK INVERT Y",
            StickLayout => "STICK LAYOUT",
            LeftDeadZone => "LEFT DEAD ZONE",
            RightDeadZone => "RIGHT DEAD ZONE",
            Vibration => "VIBRATION",
            ButtonPrompts => "BUTTON PROMPTS",
            AimKeys => "AIM (K&M)",
            AimPad => "AIM (PAD)",
            CrouchKeys => "CROUCH (K&M)",
            CrouchPad => "CROUCH (PAD)",
            WalkKeys => "WALK (K&M)",
            WalkPad => "WALK (PAD)",
            CrosshairColour => "CROSSHAIR COLOUR",
            CrosshairSize => "CROSSHAIR SIZE",
        }
    }

    /// Ranges and steps as [`Settings::clamped`]'s.
    fn value(self, s: &mut Settings) -> Value<'_> {
        use Field::*;
        let (m, p, c) = (&mut s.mouse, &mut s.pad, &mut s.crosshair);
        let [left, right] = &mut p.dead_zone;
        let on = ["OFF", "ON"];
        match self {
            MouseSensitivity => Value::Number(&mut m.sensitivity, [0.1, 10.0, 0.1]),
            MouseAim => Value::Number(&mut m.aim_multiplier, [0.1, 4.0, 0.05]),
            MouseInvert => Value::Toggle(&mut m.invert_y, on),
            WheelZoom => Value::Number(&mut m.wheel_zoom, [1.0, 10.0, 1.0]),
            PadSensitivity => Value::Number(&mut p.sensitivity, [0.5, 2.0, 0.05]),
            PadAim => Value::Number(&mut p.aim_multiplier, [0.1, 4.0, 0.05]),
            PadInvert => Value::Toggle(&mut p.invert_y, on),
            StickLayout => Value::Toggle(&mut p.southpaw, ["DEFAULT", "SOUTHPAW"]),
            LeftDeadZone => Value::Number(left, [0.0, 0.5, 0.01]),
            RightDeadZone => Value::Number(right, [0.0, 0.5, 0.01]),
            Vibration => Value::Toggle(&mut p.vibration, on),
            ButtonPrompts => Value::Prompts(&mut p.prompts),
            AimKeys => Value::Mode(&mut m.aim),
            AimPad => Value::Mode(&mut p.aim),
            CrouchKeys => Value::Mode(&mut m.crouch),
            CrouchPad => Value::Mode(&mut p.crouch),
            WalkKeys => Value::Mode(&mut m.walk),
            WalkPad => Value::Mode(&mut p.walk),
            CrosshairColour => Value::Colour(&mut c.colour),
            CrosshairSize => Value::Number(&mut c.size, [0.5, 2.0, 0.05]),
        }
    }

    /// One step up (`dir` 1) or down (-1): a number by its step within its range, a toggle flips,
    /// a choice cycles.
    pub fn step(self, s: &mut Settings, dir: i32) {
        match self.value(s) {
            Value::Number(v, [lo, hi, step]) => {
                *v = (((*v / step).round() + dir as f32) * step).clamp(lo, hi)
            }
            Value::Toggle(v, _) => *v = !*v,
            Value::Mode(v) => *v = cycle(&[Mode::Hold, Mode::Toggle], *v, dir),
            Value::Prompts(v) => {
                *v = cycle(
                    &[Prompts::Auto, Prompts::Xbox, Prompts::PlayStation],
                    *v,
                    dir,
                )
            }
            Value::Colour(v) => {
                use Colour::*;
                *v = cycle(&[Red, Green, Yellow, Blue, Cyan], *v, dir)
            }
        }
    }

    /// Back to its default.
    pub fn reset(self, s: &mut Settings) {
        let mut d = Settings::default();
        match (self.value(s), self.value(&mut d)) {
            (Value::Number(v, _), Value::Number(w, _)) => *v = *w,
            (Value::Toggle(v, _), Value::Toggle(w, _)) => *v = *w,
            (Value::Mode(v), Value::Mode(w)) => *v = *w,
            (Value::Prompts(v), Value::Prompts(w)) => *v = *w,
            (Value::Colour(v), Value::Colour(w)) => *v = *w,
            _ => {}
        }
    }

    /// The value as the row shows it.
    pub fn text(self, s: &Settings) -> String {
        let text = match self.value(&mut s.clone()) {
            Value::Number(v, [.., step]) => {
                let places = match step {
                    s if s < 0.1 => 2,
                    s if s < 1.0 => 1,
                    _ => 0,
                };
                format!("{:.*}", places, *v)
            }
            Value::Toggle(v, words) => words[usize::from(*v)].into(),
            Value::Mode(v) => format!("{v:?}"),
            Value::Prompts(v) => format!("{v:?}"),
            Value::Colour(v) => format!("{v:?}"),
        };
        text.to_uppercase()
    }
}

/// A slot of an action: keyboard/mouse primary (0) or alternate (1), or the pad's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Key(usize),
    Pad,
}

impl Slot {
    pub fn label(self) -> &'static str {
        match self {
            Slot::Key(0) => "PRIMARY",
            Slot::Key(_) => "ALTERNATE",
            Slot::Pad => "CONTROLLER",
        }
    }
}

/// What a slot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    Key(Bind),
    Pad(GamepadButton),
}

pub fn get(b: &Bindings, a: Action, slot: Slot) -> Option<Bound> {
    let s = b.get(a);
    match slot {
        Slot::Key(i) => s.keys[i].map(Bound::Key),
        Slot::Pad => s.pad.map(Bound::Pad),
    }
}

/// `v` into `a`'s `slot` (none clears it); another device's input doesn't fit and is dropped.
pub fn set(b: &mut Bindings, a: Action, slot: Slot, v: Option<Bound>) {
    let mut s = *b.get(a);
    match (slot, v) {
        (Slot::Key(i), None) => s.keys[i] = None,
        (Slot::Key(i), Some(Bound::Key(k))) => s.keys[i] = Some(k),
        (Slot::Pad, None) => s.pad = None,
        (Slot::Pad, Some(Bound::Pad(p))) => s.pad = Some(p),
        _ => return,
    }
    b.set(a, s);
}

/// The other slots holding `v`, but the pad's tap / hold pair (Reload and Interact on X).
pub fn conflicts(b: &Bindings, a: Action, slot: Slot, v: Bound) -> Vec<(Action, Slot)> {
    use Action::{Interact, Reload};
    let pair =
        |x: Action| slot == Slot::Pad && matches!((a, x), (Reload, Interact) | (Interact, Reload));
    (Action::ALL.into_iter())
        .flat_map(|x| [Slot::Key(0), Slot::Key(1), Slot::Pad].map(|s| (x, s)))
        .filter(|&(x, s)| (x, s) != (a, slot) && get(b, x, s) == Some(v) && !pair(x))
        .collect()
}

/// How a conflict is settled: the other slots take this slot's old input, or are cleared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolve {
    Swap,
    Replace,
}

/// `v` into `a`'s `slot`, its conflicts settled `how`.
pub fn bind(b: &mut Bindings, a: Action, slot: Slot, v: Bound, how: Resolve) {
    let old = get(b, a, slot);
    for (x, s) in conflicts(b, a, slot, v) {
        set(b, x, s, if how == Resolve::Swap { old } else { None });
    }
    set(b, a, slot, Some(v));
}

/// DEFAULTS: tab `tab`'s rows back to their defaults (a binding tab: its slots of every action).
pub fn reset(tab: Tab, s: &mut Settings, b: &mut Bindings) {
    let d = Bindings::default();
    for row in rows(tab) {
        match row {
            Row::Bind(a) => {
                let slots = match tab {
                    Tab::Pad => [Slot::Pad].as_slice(),
                    _ => &[Slot::Key(0), Slot::Key(1)],
                };
                for &slot in slots {
                    set(b, a, slot, get(&d, a, slot));
                }
            }
            Row::Value(f) => f.reset(s),
            Row::Group(_) => {}
        }
    }
    s.bindings = b.overrides();
}

/// What the devices hold this frame: keys, mouse buttons, the wheel's turn, the pad's buttons
/// (a trigger past [`TRIGGER`], [`held_pad`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Held {
    pub keys: Vec<KeyCode>,
    pub mouse: Vec<MouseButton>,
    pub wheel: f32,
    pub pad: Vec<GamepadButton>,
}

/// The pad's held buttons, its triggers by their travel past [`TRIGGER`].
pub fn held_pad(pad: &Gamepad) -> Vec<GamepadButton> {
    use GamepadButton::{LeftTrigger2, RightTrigger2};
    let triggers = [LeftTrigger2, RightTrigger2];
    let mut out: Vec<_> = (pad.digital().get_pressed().copied())
        .filter(|b| !triggers.contains(b))
        .collect();
    out.extend(
        triggers
            .into_iter()
            .filter(|&t| pad.get(t).unwrap_or(0.0) > TRIGGER),
    );
    out
}

/// A capture's end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Captured {
    Cancel,
    Clear,
    Input(Bound),
}

/// A slot waiting for its input ([`PROMPT`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capture {
    pub action: Action,
    pub slot: Slot,
    pub since: f64,
    /// Everything was let go since it opened: the opening click or press is not the input.
    armed: bool,
}

impl Capture {
    pub fn new(action: Action, slot: Slot, now: f64) -> Self {
        Self {
            action,
            slot,
            since: now,
            armed: false,
        }
    }

    /// Whole seconds left, as shown.
    pub fn left(&self, now: f64) -> u32 {
        (TIMEOUT - (now - self.since)).max(0.0).ceil() as u32
    }

    /// One frame: Esc (or the pad's Menu, which never binds either) and the timeout cancel,
    /// Backspace clears; once all was let go, the slot's device's first input (a key, mouse
    /// buttons 1-5, the wheel; a pad button) is the one.
    pub fn step(&mut self, now: f64, h: &Held) -> Option<Captured> {
        let menu = h.keys.contains(&KeyCode::Escape) || h.pad.contains(&GamepadButton::Start);
        if menu || now - self.since >= TIMEOUT {
            return Some(Captured::Cancel);
        }
        if h.keys.contains(&KeyCode::Backspace) {
            return Some(Captured::Clear);
        }
        if !self.armed {
            self.armed = h.keys.is_empty() && h.mouse.is_empty() && h.pad.is_empty();
            return None;
        }
        let input = match self.slot {
            Slot::Key(_) => {
                let mouse = h.mouse.iter().find(|m| !matches!(m, MouseButton::Other(_)));
                let wheel = match h.wheel {
                    w if w > 0.0 => Some(Bind::WheelUp),
                    w if w < 0.0 => Some(Bind::WheelDown),
                    _ => None,
                };
                (h.keys.first().map(|&k| Bind::Key(k)))
                    .or(mouse.map(|&m| Bind::Mouse(m)))
                    .or(wheel)
                    .map(Bound::Key)
            }
            Slot::Pad => (h.pad.iter())
                .find(|p| !matches!(p, GamepadButton::Other(_)))
                .map(|&p| Bound::Pad(p)),
        };
        input.map(Captured::Input)
    }
}

/// An action's name in words: MoveForward → MOVE FORWARD.
pub fn action_label(a: Action) -> String {
    let mut out = String::new();
    for c in a.name().chars() {
        if c.is_uppercase() && !out.is_empty() {
            out.push(' ');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// A key as its player knows it: what it types on their layout (the AZERTY cache, letters and
/// digits; not AZERTY's ², which the HUD font lacks), else its physical name shortened (L SHIFT,
/// UP, NUM 1).
pub fn key_label(k: KeyCode, labels: Option<&KeyLabels>) -> String {
    if let Some(Key::Character(c)) = labels.and_then(|l| l.0.get(&k))
        && c.chars().all(|c| c.is_alphabetic() || c.is_ascii_digit())
    {
        return c.to_uppercase();
    }
    let name = format!("{k:?}");
    if let Some(r) = name.strip_prefix("Arrow") {
        return r.to_uppercase();
    }
    let short = name.strip_prefix("Key").or(name.strip_prefix("Digit"));
    if let Some(r) = short.filter(|r| r.len() == 1) {
        return r.into();
    }
    if let Some(r) = name.strip_prefix("Numpad") {
        return format!("NUM {}", r.to_uppercase());
    }
    for (end, side) in [("Left", "L"), ("Right", "R")] {
        if let Some(r) = name.strip_suffix(end) {
            let r = match r {
                "Control" => "CTRL",
                "Super" => "META",
                r => r,
            };
            return format!("{side} {}", r.to_uppercase());
        }
    }
    name.to_uppercase()
}

/// The pad's button names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyphs {
    Xbox,
    PlayStation,
}

/// The pad's glyphs: the setting's, else (Auto) by its USB vendor (054c Sony, 045e Microsoft) or
/// its name, else Xbox.
pub fn glyphs(setting: Prompts, vendor: Option<u16>, name: &str) -> Glyphs {
    let name = name.to_lowercase();
    let sony = ["playstation", "dualsense", "dualshock", "sony"];
    match (setting, vendor) {
        (Prompts::Xbox, _) => Glyphs::Xbox,
        (Prompts::PlayStation, _) | (Prompts::Auto, Some(0x054C)) => Glyphs::PlayStation,
        (Prompts::Auto, Some(0x045E)) => Glyphs::Xbox,
        _ if sony.iter().any(|s| name.contains(s)) => Glyphs::PlayStation,
        _ => Glyphs::Xbox,
    }
}

/// The first pad's glyphs ([`glyphs`] by its vendor and name), the setting's without one.
pub fn pad_glyphs(setting: Prompts, pad: Option<(&Gamepad, Option<&Name>)>) -> Glyphs {
    match pad {
        Some((p, name)) => glyphs(setting, p.vendor_id(), name.map_or("", |n| n.as_str())),
        None => glyphs(setting, None, ""),
    }
}

pub fn pad_label(b: GamepadButton, g: Glyphs) -> String {
    use GamepadButton::*;
    let pick = |xbox: &'static str, ps: &'static str| match g {
        Glyphs::Xbox => xbox,
        Glyphs::PlayStation => ps,
    };
    match b {
        South => pick("A", "CROSS"),
        East => pick("B", "CIRCLE"),
        West => pick("X", "SQUARE"),
        North => pick("Y", "TRIANGLE"),
        LeftTrigger => pick("LB", "L1"),
        RightTrigger => pick("RB", "R1"),
        LeftTrigger2 => pick("LT", "L2"),
        RightTrigger2 => pick("RT", "R2"),
        Select => pick("VIEW", "CREATE"),
        Start => pick("MENU", "OPTIONS"),
        GamepadButton::Mode => pick("GUIDE", "PS"),
        LeftThumb => pick("LS", "L3"),
        RightThumb => pick("RS", "R3"),
        DPadUp => "D-PAD UP",
        DPadDown => "D-PAD DOWN",
        DPadLeft => "D-PAD LEFT",
        DPadRight => "D-PAD RIGHT",
        C => "C",
        Z => "Z",
        Other(n) => return format!("BUTTON {n}"),
    }
    .into()
}

/// A slot's input as shown (`-`: none).
pub fn bound_label(v: Option<Bound>, labels: Option<&KeyLabels>, g: Glyphs) -> String {
    match v {
        None => "-".into(),
        Some(Bound::Key(Bind::Key(k))) => key_label(k, labels),
        Some(Bound::Key(Bind::Mouse(m))) => match m {
            MouseButton::Left => "MOUSE 1".into(),
            MouseButton::Right => "MOUSE 2".into(),
            MouseButton::Middle => "MOUSE 3".into(),
            MouseButton::Back => "MOUSE 4".into(),
            MouseButton::Forward => "MOUSE 5".into(),
            MouseButton::Other(n) => format!("MOUSE {n}"),
        },
        Some(Bound::Key(Bind::WheelUp)) => "WHEEL UP".into(),
        Some(Bound::Key(Bind::WheelDown)) => "WHEEL DOWN".into(),
        Some(Bound::Pad(p)) => pad_label(p, g),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_and_setting_has_one_row() {
        let mut actions: Vec<Action> = GROUPS.iter().flat_map(|g| g.1.iter().copied()).collect();
        actions.sort_by_key(|a| *a as usize);
        assert_eq!(actions, Action::ALL.to_vec());
        for tab in [Tab::Keys, Tab::Pad] {
            let binds = rows(tab)
                .iter()
                .filter(|r| matches!(r, Row::Bind(_)))
                .count();
            assert_eq!(binds, Action::ALL.len(), "{tab:?}");
        }
        let fields: Vec<Field> = (Tab::ALL.iter().flat_map(|&t| rows(t)))
            .filter_map(|r| match r {
                Row::Value(f) => Some(f),
                _ => None,
            })
            .collect();
        assert_eq!(fields.len(), 20);
        for f in &fields {
            assert_eq!(fields.iter().filter(|g| *g == f).count(), 1, "{f:?}");
        }
        // Every row sits under one of the four groups.
        for tab in Tab::ALL {
            assert!(matches!(rows(tab)[0], Row::Group(_)), "{tab:?}");
        }
    }

    #[test]
    fn values_step_within_their_ranges() {
        let mut s = Settings::default();
        Field::MouseSensitivity.step(&mut s, 1);
        assert!((s.mouse.sensitivity - 1.6).abs() < 1e-5);
        assert_eq!(Field::MouseSensitivity.text(&s), "1.6");
        for _ in 0..200 {
            Field::MouseSensitivity.step(&mut s, 1);
            Field::LeftDeadZone.step(&mut s, -1);
            Field::CrosshairSize.step(&mut s, 1);
        }
        assert_eq!(s.mouse.sensitivity, 10.0);
        assert_eq!((s.pad.dead_zone, s.crosshair.size), ([0.0, 0.1], 2.0));
        assert_eq!(Field::LeftDeadZone.text(&s), "0.00");
        Field::PadInvert.step(&mut s, -1);
        Field::StickLayout.step(&mut s, 1);
        assert!(s.pad.invert_y && s.pad.southpaw);
        assert_eq!(Field::StickLayout.text(&s), "SOUTHPAW");
        Field::AimKeys.step(&mut s, 1);
        assert_eq!(Field::AimKeys.text(&s), "TOGGLE");
        Field::ButtonPrompts.step(&mut s, -1);
        assert_eq!(s.pad.prompts, Prompts::PlayStation);
        Field::CrosshairColour.step(&mut s, 1);
        assert_eq!(Field::CrosshairColour.text(&s), "YELLOW");
        assert_eq!(s.clone().clamped(), s);
    }

    fn keys(k: &[KeyCode]) -> Held {
        Held {
            keys: k.to_vec(),
            ..Held::default()
        }
    }

    #[test]
    fn a_capture_waits_for_the_opening_press_to_go_and_times_out() {
        let mut c = Capture::new(Action::Jump, Slot::Key(0), 10.0);
        let click = Held {
            mouse: vec![MouseButton::Left],
            ..Held::default()
        };
        // The click that opened it, held: not the input.
        assert_eq!(c.step(10.0, &click), None);
        assert_eq!(c.step(10.1, &click), None);
        assert_eq!(c.step(10.2, &Held::default()), None);
        let k = Bound::Key(Bind::Key(KeyCode::KeyJ));
        assert_eq!(
            c.step(10.3, &keys(&[KeyCode::KeyJ])),
            Some(Captured::Input(k))
        );
        assert_eq!(c.left(10.3), 5);
        assert_eq!(c.left(14.5), 1);
        // A held key keeps it unarmed; nothing for 5 s: cancelled.
        let mut c = Capture::new(Action::Jump, Slot::Key(1), 0.0);
        assert_eq!(c.step(1.0, &keys(&[KeyCode::KeyW])), None);
        assert_eq!(c.step(2.0, &keys(&[KeyCode::KeyW])), None);
        assert_eq!(c.step(4.9, &Held::default()), None);
        assert_eq!(c.step(5.0, &Held::default()), Some(Captured::Cancel));
    }

    #[test]
    fn esc_cancels_backspace_clears_and_the_menu_buttons_never_bind() {
        let mut c = Capture::new(Action::Fire, Slot::Key(0), 0.0);
        c.step(0.0, &Held::default());
        assert_eq!(
            c.step(0.1, &keys(&[KeyCode::Escape])),
            Some(Captured::Cancel)
        );
        assert_eq!(
            c.step(0.1, &keys(&[KeyCode::Backspace])),
            Some(Captured::Clear)
        );
        let mut p = Capture::new(Action::Fire, Slot::Pad, 0.0);
        p.step(0.0, &Held::default());
        let start = Held {
            pad: vec![GamepadButton::Start],
            ..Held::default()
        };
        assert_eq!(p.step(0.1, &start), Some(Captured::Cancel));
        // A key slot takes mouse buttons 1-5 and the wheel, not the pad (nor mouse 6+).
        let other = Held {
            mouse: vec![MouseButton::Other(7)],
            pad: vec![GamepadButton::North],
            ..Held::default()
        };
        assert_eq!(c.step(0.2, &other), None);
        let back = Held {
            mouse: vec![MouseButton::Back],
            ..Held::default()
        };
        let mouse4 = Bound::Key(Bind::Mouse(MouseButton::Back));
        assert_eq!(c.step(0.3, &back), Some(Captured::Input(mouse4)));
        let wheel = Held {
            wheel: -1.0,
            ..Held::default()
        };
        let down = Bound::Key(Bind::WheelDown);
        assert_eq!(c.step(0.4, &wheel), Some(Captured::Input(down)));
        // A pad slot takes the pad only.
        assert_eq!(p.step(0.2, &keys(&[KeyCode::KeyJ])), None);
        let north = Bound::Pad(GamepadButton::North);
        assert_eq!(p.step(0.3, &other), Some(Captured::Input(north)));
    }

    #[test]
    fn a_trigger_counts_past_half_its_travel() {
        let mut pad = Gamepad::default();
        let rt = GamepadButton::RightTrigger2;
        pad.analog_mut().set(rt, 0.4);
        assert_eq!(held_pad(&pad), vec![]);
        pad.analog_mut().set(rt, 0.6);
        assert_eq!(held_pad(&pad), vec![rt]);
        // Held as the capture opens, it counts once let go and pushed again.
        let mut c = Capture::new(Action::Fire, Slot::Pad, 0.0);
        let held = Held {
            pad: held_pad(&pad),
            ..Held::default()
        };
        assert_eq!(c.step(0.1, &held), None);
        pad.analog_mut().set(rt, 0.3);
        let let_go = Held {
            pad: held_pad(&pad),
            ..Held::default()
        };
        assert_eq!(c.step(0.2, &let_go), None);
        assert_eq!(c.step(0.3, &held), Some(Captured::Input(Bound::Pad(rt))));
    }

    #[test]
    fn conflicts_swap_or_replace_and_the_pads_x_pair_is_none() {
        let d = Bindings::default();
        let w = Bound::Key(Bind::Key(KeyCode::KeyW));
        assert_eq!(
            conflicts(&d, Action::Jump, Slot::Key(0), w),
            vec![(Action::MoveForward, Slot::Key(0))]
        );
        // Its own slot is no conflict; the other slot of the same action is.
        assert!(conflicts(&d, Action::MoveForward, Slot::Key(0), w).is_empty());
        assert_eq!(
            conflicts(&d, Action::MoveForward, Slot::Key(1), w),
            vec![(Action::MoveForward, Slot::Key(0))]
        );
        // Reload and Interact share the pad's X by design; another action on X conflicts with both.
        let x = Bound::Pad(GamepadButton::West);
        assert!(conflicts(&d, Action::Reload, Slot::Pad, x).is_empty());
        assert_eq!(conflicts(&d, Action::Jump, Slot::Pad, x).len(), 2);
        // Swap: Move Forward takes Space; Replace: it has none.
        let mut b = d.clone();
        bind(&mut b, Action::Jump, Slot::Key(0), w, Resolve::Swap);
        assert_eq!(get(&b, Action::Jump, Slot::Key(0)), Some(w));
        let space = Bound::Key(Bind::Key(KeyCode::Space));
        assert_eq!(get(&b, Action::MoveForward, Slot::Key(0)), Some(space));
        let mut b = d.clone();
        bind(&mut b, Action::Jump, Slot::Key(0), w, Resolve::Replace);
        assert_eq!(get(&b, Action::MoveForward, Slot::Key(0)), None);
        assert_eq!(
            get(&b, Action::MoveForward, Slot::Key(1)),
            get(&d, Action::MoveForward, Slot::Key(1))
        );
        // Another device's input never lands in a slot.
        set(&mut b, Action::Jump, Slot::Pad, Some(w));
        assert_eq!(b.get(Action::Jump).pad, d.get(Action::Jump).pad);
    }

    #[test]
    fn labels_show_the_players_keys_and_pad() {
        assert_eq!(action_label(Action::MoveForward), "MOVE FORWARD");
        assert_eq!(action_label(Action::TextHud), "TEXT HUD");
        let mut l = KeyLabels::default();
        assert_eq!(key_label(KeyCode::KeyW, Some(&l)), "W");
        // AZERTY: the physical W types z; the comma's key types ";" (not a letter: its name).
        l.0.insert(KeyCode::KeyW, Key::Character("z".into()));
        l.0.insert(KeyCode::KeyM, Key::Character(",".into()));
        assert_eq!(key_label(KeyCode::KeyW, Some(&l)), "Z");
        assert_eq!(key_label(KeyCode::KeyM, Some(&l)), "M");
        l.0.insert(KeyCode::Backquote, Key::Character("²".into()));
        assert_eq!(key_label(KeyCode::Backquote, Some(&l)), "BACKQUOTE");
        for (k, label) in [
            (KeyCode::ShiftLeft, "L SHIFT"),
            (KeyCode::ControlRight, "R CTRL"),
            (KeyCode::ArrowUp, "UP"),
            (KeyCode::Digit1, "1"),
            (KeyCode::Numpad1, "NUM 1"),
            (KeyCode::Space, "SPACE"),
            (KeyCode::F8, "F8"),
        ] {
            assert_eq!(key_label(k, None), label);
        }
        let g = Glyphs::Xbox;
        let mouse = |m| bound_label(Some(Bound::Key(Bind::Mouse(m))), None, g);
        assert_eq!(mouse(MouseButton::Forward), "MOUSE 5");
        assert_eq!(bound_label(None, None, g), "-");
        // Glyphs: the vendor, else the name, else Xbox; the setting overrides.
        assert_eq!(
            glyphs(Prompts::Auto, Some(0x054C), "Gamepad"),
            Glyphs::PlayStation
        );
        assert_eq!(
            glyphs(Prompts::Auto, Some(0x045E), "Xbox Controller"),
            Glyphs::Xbox
        );
        assert_eq!(
            glyphs(Prompts::Auto, None, "DualSense Wireless Controller"),
            Glyphs::PlayStation
        );
        assert_eq!(glyphs(Prompts::Auto, None, "8BitDo Pro 2"), Glyphs::Xbox);
        assert_eq!(glyphs(Prompts::Xbox, Some(0x054C), ""), Glyphs::Xbox);
        assert_eq!(glyphs(Prompts::PlayStation, None, ""), Glyphs::PlayStation);
        let rt = GamepadButton::RightTrigger2;
        assert_eq!(
            (
                pad_label(rt, Glyphs::Xbox),
                pad_label(rt, Glyphs::PlayStation)
            ),
            ("RT".into(), "R2".into())
        );
        assert_eq!(
            pad_label(GamepadButton::South, Glyphs::PlayStation),
            "CROSS"
        );
    }
}
