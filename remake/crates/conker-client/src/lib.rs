//! Browser (wasm, WebGL2) and native client: Bevy + lightyear.
//!
//! The local player is predicted with the server's own rules on the scene's world, read from the
//! asset pack (`conker-extract pack`; the browser fetches it from the page's site): conker-rules' frame for its slot ([`Fighting`], `Match::step_player`: its
//! weapon's states, its movement, a knock-back, death and respawn), among the other players where
//! this screen shows them, then its follow camera with what the rules know of it; the camera's aim
//! gate and scope go into the next tick. It is rolled back on server corrections. Remote players are
//! interpolated. What others do to it (shots, hits, pickups, the clock) the server decides: the
//! client shows its [`Combat`] and [`MatchView`]. The camera's yaw goes out with the next input: the
//! stick is camera-relative. With each input goes what this screen showed of the others ([`Saw`]):
//! the server rewinds its hit tests to exactly that. Every player's animation clock (conker-anim)
//! ticks from the rules' animation id. The windowed client draws it all ([`render`]), with the
//! original's HUD ([`hud`]); headless clients run the same loop for tests.
//! Options come from CLI flags (`--transport ws`) or the page URL (`?transport=ws`), see [`Config`].

pub mod audio;
pub mod beach;
pub mod connection;
pub mod flags;
pub mod guns;
pub mod heist;
pub mod held;
pub mod hud;
pub mod lights;
pub mod outro;
pub mod projectiles;
pub mod render;
pub mod results;
pub mod smooth;
pub mod traps;

/// Browser fetches: the asset pack's data files and the connect token.
#[cfg(target_family = "wasm")]
pub mod web {
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    /// GETs `url` (relative to the page): its body, or why not. The request starts at the call, so
    /// several can be in flight before any is awaited.
    pub fn fetch(url: &str) -> impl Future<Output = Result<Vec<u8>, String>> + use<> {
        let url = url.to_string();
        let request = web_sys::window().map(|w| w.fetch_with_str(&url));
        async move {
            let why = |e: JsValue| match e.dyn_ref::<js_sys::Error>() {
                Some(e) => format!("{url}: {}", String::from(e.message())),
                None => format!("{url}: {e:?}"),
            };
            let request = request.ok_or_else(|| format!("{url}: no window"))?;
            let response: web_sys::Response =
                JsFuture::from(request).await.map_err(why)?.unchecked_into();
            if !response.ok() {
                return Err(format!("{url}: HTTP {}", response.status()));
            }
            let body = JsFuture::from(response.array_buffer().map_err(why)?)
                .await
                .map_err(why)?;
            Ok(js_sys::Uint8Array::new(&body).to_vec())
        }
    }
}

use bevy::app::ScheduleRunnerPlugin;
use bevy::camera_controller::free_camera::FreeCameraState;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use conker_anim::{AnimPlayer, NONE, request_at};
use conker_assets::audio::Audio;
use conker_assets::hud::Hud;
use conker_assets::{ClipInfo, PACK_CHARACTERS, Slice, SpawnPoint};
use conker_rules::gun;
use conker_rules::lift::Lift;
use conker_rules::net::Fighting;
use conker_rules::projectile::Grenade;
use conker_rules::{Arena, Event, Fighter, Match, Placement, Rng, Settings, Spawn};
use conker_sim::TICK_HZ;
use conker_sim::collision::CollisionWorld;
use conker_sim::follow::Follow;
use conker_sim::movement::{BUTTON_A, BUTTON_Z, Input, PlayerState, STRAFE_PERCENT, Side, state};
use conker_sim::net::{
    Collision, Combat, DEFAULT_PORT, MatchView, Player, PlayerId, ProtocolPlugin, Saw, SawChannel,
    map_scene, scene_world,
};
use core::time::Duration;
use lightyear::core::prelude::ConfirmedHistory;
use lightyear::core::timeline::is_in_rollback;
use lightyear::prediction::diagnostics::PredictionMetrics;
use lightyear::prelude::client::input::InputSystems;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::{ActionState, InputMarker};
use lightyear::prelude::*;
use std::collections::VecDeque;

/// Full stick, as on a real N64 pad (about ±80), and the walk modifier's half stick (raw 41, the
/// oracle's "walk": game stick 34, target speed 20.4).
pub const STICK_MAX: f32 = 80.0;
pub const STICK_WALK: f32 = 41.0;
/// N64 B (draw or put away the weapon) and R (aim).
pub const BUTTON_B: u16 = 0x4000;
pub const BUTTON_R: u16 = 0x0010;
/// N64 C buttons (the right stick of a gamepad).
pub const C_UP: u16 = 0x0008;
pub const C_DOWN: u16 = 0x0004;
pub const C_LEFT: u16 = 0x0002;
pub const C_RIGHT: u16 = 0x0001;
/// The original's match goes live at controller read 23: oracle scripts count from there.
pub const FIRST_READ: u32 = 23;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    WebTransport,
    WebSocket,
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Config {
    /// `transport=wt|ws` (default wt).
    pub transport: Transport,
    /// `server=host:port` (default 127.0.0.1:5888).
    pub server: String,
    /// `digest=HEX`: SHA-256 of the server's self-signed WebTransport certificate.
    pub digest: String,
    /// `token=HEX`: the netcode connect token the server signed (the page fetches it); natively,
    /// without it, the client fetches one from `token-url`.
    pub token: String,
    /// `token-url=URL` (native, plain http): where to fetch the token; default the game server's
    /// host, its port + 1 ([`connection::token_url`]).
    pub token_url: Option<String>,
    /// `ca=PATH` (native, dev/test only): check the server's certificate against this CA, over
    /// `wss://` and WebTransport without a digest ([`connection::secure`]).
    pub ca: Option<String>,
    /// `id=N`: netcode client id (default random).
    pub id: u64,
    /// `script`: drive the player with a test pattern instead of keyboard/gamepad;
    /// `script=PATH` (native): an oracle input script (see [`ScriptLine`]).
    pub script: Option<String>,
    /// `headless`: no window or rendering (native only).
    pub headless: bool,
    /// `seconds=N`: exit after N seconds.
    pub seconds: Option<f64>,
    /// `assets=DIR` (native): the asset pack's folder (default `CONKER_ASSETS`, else `../assets`).
    /// The browser fetches the pack from `assets/` next to the page.
    pub assets: Option<String>,
    /// `screenshot=PATH`: when `seconds` runs out, save the window there, then exit (windowed).
    pub screenshot: Option<String>,
    /// `view=x,y,z,lx,ly,lz`: start in the free debug camera at this eye, looking at this point.
    pub view: Option<[f32; 6]>,
    /// `position=x,y`: window position on screen (native; two windowed clients side by side).
    pub position: Option<[i32; 2]>,
    /// `lag=MS`: delay every packet from the server and every packet to it by this much (simulated
    /// latency both ways: a round trip takes 2·MS more).
    pub lag: Option<u64>,
    /// `at=READ`: with `screenshot=PATH`, save the window when the script reaches oracle read READ
    /// (windowed, scripted; the client goes on until `seconds`).
    pub at: Option<u32>,
    /// `offscreen` (native): draw into an image instead of a window, for screenshots where no window
    /// can show (macOS draws nothing in a locked session).
    pub offscreen: bool,
    /// `log=DIRECTIVES`: more log filter directives (`EnvFilter` syntax, e.g. `conker_client=debug`
    /// for the `STATE` and `SEEN` lines); the browser has no `RUST_LOG`, which wins natively.
    pub log: Option<String>,
    /// `side=squirrels|tediz|cavemen`: the side to play on (its model), sent when joining; default
    /// the server's pick, the side with fewer players (the Tediz against caveman bots).
    pub side: Option<Side>,
    /// `map=2d|30|34|3f|2b`: the map (the pack's `maps/<map>/`); default the server's, from its `/info`
    /// ([`connection::info_url`]).
    pub map: String,
}

impl Config {
    pub fn from_pairs(pairs: impl IntoIterator<Item = (String, String)>) -> Result<Self, String> {
        let mut cfg = Config {
            transport: Transport::WebTransport,
            server: format!("127.0.0.1:{DEFAULT_PORT}"),
            digest: String::new(),
            token: String::new(),
            token_url: None,
            ca: None,
            id: 0,
            script: None,
            headless: false,
            seconds: None,
            assets: None,
            screenshot: None,
            view: None,
            position: None,
            lag: None,
            at: None,
            offscreen: false,
            log: None,
            side: None,
            map: String::new(),
        };
        for (key, value) in pairs {
            let bad = || format!("bad value for {key}: {value:?}");
            let floats = || -> Result<Vec<f32>, String> {
                value
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad())
            };
            match key.as_str() {
                "transport" => {
                    cfg.transport = match value.as_str() {
                        "wt" | "webtransport" => Transport::WebTransport,
                        "ws" | "websocket" => Transport::WebSocket,
                        _ => return Err(bad()),
                    }
                }
                "server" => cfg.server = value,
                "digest" => cfg.digest = value,
                "token" => cfg.token = value,
                "token-url" => cfg.token_url = Some(value),
                "ca" => cfg.ca = Some(value),
                "id" => cfg.id = value.parse().map_err(|_| bad())?,
                "script" => cfg.script = Some(value),
                "headless" => cfg.headless = true,
                "seconds" => cfg.seconds = Some(value.parse().map_err(|_| bad())?),
                "assets" => cfg.assets = Some(value),
                "screenshot" => cfg.screenshot = Some(value),
                "view" => cfg.view = Some(floats()?.try_into().map_err(|_| bad())?),
                "position" => {
                    let v: Vec<i32> = floats()?.into_iter().map(|f| f as i32).collect();
                    cfg.position = Some(v.try_into().map_err(|_| bad())?);
                }
                "lag" => cfg.lag = Some(value.parse().map_err(|_| bad())?),
                "at" => cfg.at = Some(value.parse().map_err(|_| bad())?),
                "offscreen" => cfg.offscreen = true,
                "log" => cfg.log = Some(value),
                "map" => cfg.map = value,
                "side" => {
                    cfg.side = Some(match value.as_str() {
                        "squirrels" => Side::Squirrels,
                        "tediz" => Side::Tediz,
                        "cavemen" => Side::Cavemen,
                        _ => return Err(bad()),
                    })
                }
                _ => return Err(format!("unknown option {key}")),
            }
        }
        if cfg.id == 0 {
            cfg.id = getrandom::u64().map_err(|e| e.to_string())?;
        }
        Ok(cfg)
    }
}

