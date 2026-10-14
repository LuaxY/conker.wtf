//! The player's settings (CONTROLS_RESEARCH.md §5, "v1 settings"), saved as JSON: in the browser
//! in localStorage, natively in `$CONKER_SETTINGS` if set, else `<config dir>/conker/settings.json`.
//! Only the bindings that differ from the defaults are stored (by action name), so new actions get
//! their defaults; missing values take theirs, and values out of range are clamped. A file that
//! doesn't parse gives the defaults and is kept as `.bak`. Nothing applies the values yet (round 2
//! and the Esc menu do), but the bindings.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const VERSION: u32 = 1;

#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// The actions bound otherwise than by default: `[primary, alternate, pad]` by action name
    /// ([`crate::bindings::Bindings::overrides`]).
    pub bindings: BTreeMap<String, [Option<String>; 3]>,
    pub mouse: Mouse,
    pub pad: Pad,
    pub crosshair: Crosshair,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            bindings: BTreeMap::new(),
            mouse: Mouse::default(),
            pad: Pad::default(),
            crosshair: Crosshair::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Hold,
    Toggle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mouse {
    /// Look, on CS's scale (0.022° a count): 0.1-10.
    pub sensitivity: f32,
    /// Aiming and scoped, FOV-normalised: 0.1-4.
    pub aim_multiplier: f32,
    pub invert_y: bool,
    pub aim: Mode,
    pub crouch: Mode,
    pub walk: Mode,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            sensitivity: 1.5,
            aim_multiplier: 1.0,
            invert_y: false,
            aim: Mode::Hold,
            crouch: Mode::Hold,
            walk: Mode::Hold,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Prompts {
    #[default]
    Auto,
    Xbox,
    PlayStation,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pad {
    /// Look (1.0: about 450°/s yaw, 270°/s pitch): 0.5-2.
    pub sensitivity: f32,
    /// Aiming and scoped: 0.1-4.
    pub aim_multiplier: f32,
    /// The look stick's.
    pub invert_y: bool,
    /// The sticks swapped.
    pub southpaw: bool,
    /// The left and the right stick's: 0-0.5.
    pub dead_zone: [f32; 2],
    pub vibration: bool,
    pub prompts: Prompts,
    pub aim: Mode,
    pub crouch: Mode,
    pub walk: Mode,
}

impl Default for Pad {
    fn default() -> Self {
        Self {
            sensitivity: 1.0,
            aim_multiplier: 1.0,
            invert_y: false,
            southpaw: false,
            dead_zone: [0.1; 2],
            vibration: true,
            prompts: Prompts::Auto,
            aim: Mode::Hold,
            crouch: Mode::Hold,
            walk: Mode::Hold,
        }
    }
}

/// The crosshair's colour presets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Colour {
    Red,
    #[default]
    Green,
    Yellow,
    Blue,
    Cyan,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crosshair {
    pub colour: Colour,
    /// 0.5-2.
    pub size: f32,
}

impl Default for Crosshair {
    fn default() -> Self {
        Self {
            colour: Colour::Green,
            size: 1.0,
        }
    }
}

impl Settings {
    /// Every value in its range.
    pub fn clamped(mut self) -> Self {
        let m = &mut self.mouse;
        m.sensitivity = m.sensitivity.clamp(0.1, 10.0);
        m.aim_multiplier = m.aim_multiplier.clamp(0.1, 4.0);
        let p = &mut self.pad;
        p.sensitivity = p.sensitivity.clamp(0.5, 2.0);
        p.aim_multiplier = p.aim_multiplier.clamp(0.1, 4.0);
        p.dead_zone = p.dead_zone.map(|d| d.clamp(0.0, 0.5));
        self.crosshair.size = self.crosshair.size.clamp(0.5, 2.0);
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialize")
    }

    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str::<Self>(text).map(Self::clamped)
    }

    /// The settings saved as `text` (None: none saved): the defaults if it doesn't parse, after
    /// `backup` keeps it.
    pub fn from_saved(text: Option<&str>, backup: impl FnOnce()) -> Self {
        let Some(text) = text else {
            return Self::default();
        };
        Self::parse(text).unwrap_or_else(|e| {
            warn!("settings: {e}: the defaults (the old ones kept as .bak)");
            backup();
            Self::default()
        })
    }
}

#[cfg(not(target_family = "wasm"))]
mod storage {
    use super::Settings;
    use std::path::{Path, PathBuf};

    /// `$CONKER_SETTINGS`, else `<config dir>/conker/settings.json`.
    pub fn path() -> Option<PathBuf> {
        let env = |k| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if let Some(p) = env("CONKER_SETTINGS") {
            return Some(p);
        }
        let config = match () {
            _ if cfg!(windows) => env("APPDATA"),
            _ if cfg!(target_os = "macos") => {
                env("HOME").map(|h| h.join("Library/Application Support"))
            }
            _ => env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config"))),
        };
        config.map(|c| c.join("conker").join("settings.json"))
    }

    /// `path` + `.bak`.
    pub fn backup_path(path: &Path) -> PathBuf {
        let mut p = path.as_os_str().to_owned();
        p.push(".bak");
        p.into()
    }

    pub fn load_file(path: &Path) -> Settings {
        let bytes = std::fs::read(path).ok();
        let text = bytes.map(|b| String::from_utf8(b).unwrap_or_default());
        Settings::from_saved(text.as_deref(), || {
            let _ = std::fs::copy(path, backup_path(path));
        })
    }

    pub fn save_file(path: &Path, settings: &Settings) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, settings.to_json())
    }

    pub fn load() -> Settings {
        let Some(p) = path() else {
            return Settings::default();
        };
        bevy::log::info!("settings: {}", p.display());
        load_file(&p)
    }

    pub fn save(settings: &Settings) {
        let saved = path().map(|p| save_file(&p, settings));
        if let Some(Err(e)) = saved {
            bevy::log::warn!("settings: not saved: {e}");
        }
    }
}