/// Bevy's log setup with `log=`'s directives added.
pub fn log_plugin(cfg: &Config) -> LogPlugin {
    let mut plugin = LogPlugin::default();
    if let Some(log) = &cfg.log {
        plugin.filter = format!("{},{log}", plugin.filter);
    }
    plugin
}

/// `--key value` / `--flag` command-line arguments as key/value pairs.
pub fn arg_pairs(args: impl IntoIterator<Item = String>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for arg in args {
        match (arg.strip_prefix("--"), out.last_mut()) {
            (Some(key), _) => out.push((key.to_string(), String::new())),
            (None, Some((_, value))) if value.is_empty() => *value = arg,
            (None, _) => out.push((arg, String::new())), // rejected by from_pairs
        }
    }
    out
}

/// `?key=value&flag` URL query string as key/value pairs, `%XX` escapes decoded (browsers write
/// `server=host:port` as `server=host%3Aport` when a page script edits the query).
pub fn query_pairs(query: &str) -> Vec<(String, String)> {
    query
        .trim_start_matches('?')
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

/// Decodes `%XX` escapes; anything else, malformed escapes included, stays as is.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let byte = b
            .get(i + 1..i + 3)
            .filter(|h| h.iter().all(u8::is_ascii_hexdigit))
            .and_then(|h| u8::from_str_radix(std::str::from_utf8(h).ok()?, 16).ok());
        match (b[i], byte) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The game server for a page served from `host`: the same host, port 5888. The server listens on
/// IPv4 and IPv6, so `localhost` (which Chrome's WebTransport resolves to ::1) and `[::1]` work.
pub fn page_server(host: &str) -> String {
    format!("{host}:{DEFAULT_PORT}")
}

/// One line of an oracle input script (`lab/oracle` README): reads `from..=to` get these buttons
/// and raw stick. Read [`FIRST_READ`] is the client's first tick with its player.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptLine {
    pub from: u32,
    pub to: u32,
    pub buttons: u16,
    pub x: i8,
    pub y: i8,
}

/// Parses `from to buttons_hex x y` lines; `#` starts a comment.
pub fn parse_script(text: &str) -> Result<Vec<ScriptLine>, String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or_default().trim())
        .filter(|l| !l.is_empty())
        .map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let bad = || format!("bad script line: {l:?}");
            let [from, to, buttons, x, y] = w[..] else {
                return Err(bad());
            };
            Ok(ScriptLine {
                from: from.parse().map_err(|_| bad())?,
                to: to.parse().map_err(|_| bad())?,
                buttons: u16::from_str_radix(buttons, 16).map_err(|_| bad())?,
                x: x.parse().map_err(|_| bad())?,
                y: y.parse().map_err(|_| bad())?,
            })
        })
        .collect()
}

/// The script's pad at `read`: later lines win, uncovered reads are neutral (the oracle's rule).
pub fn script_input(lines: &[ScriptLine], read: u32) -> (u16, i8, i8) {
    lines
        .iter()
        .rev()
        .find(|l| (l.from..=l.to).contains(&read))
        .map_or((0, 0, 0), |l| (l.buttons, l.x, l.y))
}

/// Test pattern: the stick turns a full circle every 100 ticks (so 1 s log samples do not alias;
/// phase set by `id`), A tapped every 1.5 s.
pub fn circle_input(id: u64, tick: u32) -> (u16, i8, i8) {
    let angle = (tick as f32 / 100.0 + (id % 4) as f32 * 0.25) * core::f32::consts::TAU;
    let q = |v: f32| (v * STICK_MAX).round() as i8;
    let a = if tick.is_multiple_of(45) { BUTTON_A } else { 0 };
    (a, q(angle.cos()), q(angle.sin()))
}

/// Keyboard, mouse and the first gamepad as an N64 pad: (buttons, stick x, stick y).
/// - Keyboard: WASD/arrows full stick (Alt: walk at half stick; not Ctrl, as browsers keep Ctrl+W),
///   Space = A (jump; reloads a drawn gun), Shift = Z (crouch; fires a drawn weapon), F = Z,
///   B = B (draw or put away the weapon), R = R (aim), G = C-down (grenade), Q / E = C-left /
///   C-right (strafe).
/// - Mouse: left button = Z (fire), right button = R (aim).
/// - Gamepad: left stick × 80, South = A, West = B, both triggers and the left bumper = Z, the right
///   bumper = R, right stick = C buttons (down: grenade, left/right: strafe).
///
/// The C buttons do not turn the camera in the original's multiplayer (held, C-left/right stop its
/// swing-back and strafe), so no key or mouse turns it either.
pub fn device_input(
    keys: Option<&ButtonInput<KeyCode>>,
    mouse: Option<&ButtonInput<MouseButton>>,
    pad: Option<&Gamepad>,
) -> (u16, i8, i8) {
    use KeyCode::*;
    let held = |codes: &[KeyCode]| keys.is_some_and(|k| k.any_pressed(codes.iter().copied()));
    let clicked = |b: MouseButton| mouse.is_some_and(|m| m.pressed(b));
    let axis = |pos: &[KeyCode], neg: &[KeyCode]| held(pos) as i8 as f32 - held(neg) as i8 as f32;
    let mut stick = Vec2::new(
        axis(&[KeyD, ArrowRight], &[KeyA, ArrowLeft]),
        axis(&[KeyW, ArrowUp], &[KeyS, ArrowDown]),
    );
    let walk = held(&[AltLeft, AltRight]);
    let mut scale = if walk { STICK_WALK } else { STICK_MAX };
    let mut buttons = 0;
    for (on, bit) in [
        (held(&[Space]), BUTTON_A),
        (
            held(&[ShiftLeft, ShiftRight, KeyF]) || clicked(MouseButton::Left),
            BUTTON_Z,
        ),
        (held(&[KeyB]), BUTTON_B),
        (held(&[KeyR]) || clicked(MouseButton::Right), BUTTON_R),
        (held(&[KeyT]), C_UP),
        (held(&[KeyG]), C_DOWN),
        (held(&[KeyQ]), C_LEFT),
        (held(&[KeyE]), C_RIGHT),
    ] {
        if on {
            buttons |= bit;
        }
    }
    if let Some(pad) = pad {
        if stick == Vec2::ZERO {
            (stick, scale) = (pad.left_stick(), STICK_MAX);
        }
        use GamepadButton::*;
        for (on, bit) in [
            (pad.pressed(South), BUTTON_A),
            (pad.pressed(West), BUTTON_B),
            (
                pad.pressed(LeftTrigger2) || pad.pressed(RightTrigger2) || pad.pressed(LeftTrigger),
                BUTTON_Z,
            ),
            (pad.pressed(RightTrigger), BUTTON_R),
        ] {
            if on {
                buttons |= bit;
            }
        }
        let c = pad.right_stick();
        for (on, bit) in [
            (c.x < -0.5, C_LEFT),
            (c.x > 0.5, C_RIGHT),
            (c.y > 0.5, C_UP),
            (c.y < -0.5, C_DOWN),
        ] {
            if on {
                buttons |= bit;
            }
        }
    }
    let q = |v: f32| (v.clamp(-1.0, 1.0) * scale).round() as i8;
    (buttons, q(stick.x), q(stick.y))
}

/// Stick units per pixel the mouse moves in a frame while R aims ([`mouse_aim`]).
pub const MOUSE_AIM: f32 = 2.0;

/// While R is held (aiming a gun, or looking through a scope) and no key moves the stick, the mouse
/// moves it: `motion` pixels this frame, [`MOUSE_AIM`] stick units each (y up), as a stick turns
/// the aim or the scope's look (a keyboard's full-stick steps are too coarse to aim with).
pub fn mouse_aim(buttons: u16, stick: (i8, i8), motion: Vec2) -> (i8, i8) {
    if buttons & BUTTON_R == 0 || stick != (0, 0) {
        return stick;
    }
    let q = |v: f32| (v * MOUSE_AIM).round().clamp(-STICK_MAX, STICK_MAX) as i8;
    (q(motion.x), q(-motion.y))
}

/// Ticks of aim-camera history kept for rollbacks (more than any rollback reaches back).
const AIMS: usize = 128;

/// What the camera left for the next tick after a live tick (`Fighter::take_camera`): R's aim gate
/// on the stick and the scope's look. Rollbacks re-run the rules, not the camera, so they take it
/// from here.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Aim {
    pub aiming: bool,
    pub scope_yaw: Option<f32>,
    pub scope_pitch: f32,
}

impl Aim {
    pub fn of(f: &Fighter) -> Self {
        Self {
            aiming: f.body.aiming,
            scope_yaw: f.body.scope_yaw,
            scope_pitch: f.scope_pitch,
        }
    }

    pub fn give(self, f: &mut Fighter) {
        (f.body.aiming, f.body.scope_yaw, f.scope_pitch) =
            (self.aiming, self.scope_yaw, self.scope_pitch);
    }
}

/// The local player's follow camera, the tick its player arrived (scripts count from it), what
/// the camera left after each recent tick ([`Aim`], for rollbacks), the respawns it has cut to,
/// and what the screen draws of the player after a correction ([`smooth::Drawn`]).
#[derive(Component, Clone, Copy, Debug)]
pub struct FollowCamera {
    pub follow: Follow,
    pub since: u32,
    pub aims: [Aim; AIMS],
    pub lives: u16,
    pub drawn: smooth::Drawn,
}

impl FollowCamera {
    pub fn new(follow: Follow, since: u32, lives: u16) -> Self {
        Self {
            follow,
            since,
            aims: [Aim::default(); AIMS],
            lives,
            drawn: smooth::Drawn::default(),
        }
    }
}

/// A player's animation clock (local: from the predicted state; remote: the interpolated one), and
/// its speed percentage before the last tick (the armed walks' rates count it, `request_at`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Anim(pub AnimPlayer, pub u8);

/// Every animation id the movement produces (its state byte is the id, V: conker-anim). The local
/// player shows its predicted state for these, the rules' animation for anything else.
pub const ANIM_IDS: [u8; 13] = [
    state::CREEP,
    state::WALK,
    state::RUN,
    state::IDLE,
    state::CROUCH,
    state::CROUCH_WALK,
    state::SKID,
    state::JUMP,
    state::LAND,
    state::RUN_JUMP,
    state::FALL,
    state::PUSH_WALL,
    state::HIGH_JUMP,
];

/// The first clip that plays logical animation `logical`.
pub fn clip_for(clips: &[ClipInfo], logical: u16) -> Option<&ClipInfo> {
    clips.iter().find(|c| c.logical.contains(&logical))
}

/// A character's clip for every logical animation id it has (movement, weapons, hits, deaths),
/// sorted by id: (id, glTF animation name, loop length in ticks).
#[derive(Clone, Debug)]
pub struct Clips(pub Vec<(u16, String, u16)>);

impl Clips {
    pub fn new(clips: &[ClipInfo]) -> Self {
        let mut ids: Vec<u16> = clips
            .iter()
            .flat_map(|c| c.logical.iter().copied())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        Self(
            ids.into_iter()
                .filter_map(|id| {
                    let c = clip_for(clips, id)?;
                    Some((id, c.animation(), c.duration_ticks))
                })
                .collect(),
        )
    }

    /// Loop length of clip `id` in ticks (1 if the character has none).
    pub fn duration(&self, id: u16) -> u16 {
        self.0
            .binary_search_by_key(&id, |c| c.0)
            .map_or(1, |i| self.0[i].2)
    }
}

/// War: Beach's villagers, models 156 (the men) and 157 (the women), packed with map 2b.
pub const VILLAGERS: [&str; 2] = ["156", "157"];

/// The Wise Guy, model 59: every player's on 1a (class 1, heist.md §1, V t0/dm6), packed with map 1a.
pub const WISE_GUY: &str = "59";

/// The characters the client draws on `map`, its bodies: the sides' ([`PACK_CHARACTERS`], in
/// [`Side`] order), then on 2b War: Beach's [`VILLAGERS`], on 1a the [`WISE_GUY`].
pub fn characters(map: &str) -> Vec<&'static str> {
    let villagers = (map == "2b").then_some(VILLAGERS).into_iter().flatten();
    let wise_guy = (map == "1a").then_some(WISE_GUY);
    PACK_CHARACTERS
        .into_iter()
        .chain(villagers)
        .chain(wise_guy)
        .collect()
}

/// The body (of [`characters`]) a player is drawn as: a villager's class picks its model
/// (`beach::model`: 156 below 0x1D, else 157), class 1 (1a's) is the [`WISE_GUY`], anyone else is
/// its side's.
pub fn body(c: &Combat) -> usize {
    match c.class {
        0x11..=0x20 => {
            PACK_CHARACTERS.len() + usize::from(conker_rules::beach::model(c.class) == 157)
        }
        1 => PACK_CHARACTERS.len(),
        _ => c.side as usize,
    }
}

/// Each body's character's [`Clips`] (of [`characters`]).
#[derive(Resource, Clone, Debug)]
pub struct BodyClips(pub Vec<Clips>);

impl BodyClips {
    pub fn of(&self, body: usize) -> &Clips {
        &self.0[body]
    }
}

/// The animation a player shows: the rules' ([`Combat::anim`]), except that the local player shows
/// its predicted movement state while the rules play a movement animation.
pub fn shown_anim(p: &PlayerState, combat: Option<&Combat>, predicted: bool) -> u16 {
    match combat {
        Some(c) if !(predicted && c.anim < 0x100 && ANIM_IDS.contains(&(c.anim as u8))) => c.anim,
        _ => u16::from(p.state),
    }
}

/// A weapon's name for the HUD: what the game's model of it in the hand shows (bank 09, `held`).
pub fn weapon_name(id: u8) -> String {
    match id {
        0x09 => "twin guns".into(),
        0x39 => "rifle".into(),
        0x24 => "sniper rifle".into(),
        0x14 => "throwing knives".into(),
        0x15 => "flamethrower".into(),
        0x16 => "rocket launcher".into(),
        0x21 => "katana".into(),
        0x22 => "chainsaw".into(),
        0x23 => "scoped revolver".into(),
        _ => format!("weapon {id:#04x}"),
    }
}

/// The text readout (logged as `HUD` lines; on screen with `render::TEXT_HUD_KEY`): health, weapon
/// and ammo, grenades, score, clock, and the respawn countdown.
pub fn hud_text(c: &Combat, m: &MatchView) -> String {
    let secs = m.timer.max(0) / 60;
    let mut out = format!("HP {}/6", c.health);
    if m.time == 0 {
        out += &format!("   lives {}", c.lives_left);
    }
    if c.weapon != 0 {
        let state = if c.drawn { "drawn" } else { "away" };
        out += &format!("   {} ({state})", weapon_name(c.weapon));
        if c.rounds > 0 {
            out += &format!(" {} rounds", c.rounds);
        }
    }
    out += &format!("   grenades {}   kills {}", c.grenades, c.kills);
    if m.time > 0 {
        out += &format!("   {}:{:02}", secs / 60, secs % 60);
    }
    if c.out > 0 {
        out += "\nOUT OF LIVES";
    } else if c.health == 0 {
        out += &format!(
            "\nDEAD - respawn in {:.1} s",
            f32::from(c.respawn_in) / TICK_HZ as f32
        );
    }
    match m.phase {
        1 => out += "\nTIME UP",
        2 | 3 => out += "\nMATCH OVER",
        _ => {}
    }
    out
}

/// The scene's spawn records (the follow camera starts at the nearest one's eye).
#[derive(Resource, Clone, Debug)]
pub struct Spawns(pub Vec<SpawnPoint>);

/// The match's static data the local player's prediction needs: the spawn records (a predicted
/// respawn picks one, among its fighter's `spawn_mask`), the scene's mounted guns at home (their
/// bodies push it) and its lifts (conker-bots' `lift`, which the server rides); pickups are the
/// server's.
#[derive(Resource, Clone, Debug)]
pub struct MatchArena(pub Arena, pub Vec<gun::Gun>, pub Vec<Lift>);

impl MatchArena {
    pub fn new(spawns: &[SpawnPoint], scene: u16, triggers: &[u8]) -> Self {
        let spawns = spawns
            .iter()
            .map(|s| Spawn {
                pos: s.position,
                b6: s.byte_06,
                yaw: s.yaw,
                camera: s.camera,
                b14: s.byte_14,
                b15: s.byte_15,
            })
            .collect();
        let arena = Arena {
            spawns,
            pickups: Vec::new(),
            ..Default::default()
        };
        let lifts = conker_rules::lift::parse(triggers, scene as u8);
        Self(arena, gun::spawn(scene.into()), lifts)
    }
}

/// The match as this client knows it, to predict its own slot `slot`: its fighter `own`, every
/// other player as this screen shows it (`others`: slot and fighter), the grenades in flight, and
/// `Fighter::vacant` in any slot it has not heard of (up to the grenades' owners and the fire's
/// attacker, whom a hit credits).
pub fn known_match(
    tick: u32,
    slot: usize,
    own: Fighter,
    others: impl IntoIterator<Item = (usize, Fighter)>,
    grenades: &[(u32, Grenade)],
    arena: &Arena,
) -> Match {
    let named = (grenades.iter().map(|g| g.1.owner)).chain(own.burn.and_then(|b| b.attacker));
    let mut players = vec![None; named.max().map_or(0, |s| s + 1)];
    for (s, f) in others.into_iter().chain([(slot, own)]) {
        if players.len() <= s {
            players.resize(s + 1, None);
        }
        players[s] = Some(f);
    }
    let players = (players.into_iter())
        .map(|f| f.unwrap_or(Fighter::vacant(tick)))
        .collect();
    Match {
        grenades: grenades.to_vec(),
        ..Match::new(arena, players, Rng::default(), tick)
    }
}