#[cfg(target_family = "wasm")]
mod storage {
    use super::Settings;

    const KEY: &str = "conker-settings";

    fn local() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn load() -> Settings {
        let local = local();
        let text = local.as_ref().and_then(|s| s.get_item(KEY).ok()?);
        Settings::from_saved(text.as_deref(), || {
            if let (Some(s), Some(t)) = (&local, &text) {
                let _ = s.set_item(&format!("{KEY}.bak"), t);
            }
        })
    }

    pub fn save(settings: &Settings) {
        if local().is_none_or(|s| s.set_item(KEY, &settings.to_json()).is_err()) {
            bevy::log::warn!("settings: not saved (no localStorage)");
        }
    }
}

pub use storage::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Action, Bind, Bindings, Slots};

    #[test]
    fn settings_round_trip_with_only_their_overrides() {
        let mut b = Bindings::default();
        b.set(
            Action::Crouch,
            Slots {
                keys: [Some(Bind::Key(KeyCode::ControlLeft)), None],
                pad: Some(GamepadButton::RightThumb),
            },
        );
        let mut s = Settings {
            bindings: b.overrides(),
            ..Settings::default()
        };
        s.mouse.sensitivity = 2.5;
        s.pad.southpaw = true;
        s.crosshair.colour = Colour::Cyan;
        let json = s.to_json();
        assert!(
            json.contains("\"Crouch\"") && !json.contains("\"Fire\""),
            "{json}"
        );
        let back = Settings::parse(&json).unwrap();
        assert_eq!(back, s);
        assert_eq!(Bindings::with(&back.bindings), b);
        assert_eq!(
            Settings::parse(&Settings::default().to_json()).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn missing_actions_and_values_take_their_defaults() {
        // An older file: one override, no pad or crosshair, an action since removed.
        let s = Settings::parse(
            r#"{"version":1,"bindings":{"Jump":["KeyJ",null,null],"Gone":[null,null,null]},
                "mouse":{"invert_y":true}}"#,
        )
        .unwrap();
        let b = Bindings::with(&s.bindings);
        assert_eq!(b.get(Action::Jump).keys[0], Some(Bind::Key(KeyCode::KeyJ)));
        let d = Bindings::default();
        for a in Action::ALL.into_iter().filter(|&a| a != Action::Jump) {
            assert_eq!(b.get(a), d.get(a), "{a:?}");
        }
        assert!(s.mouse.invert_y);
        assert_eq!(s.mouse.sensitivity, 1.5);
        assert_eq!((s.pad, s.crosshair), (Pad::default(), Crosshair::default()));
    }

    #[test]
    fn values_are_clamped_on_load() {
        let s = Settings::parse(
            r#"{"mouse":{"sensitivity":50,"aim_multiplier":0},
                "pad":{"sensitivity":0.1,"aim_multiplier":9,"dead_zone":[-1,0.7]},
                "crosshair":{"size":3}}"#,
        )
        .unwrap();
        assert_eq!((s.mouse.sensitivity, s.mouse.aim_multiplier), (10.0, 0.1));
        assert_eq!((s.pad.sensitivity, s.pad.aim_multiplier), (0.5, 4.0));
        assert_eq!((s.pad.dead_zone, s.crosshair.size), ([0.0, 0.5], 2.0));
    }

    /// A scratch directory next to `$CONKER_SETTINGS` (tests never touch the real config dir).
    #[cfg(not(target_family = "wasm"))]
    fn scratch(name: &str) -> std::path::PathBuf {
        let base = std::env::var_os("CONKER_SETTINGS")
            .map(std::path::PathBuf::from)
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(std::env::temp_dir);
        let dir = base.join(format!("conker-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn a_corrupt_file_gives_the_defaults_and_is_kept_as_bak() {
        let dir = scratch("corrupt");
        let path = dir.join("settings.json");
        assert_eq!(load_file(&path), Settings::default(), "none saved");
        assert!(!backup_path(&path).exists());
        let mut s = Settings::default();
        s.pad.vibration = false;
        save_file(&path, &s).unwrap();
        assert_eq!(load_file(&path), s);
        std::fs::write(&path, "{\"mouse\": {\"sensitivity\": ").unwrap();
        assert_eq!(load_file(&path), Settings::default());
        let bak = std::fs::read_to_string(backup_path(&path)).unwrap();
        assert_eq!(bak, "{\"mouse\": {\"sensitivity\": ");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