/// The interpolation time (server tick, overstep) the other players were last drawn at, once the
/// interpolation timeline runs: what the next input's [`Saw`] reports.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Shown(pub Option<(u32, f32)>);

/// The script driving the local player, if any (`script=PATH`; empty: the circle pattern).
#[derive(Resource, Clone, Debug, Default)]
pub struct Script(pub Vec<ScriptLine>);

/// The nearest spawn record's camera eye to `pos` (the record a player spawned on), else one
/// behind it.
fn spawn_eye(spawns: &[SpawnPoint], pos: [f32; 3]) -> [f32; 3] {
    let near = |s: &&SpawnPoint| {
        let d = s.position.map(f32::from);
        (0..3).map(|i| (d[i] - pos[i]).powi(2)).sum::<f32>()
    };
    spawns
        .iter()
        .min_by(|a, b| near(a).total_cmp(&near(b)))
        .map_or([pos[0], pos[1] + 100.0, pos[2] + 247.0], |s| s.camera)
}

/// Runs the client on the asset pack's data and the world built from it, as the server builds it
/// ([`scene_world`]; headless uses the world, spawns and clip lengths; windowed draws the pack's
/// glTF models too, and the HUD, [`hud`]), with its sound if it has some ([`audio`]; headless only
/// logs it). `script`: the lines of `script=PATH`.
pub fn run(cfg: Config, slice: Slice, hud: Hud, sound: Option<Audio>, script: Vec<ScriptLine>) {
    let tick = Duration::from_secs_f64(1.0 / TICK_HZ);
    let mut app = App::new();
    let scene = map_scene(&cfg.map).expect("a known map");
    app.insert_resource(Collision(scene_world(&slice, scene)));
    let clips = slice.characters.iter().map(|c| Clips::new(&c.clips));
    app.insert_resource(BodyClips(clips.collect()));
    app.insert_resource(Spawns(slice.spawns.clone()));
    app.insert_resource(MatchArena::new(&slice.spawns, scene, &slice.triggers));
    app.insert_resource(Shown::default());
    app.insert_resource(Shows::default());
    app.insert_resource(Script(script));
    outro::build(&mut app, slice.outros.clone());
    heist::build(&mut app, !cfg.headless);
    lights::flickers(&mut app, &slice.lights, cfg.id);
    let (held, pickups) = (slice.held.clone(), slice.pickups.clone());
    let triggers = slice.triggers.clone();
    if cfg.headless {
        app.add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick / 2)),
            log_plugin(&cfg),
            StatesPlugin,
        ));
    } else {
        if scene == conker_rules::traps::SCENE {
            traps::build(&mut app, &triggers);
        }
        render::build(&mut app, slice, &cfg);
        hud::build(&mut app, hud);
    }
    if let Some(sound) = sound {
        audio::build(
            &mut app,
            sound,
            held,
            pickups,
            !cfg.headless,
            &cfg.map,
            &triggers,
            cfg.id,
        );
    }
    if let Some(seconds) = cfg.seconds {
        app.add_systems(
            Update,
            move |time: Res<Time<Real>>,
                  cfg: Res<Config>,
                  mut exit: MessageWriter<AppExit>,
                  mut commands: Commands,
                  mut done: Local<bool>| {
                if *done || time.elapsed_secs_f64() < seconds {
                    return;
                }
                *done = true;
                match cfg.screenshot.clone() {
                    Some(path) if !cfg.headless && cfg.at.is_none() => {
                        render::screenshot_then_exit(&mut commands, path)
                    }
                    _ => {
                        exit.write(AppExit::Success);
                    }
                }
            },
        );
    }
    app.add_plugins((
        ClientPlugins {
            tick_duration: tick,
        },
        ProtocolPlugin,
    ));
    conker_rules::net::register(&mut app);
    app.local_rollback::<Before>();
    app.insert_resource(PredictionManager::default());
    if !cfg.headless && cfg.at.is_some() {
        app.add_systems(FixedUpdate, screenshot_at_read.run_if(not(is_in_rollback)));
    }
    app.insert_resource(cfg);
    connection::build(&mut app);
    results::build(&mut app);
    app.add_systems(
        FixedPreUpdate,
        buffer_input
            .in_set(InputSystems::WriteClientInputs)
            .run_if(not(is_in_rollback)),
    );
    // Rollbacks re-run the move only (`predict_again`): the camera, the clocks and the logs follow
    // live ticks.
    app.add_systems(
        FixedUpdate,
        (
            seat_props,
            predict_again.run_if(is_in_rollback),
            (predict, attach, tick_anims, log_players)
                .chain()
                .run_if(not(is_in_rollback)),
        )
            .chain(),
    );
    app.add_systems(PreUpdate, interpolation_margin);
    app.add_systems(
        PreUpdate,
        smooth::note_corrections.after(PredictionSystems::Rollback),
    );
    app.add_systems(Update, smooth::log_drawn);
    app.add_systems(Update, note_shown.after(InterpolationSystems::All));
    app.run();
}

/// The interpolation delay covers half the round trip and a tick more, in whole ticks: lightyear
/// 0.30 aims its interpolation a send interval behind its estimate of the server's present, which
/// runs RTT/2 ahead of anything received, so with latency it ran past its data and held the newest
/// state (V: 150 ms each way, every remote drawn at a whole tick 1-2 ticks behind its interpolation
/// time). Behind its data, the timeline draws between two server states, as the server's rewind
/// ([`Saw`]) reconstructs them.
fn interpolation_margin(
    pings: Query<&PingManager, With<Client>>,
    mut config: ResMut<InterpolationConfig>,
) {
    let Ok(pings) = pings.single() else {
        return;
    };
    let tick = 1.0 / TICK_HZ;
    let ticks = (pings.rtt().as_secs_f64() / 2.0 / tick).ceil() + 1.0;
    let delay = Duration::from_secs_f64(ticks * tick);
    if config.min_delay != delay {
        config.min_delay = delay;
    }
}

/// [`Shown`]: the interpolation time this frame drew the other players at.
fn note_shown(timeline: Option<Res<InterpolationTimeline>>, mut shown: ResMut<Shown>) {
    shown.0 = timeline
        .filter(|t| t.is_synced())
        .map(|t| (t.now().tick().0, t.overstep().to_f32()));
}

/// New players: an animation clock for each, and the follow camera for ours, starting at the
/// nearest spawn record's eye (the record the server spawned it on). The camera (which also starts
/// the input and the scripts) waits for the timeline sync: ticks before it are renumbered, and
/// moves predicted then would be rolled back. The camera also cuts to the new record after a lift
/// ride or a respawn that `predict` did not cut for (the server's count passes this one's).
#[allow(clippy::type_complexity)]
fn attach(
    timeline: Res<LocalTimeline>,
    sync: Option<Res<LocalTimelineSync>>,
    spawns: Res<Spawns>,
    new: Query<
        Entity,
        (
            With<Player>,
            Without<Anim>,
            Or<(With<Predicted>, With<Interpolated>)>,
        ),
    >,
    ours: Query<(Entity, &Player, Option<&Combat>), (With<Predicted>, Without<FollowCamera>)>,
    mut cameras: Query<(&Player, &Combat, &mut FollowCamera)>,
    mut commands: Commands,
) {
    for entity in &new {
        commands.entity(entity).insert(Anim::default());
    }
    if !sync.is_none_or(|s| s.is_synced()) {
        return;
    }
    for (entity, p, combat) in &ours {
        let since = timeline.tick().0;
        info!("synced: camera, input and script start at tick {since} (script read {FIRST_READ})");
        let follow = Follow::spawn(spawn_eye(&spawns.0, p.pos), p);
        let lives = combat.map_or(0, |c| c.lives);
        commands
            .entity(entity)
            .insert((FollowCamera::new(follow, since, lives), Before::default()));
    }
    for (p, combat, mut cam) in &mut cameras {
        if combat.lives > cam.lives {
            cam.lives = combat.lives;
            cam.follow = Follow::spawn(spawn_eye(&spawns.0, p.pos), p);
            info!("respawned at {:?}: the camera cuts", p.pos);
        }
    }
}

/// `at=READ`: the screenshot once the script reaches READ.
fn screenshot_at_read(
    cfg: Res<Config>,
    timeline: Res<LocalTimeline>,
    game: Query<&MatchView>,
    camera: Query<&FollowCamera>,
    mut commands: Commands,
    mut done: Local<bool>,
) {
    let (Some(at), Some(path), Ok(camera)) = (cfg.at, cfg.screenshot.clone(), camera.single())
    else {
        return;
    };
    let read = script_read(timeline.tick().0, camera.since, game.iter().next());
    if !*done && read.is_some_and(|r| r >= at) {
        *done = true;
        info!("screenshot at read {at}: {path}");
        render::screenshot(&mut commands, path);
    }
}

/// The oracle read a scripted client feeds at `tick`: counted from the match start in the server's
/// `--wait` replays (every client on the same reads), else from its own camera's start.
pub fn script_read(tick: u32, since: u32, m: Option<&MatchView>) -> Option<u32> {
    match m.and_then(|m| m.start.map(|s| (s, m.first_read))) {
        Some((start, first)) => (tick >= start).then(|| first + (tick - start)),
        None => Some(FIRST_READ + tick.wrapping_sub(since)),
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn buffer_input(
    cfg: Res<Config>,
    script: Res<Script>,
    timeline: Res<LocalTimeline>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    gamepads: Query<&Gamepad>,
    windows: Query<&Window>,
    free_camera: Query<&FreeCameraState>,
    game: Query<&MatchView>,
    motion: Option<Res<AccumulatedMouseMotion>>,
    mut player: Query<(&mut ActionState<Input>, &FollowCamera), With<InputMarker<Input>>>,
    (sync, shown, mut senders, remotes): (
        Option<Res<LocalTimelineSync>>,
        Res<Shown>,
        Query<&mut MessageSender<Saw>>,
        Query<(&PlayerId, &Player), With<Interpolated>>,
    ),
) {
    let Ok((mut action, camera)) = player.single_mut() else {
        return;
    };
    let tick = timeline.tick().0;
    // What the screen shows of the others as this input is taken (the last frame's draw): the
    // server rewinds this input's hit tests to it. `SEEN` logs it (as f64: the exact value).
    if let Some((at, overstep)) = shown.0 {
        let input_tick = tick + sync.map_or(0, |s| u32::from(s.input_delay()));
        for mut sender in &mut senders {
            sender.send::<SawChannel>(Saw {
                tick: input_tick,
                at,
                overstep,
            });
        }
        for (id, p) in &remotes {
            let [x, y, z] = p.pos.map(f64::from);
            debug!("SEEN t={input_tick} p={:?} pos={x},{y},{z}", id.0);
        }
    }
    let (buttons, stick_x, stick_y) = match &cfg.script {
        Some(_) if !script.0.is_empty() => script_read(tick, camera.since, game.iter().next())
            .map_or((0, 0, 0), |read| script_input(&script.0, read)),
        Some(_) => circle_input(cfg.id, tick),
        None if free_camera.iter().any(|c| c.enabled) => (0, 0, 0), // the keys fly the camera
        None => {
            // The pad only drives a focused window (with two windows open, one pad drove both).
            let pad = gamepads
                .iter()
                .next()
                .filter(|_| windows.iter().any(|w| w.focused));
            let (b, x, y) = device_input(keys.as_deref(), mouse.as_deref(), pad);
            let (x, y) = mouse_aim(b, (x, y), motion.map_or(Vec2::ZERO, |m| m.delta));
            (b, x, y)
        }
    };
    action.0 = Input {
        stick_x,
        stick_y,
        buttons,
        camera_yaw: camera.follow.yaw(),
    };
}

/// The other players as this client has them: slot, fighter with its body where the server's step
/// of the local player (slot `slot`) at `tick` pushes against it ([`pushing_body`]; else its newest
/// state from the server), and where this screen shows it (interpolated: what the server's rewind
/// aims the local player's strikes at).
type Known = Vec<(usize, Fighter, [f32; 3])>;

#[allow(clippy::type_complexity)]
fn known(
    others: &Query<(&Player, &Combat, Option<&ConfirmedHistory<Player>>), Without<Predicted>>,
    slot: Option<usize>,
    tick: u32,
) -> Known {
    others
        .iter()
        .map(|(p, c, history)| {
            let newest = history
                .and_then(|h| h.newest_present())
                .map_or(p.0, |n| n.1.0);
            let body = (history.zip(slot))
                .and_then(|(h, own)| pushing_body(h, usize::from(c.slot), own, tick))
                .unwrap_or(newest);
            (usize::from(c.slot), rival(c, body), p.pos)
        })
        .collect()
}

/// Another player's body (slot `slot`) where the server's step of the local player (slot `own`)
/// at `tick` finds it: as it was after `tick`'s step if it moves first (the slot order), else
/// after the tick before. From its server history `h`; past the history's end (the client's lead,
/// about 4 ticks on localhost, 13 at 150 ms each way), carried on as it last moved (V: e_w15's
/// shoves mispredict 11 ticks instead of 13-14 at 0 ms, 21 instead of 22-24 at 150 ms).
pub fn pushing_body(
    h: &ConfirmedHistory<Player>,
    slot: usize,
    own: usize,
    tick: u32,
) -> Option<PlayerState> {
    let (newest, last) = h.newest_present()?;
    let at = if slot < own {
        tick
    } else {
        tick.saturating_sub(1)
    };
    if at <= newest.0 {
        return h.get_present(Tick(at)).map(|p| p.0);
    }
    let before = h.get_present(newest - 1).map_or(last.pos, |p| p.pos);
    let n = (at - newest.0) as f32;
    let pos = core::array::from_fn(|k| last.pos[k] + (last.pos[k] - before[k]) * n);
    Some(PlayerState { pos, ..last.0 })
}

/// Another player's fighter from its [`Combat`] (the server sends a `Fighting` to its owner only):
/// what the local player's frame reads of the others. Its body pushes; its health, immunity and
/// stun decide a lock-on and a hit, its team a respawn pick; the rest stays as spawned.
pub fn rival(c: &Combat, body: PlayerState) -> Fighter {
    let place = Placement {
        index: 0,
        pos: body.pos,
        facing: body.facing,
        camera: [0.0; 3],
    };
    Fighter {
        body,
        health: c.health,
        immune: c.immune,
        stun: c.stun,
        anim: c.anim,
        weapon: (c.weapon != 0).then_some(c.weapon),
        drawn: c.drawn,
        side: c.side,
        deaths: c.deaths,
        out: c.out,
        ..Fighter::new(&place, c.team, 0)
    }
}

/// The match's settings as the server sends them ([`MatchView`]); the bots' skill is the server's.
pub fn match_settings(m: &MatchView) -> Settings {
    Settings {
        time: (m.time > 0).then_some(m.time),
        lives: m.lives,
        kills: m.kill_limit,
        radar: m.radar,
        total_war: m.total_war,
        colors: m.colors,
        score: m.score_limit,
        beach: m.beach,
        heist: m.heist,
        ..Settings::default()
    }
}

/// The predicted body's position before its last step, as the server's lifts keep it (`Lifts::ride`'s
/// `old`; None: none to test a move from, after a respawn): 0x2D's gates (shape 3) take the move
/// from there. Rolled back with the body (a local rollback component).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Before(pub Option<[f32; 3]>);

/// The local player's tick as the server runs it: conker-rules' frame for its slot among the others
/// ([`known_match`], `Match::step_player`, its strikes aimed where `shown` has them), and its events.
/// None once the match is over, nor before the local timeline is on the server's ticks (the
/// fighter's frames count in them). `before`: the body before its last step ([`Before`]), then
/// before this one.
#[allow(clippy::too_many_arguments)]
fn own_tick(
    tick: u32,
    over: bool,
    settings: Settings,
    body: &PlayerState,
    fighting: &Fighting,
    input: &Input,
    others: &Known,
    shown: &[(usize, [f32; 3])],
    arena: &MatchArena,
    world: &CollisionWorld,
    before: &mut Option<[f32; 3]>,
) -> (Match, usize, Vec<Event>) {
    let slot = usize::from(fighting.slot);
    let bodies = others.iter().map(|o| (o.0, o.1));
    let own = fighting.fighter(*body);
    let mut m = known_match(tick, slot, own, bodies, &fighting.grenades, &arena.0).with(settings);
    // ponytail: the guns at home, free; a manned gun's state isn't replicated into the prediction,
    // so walking into one mispredicts a mount until the server corrects it.
    m.guns = arena.1.clone();
    let mut events = Vec::new();
    if !over && fighting.state.since <= tick {
        // A lift before the step, as the server rides them, counted as a respawn (the camera cuts
        // on its tick): the boxes (0x30's, 0x2B's) by where the player stands, 0x2D's gates by its
        // last move (from `before`). The record's jitter is this match's draw, not the server's (a
        // correction of up to 40).
        let b = m.players[slot].body;
        let old = before.unwrap_or(b.pos);
        let lift = (arena.2.iter()).find(|l| l.takes(old, b.pos, b.floor_y));
        if let Some(l) = lift.filter(|_| m.players[slot].health != 0) {
            let at = conker_rules::spawn::place(&arena.0.spawns, l.record, &mut m.rng);
            let b = &mut m.players[slot].body;
            (b.pos, b.floor_y) = (at.pos, at.pos[1]);
            (b.facing, b.move_angle) = (at.facing, at.facing);
            events.push(Event::Respawn { player: slot, at });
        }
        *before = Some(m.players[slot].body.pos);
        let seen = |s: usize, v: usize| {
            (s == slot)
                .then(|| shown.iter().find(|o| o.0 == v).map(|o| o.1))
                .flatten()
        };
        let stepped = m.step_player(slot, &arena.0, input, world, &seen);
        // A respawn's move is no move (the server's `Lifts::reset` on `Event::Respawn`).
        if (stepped.iter()).any(|e| matches!(e, Event::Respawn { player, .. } if *player == slot)) {
            *before = None;
        }
        events.extend(stepped);
    }
    (m, slot, events)
}

/// War: Beach's plunger collision where the server last had the prop (`conker_rules::beach::seat`),
/// scene 0x2D's doors and covers where its props stand (`conker_rules::gas::seat`), and scene 0x34's
/// pressure plates (`conker_rules::traps::seat`).
/// ponytail: a predicted presser stands on the server's y, a round trip behind its own; the server
/// corrects it.
fn seat_props(mut world: ResMut<Collision>, game: Query<&MatchView>, cfg: Res<Config>) {
    let Some(m) = game.iter().next() else {
        return;
    };
    if m.beach {
        conker_rules::beach::seat(&mut world.0, m.plunger);
    }
    if map_scene(&cfg.map) == Some(0x2D) {
        conker_rules::gas::seat(&mut world.0, m.total_war, m.props, m.lifts);
    }
    if map_scene(&cfg.map) == Some(conker_rules::traps::SCENE) {
        conker_rules::traps::seat(&mut world.0, m.plates);
    }
}

/// Client-side prediction: [`own_tick`], then the follow camera with what the rules know of the
/// player (`Fighter::camera_view`: its weapon's aim, the scope, the twist), whose aim gate and
/// scope go into the next tick ([`Aim`]). A predicted respawn (or lift ride) cuts the camera to its record's on
/// its own tick, as the server's does: `Combat.lives` arrives the client's lead later (V: 13 ticks
/// at 150 ms each way, the view stuck at the body's old place meanwhile).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn predict(
    world: Res<Collision>,
    arena: Res<MatchArena>,
    timeline: Res<LocalTimeline>,
    sync: Option<Res<LocalTimelineSync>>,
    game: Query<&MatchView>,
    mut own: Query<
        (
            &mut Player,
            &mut Fighting,
            &ActionState<Input>,
            Option<&mut FollowCamera>,
            Option<&mut Before>,
        ),
        With<Predicted>,
    >,
    others: Query<(&Player, &Combat, Option<&ConfirmedHistory<Player>>), Without<Predicted>>,
    mut shows: ResMut<Shows>,
) {
    let tick = timeline.tick().0;
    let over = game.iter().any(|m| m.phase >= 2) || !sync.is_none_or(|s| s.is_synced());
    let settings = game
        .iter()
        .next()
        .map_or_else(Settings::default, match_settings);
    let slot = own.iter().next().map(|o| usize::from(o.1.slot));
    let others = known(&others, slot, tick);
    // Where this screen shows the others this tick: what the server rewinds this tick's strikes to.
    let shown: Positions = others.iter().map(|o| (o.0, o.2)).collect();
    for (mut player, mut fighting, input, camera, before) in &mut own {
        let mut old = before.as_deref().and_then(|b| b.0);
        let (mut m, i, events) = own_tick(
            tick, over, settings, &player, &fighting, &input.0, &others, &shown, &arena, &world,
            &mut old,
        );
        if let Some(mut b) = before {
            b.set_if_neq(Before(old));
        }
        let f = &mut m.players[i];
        if let Some(mut camera) = camera {
            if let Some(at) = events.iter().find_map(|e| match e {
                Event::Respawn { player, at } if *player == i => Some(at),
                _ => None,
            }) {
                camera.follow = Follow::spawn(at.camera, &f.body);
                camera.lives += 1;
                camera.drawn = smooth::Drawn::default();
                info!(
                    "t={tick} predicted respawn on record {}: the camera cuts",
                    at.index
                );
            }
            camera
                .follow
                .step_view(f.camera_view(m.tick), &input.0, &world);
            f.take_camera(&camera.follow);
            camera.aims[tick as usize % AIMS] = Aim::of(f);
            let cam = &mut *camera;
            cam.drawn.step(f, m.tick, &cam.follow, &input.0, &world);
        }
        player.0 = f.body;
        *fighting = Fighting::of(&m, i);
    }
    if shows.0.len() == AIMS {
        shows.0.pop_front();
    }
    shows.0.push_back((tick, shown));
}

/// Where this screen showed the other players on each recent live tick (slot, position), for
/// rollbacks.
#[derive(Resource, Clone, Debug, Default)]
pub struct Shows(pub VecDeque<(u32, Positions)>);

/// Players' positions by slot.
pub type Positions = Vec<(usize, [f32; 3])>;

/// A rollback's re-run of a tick: [`own_tick`], with the others shown as the live tick showed them
/// and what the camera left as it left it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn predict_again(
    world: Res<Collision>,
    arena: Res<MatchArena>,
    timeline: Res<LocalTimeline>,
    sync: Option<Res<LocalTimelineSync>>,
    game: Query<&MatchView>,
    mut own: Query<
        (
            &mut Player,
            &mut Fighting,
            &ActionState<Input>,
            Option<&FollowCamera>,
            Option<&mut Before>,
        ),
        With<Predicted>,
    >,
    others: Query<(&Player, &Combat, Option<&ConfirmedHistory<Player>>), Without<Predicted>>,
    shows: Res<Shows>,
) {
    let tick = timeline.tick().0;
    let over = game.iter().any(|m| m.phase >= 2) || !sync.is_none_or(|s| s.is_synced());
    let settings = game
        .iter()
        .next()
        .map_or_else(Settings::default, match_settings);
    let slot = own.iter().next().map(|o| usize::from(o.1.slot));
    let others = known(&others, slot, tick);
    let shown = shows.0.iter().find(|s| s.0 == tick).map_or_else(
        || others.iter().map(|o| (o.0, o.2)).collect(),
        |s| s.1.clone(),
    );
    for (mut player, mut fighting, input, camera, before) in &mut own {
        let mut old = before.as_deref().and_then(|b| b.0);
        let (mut m, i, _) = own_tick(
            tick, over, settings, &player, &fighting, &input.0, &others, &shown, &arena, &world,
            &mut old,
        );
        if let Some(mut b) = before {
            b.set_if_neq(Before(old));
        }
        let f = &mut m.players[i];
        if let Some(camera) = camera {
            camera.aims[tick as usize % AIMS].give(f);
        }
        player.0 = f.body;
        *fighting = Fighting::of(&m, i);
    }
}

/// Each player's clock advances one frame with its animation's request: the local player's
/// predicted rules animation, the others' [`shown_anim`]. The armed walks' rates count the speed
/// percentage before the tick, 65 on a strafing tick (`request_at`). The first frame is a cut. The
/// jump onto a mounted gun plays at 1.3 (the gun's launch, `gun::LAUNCH`).
#[allow(clippy::type_complexity)]
fn tick_anims(
    clips: Res<BodyClips>,
    game: Query<&MatchView>,
    mut players: Query<(
        &Player,
        Option<&Combat>,
        Option<&Fighting>,
        Has<Predicted>,
        &mut Anim,
    )>,
) {
    for (p, combat, fighting, predicted, mut anim) in &mut players {
        let id = match fighting {
            Some(f) if predicted => f.state.anim,
            _ => shown_anim(p, combat, predicted),
        };
        let percent = if p.prev_strafe {
            STRAFE_PERCENT
        } else {
            anim.1
        };
        let mut req = request_at(id, p.xz_speed, p.target_speed, percent);
        if anim.0.id == NONE {
            req.blend = 0.0;
        }
        let launching = match fighting {
            Some(f) if predicted => f.state.gun.is_some_and(|g| g.state == gun::LAUNCH),
            _ => (game.iter().flat_map(|m| &m.guns)).any(|g| {
                g.state == gun::LAUNCH && combat.is_some_and(|c| g.gunner == Some(c.slot))
            }),
        };
        if launching && id == conker_anim::id::JUMP {
            req.rate = 1.3;
        }
        // Heist's carrier walks at e/6 + 0.4 and runs at e/26 + 0.5, the target capped at 25.2
        // (V `heist-presentation.md` §2).
        let e = || conker_anim::effective_speed(p.xz_speed, p.target_speed.min(25.2));
        match id {
            conker_rules::heist::WALK => req.rate = e() / 6.0 + 0.4,
            conker_rules::heist::RUN => req.rate = e() / 26.0 + 0.5,
            _ => {}
        }
        let body = combat.map_or(Side::default() as usize, body);
        anim.0.tick(req, clips.of(body).duration(id));
        anim.1 = p.speed_percent;
    }
}

/// `STATE` lines (debug, every tick) for the predicted player, to compare with the server's;
/// a summary of every player once a second (info).
#[allow(clippy::type_complexity)]
fn log_players(
    timeline: Res<LocalTimeline>,
    metrics: Option<Res<PredictionMetrics>>,
    game: Query<&MatchView>,
    players: Query<(
        &PlayerId,
        &Player,
        Option<&Anim>,
        Option<&Combat>,
        Has<Predicted>,
        Has<Interpolated>,
    )>,
    mut last: Local<String>,
) {
    let tick = timeline.tick().0;
    for (id, p, anim, combat, predicted, interpolated) in &players {
        // As f64: the exact value, as the server logs it.
        let [x, y, z] = p.pos.map(f64::from);
        if predicted {
            debug!(
                "STATE t={tick} p={:?} pos={x},{y},{z} st={} fy={}",
                id.0, p.state, p.floor_y
            );
            // When the text changes (`Combat` changes every tick: immunity, animation).
            if let (Some(c), Some(m)) = (combat, game.iter().next()) {
                let text = hud_text(c, m);
                if *last != text {
                    info!("HUD t={tick} {}", text.replace('\n', " | "));
                    *last = text;
                }
            }
        }
        if !tick.is_multiple_of(30) {
            continue;
        }
        let role = match (predicted, interpolated) {
            (true, _) => "local/predicted",
            (_, true) => "remote/interpolated",
            _ => "confirmed",
        };
        let (clip, time) = anim.map_or((NONE, 0.0), |a| (a.0.id, a.0.time));
        let hp = combat.map_or(0, |c| c.health);
        info!(
            "tick {tick} {role} player {:?} pos ({x:.2}, {y:.2}, {z:.2}) state {} clip {clip} time {time:.2} hp {hp}",
            id.0, p.state
        );
    }
    if tick.is_multiple_of(30)
        && let Some(m) = metrics
    {
        info!(
            "tick {tick} rollbacks {} rollback ticks {}",
            m.rollbacks, m.rollback_ticks
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn cli_and_url_parse_to_the_same_pairs() {
        let args = [
            "--transport",
            "ws",
            "--headless",
            "--server",
            "10.0.0.2:5888",
            "--id",
            "7",
        ];
        let expected = pairs(&[
            ("transport", "ws"),
            ("headless", ""),
            ("server", "10.0.0.2:5888"),
            ("id", "7"),
        ]);
        assert_eq!(arg_pairs(args.map(String::from)), expected);
        assert_eq!(
            query_pairs("?transport=ws&headless&server=10.0.0.2:5888&id=7"),
            expected
        );
        // As a page script's URLSearchParams writes it back.
        assert_eq!(
            query_pairs("transport=ws&headless&server=10.0.0.2%3A5888&id=7"),
            expected
        );
    }

    #[test]
    fn percent_escapes_decode_and_malformed_ones_stay() {
        assert_eq!(percent_decode("a%3Ab%3a%2F%C3%A9"), "a:b:/é");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
        assert_eq!(percent_decode("%+f"), "%+f");
    }

    #[test]
    fn page_server_is_the_page_host() {
        assert_eq!(page_server("localhost"), "localhost:5888");
        assert_eq!(page_server("[::1]"), "[::1]:5888");
        assert_eq!(page_server("game.example.org"), "game.example.org:5888");
        assert_eq!(page_server("192.168.1.20"), "192.168.1.20:5888");
    }

    #[test]
    fn config_defaults_and_overrides() {
        let cfg = Config::from_pairs(pairs(&[("id", "3")])).unwrap();
        assert_eq!(
            (cfg.transport, cfg.server.as_str(), cfg.id),
            (Transport::WebTransport, "127.0.0.1:5888", 3)
        );
        assert!(cfg.script.is_none() && !cfg.headless && cfg.seconds.is_none());
        assert!(cfg.assets.is_none() && cfg.screenshot.is_none() && cfg.view.is_none());
        let cfg = Config::from_pairs(pairs(&[
            ("transport", "ws"),
            ("script", ""),
            ("seconds", "2.5"),
        ]))
        .unwrap();
        assert_eq!(
            (cfg.transport, cfg.script.as_deref(), cfg.seconds),
            (Transport::WebSocket, Some(""), Some(2.5))
        );
        assert_ne!(cfg.id, 0, "random id");
        let cfg = Config::from_pairs(pairs(&[
            ("assets", "/packs/conker"),
            ("screenshot", "a.png"),
            ("view", "1,2.5,-3,0,0,0"),
            ("position", "0,40"),
            ("script", "walk.txt"),
        ]))
        .unwrap();
        assert_eq!(cfg.assets.as_deref(), Some("/packs/conker"));
        assert_eq!(cfg.screenshot.as_deref(), Some("a.png"));
        assert_eq!(cfg.view, Some([1.0, 2.5, -3.0, 0.0, 0.0, 0.0]));
        assert_eq!(cfg.position, Some([0, 40]));
        assert_eq!(cfg.script.as_deref(), Some("walk.txt"));
        assert!(Config::from_pairs(pairs(&[("view", "1,2,3")])).is_err());
        assert!(Config::from_pairs(pairs(&[("view", "1,2,3,4,5,x")])).is_err());
        assert!(Config::from_pairs(pairs(&[("position", "1")])).is_err());
        assert!(Config::from_pairs(pairs(&[("transport", "udp")])).is_err());
        assert!(Config::from_pairs(pairs(&[("bogus", "1")])).is_err());
        let cfg = Config::from_pairs(pairs(&[("lag", "150"), ("at", "900")])).unwrap();
        assert_eq!((cfg.lag, cfg.at), (Some(150), Some(900)));
        let cfg = Config::from_pairs(query_pairs("log=conker_client%3Ddebug")).unwrap();
        assert!(log_plugin(&cfg).filter.ends_with(",conker_client=debug"));
        assert_eq!(cfg.side, None, "the server's pick");
        let side = |v| Config::from_pairs(query_pairs(v)).map(|c| c.side);
        assert_eq!(side("side=squirrels"), Ok(Some(Side::Squirrels)));
        assert_eq!(side("?side=tediz"), Ok(Some(Side::Tediz)));
        assert_eq!(side("side=cavemen"), Ok(Some(Side::Cavemen)));
        assert!(side("side=bears").is_err());
    }

    #[test]
    fn oracle_scripts_parse_and_later_lines_win() {
        let lines =
            parse_script("# walk, then jump\n53 142 0000 0 41\n113 115 8000 0 41  # tap A\n\n")
                .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(script_input(&lines, 52), (0, 0, 0));
        assert_eq!(script_input(&lines, 53), (0, 0, 41));
        assert_eq!(script_input(&lines, 114), (BUTTON_A, 0, 41));
        assert_eq!(script_input(&lines, 143), (0, 0, 0));
        assert!(parse_script("1 2 zz 0 0").is_err());
        assert!(parse_script("1 2 0000 0").is_err());
    }

    #[test]
    fn circle_moves_and_taps_a() {
        let inputs: Vec<_> = (0..90).map(|t| circle_input(1, t)).collect();
        assert!(inputs.iter().all(|i| (i.1, i.2) != (0, 0)));
        assert_eq!(inputs.iter().filter(|i| i.0 == BUTTON_A).count(), 2); // ticks 0 and 45
        assert_ne!(
            circle_input(1, 10),
            circle_input(2, 10),
            "clients differ by phase"
        );
    }

    #[test]
    fn keyboard_maps_to_the_n64_pad() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::KeyD);
        keys.press(KeyCode::Space);
        assert_eq!(device_input(Some(&keys), None, None), (BUTTON_A, 80, 80));
        keys.release(KeyCode::Space);
        keys.release(KeyCode::KeyD);
        keys.press(KeyCode::ShiftLeft);
        keys.press(KeyCode::ControlLeft);
        assert_eq!(device_input(Some(&keys), None, None), (BUTTON_Z, 0, 80)); // Ctrl does not walk
        keys.press(KeyCode::AltLeft);
        assert_eq!(device_input(Some(&keys), None, None), (BUTTON_Z, 0, 41)); // walk, crouched
        assert_eq!(device_input(None, None, None), (0, 0, 0));
    }

    #[test]
    fn combat_keys_and_mouse_map_to_the_n64_buttons() {
        let mut keys = ButtonInput::<KeyCode>::default();
        let mut mouse = ButtonInput::<MouseButton>::default();
        let pad = |k: &ButtonInput<KeyCode>, m: &ButtonInput<MouseButton>| {
            device_input(Some(k), Some(m), None).0
        };
        mouse.press(MouseButton::Left);
        assert_eq!(pad(&keys, &mouse), BUTTON_Z);
        mouse.press(MouseButton::Right);
        assert_eq!(pad(&keys, &mouse), BUTTON_Z | BUTTON_R);
        mouse.release_all();
        for (key, bit) in [
            (KeyCode::KeyF, BUTTON_Z),
            (KeyCode::KeyB, BUTTON_B),
            (KeyCode::KeyR, BUTTON_R),
            (KeyCode::KeyG, C_DOWN),
            (KeyCode::KeyQ, C_LEFT),
            (KeyCode::KeyE, C_RIGHT),
        ] {
            keys.press(key);
            assert_eq!(pad(&keys, &mouse), bit, "{key:?}");
            keys.release(key);
        }
    }

    #[test]
    fn the_mouse_aims_while_r_is_held() {
        let m = Vec2::new(10.0, -60.0);
        assert_eq!(mouse_aim(BUTTON_R, (0, 0), m), (20, 80));
        assert_eq!(
            mouse_aim(0, (0, 0), m),
            (0, 0),
            "no R: the mouse does nothing"
        );
        assert_eq!(mouse_aim(BUTTON_R, (80, 0), m), (80, 0), "the keys win");
    }

    /// The prediction's match: every player in its server slot, the local one over a stale copy,
    /// a dead stand-in where nothing is known.
    #[test]
    fn the_known_match_puts_everyone_in_their_slots() {
        let at = |x: f32| {
            let place = Placement {
                index: 0,
                pos: [x, 0.0, 0.0],
                facing: 0,
                camera: [0.0; 3],
            };
            Fighter::new(&place, 0, 5)
        };
        let arena = MatchArena::new(&[], 0x2D, &[]).0;
        let m = known_match(9, 2, at(2.0), [(0, at(0.0)), (2, at(-1.0))], &[], &arena);
        assert_eq!((m.tick, m.players.len()), (9, 3));
        assert_eq!(m.players[0].body.pos[0], 0.0);
        assert_eq!(m.players[1], Fighter::vacant(9));
        assert_eq!(m.players[2].body.pos[0], 2.0);
        // A grenade (or a fire) names its owner: the slot is there for the hit's credit.
        let g = Grenade::throw(4, [0.0; 3], 0, 0, 99);
        let m = known_match(9, 2, at(2.0), [], &[(5, g)], &arena);
        assert_eq!((m.players.len(), m.grenades.len()), (5, 1));
        assert_eq!(m.players[4], Fighter::vacant(9));
        let mut f = at(0.0);
        let aim = Aim {
            aiming: true,
            scope_yaw: Some(12.5),
            scope_pitch: -3.0,
        };
        aim.give(&mut f);
        assert_eq!(Aim::of(&f), aim);
    }

    #[test]
    fn scripts_key_on_the_match_start_in_replays() {
        let mut m = MatchView {
            first_read: 745,
            ..MatchView::default()
        };
        assert_eq!(
            script_read(110, 100, Some(&m)),
            Some(FIRST_READ + 10),
            "own start"
        );
        assert_eq!(script_read(110, 100, None), Some(FIRST_READ + 10));
        m.start = Some(500);
        assert_eq!(script_read(499, 100, Some(&m)), None);
        assert_eq!(script_read(580, 100, Some(&m)), Some(825));
    }

    #[test]
    fn the_local_player_shows_its_predicted_movement() {
        let mut p = PlayerState::spawn([0.0; 3], 0);
        p.state = state::RUN;
        let mut c = Combat {
            anim: u16::from(state::IDLE),
            ..Combat::default()
        };
        assert_eq!(shown_anim(&p, Some(&c), true), u16::from(state::RUN));
        assert_eq!(shown_anim(&p, Some(&c), false), u16::from(state::IDLE));
        c.anim = 0x7C; // twin guns firing
        assert_eq!(shown_anim(&p, Some(&c), true), 0x7C);
        assert_eq!(shown_anim(&p, None, false), u16::from(state::RUN));
    }

    /// The prediction rides 0x2B's lift 0 (a box, kind 8, to record 4; here record 1) on the tick its
    /// player stands in it, as the server's ride: on the record (within its jitter), facing its way,
    /// with a respawn's event (the camera's cut). Outside the box, no ride.
    #[test]
    fn the_prediction_rides_a_lift() {
        let mut r = [0u8; 0x34];
        for (k, v) in [-1063i16, -320, -4892, 262, 350, 100]
            .into_iter()
            .enumerate()
        {
            r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        r[0x10..0x14].copy_from_slice(&45.8f32.to_be_bytes());
        (r[0x15], r[0x17], r[0x1B]) = (8 << 2 | 2, 0x2B, 1);
        let record = |position| SpawnPoint {
            position,
            byte_06: 0,
            yaw: 175,
            camera: [0.0; 3],
            byte_14: 0,
            byte_15: 0,
        };
        let arena = MatchArena::new(&[record([0; 3]), record([-2337, 1384, -3894])], 0x2B, &r);
        let tick = |pos| {
            let place = Placement {
                index: 0,
                pos,
                facing: 0,
                camera: [0.0; 3],
            };
            let f = Fighter::new(&place, 0, 5);
            let (fighting, world) = (Fighting::new(0, &f), CollisionWorld::default());
            let (m, i, events) = own_tick(
                9,
                false,
                Settings::default(),
                &f.body,
                &fighting,
                &Input::default(),
                &Vec::new(),
                &[],
                &arena,
                &world,
                &mut None,
            );
            let rode = events
                .iter()
                .any(|e| matches!(e, Event::Respawn { at, .. } if at.index == 1));
            (rode, m.players[i].body)
        };
        let (rode, b) = tick([-1060.0, -259.0, -4860.0]);
        let off = Vec2::new(b.pos[0] + 2337.0, b.pos[2] + 3894.0).length();
        assert!(rode && off <= 41.0 && b.facing == 175 << 8, "{off} {b:?}");
        assert!(!tick([-827.0, -437.0, -4620.0]).0);
    }

    /// 0x2D's gate lifts (shape 3) take the last move (`beach-environment.md` §7): a body whose move
    /// crossed a gate's plane from behind rides on that tick, as the server's `Lifts::ride` on the
    /// same two positions; past the gate with no earlier position (a respawn's), no ride.
    #[test]
    fn the_prediction_rides_a_gate() {
        let mut r = [0u8; 0x34];
        for (k, v) in [0i16, 0, 0, 100, 100, 20].into_iter().enumerate() {
            r[2 * k..2 * k + 2].copy_from_slice(&v.to_be_bytes());
        }
        (r[0x15], r[0x17], r[0x1B]) = (8 << 2 | 3, 0x2D, 1);
        let record = |position| SpawnPoint {
            position,
            byte_06: 0,
            yaw: 0,
            camera: [0.0; 3],
            byte_14: 0,
            byte_15: 0,
        };
        let arena = MatchArena::new(&[record([0; 3]), record([3000, 0, 3000])], 0x2D, &r);
        let place = |pos| Placement {
            index: 0,
            pos,
            facing: 0,
            camera: [0.0; 3],
        };
        let tick = |pos, before: &mut Option<[f32; 3]>| {
            let f = Fighter::new(&place(pos), 0, 5);
            let (fighting, world) = (Fighting::new(0, &f), CollisionWorld::default());
            let (_, _, events) = own_tick(
                9,
                false,
                Settings::default(),
                &f.body,
                &fighting,
                &Input::default(),
                &Vec::new(),
                &[],
                &arena,
                &world,
                before,
            );
            (events.iter()).any(|e| matches!(e, Event::Respawn { at, .. } if at.index == 1))
        };
        let (behind, past) = ([0.0, 10.0, -5.0], [0.0, 10.0, 5.0]);
        let mut before = None;
        assert!(!tick(behind, &mut before));
        assert_eq!(before, Some(behind));
        assert!(tick(past, &mut before));
        assert!(!tick(past, &mut None));
        let mut lifts = conker_rules::lift::Lifts::new(&r, 0x2D);
        let players = vec![Fighter::new(&place(behind), 0, 5)];
        let mut m = Match::new(&arena.0, players, Rng::default(), 0);
        assert!(lifts.ride(&mut m, &arena.0).is_empty());
        m.players[0].body.pos = past;
        assert_eq!(lifts.ride(&mut m, &arena.0).len(), 1);
    }

    #[test]
    fn others_push_from_where_the_server_step_finds_them() {
        let at = |x: f32| Player(PlayerState::spawn([x, 0.0, 0.0], 0));
        let mut h = ConfirmedHistory::default();
        h.insert_present(Tick(10), at(0.0));
        h.insert_present(Tick(11), at(3.0));
        let x = |slot, own, tick| pushing_body(&h, slot, own, tick).map(|b| b.pos[0]);
        // Within the history: after `tick`'s step for an earlier slot, the tick before for a later.
        assert_eq!(x(0, 1, 11), Some(3.0));
        assert_eq!(x(2, 1, 11), Some(0.0));
        // Past its end, on at the last step's pace.
        assert_eq!(x(0, 1, 15), Some(15.0));
        assert_eq!(x(2, 1, 15), Some(12.0));
        assert_eq!(x(2, 1, 5), None);
    }

    #[test]
    fn hud_reads_health_weapon_score_clock_and_respawn() {
        let mut c = Combat {
            health: 5,
            weapon: 0x09,
            drawn: true,
            rounds: 58,
            grenades: 3,
            kills: 2,
            ..Combat::default()
        };
        let m = MatchView {
            timer: 34_554,
            time: 10,
            ..MatchView::default()
        };
        assert_eq!(
            hud_text(&c, &m),
            "HP 5/6   twin guns (drawn) 58 rounds   grenades 3   kills 2   9:35"
        );
        (c.health, c.weapon, c.respawn_in) = (0, 0, 45);
        assert!(hud_text(&c, &m).ends_with("kills 2   9:35\nDEAD - respawn in 1.5 s"));
        // Untimed: no clock, the lives left.
        c.lives_left = 2;
        let untimed = MatchView { time: 0, ..m };
        assert!(hud_text(&c, &untimed).starts_with("HP 0/6   lives 2   grenades 3   kills 2\n"));
    }

    #[test]
    fn clips_cover_the_movement_states() {
        let clip = |pair, logical: &[u16], duration_ticks| ClipInfo {
            pair,
            logical: logical.to_vec(),
            frames: 10,
            duration_ticks,
        };
        let clips = Clips::new(&[
            clip(0, &[16, 83], 44),
            clip(1, &[2, 18], 28),
            clip(2, &[15, 186, 2], 52),
        ]);
        let names: Vec<(u16, &str, u16)> = (clips.0.iter())
            .map(|(id, name, ticks)| (*id, name.as_str(), *ticks))
            .collect();
        assert_eq!(
            names,
            vec![
                (2, "pair_0001_anim_0002", 28),
                (15, "pair_0002_anim_0015", 52),
                (16, "pair_0000_anim_0016", 44),
                (18, "pair_0001_anim_0002", 28),
                (83, "pair_0000_anim_0016", 44),
                (186, "pair_0002_anim_0015", 52)
            ]
        );
        assert_eq!((clips.duration(15), clips.duration(54)), (52, 1));
    }

    /// Each map's bodies: 1a's class 1 is the Wise Guy, 2b's villagers keep theirs, the sides are
    /// their own everywhere.
    #[test]
    fn bodies_by_map_and_class() {
        let c = |side, class| Combat {
            side,
            class,
            ..Combat::default()
        };
        let drawn = |map, c: Combat| characters(map)[body(&c)];
        assert_eq!(drawn("1a", c(Side::Squirrels, 1)), "59");
        assert_eq!(drawn("1a", c(Side::Tediz, 0)), "117");
        assert_eq!(drawn("2b", c(Side::Squirrels, 0x14)), "156");
        assert_eq!(drawn("2b", c(Side::Squirrels, 0x1D)), "157");
        assert_eq!(characters("2d"), PACK_CHARACTERS);
    }
}
