//! Sound, as the original plays it in a match (`../lab/audio/README.md`): the asset pack's
//! `audio/sounds.json` and `audio/events.json` ([`conker_assets::audio`]) say which game event plays
//! which sound; this module finds the events in what the client has of the match and plays them.
//!
//! - **Events.** The server sends none, so each tick ([`hear`], after the animation clocks) reads
//!   them from the replicated state: a player's clock passing a clip event's time (the local
//!   player's predicted clock, the others' from [`Combat::anim`]; `held.json`'s rule, [`passes`]);
//!   its clip, weapon and model in hand starting or ending a cue's condition (firing, holding,
//!   burning); its fire clip starting (a shot) or its scope coming up; its health dropping (a hit:
//!   [`hit_weapon`] reads the weapon from the victim's first reaction); the match ([`MatchView`]: a
//!   pickup taken, a rocket gone, weapon 0x14's shots, War: Total War's gas); a gas canister taken
//!   (its player's [`Combat::carrying`] going on), War: Colors' flag taken ([`Combat::flag`]); the grenades in flight (the local fighter
//!   carries them all: launch, bounces, fuse and blast); the background and the lamps' buzz (each
//!   flash of [`crate::lights::Flickers`]); a War outro's voice line at its frame (unpositioned, at
//!   [`MIX`]; its volume is I; it stops a jingle playing, one MP3 stream, I); a mounted
//!   gun's state changing and its trigger let go ([`MatchView::guns`]), the local gunner's zoom and
//!   aim moving; War: Beach's detonation ([`MatchView::cooldown`] going on: its three blasts and
//!   debris, the presser's radio voice), strikes (a villager's [`Combat::deadline`] reaching 0: the
//!   call and the villager's cry, the blast 40 frames on, debris), the plunger's radio voice in trigger
//!   0x25 while it is ready, and the local escapee's radio line; War: Colors' captures (a team's
//!   score rising: 30 ticks on, its jingle for everyone and each loser's voice); the music ducks
//!   while a line plays ([`duck`]). Radio lines ([`Sounds::radio`]: Beach's voices, a canister or a
//!   flag taken) are the click on, the line its cue's `delay` later, the click off once it has
//!   played (a bot's: the line only), at most one a slot every 300 ticks.
//! - **The mask's muffle** ([`Sounds::mask`]): while a gas mask's put-on is in force, any player's
//!   (C `func_15074664`: no camera test), the effects (not the background or the lines) go through
//!   a [`MUFFLE_HZ`] low-pass ([`Biquad`]), the ones already playing too.
//! - **Rules** (`events.json`): the listener is the local player. Its own sounds with `own` play
//!   centred at that volume, the rest by distance (full within `near`, none from `far`, linear
//!   between, silent below [`QUIET`]) and pan (along the follow camera's right). A sound on channel
//!   a or b stops its owner's last one there; `chance`, `if_idle`, `pitch` (cents, as the playback
//!   rate); a loop plays until its cue's condition ends, a stop event, its owner's next sound on the
//!   channel or `ticks`. Every sound started logs a `SOUND` line (debug), headless clients too
//!   (without a device a sound that does not loop is over at once, so `if_idle` always passes).
//! - **Playback** (windowed): rodio's mixer, each sound a [`Playing`] source with its loop points,
//!   envelope and pitch, at the two ear volumes the tick sets ([`MIX`] for effects, the background
//!   recording at 1.0, equal-power pan), the effects through the muffle while [`MUFFLED`]. The device opens at once natively and on the first key
//!   press or click in the browser (a page may start audio only after a gesture), then every WAV
//!   loads ([`WavLoader`]). M mutes ([`MUTE_KEY`]).

use crate::{Anim, FollowCamera, held};
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;
use conker_anim::AnimPlayer;
use conker_anim::combat::{
    BLOWN_UP, BURN_END, BURNING, DEATH_BLAST, DEATH_SHOT, FLAME_HIT, GRENADE_HOLD, GRENADE_PULL,
    GRENADE_THROW, HIT_CLUB, HIT_MELEE, HIT_SHOT,
};
use conker_assets::audio::Line;
use conker_assets::audio::{
    Audio, Background, Channel, ClipAction, ClipSound, Cue, Emitter, Follow, On, Play, Sound,
};
use conker_assets::outro::{Outro, frame};
use conker_assets::{Held, Manifest, Pickup};
use conker_camera::volume::CameraVolume;
use conker_rules::net::Fighting;
use conker_rules::projectile::{Grenade, ROCKET_SPEED, SHOT_STEP};
use conker_rules::weapon::{Attack, Weapon, weapon};
use conker_rules::{Rng, beach, gun};
use conker_sim::net::{Combat, GunView, MatchView, Player, PlayerId};
use core::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, TAU};
use core::time::Duration;
use lightyear::core::timeline::is_in_rollback;
use lightyear::prelude::*;
use rodio::{ChannelCount, SampleRate, Source};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

/// Sound effects against the background recording, which plays at 1.0: the original's mix, measured
/// 0.56-0.67 (`../lab/audio`, V `pick57`, `ctl_cd`; centre pan's 0.707 apart, which the pan law
/// adds back).
pub const MIX: f32 = 0.6;
/// A positional sound below this volume (out of 32767) is silent (`func_1000A420`).
pub const QUIET: f32 = 401.0;
/// Toggles the sound.
pub const MUTE_KEY: KeyCode = KeyCode::KeyM;
/// Every [`Playing`] is stereo.
const STEREO: ChannelCount = ChannelCount::new(2).unwrap();
/// The background's voice id (the game's sound ids start at 1).
const BACKGROUND: u16 = 0;
/// A War outro `n`'s voice line ([`crate::outro`]) plays as id `VOICE - n` (the game's
/// sound ids end at 1762), the maps' lines ([`Pack::lines`]) as in [`LINES`].
const VOICE: u16 = u16::MAX;
const ESCAPE: u16 = VOICE - 0x100;
/// The maps' voice lines by id: War: Beach's escape, War: Colors' capture jingles (by the scoring
/// team: the squirrels', the Tediz').
const LINES: [(u16, &str); 3] = [
    (ESCAPE, "escape"),
    (ESCAPE - 1, "jingle-squirrels"),
    (ESCAPE - 2, "jingle-tediz"),
];
/// A War outro `n`'s song ([`conker_assets::outro::Outro::song`]) plays as id `SONG - n`: not an
/// MP3 (it ducks nothing) nor the background (nothing ducks it).
const SONG: u16 = ESCAPE - 0x100;
/// The background's stem ([`Follow`]): War: Beach's song 0x4D's channels 13 and 14.
const STEM: u16 = SONG - 0x100;
/// Whether a voice is an MP3 line: a map's ([`LINES`]) or an outro's voice.
fn mp3(id: u16) -> bool {
    id > ESCAPE - LINES.len() as u16
}
/// The outros' script sounds' volume and distances (`func_1000FA64(id, …, 0x7F7F, 0x7D00, 0x7918,
/// …)`: full volume within 31000, so only the pan tells where; I: the near/far reading).
const SCRIPT: [f32; 3] = [32639.0, 31000.0, 32000.0];
/// Ticks from a capture to its jingle and the losers' voices (script 0xBB's timer; V c1a, c1b:
/// capture 382, the calls 412).
const CHEER: u32 = 30;
/// The muffle's cutoff (Hz): the effects player's FX param 9 during a mask put-on (`func_10012020`
/// mode 5, C: 11025 → 520, its param 8 0 → 127, I: the wet mix, full).
pub const MUFFLE_HZ: f32 = 520.0;
/// The radio: ticks between a slot's lines (`0x800D9AB0`'s 600 VIs), the plunger's
/// voice trigger, and the ticks after a blast its debris lands in (V ev2 655-694 after 617, ev3
/// 1082-1109 after 1044).
const RADIO_GAP: u32 = 300;
const READY_ZONE: usize = 0x25;
const DEBRIS: [i32; 2] = [38, 77];
/// Ticks a loop with `ticks` fades out over once they have run (killing bullets' 1278, C
/// `func_1000EBC4`; the length is I).
const FADE: u32 = 12;
/// Weapon ids the cues use for the grenade, the rocket launcher and weapon 0x14.
const GRENADE: u8 = 0x26;
const ROCKETS: u8 = 0x16;
const SHOTS: u8 = 0x14;
/// The browser's hint until the first key press or click.
#[cfg(target_family = "wasm")]
const HINT: &str = "Press a key or click for sound";

/// Whether a pack has sound: `pack.json`'s `audio` list is not empty.
pub fn has_sound(manifest: &[u8]) -> bool {
    serde_json::from_slice::<Manifest>(manifest).is_ok_and(|m| !m.audio.is_empty())
}

/// The pack's sound data, the held models (Holding cues) and the pickups (Pickup cues), with each
/// body's character ([`crate::characters`]) and clip events sorted by clip; the map's background,
/// emitters (its lamps' buzz), voice lines and (War: Beach) the plunger's voice trigger.
#[derive(Resource)]
struct Pack {
    audio: Audio,
    held: Vec<Held>,
    pickups: Vec<Pickup>,
    characters: Vec<&'static str>,
    clips: Vec<Vec<ClipSound>>,
    background: Option<Background>,
    /// The background's stem and the point its volume follows ([`Follow`]).
    stem: Option<(Background, Follow)>,
    emitters: Vec<Emitter>,
    lines: std::collections::BTreeMap<String, Line>,
    ready_zone: Option<CameraVolume>,
    /// The map's sound zones ([`conker_assets::audio::Zone`]) and their trigger records' tests.
    zones: Vec<(conker_assets::audio::Zone, CameraVolume)>,
    /// Scene 0x34's traps ([`crate::traps::sounding`]).
    traps: Vec<conker_rules::traps::Trap>,
}

impl Pack {
    fn new(
        mut audio: Audio,
        held: Vec<Held>,
        pickups: Vec<Pickup>,
        map: &str,
        triggers: &[u8],
    ) -> Self {
        let characters = crate::characters(map);
        let mut clips: Vec<_> = (characters.iter())
            .map(|c| audio.events.characters.remove(*c).unwrap_or_default())
            .collect();
        for c in &mut clips {
            c.sort_by_key(|e| e.clip); // stable: each clip's events stay in time order
        }
        let traps = match map == "34" {
            true => conker_rules::traps::parse(triggers),
            false => Vec::new(),
        };
        let ready_zone = (map == "2b")
            .then(|| triggers.as_chunks::<0x34>().0.get(READY_ZONE))
            .flatten()
            .map(CameraVolume::parse_record);
        let map = audio.events.maps.get(map);
        let backgrounds = map.map_or(&[][..], |m| &m.background[..]);
        let background = backgrounds.iter().find(|b| b.follow.is_none()).cloned();
        let stem = (backgrounds.iter()).find_map(|b| Some((b.clone(), b.follow.clone()?)));
        let emitters = map.map_or(Vec::new(), |m| m.emitters.clone());
        let lines = map.map_or_else(Default::default, |m| m.lines.clone());
        let records = triggers.as_chunks::<0x34>().0;
        let zones = (map.iter().flat_map(|m| &m.zones))
            .filter_map(|z| {
                let r = records.get(usize::from(z.trigger))?;
                Some((z.clone(), CameraVolume::parse_record(r)))
            })
            .collect();
        Self {
            audio,
            held,
            pickups,
            characters,
            clips,
            background,
            stem,
            emitters,
            lines,
            ready_zone,
            zones,
            traps,
        }
    }
}

/// Registers sound: the tick's events and voices ([`hear`], headless too) and, windowed, the device,
/// the WAV loader and the mute key. `map`: whose ambience plays (`triggers`: its trigger table).
/// `seed`: the client's own random sounds.
#[allow(clippy::too_many_arguments)]
pub fn build(
    app: &mut App,
    audio: Audio,
    held: Vec<Held>,
    pickups: Vec<Pickup>,
    windowed: bool,
    map: &str,
    triggers: &[u8],
    seed: u64,
) {
    app.insert_resource(Pack::new(audio, held, pickups, map, triggers));
    app.insert_resource(Sounds {
        rng: Rng::new(seed | 1),
        duck: DUCK[0],
        ..default()
    });
    app.add_systems(
        FixedUpdate,
        hear.after(crate::tick_anims)
            .after(crate::lights::flicker)
            .run_if(not(is_in_rollback)),
    );
    if windowed {
        app.init_asset::<Pcm>().init_asset_loader::<WavLoader>();
        app.add_systems(
            Update,
            (open.run_if(not(resource_exists::<Output>)), mute, running),
        );
        #[cfg(target_family = "wasm")]
        note(HINT);
    }
}

/// Whose a sound is: a player's (channels a and b are per player, by [`Combat::slot`]), a grenade's
/// (birth tick, thrower), one of weapon 0x14's shots in flight (its index), a mounted gun's (its
/// index), or nobody's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Player(u16),
    Grenade(u32, usize),
    Shot(usize),
    Gun(usize),
    /// An outro's script sound (an index of its `sounds`).
    Outro(usize),
    /// A map's sound zone (an index of its `zones`).
    Zone(usize),
    /// Scene 0x34's trap (an index of its `traps`).
    Trap(usize),
    World,
}

/// Where a sound starts from and whose it is: `follows` a player's slot (it moves with it), `tag` the
/// cue whose condition keeps it playing; `flat`: heard centred at its volume wherever it is (the
/// radio, flag 0x100 with near/far 0x7FFE/0x7FFF).
#[derive(Clone, Copy)]
struct Src {
    owner: Owner,
    pos: [f32; 3],
    follows: Option<u16>,
    tag: Option<usize>,
    flat: bool,
}

impl Src {
    fn player(p: &Seen) -> Self {
        Self {
            owner: Owner::Player(p.slot),
            pos: p.pos,
            follows: Some(p.slot),
            tag: None,
            flat: false,
        }
    }

    fn at(pos: [f32; 3]) -> Self {
        Self {
            owner: Owner::World,
            pos,
            follows: None,
            tag: None,
            flat: false,
        }
    }

    /// The radio at `p`: its owner's, heard centred everywhere.
    fn radio(p: &Seen) -> Self {
        Self {
            flat: true,
            ..Self::player(p)
        }
    }
}

/// What the sound rules read of a player in a tick; last tick's is its [`Heard`].
#[derive(Clone, Copy)]
struct Seen {
    slot: u16,
    /// Its body ([`crate::body`]): its character's sounds.
    body: usize,
    pos: [f32; 3],
    clock: AnimPlayer,
    /// The weapon held (0: none) and drawn: the local player's prediction, the others' [`Combat`].
    weapon: u8,
    drawn: bool,
    health: u8,
    /// [`Combat::anim`]: the rules' animation, a hit's first reaction on the tick it lands.
    reaction: u16,
    scope: bool,
    local: bool,
    /// The floor's surface kind (bits 0-4 of its word) and the horizontal speed: footsteps.
    floor: Option<u8>,
    speed: f32,
    /// Total War: carrying its team's gas canister, and whether it is a bot ([`On::Carry`]).
    carry: Option<(u8, bool)>,
    /// War: Colors: carrying flag f, and whether it is a bot ([`On::Flag`]).
    flag: Option<(u8, bool)>,
    /// A bot (no radio), its team; War: Beach's escape freeze and deadline ([`Combat`]).
    bot: bool,
    team: u8,
    freeze: u8,
    deadline: u16,
    /// Wearing a gas mask ([`Combat::masked`]; the local player's prediction).
    masked: bool,
}

/// A player as the last tick saw it.
#[derive(Component, Clone, Copy)]
struct Heard(Seen);

/// The listener: the local player (its slot and position) and its follow camera's right (xz).
#[derive(Clone, Copy)]
struct Ear {
    slot: u16,
    pos: [f32; 3],
    right: [f32; 2],
}

impl Ear {
    fn new(slot: u16, pos: [f32; 3], camera: &FollowCamera) -> Self {
        let (eye, at) = camera.drawn.view(&camera.follow);
        let ahead = at - eye;
        let right = Vec2::new(-ahead.z, ahead.x).normalize_or_zero();
        Self {
            slot,
            pos,
            right: right.to_array(),
        }
    }
}

/// A sound playing (or, without a device, as if it were): what it is and how loud, its mixer
/// controls once it is on the mixer.
struct Voice {
    id: u16,
    owner: Owner,
    channel: Channel,
    tag: Option<usize>,
    pos: [f32; 3],
    follows: Option<u16>,
    /// Unpositioned stereo at `volume` (the background recording).
    flat: bool,
    /// Volumes out of 32767 (footsteps scaled by speed), and the distances.
    volume: f32,
    own: Option<f32>,
    near: f32,
    far: f32,
    /// [`MIX`] × the sample's own volume (1 for the background).
    base: f32,
    speed: f32,
    looped: bool,
    /// The tick a loop with `ticks` ends, and the tick its fade began.
    until: Option<u32>,
    fade: Option<u32>,
    started: u32,
    stopped: bool,
    control: Option<Arc<Control>>,
}

impl Voice {
    /// Its volume out of 1 and pan (0 left, 0.5 centre, 1 right) for `ear` (`func_1000A420`): the
    /// listener's own player's sounds with `own` centred at that volume; the others by distance
    /// (linear from `near` to `far`) and the side of the camera they are on.
    fn level(&self, ear: &Ear) -> (f32, f32) {
        if self.flat {
            return (self.volume / 32767.0, 0.5);
        }
        if let (Some(own), true) = (self.own, self.follows == Some(ear.slot)) {
            return (own / 32767.0, 0.5);
        }
        let d: [f32; 3] = core::array::from_fn(|i| self.pos[i] - ear.pos[i]);
        let dist = dist2(self.pos, ear.pos).sqrt();
        let near = if self.far > self.near {
            ((self.far - dist) / (self.far - self.near)).clamp(0.0, 1.0)
        } else {
            f32::from(dist <= self.near)
        };
        let volume = self.volume * near;
        if volume < QUIET {
            return (0.0, 0.5);
        }
        let xz = (d[0] * d[0] + d[2] * d[2]).sqrt();
        let side = if xz < 1.0 {
            0.0
        } else {
            (d[0] * ear.right[0] + d[2] * ear.right[1]) / xz
        };
        (volume / 32767.0, 0.5 + 0.5 * side)
    }

    /// The two ears' volumes on the mixer at `tick`.
    fn ears(&self, ear: &Ear, tick: u32, muted: bool) -> [f32; 2] {
        let (volume, pan) = self.level(ear);
        let fade = self
            .fade
            .map_or(1.0, |f| 1.0 - tick.saturating_sub(f) as f32 / FADE as f32);
        let v = if muted {
            0.0
        } else {
            self.base * volume * fade
        };
        if self.flat {
            return [v, v];
        }
        let a = pan * FRAC_PI_2;
        [v * a.cos(), v * a.sin()]
    }
}

/// A cue's sound due later (a gun's case, a body part landing), or (`end`) the end of a cue held by
/// a condition: its sounds stop and its `then` plays.
struct Later {
    tick: u32,
    cue: usize,
    src: Src,
    end: bool,
}

/// The sound state: the voices, sounds due later, and last tick's projectiles and pickups (what
/// changed is an event).
#[derive(Resource, Default)]
struct Sounds {
    rng: Rng,
    voices: Vec<Voice>,
    later: Vec<Later>,
    /// Last tick's grenades (the local fighter's: all in flight), their bounces so far, rockets,
    /// shots and taken pickups; false until the first tick with a match.
    grenades: Vec<(u32, Grenade)>,
    bounces: Vec<((u32, usize), usize)>,
    rockets: Vec<[f32; 3]>,
    shots: Vec<[f32; 3]>,
    taken: u64,
    /// Total War's gas out last tick ([`On::Gas`]).
    gas: bool,
    primed: bool,
    /// The tick a rocket last went off (a blast's hit is a rocket's if one did just before).
    rocket_at: Option<u32>,
    background: bool,
    /// This tick's players (last tick's, this tick's), reused.
    players: Vec<(Seen, Seen)>,
    muted: bool,
    /// The mounted guns last tick, and the local gunner's zoom and aim ([`Self::gunner`]).
    guns: Vec<GunView>,
    turning: Option<Turning>,
    /// War: Beach: the plunger's cooldown last tick (None before the first), the strikes called
    /// (slot, where, the call's tick; till their blast). The radio: each slot's last line (War:
    /// Beach's by speaker, Total War's and Colors' by team) and the lines playing (speaker, the tick
    /// the line is due, its cue). The music's volume ([`duck`]).
    cooldown: Option<u16>,
    strikes: Vec<(u16, [f32; 3], u32)>,
    radio_at: HashMap<u16, u32>,
    radios: Vec<(u16, u32, usize)>,
    duck: f32,
    /// War: Colors: the scores last tick (None before the first), the captures' cheers due (tick,
    /// the scoring team).
    scores: Option<[u16; 2]>,
    cheers: Vec<(u32, u8)>,
    /// The mask's muffle ([`Self::mask`]) and the players whose put-on was their last mask event
    /// (s126+0x94).
    muffled: bool,
    wearers: HashSet<u16>,
    /// The outro's script sounds started (indices of its `sounds`).
    outro: HashSet<usize>,
    /// The map's sound zones started (indices of [`Pack::zones`]).
    zones: HashSet<usize>,
    /// The gun releases' replays due: tick, cue, sound id, where ([`Self::guns`]).
    replays: Vec<(u32, usize, u16, Src)>,
}

/// The songs' duck factor while an MP3 line plays (`func_1000D758`, each frame; a map's line or an
/// outro's voice): 0x8000 down to 0x36B0 at −0x400 a frame, back at +0x1000 a frame once it is
/// over; the music plays at its volume × factor / 0x8000 (V Beach's 0x4650: 0x1E09 at the bottom,
/// −0x233 and +0x8CA a frame; `b3` 169-187, 686-690; snd ev 103-121, 308-312).
const DUCK: [f32; 4] = [32768.0, 14000.0, 1024.0, 4096.0];

/// A stem's gain at the listener's distance `d` from its point (`func_1000A420`, `func_1000E588`):
/// v = clamp(32767·(far − d)/(far − near), 0, 32767) >> 8, then min(v, 100) %.
pub fn follow(f: &Follow, d: f32) -> f32 {
    let (near, far) = (f32::from(f.near), f32::from(f.far));
    let v = (32767.0 * (far - d) / (far - near)).clamp(0.0, 32767.0) as u32 >> 8;
    v.min(100) as f32 / 100.0
}

/// A sound zone's volume (out of 32767) at the listener's distance `d` (`func_1000A420`): full
/// within `near`, linear to none at `far`, silent below [`QUIET`].
pub fn zone_volume(play: &Play, d: f32) -> f32 {
    let (near, far) = (f32::from(play.near), f32::from(play.far));
    let v = f32::from(play.volume) * ((far - d) / (far - near)).clamp(0.0, 1.0);
    if v < QUIET { 0.0 } else { v }
}

/// The duck factor a tick on from `level`, a line playing or not ([`DUCK`]).
pub fn duck(level: f32, line: bool) -> f32 {
    let [full, low, down, up] = DUCK;
    match line {
        true => (level - down).max(low),
        false => (level + up).min(full),
    }
}

/// The local gunner's zoom and aim (the scope's yaw target times its pitch target) last tick, and
/// how many ticks in a row each has moved.
#[derive(Clone, Copy)]
struct Turning {
    last: [f32; 2],
    moved: [u32; 2],
}

/// What a tick's rules read besides the sound state.
struct Cx<'a> {
    pack: &'a Pack,
    tick: u32,
    /// The oracle read the tick stands for in `--wait` replays, else -1 (logs).
    read: i64,
    ear: Ear,
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

/// A player's character folder (cues name it).
fn character<'a>(cx: &Cx<'a>, p: &Seen) -> &'a str {
    cx.pack.characters.get(p.body).copied().unwrap_or_default()
}

/// Whether a clip event at `time` fires this tick: the clock passed it (`held.json`'s rule, V on the
/// oracle: an event at 1.0 fires as the clock goes from 0.43, or 0.009, past it). A new clip from its
/// start (this tick's time less the rate) through this tick's time; the same clip after last tick's
/// time through this one's, round the loop when it wrapped.
fn passes(was: &AnimPlayer, is: &AnimPlayer, time: f32) -> bool {
    if is.id != was.id {
        return (is.time - is.rate..=is.time).contains(&time);
    }
    if is.time >= was.time {
        was.time < time && time <= is.time
    } else {
        time > was.time || time <= is.time
    }
}

/// Whether held weapon `w`'s model is in the player's hand ([`held::shows`]).
fn holding(held: &[Held], w: u8, p: &Seen) -> bool {
    let draw = weapon(w).map(|w| w.draw.0);
    held.iter()
        .filter(|h| h.create.iter().any(|c| Some(c.0) == draw))
        .any(|h| held::shows(h, p.weapon, p.drawn, &p.clock))
}

fn burning(p: &Seen) -> bool {
    [FLAME_HIT, BURNING, BURN_END].contains(&p.clock.id)
}

/// The weapon that hit `victim`, which the server does not send, from its first reaction (its
/// [`Combat::anim`] as the hit lands, conker-rules' `damage::sequence`): 0x11B melee 0x21 or the
/// club 0x40 (the nearest holding it drawn), 0x11C the club's second swing, 0xD3
/// weapon 0x14's shot or the crossbow's bolt, 0x69 the flame, 0x19C weapon 0x23 or the mounted gun
/// (its seated gunner's weapon, the nearest of the two); a blast (0xF3) is a
/// rocket's when one went off this tick or the last (`rocket`), else the shotgun's or a grenade's;
/// the kill 0x17E is the sniper's, melee 0x22's, the crossbow's or the shotgun's, and the stagger
/// 0x11D a bullet's or the shotgun's (or a lethal shot or burn): the nearest other player holding
/// one of those drawn decides (a burning victim: the flame), else weapon 0x14, a grenade, the sniper
/// rifle and the twin guns. I: wrong when another player holds such a weapon nearer than the
/// shooter.
fn hit_weapon(victim: &Seen, burnt: bool, others: &[(Seen, Seen)], rocket: bool) -> u8 {
    let nearest = |ids: &[u8]| {
        let d = |p: &Seen| dist2(p.pos, victim.pos);
        (others.iter().map(|o| &o.1))
            .filter(|o| o.slot != victim.slot && o.drawn && ids.contains(&o.weapon))
            .min_by(|a, b| d(a).total_cmp(&d(b)))
            .map(|o| o.weapon)
    };
    match victim.reaction {
        HIT_MELEE => nearest(&[0x40]).unwrap_or(0x21),
        HIT_CLUB => 0x40,
        HIT_SHOT => nearest(&[SHOTS, 0x18]).unwrap_or(SHOTS),
        FLAME_HIT => 0x15,
        DEATH_BLAST => nearest(&[0x23, conker_camera::GUN]).unwrap_or(0x23),
        BLOWN_UP if rocket => ROCKETS,
        BLOWN_UP => nearest(&[0x37]).unwrap_or(GRENADE),
        DEATH_SHOT => nearest(&[0x24, 0x22, 0x18, 0x37]).unwrap_or(0x24),
        _ if burnt => 0x15,
        _ => nearest(&[0x09, 0x39, 0x24, SHOTS, 0x37]).unwrap_or(0x09),
    }
}

/// The projectiles of last tick's list `old` that `new` no longer has. Lists keep their order (new
/// ones go last), so each one left is the next of `new` within `step` of where it was.
pub(crate) fn gone(
    old: &[[f32; 3]],
    new: &[[f32; 3]],
    step: f32,
) -> impl Iterator<Item = [f32; 3]> {
    let mut next = new.iter().peekable();
    old.iter().copied().filter(move |o| {
        let kept = next.peek().is_some_and(|n| dist2(**n, *o) <= step * step);
        if kept {
            next.next();
        }
        !kept
    })
}

impl Sounds {
    /// A random number in `lo..=hi`.
    fn between(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.rng.next_u32() % (hi - lo).max(0).unsigned_abs().saturating_add(1)) as i32
    }

    /// Starts one of `sounds` as `play` says, from `src` (unless its chance or a busy channel says
    /// no), stopping its owner's last sound on the same channel; volumes × `scale`. Logs it.
    fn start(&mut self, cx: &Cx, play: &Play, sounds: &[u16], scale: f32, src: Src) {
        if play.chance < 255 && self.rng.next_u32() as u8 > play.chance {
            return;
        }
        let mine = |v: &Voice| !v.stopped && v.owner == src.owner && v.channel == play.channel;
        if play.channel != Channel::Free {
            if play.if_idle && self.voices.iter().any(mine) {
                return;
            }
            self.stop(mine);
        }
        if sounds.is_empty() {
            return;
        }
        let id = sounds[self.rng.next_u32() as usize % sounds.len()];
        let Some(sound) = cx.pack.audio.sounds.sounds.get(&id) else {
            return;
        };
        let cents = self.between(i32::from(play.pitch[0]), i32::from(play.pitch[1]));
        self.push(
            cx,
            Voice {
                id,
                owner: src.owner,
                channel: play.channel,
                tag: src.tag,
                pos: src.pos,
                follows: src.follows,
                flat: src.flat,
                volume: f32::from(play.volume) * scale,
                own: play.own.map(|v| f32::from(v) * scale),
                near: f32::from(play.near),
                far: f32::from(play.far),
                base: MIX * f32::from(sound.volume) / 127.0,
                speed: (cents as f32 / 1200.0).exp2(),
                looped: sound.looped.is_some(),
                until: play.ticks.map(|t| cx.tick + u32::from(t)),
                fade: None,
                started: cx.tick,
                stopped: false,
                control: None,
            },
        );
    }

    fn push(&mut self, cx: &Cx, v: Voice) {
        let (volume, pan) = v.level(&cx.ear);
        debug!(
            "SOUND t={} read={} id={} p={:?} vol={:.0} gain={volume:.3} pan={pan:.2}",
            cx.tick, cx.read, v.id, v.owner, v.volume
        );
        self.voices.push(v);
    }

    /// Stops the sounds `which` picks (each fades out over its envelope's release).
    fn stop(&mut self, which: impl Fn(&Voice) -> bool) {
        for v in self.voices.iter_mut().filter(|v| !v.stopped && which(v)) {
            v.stopped = true;
            if let Some(c) = &v.control {
                c.stop.store(true, Relaxed);
            }
        }
    }

    /// Plays every cue `matches` picks for player `p` (its character's or anyone's), from `src`.
    fn cues(&mut self, cx: &Cx, p: &Seen, src: Src, matches: impl Fn(&On) -> bool) {
        for cue in &cx.pack.audio.events.cues {
            if matches(&cue.on)
                && cue
                    .character
                    .as_deref()
                    .is_none_or(|c| c == character(cx, p))
            {
                self.start(cx, &cue.play, &cue.play.sounds, 1.0, src);
            }
        }
    }

    /// A cue held by a condition: it starts as the condition begins; the tick after it ends (its
    /// sound's callback sees it then, V e_w09: the twin guns' tail a read after the fire clip), its
    /// sounds stop and `then` plays where the player was.
    fn hold(&mut self, cx: &Cx, i: usize, cue: &Cue, was: bool, is: bool, p: &Seen) {
        let src = Src::player(p);
        if is && !was {
            let tagged = Src {
                tag: Some(i),
                ..src
            };
            self.start(cx, &cue.play, &cue.play.sounds, 1.0, tagged);
        } else if was && !is {
            let (tick, src) = (
                cx.tick + 1,
                Src {
                    follows: None,
                    ..src
                },
            );
            self.later.push(Later {
                tick,
                cue: i,
                src,
                end: true,
            });
        }
    }

    /// The sounds due by now ([`Later`]), before this tick's events.
    fn due(&mut self, cx: &Cx) {
        let mut k = 0;
        while k < self.later.len() {
            if self.later[k].tick > cx.tick {
                k += 1;
                continue;
            }
            let Later {
                cue: i, src, end, ..
            } = self.later.swap_remove(k);
            let cue = &cx.pack.audio.events.cues[i];
            if !end {
                self.start(cx, &cue.play, &cue.play.sounds, 1.0, src);
                continue;
            }
            self.stop(|v| v.owner == src.owner && v.tag == Some(i) && v.started < cx.tick);
            for then in &cue.then {
                self.start(cx, then, &then.sounds, 1.0, src);
            }
        }
    }

    /// Player `is`'s sounds this tick (`was`: last tick's): its clip's events, its cues' conditions,
    /// a shot, the scope, a hit.
    fn player(&mut self, cx: &Cx, was: &Seen, is: &Seen, all: &[(Seen, Seen)]) {
        let me = Src::player(is);
        let clips = &cx.pack.clips[is.body];
        let first = clips.partition_point(|c| c.clip < is.clock.id);
        for e in clips[first..].iter().take_while(|c| c.clip == is.clock.id) {
            if !passes(&was.clock, &is.clock, e.time) {
                continue;
            }
            match &e.action {
                ClipAction::Play(p) => self.start(cx, p, &p.sounds, 1.0, me),
                ClipAction::Footstep(kind) => self.footstep(cx, *kind, is),
                ClipAction::Stop(c) => self.stop(|v| v.owner == me.owner && v.channel == *c),
            }
        }
        // A canister or a flag taken: its team's radio line (`func_15103C14`'s slot).
        if let (Some((canister, bot)), None) = (is.carry, was.carry) {
            self.radio(cx, is, On::Carry { canister, bot }, u16::from(canister));
        }
        if let (Some((flag, bot)), None) = (is.flag, was.flag) {
            self.radio(cx, is, On::Flag { flag, bot }, u16::from(flag));
        }
        if self.mask(was, is) {
            debug!(
                "MUFFLE t={} read={} p={} masked={} muffled={}",
                cx.tick, cx.read, is.slot, is.masked, self.muffled
            );
        }
        // War: Beach's rockets fire with clip 0x207 (conker-rules' `beach::ROCKET_FIRE`).
        let fire = |w: u8, p: &Seen| {
            let rocket = |w: &Weapon| matches!(w.attack, Attack::Rocket { .. });
            let clip = |w: &Weapon| {
                w.fire.0 == p.clock.id || (rocket(w) && p.clock.id == beach::ROCKET_FIRE)
            };
            p.weapon == w && weapon(w).is_some_and(clip)
        };
        for (i, cue) in cx.pack.audio.events.cues.iter().enumerate() {
            if !cue
                .character
                .as_deref()
                .is_none_or(|c| c == character(cx, is))
            {
                continue;
            }
            match cue.on {
                On::Firing { weapon, ref clips } => {
                    let on = |p: &Seen| p.weapon == weapon && clips.contains(&p.clock.id);
                    self.hold(cx, i, cue, on(was), on(is), is);
                }
                // The grenade's hiss: from 4 ticks into the throw (V e_gren, sq_e_gren, ctlw_k*:
                // 1299 at the throw's read + 4, stopped by the blast), then with the grenade
                // ([`Self::grenades`]); a throw that never leaves the hand fades out.
                On::Holding { weapon: GRENADE } => {
                    let (was, is) = (&was.clock, &is.clock);
                    if is.id == GRENADE_THROW && passes(was, is, 4.0 * is.rate) {
                        let at = Src { tag: Some(i), ..me };
                        self.start(cx, &cue.play, &cue.play.sounds, 1.0, at);
                        let new = |v: &&mut Voice| v.tag == Some(i) && v.started == cx.tick;
                        if let Some(v) = self.voices.last_mut().filter(new) {
                            v.until = Some(cx.tick + 40);
                        }
                    }
                }
                On::Holding { weapon } => {
                    let held = &cx.pack.held;
                    self.hold(
                        cx,
                        i,
                        cue,
                        holding(held, weapon, was),
                        holding(held, weapon, is),
                        is,
                    );
                }
                On::Burning => self.hold(cx, i, cue, burning(was), burning(is), is),
                On::Shot { weapon } if fire(weapon, is) && !fire(weapon, was) => {
                    self.start(cx, &cue.play, &cue.play.sounds, 1.0, me);
                }
                On::Casing { weapon, delay } if fire(weapon, is) && !fire(weapon, was) => {
                    let tick = cx.tick + u32::from(delay);
                    self.later.push(Later {
                        tick,
                        cue: i,
                        src: me,
                        end: false,
                    });
                }
                On::Scope { weapon }
                    if is.local && is.weapon == weapon && is.scope && !was.scope =>
                {
                    self.start(cx, &cue.play, &cue.play.sounds, 1.0, me);
                }
                _ => {}
            }
        }
        if is.health < was.health {
            let lethal = is.health == 0;
            let rocket = self.rocket_at.is_some_and(|t| t + 1 >= cx.tick);
            let w = hit_weapon(is, burning(was), all, rocket);
            debug!(
                "HIT t={} read={} p={} weapon={w:#04x} lethal={lethal}",
                cx.tick, cx.read, is.slot
            );
            self.cues(cx, is, me, |on| {
                matches!(*on, On::Hit { weapon, lethal: l } if weapon == w && l.is_none_or(|l| l == lethal))
            });
            if lethal {
                self.gibs(cx, w, is);
            }
            // A grenade cooked too long goes off in the thrower's hand: no grenade flies.
            if w == GRENADE && [GRENADE_PULL, GRENADE_HOLD].contains(&was.clock.id) {
                self.world(cx, is.pos, |on| *on == On::Explosion { weapon: GRENADE });
            }
        }
    }

    /// The mask's muffle (`func_150642AC` cases 3 and 4 → `func_15074664`, C): any player's put-on
    /// (its mask going on; bots too, no camera test) muffles the effects; a take-off unmuffles them
    /// if that player's last mask event was a put-on (s126+0x94). A death drops the mask without a
    /// take-off. Whether the muffle changed.
    fn mask(&mut self, was: &Seen, is: &Seen) -> bool {
        let muffled = self.muffled;
        match (was.masked, is.masked) {
            (false, true) => {
                self.wearers.insert(is.slot);
                self.muffled = true;
            }
            (true, false) if is.health > 0 && self.wearers.remove(&is.slot) => {
                self.muffled = false;
            }
            _ => {}
        }
        self.muffled != muffled
    }

    /// A footstep of `kind` on the player's floor (`func_10011EB8`), louder with speed.
    fn footstep(&mut self, cx: &Cx, kind: u8, p: &Seen) {
        let steps = &cx.pack.audio.events.footsteps;
        let surface = p.floor.and_then(|f| steps.surfaces.get(&f));
        let Some(sounds) = surface.and_then(|s| s.get(usize::from(kind))) else {
            return;
        };
        let full = steps.full_speed.max(1.0);
        let scale = 0.5 + 0.5 * p.speed.clamp(0.0, full) / full;
        self.start(cx, &steps.play, sounds, scale, Src::player(p));
    }

    /// Body parts landing after a gory death by `w`: 6-12 of them, near the victim, over 2 s (I: the
    /// count and spread seen on the oracle, not read).
    fn gibs(&mut self, cx: &Cx, w: u8, p: &Seen) {
        let cues = &cx.pack.audio.events.cues;
        for (i, _) in (cues.iter().enumerate()).filter(|c| c.1.on == On::Gibs { weapon: w }) {
            for _ in 0..self.between(6, 12) {
                let (dx, dz) = (self.between(-200, 200), self.between(-200, 200));
                let pos = [p.pos[0] + dx as f32, p.pos[1], p.pos[2] + dz as f32];
                let tick = cx.tick + self.between(10, 60) as u32;
                self.later.push(Later {
                    tick,
                    cue: i,
                    src: Src::at(pos),
                    end: false,
                });
            }
        }
    }

    /// The grenades in flight (`now`; last tick's in `self.grenades`), whose sounds follow them: the
    /// thrower's hiss (its Holding cue, from the throw) goes with its grenade as it leaves the hand;
    /// its first landing plays the Flight cue and the next ones the Bounce cue's sounds in turn,
    /// none past them (V e_gren, sq_e_gren, ctlw_k*: 1447, 1448, 1449 at the three landings 27, 42
    /// and 50 ticks after the launch; the original picks the three in a random order); the fuse's
    /// last 60 ticks; the blast once one is gone, which stops them.
    fn grenades(&mut self, cx: &Cx, now: &[(u32, Grenade)]) {
        let cues = &cx.pack.audio.events.cues;
        for &(birth, g) in now {
            let owner = Owner::Grenade(birth, g.owner);
            let src = Src {
                owner,
                ..Src::at(g.pos)
            };
            let was = self
                .grenades
                .iter()
                .find(|o| (o.0, o.1.owner) == (birth, g.owner));
            let thrower = Owner::Player(g.owner as u16);
            for v in &mut self.voices {
                let hiss = v
                    .tag
                    .is_some_and(|i| cues[i].on == On::Holding { weapon: GRENADE });
                if was.is_none() && v.owner == thrower && hiss {
                    (v.owner, v.follows, v.until) = (owner, None, None);
                }
                if v.owner == owner {
                    v.pos = g.pos;
                }
            }
            let landed = was.is_some_and(|w| w.1.y_vel < 0.0 && g.y_vel > 0.0);
            let n = match self.bounces.iter_mut().find(|b| b.0 == (birth, g.owner)) {
                Some(b) => {
                    b.1 += usize::from(landed);
                    b.1
                }
                None if landed => {
                    self.bounces.push(((birth, g.owner), 1));
                    1
                }
                None => 0,
            };
            for (i, cue) in cues.iter().enumerate() {
                let sounds = match cue.on {
                    On::Flight {
                        weapon: GRENADE, ..
                    } if landed && n == 1 => Some(&cue.play.sounds[..]),
                    On::Bounce { weapon: GRENADE } if landed && n > 1 => {
                        cue.play.sounds.get(n - 2).map(core::slice::from_ref)
                    }
                    On::Fuse { weapon: GRENADE }
                        if cx.tick + 60 >= g.fuse
                            && !self
                                .voices
                                .iter()
                                .any(|v| v.owner == owner && v.tag == Some(i)) =>
                    {
                        Some(&cue.play.sounds[..])
                    }
                    _ => None,
                };
                if let Some(sounds) = sounds {
                    self.start(
                        cx,
                        &cue.play,
                        sounds,
                        1.0,
                        Src {
                            tag: Some(i),
                            ..src
                        },
                    );
                }
            }
        }
        let old = std::mem::take(&mut self.grenades);
        for (birth, g) in old
            .iter()
            .filter(|o| !now.iter().any(|n| (n.0, n.1.owner) == (o.0, o.1.owner)))
        {
            let owner = Owner::Grenade(*birth, g.owner);
            self.stop(|v| v.owner == owner);
            self.bounces.retain(|b| b.0 != (*birth, g.owner));
            self.world(cx, g.pos, |on| *on == On::Explosion { weapon: GRENADE });
        }
        self.grenades = old;
        self.grenades.clear();
        self.grenades.extend_from_slice(now);
    }

    /// The mounted guns' sounds (`On::Gun`, at the gun on its own channels: a state change stops
    /// its loops and plays its change's cues; C `func_1507BC14`) and the release (`On::GunStop`, at
    /// the gunner as the trigger falls; C `func_15065A5C`), whose sound plays again 15 ticks on, no
    /// longer following the gunner (its callback `func_1000EC24` after 30 VIs restarts it at the
    /// voice's level and pan, unpositioned; V gcQ).
    fn guns(&mut self, cx: &Cx, guns: &[GunView], players: &[(Seen, Seen)]) {
        let cues = &cx.pack.audio.events.cues;
        for (_, i, id, src) in (self.replays)
            .extract_if(.., |r| r.0 <= cx.tick)
            .collect::<Vec<_>>()
        {
            self.start(cx, &cues[i].play, &[id], 1.0, src);
        }
        let old = std::mem::replace(&mut self.guns, guns.to_vec());
        for (k, (was, is)) in old.iter().zip(guns).enumerate() {
            let owner = Owner::Gun(k);
            if was.state != is.state {
                self.stop(|v| v.owner == owner && v.looped);
                let on = On::Gun {
                    from: was.state,
                    to: is.state,
                };
                let at = Src {
                    owner,
                    ..Src::at(is.pos)
                };
                for cue in cues.iter().filter(|c| c.on == on) {
                    self.start(cx, &cue.play, &cue.play.sounds, 1.0, at);
                }
            }
            if was.trigger && !is.trigger {
                let gunner = (was.gunner).and_then(|s| players.iter().find(|p| p.1.slot == s));
                let src = gunner.map_or(Src::at(is.pos), |p| Src::player(&p.1));
                for (i, cue) in cues.iter().enumerate().filter(|c| c.1.on == On::GunStop) {
                    let Some(&id) = (cue.play.sounds)
                        .get(self.rng.next_u32() as usize % cue.play.sounds.len().max(1))
                    else {
                        continue;
                    };
                    self.start(cx, &cue.play, &[id], 1.0, src);
                    let again = Src {
                        follows: None,
                        ..src
                    };
                    self.replays.push((cx.tick + 15, i, id, again));
                }
            }
        }
    }

    /// The local gunner's zoom and aim (`scope`: its zoom and aim this tick; `None` off a gun; C
    /// `func_15126378`, `func_15127520`, `func_151277B0`), heard by the gunner only: the zoom's loop
    /// while it moves, its stop's sound as it stops and, 16 ticks on, the last one's, which decays
    /// in 0.1 s (its loop is held at full in the pack: stopped here); the aim's loop from the 5th
    /// tick (10th VI) it keeps changing, its stop's sound as it stops. Leaving the gun stops them.
    fn gunner(&mut self, cx: &Cx, scope: Option<[f32; 2]>) {
        let cues = &cx.pack.audio.events.cues;
        let cue = |on: On| cues.iter().position(|c| c.on == on);
        let zoom = [0, 1, 2].map(|step| cue(On::GunZoom { step }));
        let aim = [0, 1].map(|step| cue(On::GunAim { step }));
        let looping = [zoom[0], aim[0]];
        let src = Src {
            owner: Owner::Player(cx.ear.slot),
            follows: Some(cx.ear.slot),
            ..Src::at(cx.ear.pos)
        };
        let Some(now) = scope else {
            if self.turning.take().is_some() {
                self.stop(|v| v.tag.is_some() && looping.contains(&v.tag));
            }
            return;
        };
        let t = self.turning.get_or_insert(Turning {
            last: now,
            moved: [0; 2],
        });
        let before = t.moved;
        t.moved = [0, 1].map(|i| {
            if now[i] != t.last[i] {
                t.moved[i] + 1
            } else {
                0
            }
        });
        t.last = now;
        let after = t.moved;
        let play = |s: &mut Self, i: Option<usize>| {
            if let Some(i) = i {
                let tagged = Src {
                    tag: Some(i),
                    ..src
                };
                s.start(cx, &cues[i].play, &cues[i].play.sounds, 1.0, tagged);
            }
        };
        for (k, (steps, from)) in [(&zoom[..], 1), (&aim[..], 5)].into_iter().enumerate() {
            if after[k] == from {
                play(self, steps[0]);
            } else if after[k] == 0 && before[k] >= from {
                self.stop(|v| v.tag.is_some() && v.tag == steps[0]);
                play(self, steps[1]);
                if let Some(&Some(last)) = steps.get(2) {
                    let src = Src {
                        tag: Some(last),
                        ..src
                    };
                    let (tick, end) = (cx.tick + 16, false);
                    (self.later).push(Later {
                        tick,
                        cue: last,
                        src,
                        end,
                    });
                }
            }
        }
        for v in &mut self.voices {
            if v.tag.is_some() && v.tag == zoom[2] && v.started == cx.tick {
                v.until = Some(cx.tick + 3);
            }
        }
    }

    /// War: Beach (see the module doc): the detonation, the strikes, the plunger's voice trigger, the
    /// local escapee's line and the music's duck, and the radio's clicks off.
    fn beach(&mut self, cx: &Cx, m: &MatchView, players: &[(Seen, Seen)]) {
        let cues = &cx.pack.audio.events.cues;
        let later = |s: &mut Self, i: usize, src: Src, delay: u16| {
            let (tick, end) = (cx.tick + u32::from(delay), false);
            s.later.push(Later {
                tick,
                cue: i,
                src,
                end,
            });
        };
        let debris = |s: &mut Self, at: [f32; 3], n: i32| {
            for (i, _) in (cues.iter().enumerate()).filter(|c| c.1.on == On::Debris) {
                for _ in 0..n {
                    let (dx, dz) = (s.between(-200, 200), s.between(-200, 200));
                    let pos = [at[0] + dx as f32, at[1], at[2] + dz as f32];
                    let delay = s.between(DEBRIS[0], DEBRIS[1]) as u16;
                    later(s, i, Src::at(pos), delay);
                }
            }
        };
        if m.beach && self.cooldown == Some(0) && m.cooldown > 0 {
            for (i, cue) in (cues.iter().enumerate()).filter(|c| c.1.on == On::Detonation) {
                let at = cue.at.map_or(cx.ear.pos, |a| a.map(f32::from));
                later(self, i, Src::at(at), cue.delay);
                let n = self.between(3, 4);
                debris(self, at, n);
            }
            let presser = players
                .iter()
                .map(|p| p.1)
                .find(|p| beach::Plunger::under(p.pos, m.plunger));
            if let Some(p) = presser.filter(|p| p.team == beach::VILLAGERS) {
                self.radio(cx, &p, On::Plunger { bot: p.bot }, p.slot);
            }
        }
        self.cooldown = m.beach.then_some(m.cooldown);
        if !m.beach {
            return;
        }
        for (was, is) in players {
            if was.deadline > 0 && is.deadline == 0 && is.team == beach::VILLAGERS {
                self.cues(cx, is, Src::at(is.pos), |on| *on == On::StrikeCall);
                self.strikes.push((is.slot, is.pos, cx.tick));
            }
            let ready = cx.pack.ready_zone.is_some_and(|z| z.contains(is.pos));
            if ready && m.cooldown == 0 && is.team == beach::VILLAGERS && is.health > 0 {
                self.radio(cx, is, On::PlungerReady { bot: is.bot }, is.slot);
            }
            if is.local && was.freeze == 0 && is.freeze > 0 && is.team == beach::VILLAGERS {
                self.line(cx, LINES[0]);
            }
        }
        let strikes = std::mem::take(&mut self.strikes);
        for (slot, at, call) in strikes {
            let blown = players
                .iter()
                .any(|p| p.1.slot == slot && p.1.reaction == BLOWN_UP);
            if blown || cx.tick >= call + 40 {
                self.world(cx, at, |on| *on == On::StrikeBlast);
                let n = self.between(3, 4);
                debris(self, at, n);
            } else {
                self.strikes.push((slot, at, call));
            }
        }
    }

    /// A radio line that has played: the click off at its speaker.
    fn clicks_off(&mut self, cx: &Cx, players: &[(Seen, Seen)]) {
        let cues = &cx.pack.audio.events.cues;
        let radios = std::mem::take(&mut self.radios);
        for (slot, due, i) in radios {
            let owner = Owner::Player(slot);
            let speaking = |v: &Voice| v.owner == owner && cues[i].play.sounds.contains(&v.id);
            if cx.tick <= due || self.voices.iter().any(speaking) {
                self.radios.push((slot, due, i));
            } else if let Some(p) = players.iter().find(|p| p.1.slot == slot) {
                self.cues(cx, &p.1, Src::radio(&p.1), |on| {
                    *on == On::RadioClick { end: true }
                });
            }
        }
    }

    /// The music's volume ([`DUCK`]) while a line ([`LINES`]) or an outro's voice plays; its stem's
    /// also by the listener's distance ([`follow`]).
    fn duck_music(&mut self, cx: &Cx) {
        let line = (self.voices.iter()).any(|v| !v.stopped && mp3(v.id));
        self.duck = duck(self.duck, line);
        let [full, ..] = DUCK;
        for v in (self.voices.iter_mut()).filter(|v| v.id == BACKGROUND || v.id == STEM) {
            let (b, gain) = match (v.id, &cx.pack.stem) {
                (STEM, Some((b, f))) => {
                    let at = f.at.map(f32::from);
                    (b, follow(f, dist2(at, cx.ear.pos).sqrt()))
                }
                _ => match &cx.pack.background {
                    Some(b) => (b, 1.0),
                    None => continue,
                },
            };
            v.volume = f32::from(b.volume) * self.duck / full * gain;
            if v.id == STEM && cx.tick.is_multiple_of(30) {
                debug!(
                    "STEM t={} read={} gain={gain:.2} vol={:.0}",
                    cx.tick, cx.read, v.volume
                );
            }
        }
    }

    /// The map's sound zones ([`Pack::zones`]): the first tick any player is inside one, its sound
    /// starts and plays on for good (restarted whenever it ends: the record's voice), centred, at
    /// its volume by the listener's distance ([`zone_volume`]).
    fn zones(&mut self, cx: &Cx, players: &[(Seen, Seen)]) {
        for (i, (z, zone)) in cx.pack.zones.iter().enumerate() {
            let inside = || players.iter().any(|p| zone.contains(p.1.pos));
            if !self.zones.contains(&i) && !inside() {
                continue;
            }
            let at = z.at.map(f32::from);
            let d = match z.horizontal {
                true => ((at[0] - cx.ear.pos[0]).powi(2) + (at[2] - cx.ear.pos[2]).powi(2)).sqrt(),
                false => dist2(at, cx.ear.pos).sqrt(),
            };
            let volume = zone_volume(&z.play, d);
            let owner = Owner::Zone(i);
            if let Some(v) = self.voices.iter_mut().find(|v| v.owner == owner) {
                v.volume = volume;
                continue;
            }
            self.zones.insert(i);
            let sounds = &cx.pack.audio.sounds.sounds;
            let Some((id, sound)) = (z.play.sounds.first()).and_then(|id| sounds.get_key_value(id))
            else {
                continue;
            };
            let v = Voice {
                id: *id,
                owner,
                channel: Channel::Free,
                tag: None,
                pos: at,
                follows: None,
                flat: true,
                volume,
                own: None,
                near: 0.0,
                far: 0.0,
                base: MIX * f32::from(sound.volume) / 127.0,
                speed: 1.0,
                looped: sound.looped.is_some(),
                until: None,
                fade: None,
                started: cx.tick,
                stopped: false,
                control: None,
            };
            self.push(cx, v);
        }
    }

    /// Scene 0x34's traps' sounds while they are on ([`crate::traps::sounding`]), started again
    /// whenever one ends (their records' flag 8), stopped as they go off.
    fn traps(&mut self, cx: &Cx, m: &MatchView) {
        let on: Vec<_> = crate::traps::sounding(&cx.pack.traps, m.traps).collect();
        self.stop(|v| matches!(v.owner, Owner::Trap(k) if on.iter().all(|s| s.trap != k)));
        for s in on {
            let owner = Owner::Trap(s.trap);
            if self.voices.iter().any(|v| !v.stopped && v.owner == owner) {
                continue;
            }
            let cents = (self.rng.next_u32() & s.pitch) as i16;
            let play = Play {
                sounds: vec![s.id],
                volume: s.volume,
                own: None,
                near: s.near,
                far: s.far,
                channel: Channel::Free,
                chance: 255,
                if_idle: false,
                pitch: [cents; 2],
                ticks: None,
            };
            let src = Src {
                owner,
                pos: s.at,
                follows: None,
                tag: None,
                flat: false,
            };
            self.start(cx, &play, &play.sounds, 1.0, src);
        }
    }

    /// A radio line `on` (its character's) at `p` (`func_15103C14`), unless slot `busy` spoke one in
    /// the last [`RADIO_GAP`] ticks: the click on at once on its voice slot, the line its cue's
    /// `delay` later, the click off once that has played (`func_15103910`; a bot's own line and
    /// clicks, `func_15103AA0`: volume −25000 is 25000 with flag 0x100). All of it heard by
    /// everyone, centred ([`Src::radio`]; V `beach-environment.md` §6). War: Beach's slots are its
    /// speakers, Total War's and Colors' the canister's and the flag's teams.
    fn radio(&mut self, cx: &Cx, p: &Seen, on: On, busy: u16) {
        if (self.radio_at.get(&busy)).is_some_and(|&t| cx.tick < t + RADIO_GAP) {
            return;
        }
        let cues = &cx.pack.audio.events.cues;
        let voice = (cues.iter().enumerate()).find(|(_, c)| {
            c.on == on && c.character.as_deref().is_none_or(|c| c == character(cx, p))
        });
        let Some((i, cue)) = voice else {
            return;
        };
        self.radio_at.insert(busy, cx.tick);
        let (tick, src, end) = (cx.tick + u32::from(cue.delay), Src::radio(p), false);
        self.later.push(Later {
            tick,
            cue: i,
            src,
            end,
        });
        self.cues(cx, p, src, |o| *o == On::RadioClick { end: false });
        self.radios.push((p.slot, tick, i));
    }

    /// War: Colors: a team's score rising is a capture. [`CHEER`] ticks after the tick this client
    /// sees it (I: the server's tick plus the latency), the scorers' jingle for everyone and each
    /// loser's voice at it, unless it is voicing (`func_1509F354`; I: one of its radio lines or
    /// loser's voices still playing).
    fn colors(&mut self, cx: &Cx, m: &MatchView, players: &[(Seen, Seen)]) {
        let was = self.scores.replace(m.scores).filter(|_| m.colors);
        for (team, (is, was)) in m.scores.iter().zip(was.unwrap_or(m.scores)).enumerate() {
            if *is > was {
                self.cheers.push((cx.tick + CHEER, team as u8));
            }
        }
        let cheers = std::mem::take(&mut self.cheers);
        let (due, later): (Vec<_>, Vec<_>) = cheers.into_iter().partition(|c| c.0 <= cx.tick);
        self.cheers = later;
        let cues = &cx.pack.audio.events.cues;
        let voice = |on: &On| matches!(on, On::Lose { .. } | On::Flag { .. } | On::Carry { .. });
        let voices: Vec<u16> = (cues.iter().filter(|c| voice(&c.on)))
            .flat_map(|c| c.play.sounds.iter().copied())
            .collect();
        for (_, team) in due {
            self.line(cx, LINES[1 + usize::from(team)]);
            for p in players.iter().map(|p| p.1).filter(|p| p.team != team) {
                let owner = Owner::Player(p.slot);
                let voicing = |v: &Voice| !v.stopped && v.owner == owner && voices.contains(&v.id);
                if !self.voices.iter().any(voicing) {
                    let team = p.team;
                    self.cues(cx, &p, Src::player(&p), |on| *on == On::Lose { team });
                }
            }
        }
    }

    /// A map's line ([`LINES`]), unpositioned, unless a line already plays (one MP3 stream, C
    /// `func_1509E640`).
    fn line(&mut self, cx: &Cx, (id, name): (u16, &str)) {
        let Some(line) = cx.pack.lines.get(name) else {
            return;
        };
        if (self.voices.iter()).any(|v| !v.stopped && LINES.iter().any(|l| l.0 == v.id)) {
            return;
        }
        let v = Voice {
            id,
            owner: Owner::World,
            channel: Channel::Free,
            tag: None,
            pos: cx.ear.pos,
            follows: None,
            flat: true,
            volume: f32::from(line.volume),
            own: None,
            near: 0.0,
            far: 0.0,
            base: MIX,
            speed: 1.0,
            looped: false,
            until: None,
            fade: None,
            started: cx.tick,
            stopped: false,
            control: None,
        };
        self.push(cx, v);
    }

    /// Plays every cue `matches` picks at `pos`, nobody's.
    fn world(&mut self, cx: &Cx, pos: [f32; 3], matches: impl Fn(&On) -> bool) {
        for cue in (cx.pack.audio.events.cues.iter()).filter(|c| matches(&c.on)) {
            self.start(cx, &cue.play, &cue.play.sounds, 1.0, Src::at(pos));
        }
    }

    /// The match's sounds: weapon 0x14's shots (one sound a tick in flight, the wall's when one is
    /// gone without hurting anyone near it), pickups taken (by the nearest player), Total War's gas
    /// (at the listener while it is out), the background (from the first tick of play to the match's
    /// end) and the lamps' buzz (an emitter at each lamp in `flashed`, a flash starting this tick).
    fn match_sounds(
        &mut self,
        cx: &Cx,
        m: &MatchView,
        players: &[(Seen, Seen)],
        flashed: &[[f32; 3]],
    ) {
        let cues = &cx.pack.audio.events.cues;
        for (i, _) in (cues.iter().enumerate()).filter(|c| c.1.on == On::Gas) {
            let tagged = |v: &Voice| v.tag == Some(i);
            match (self.gas, m.gas) {
                (false, true) => {
                    let src = Src {
                        follows: Some(cx.ear.slot),
                        tag: Some(i),
                        ..Src::at(cx.ear.pos)
                    };
                    self.start(cx, &cues[i].play, &cues[i].play.sounds, 1.0, src);
                }
                (true, false) => self.stop(tagged),
                _ => {}
            }
        }
        self.gas = m.gas;
        let old = std::mem::take(&mut self.shots);
        for (k, &pos) in m.shots.iter().enumerate() {
            for cue in cues
                .iter()
                .filter(|c| matches!(c.on, On::Flight { weapon: SHOTS, .. }))
            {
                let sounds = &cue.play.sounds;
                if let Some(s) = sounds.get(cx.tick as usize % sounds.len().max(1)) {
                    let src = Src {
                        owner: Owner::Shot(k),
                        ..Src::at(pos)
                    };
                    self.start(cx, &cue.play, core::slice::from_ref(s), 1.0, src);
                }
            }
        }
        for pos in gone(&old, &m.shots, 2.0 * SHOT_STEP) {
            let hurt =
                |p: &(Seen, Seen)| p.1.health < p.0.health && dist2(p.1.pos, pos) < 150.0 * 150.0;
            if !players.iter().any(hurt) {
                self.world(cx, pos, |on| *on == On::Impact { weapon: SHOTS });
            }
        }
        self.shots = old;
        self.shots.clone_from(&m.shots);
        let taken = m.taken & !self.taken;
        self.taken = m.taken;
        for pickup in
            (cx.pack.pickups.iter()).filter(|p| p.placement < 64 && taken >> p.placement & 1 != 0)
        {
            let d = |p: &Seen| dist2(p.pos, pickup.position);
            let Some(p) = players
                .iter()
                .map(|p| p.1)
                .min_by(|a, b| d(a).total_cmp(&d(b)))
            else {
                continue;
            };
            let gives = pickup.gives;
            self.cues(cx, &p, Src::player(&p), |on| *on == On::Pickup { gives });
        }
        match (m.phase, &cx.pack.background) {
            (0, Some(b)) if !self.background => {
                self.background = true;
                // Its stem with it, at its volume from the next tick on ([`Self::duck_music`]).
                let stem = cx.pack.stem.as_ref().map(|s| (STEM, &s.0));
                for (id, b) in std::iter::once((BACKGROUND, b)).chain(stem) {
                    let v = Voice {
                        id,
                        owner: Owner::World,
                        channel: Channel::Free,
                        tag: None,
                        pos: cx.ear.pos,
                        follows: None,
                        flat: true,
                        volume: f32::from(b.volume) * f32::from(id == BACKGROUND),
                        own: None,
                        near: 0.0,
                        far: 0.0,
                        base: 1.0,
                        speed: 1.0,
                        looped: true,
                        until: None,
                        fade: None,
                        started: cx.tick,
                        stopped: false,
                        control: None,
                    };
                    self.push(cx, v);
                }
            }
            // The music plays on through a War outro and its STATS (V: no stop from the match's
            // start to the bar scene's songs).
            (2 | 3, _) if !crate::outro::team_mode(m) => {
                self.stop(|v| v.id == BACKGROUND || v.id == STEM)
            }
            _ => {}
        }
        for &at in flashed {
            for e in cx
                .pack
                .emitters
                .iter()
                .filter(|e| e.at.map(f32::from) == at)
            {
                self.start(cx, &e.play, &e.play.sounds, 1.0, Src::at(at));
            }
        }
    }

    /// An outro's script sounds at frame `k` (none: they stop): each at its actor, a loop from its
    /// `t` (a late client's from where the outro is) to its `stop` with its linear fades (V cs5b:
    /// 0x4A8's bank loop repeats every 1.40 s from 17.6 s to 27.4 s), a one-shot on its frame, at
    /// `cents`.
    fn outro_sounds(&mut self, cx: &Cx, playing: Option<(&Outro, u32)>) {
        let Some((o, k)) = playing else {
            self.stop(|v| matches!(v.owner, Owner::Outro(_)));
            self.outro.clear();
            return;
        };
        let t = o.t(k);
        for (i, s) in o.sounds.iter().enumerate() {
            let owner = Owner::Outro(i);
            let Some(a) = o.actors().nth(usize::from(s.actor)) else {
                continue;
            };
            let level = match s.stop {
                Some(stop) if t >= f32::from(stop) => None,
                Some(stop) => {
                    let ramp =
                        |from: f32, to: f32| ((t - from) / (to - from).max(1.0)).clamp(0.0, 1.0);
                    let fade_in = s
                        .fade_in
                        .map_or(1.0, |f| ramp(f32::from(s.t), f32::from(f)));
                    let fade_out = s
                        .fade_out
                        .map_or(1.0, |f| 1.0 - ramp(f32::from(f), f32::from(stop)));
                    (t >= f32::from(s.t)).then_some(fade_in * fade_out)
                }
                None => (k == frame(s.t)).then_some(1.0),
            };
            let Some(level) = level else {
                if s.stop.is_some() {
                    self.stop(|v| v.owner == owner);
                }
                continue;
            };
            let pos = a.place(t);
            let live = self
                .voices
                .iter_mut()
                .find(|v| v.owner == owner && !v.stopped);
            match (live, cx.pack.audio.sounds.sounds.get(&s.id)) {
                (Some(v), _) => (v.pos, v.volume) = (pos, SCRIPT[0] * level),
                (None, Some(sound)) if self.outro.insert(i) => {
                    let [volume, near, far] = SCRIPT;
                    let v = Voice {
                        id: s.id,
                        owner,
                        channel: Channel::Free,
                        tag: None,
                        pos,
                        follows: None,
                        flat: false,
                        volume: volume * level,
                        own: None,
                        near,
                        far,
                        base: MIX * f32::from(sound.volume) / 127.0,
                        speed: (f32::from(s.cents) / 1200.0).exp2(),
                        looped: s.stop.is_some() && sound.looped.is_some(),
                        until: None,
                        fade: None,
                        started: cx.tick,
                        stopped: false,
                        control: None,
                    };
                    self.push(cx, v);
                }
                _ => {}
            }
        }
    }

    /// Each voice where its player is, its `ticks` and fade run out, on the mixer once its samples
    /// are in (a loop waits for them; a sound that does not loop plays only from its first tick),
    /// at this tick's ear volumes; finished ones go.
    fn upkeep(&mut self, cx: &Cx, players: &[(Seen, Seen)], out: Option<(&Output, &Assets<Pcm>)>) {
        // The background and its stem go on the mixer together, once both are loaded: their
        // samples stay aligned.
        let loaded = |id| out.is_none_or(|(o, p)| o.sounds.get(&id).is_none_or(|h| p.contains(h)));
        let music = loaded(BACKGROUND) && loaded(STEM);
        for v in &mut self.voices {
            if let Some(p) = (v.follows).and_then(|s| players.iter().find(|p| p.1.slot == s)) {
                v.pos = p.1.pos;
            }
            if v.until.is_some_and(|u| cx.tick >= u) && v.fade.is_none() {
                v.fade = Some(cx.tick);
            }
            if v.fade.is_some_and(|f| cx.tick >= f + FADE) {
                v.stopped = true;
            }
            if v.stopped {
                if let Some(c) = &v.control {
                    c.stop.store(true, Relaxed);
                }
                continue;
            }
            let ears = v.ears(&cx.ear, cx.tick, self.muted);
            if v.control.is_none()
                && (v.looped || v.started == cx.tick)
                && (music || (v.id != BACKGROUND && v.id != STEM))
                && let Some((out, pcms)) = out
                && let Some(pcm) = out.sounds.get(&v.id).and_then(|h| pcms.get(h))
            {
                let sound = cx.pack.audio.sounds.sounds.get(&v.id);
                v.control = Some(out.play(pcm, sound, v, ears));
            }
            if let Some(c) = &v.control {
                for (a, e) in c.ears.iter().zip(ears) {
                    a.store(e.to_bits(), Relaxed);
                }
            }
        }
        self.voices.retain(|v| {
            !v.stopped && (v.control.as_ref()).map_or(v.looped, |c| !c.done.load(Relaxed))
        });
    }
}

/// The tick's sounds (see the module doc): each player's against last tick ([`Heard`]), the
/// grenades', the match's, then every voice's volumes. Nothing plays until the local player (the
/// listener) is in.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn hear(
    timeline: Res<LocalTimeline>,
    pack: Res<Pack>,
    mut sounds: ResMut<Sounds>,
    output: Option<Res<Output>>,
    pcms: Option<Res<Assets<Pcm>>>,
    game: Query<&MatchView>,
    mut players: Query<(
        Entity,
        &Player,
        &Combat,
        &Anim,
        Option<&Fighting>,
        Has<Predicted>,
        Option<&mut Heard>,
        Option<&PlayerId>,
    )>,
    listener: Query<(&Player, &Combat, &FollowCamera)>,
    (playing, outros): (Res<crate::outro::Playing>, Res<crate::outro::Outros>),
    flickers: Option<Res<crate::lights::Flickers>>,
    mut commands: Commands,
) {
    let tick = timeline.tick().0;
    let s = &mut *sounds;
    let mut all = std::mem::take(&mut s.players);
    all.clear();
    let mut grenades: &[(u32, Grenade)] = &[];
    let mut mount = None;
    let guns = game.single().map_or(&[][..], |m| &m.guns[..]);
    for (entity, p, c, anim, fighting, predicted, heard, id) in &mut players {
        // On a mounted gun the weapon is the gun's (its gunner's battle cry, `On::Firing`).
        let seated = match fighting {
            Some(f) if predicted => f.state.seated(),
            _ => (guns.iter()).any(|g| g.gunner == Some(c.slot) && g.state != gun::LAUNCH),
        };
        let (weapon, drawn) = match fighting {
            _ if seated => (conker_camera::GUN, true),
            Some(f) if predicted => (f.state.weapon.unwrap_or(0), f.state.drawn),
            _ => (c.weapon, c.drawn),
        };
        let masked = match fighting {
            Some(f) if predicted => f.state.masked,
            _ => c.masked,
        };
        if let Some(f) = fighting.filter(|_| predicted) {
            grenades = &f.grenades;
            mount = f.state.gun;
        }
        let bot = id.is_some_and(|i| matches!(i.0, PeerId::Local(_)));
        let seen = Seen {
            slot: c.slot,
            body: crate::body(c),
            pos: p.pos,
            clock: anim.0,
            weapon,
            drawn,
            health: c.health,
            reaction: c.anim,
            scope: p.scope_yaw.is_some(),
            local: predicted,
            floor: p.floor.map(|f| (f.surface & 0x1F) as u8),
            speed: p.xz_speed,
            carry: (c.carrying).then_some((c.team, bot)),
            flag: c.flag.map(|f| (f, bot)),
            bot,
            team: c.team,
            freeze: c.freeze,
            deadline: c.deadline,
            masked,
        };
        match heard {
            Some(mut h) => {
                all.push((h.0, seen));
                h.0 = seen;
            }
            None => {
                commands.entity(entity).insert(Heard(seen));
            }
        }
    }
    let (Ok(m), Ok((p, c, camera))) = (game.single(), listener.single()) else {
        s.players = all;
        return;
    };
    let read = m
        .start
        .filter(|&st| st != u32::MAX && tick >= st)
        .map_or(-1, |st| i64::from(m.first_read + (tick - st)));
    // While an outro runs the listener is its camera.
    let running = playing.running(&outros);
    let ear = match running {
        Some((o, k)) => {
            let (eye, at) = o.camera(o.t(k));
            let ahead = Vec2::new(at[0] - eye[0], at[2] - eye[2]);
            let right = Vec2::new(-ahead.y, ahead.x).normalize_or_zero();
            Ear {
                slot: u16::MAX,
                pos: eye,
                right: right.to_array(),
            }
        }
        None => Ear::new(c.slot, p.pos, camera),
    };
    let cx = Cx {
        pack: &pack,
        tick,
        read,
        ear,
    };
    if !s.primed {
        s.primed = true;
        (s.rockets, s.shots, s.taken) = (m.rockets.clone(), m.shots.clone(), m.taken);
        s.grenades = grenades.to_vec();
        s.guns.clone_from(&m.guns);
    }
    let old = std::mem::take(&mut s.rockets);
    for pos in gone(&old, &m.rockets, 2.0 * ROCKET_SPEED) {
        s.rocket_at = Some(tick);
        s.world(&cx, pos, |on| *on == On::Explosion { weapon: ROCKETS });
    }
    s.rockets = old;
    s.rockets.clone_from(&m.rockets);
    s.due(&cx);
    for (was, is) in &all {
        s.player(&cx, was, is, &all);
    }
    s.grenades(&cx, grenades);
    let lamps = flickers.iter().flat_map(|f| &f.lamps);
    let flashed: Vec<[f32; 3]> = lamps.filter(|l| l.flashed).map(|l| l.at).collect();
    s.match_sounds(&cx, m, &all, &flashed);
    s.beach(&cx, m, &all);
    s.zones(&cx, &all);
    s.traps(&cx, m);
    s.colors(&cx, m, &all);
    s.clicks_off(&cx, &all);
    s.outro_sounds(&cx, running);
    s.duck_music(&cx);
    // The muffle ends with the match (I: the original's sound mode with its scene).
    if m.phase >= 2 && s.muffled {
        (s.muffled, s.wearers) = (false, HashSet::new());
        debug!("MUFFLE t={tick} read={read}: off, over");
    }
    MUFFLED.store(s.muffled, Relaxed);
    // The local gunner's gun as predicted (its fire's sounds on the tick it fires).
    let mut guns = m.guns.clone();
    if let Some(g) = mount.and_then(|g| Some((guns.get_mut(usize::from(g.gun))?, g))) {
        (g.0.state, g.0.trigger) = (g.1.state, g.1.trigger);
    }
    s.guns(&cx, &guns, &all);
    let scope = camera.follow.cam.scope;
    let gunner = scope.on && scope.weapon == conker_camera::GUN;
    s.gunner(
        &cx,
        gunner.then_some([scope.zoom[0], scope.yaw_target * scope.pitch_target]),
    );
    let flat = |id, base| Voice {
        id,
        owner: Owner::World,
        channel: Channel::Free,
        tag: None,
        pos: cx.ear.pos,
        follows: None,
        flat: true,
        volume: 32767.0,
        own: None,
        near: 0.0,
        far: 0.0,
        base,
        speed: 1.0,
        looped: false,
        until: None,
        fade: None,
        started: tick,
        stopped: false,
        control: None,
    };
    let outro = (playing.0).and_then(|(i, k)| Some((i, k, outros.0.get(i)?)));
    if let Some((i, ..)) = outro.filter(|&(_, k, o)| Some(k) == o.voice_frame()) {
        s.stop(|v| LINES[1..].iter().any(|l| l.0 == v.id));
        s.push(&cx, flat(VOICE - i as u16, MIX));
    }
    // The song plays as recorded (the slot's 0x4650 is in the recording, as the background's).
    let song = |&(_, k, o): &(usize, u32, &Outro)| o.song.as_ref().is_some_and(|s| k == frame(s.t));
    if let Some((i, ..)) = outro.filter(song) {
        s.push(&cx, flat(SONG - i as u16, 1.0));
    }
    let out = output.as_deref().zip(pcms.as_deref());
    s.upkeep(&cx, &all, out);
    s.players = all;
}

/// The sound device (rodio's mixer) and every sound's samples by id (the background as
/// [`BACKGROUND`]), loading.
#[derive(Resource)]
struct Output {
    device: rodio::MixerDeviceSink,
    sounds: HashMap<u16, Handle<Pcm>>,
}

impl Output {
    /// Puts a sound on the mixer at `speed` × its rate, starting at these ear volumes.
    fn play(&self, pcm: &Pcm, sound: Option<&Sound>, v: &Voice, ears: [f32; 2]) -> Arc<Control> {
        let control = Arc::new(Control::default());
        for (a, e) in control.ears.iter().zip(ears) {
            a.store(e.to_bits(), Relaxed);
        }
        let playing = Playing::new(pcm, sound, v.looped, v.speed, control.clone());
        self.device.mixer().add(playing);
        control
    }
}

/// Opens the sound device and loads every sound: natively at once; in the browser on the first key
/// press or click (Chrome lets an AudioContext made after a user gesture run), clearing the hint.
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pack: Res<Pack>,
    outros: Res<crate::outro::Outros>,
    assets: Res<AssetServer>,
    mut commands: Commands,
    mut tried: Local<bool>,
) {
    let gesture =
        keys.get_just_pressed().next().is_some() || mouse.get_just_pressed().next().is_some();
    if *tried || (cfg!(target_family = "wasm") && !gesture) {
        return;
    }
    *tried = true;
    #[cfg(target_family = "wasm")]
    note("");
    let device = rodio::DeviceSinkBuilder::from_default_device()
        .map(|b| b.with_error_callback(|e| warn!("sound: {e}")))
        .and_then(|b| b.open_stream());
    let mut device = match device {
        Ok(d) => d,
        Err(e) => {
            warn!("sound: no output device ({e}): the match plays silent");
            return;
        }
    };
    device.log_on_drop(false);
    let config = device.config();
    info!(
        "sound: output open, {} channels at {} Hz",
        config.channel_count(),
        config.sample_rate()
    );
    let load = |file: &str| assets.load::<Pcm>(format!("audio/{file}"));
    let mut sounds: HashMap<u16, Handle<Pcm>> = (pack.audio.sounds.sounds.iter())
        .map(|(id, s)| (*id, load(&s.file)))
        .collect();
    if let Some(b) = &pack.background {
        sounds.insert(BACKGROUND, load(&b.file));
    }
    if let Some((b, _)) = &pack.stem {
        sounds.insert(STEM, load(&b.file));
    }
    for (n, o) in outros.0.iter().enumerate() {
        if let Some(v) = &o.voice {
            sounds.insert(VOICE - n as u16, load(&v.file));
        }
        if let Some(s) = &o.song {
            sounds.insert(SONG - n as u16, load(&s.file));
        }
    }
    for (id, name) in LINES {
        if let Some(line) = pack.lines.get(name) {
            sounds.insert(id, load(&line.file));
        }
    }
    commands.insert_resource(Output { device, sounds });
}

/// Set as the mixer first pulls a sound's samples: the output runs.
static PULLED: AtomicBool = AtomicBool::new(false);
/// The mask's muffle ([`Sounds::mask`]), set each tick: the effects play through their [`Biquad`].
static MUFFLED: AtomicBool = AtomicBool::new(false);

/// Logs once that the output runs (in the browser, that the AudioContext started).
fn running(mut said: Local<bool>) {
    if !*said && PULLED.load(Relaxed) {
        *said = true;
        info!("sound: output running");
    }
}

/// M mutes and unmutes (the page's `#sound` line says so in the browser).
fn mute(keys: Res<ButtonInput<KeyCode>>, mut sounds: ResMut<Sounds>) {
    if keys.just_pressed(MUTE_KEY) {
        sounds.muted = !sounds.muted;
        info!("sound {}", if sounds.muted { "muted" } else { "on" });
        #[cfg(target_family = "wasm")]
        note(if sounds.muted { "Sound muted (M)" } else { "" });
    }
}

/// The page's `#sound` line.
#[cfg(target_family = "wasm")]
fn note(text: &str) {
    let line = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("sound"));
    if let Some(line) = line {
        line.set_text_content(Some(text));
    }
}

/// A WAV file's samples: 16-bit PCM, interleaved.
#[derive(Asset, TypePath)]
pub struct Pcm {
    rate: u32,
    channels: u16,
    samples: Arc<[i16]>,
}

/// Reads `.wav` files into [`Pcm`].
#[derive(Default, TypePath)]
pub struct WavLoader;

impl AssetLoader for WavLoader {
    type Asset = Pcm;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<Pcm, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let bad = || std::io::Error::new(std::io::ErrorKind::InvalidData, "not a 16-bit PCM WAV");
        wav(&bytes).ok_or_else(bad)
    }

    fn extensions(&self) -> &[&str] {
        &["wav"]
    }
}

/// A RIFF WAVE file's samples, 16-bit PCM only (the pack's).
fn wav(b: &[u8]) -> Option<Pcm> {
    if b.get(..4)? != b"RIFF" || b.get(8..12)? != b"WAVE" {
        return None;
    }
    let u16_at = |s: &[u8], i: usize| Some(u16::from_le_bytes(s.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |s: &[u8], i: usize| Some(u32::from_le_bytes(s.get(i..i + 4)?.try_into().ok()?));
    let (mut format, mut i) = (None, 12);
    while i + 8 <= b.len() {
        let len = u32_at(b, i + 4)? as usize;
        let body = &b[i + 8..(i + 8).saturating_add(len).min(b.len())];
        match &b[i..i + 4] {
            b"fmt " => {
                format = Some((
                    u16_at(body, 0)?,
                    u16_at(body, 2)?,
                    u32_at(body, 4)?,
                    u16_at(body, 14)?,
                ))
            }
            b"data" => {
                let (pcm, channels, rate, bits) = format?;
                if pcm != 1 || bits != 16 || channels == 0 || rate == 0 {
                    return None;
                }
                let samples = body
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| i16::from_le_bytes(*s));
                return Some(Pcm {
                    rate,
                    channels,
                    samples: samples.collect(),
                });
            }
            _ => {}
        }
        i += 8 + len + len % 2;
    }
    None
}

/// A playing sound's controls, shared with its [`Playing`] on the mixer's thread: each ear's volume
/// (f32 bits), stop (fade out over the release), done (it ended).
#[derive(Default)]
struct Control {
    ears: [AtomicU32; 2],
    stop: AtomicBool,
    done: AtomicBool,
}

/// An ADSR envelope in frames and levels out of 1 ([`conker_assets::audio::Envelope`]): linear ramps
/// (I: the game's envelope mixer's curve is not read).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Env {
    attack: f32,
    attack_level: f32,
    /// None: holds the attack level.
    decay: Option<f32>,
    decay_level: f32,
    release: f32,
}

impl Env {
    const FLAT: Self = Self {
        attack: 0.0,
        attack_level: 1.0,
        decay: None,
        decay_level: 1.0,
        release: 0.0,
    };

    fn new(e: &conker_assets::audio::Envelope, frames_per_us: f32) -> Self {
        let frames = |us: i32| us.max(0) as f32 * frames_per_us;
        Self {
            attack: frames(e.attack_us),
            attack_level: f32::from(e.attack_volume) / 127.0,
            decay: (e.decay_us >= 0).then(|| frames(e.decay_us)),
            decay_level: f32::from(e.decay_volume) / 127.0,
            release: frames(e.release_us),
        }
    }

    fn level(&self, age: f32) -> f32 {
        if age < self.attack {
            return self.attack_level * age / self.attack;
        }
        let t = age - self.attack;
        match self.decay {
            Some(d) if t < d => self.attack_level + (self.decay_level - self.attack_level) * t / d,
            Some(_) => self.decay_level,
            None => self.attack_level,
        }
    }
}

/// A second-order low-pass (the RBJ cookbook's, Q 1/√2), direct form I.
#[derive(Clone, Copy, Debug)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    /// Cutting from `cutoff` Hz at `rate` samples a second (below its Nyquist).
    fn low_pass(cutoff: f32, rate: f32) -> Self {
        let (sin, cos) = (TAU * cutoff.min(0.45 * rate) / rate).sin_cos();
        let alpha = sin * FRAC_1_SQRT_2; // sin / (2Q)
        let a0 = 1.0 + alpha;
        let b1 = (1.0 - cos) / a0;
        Self {
            b: [b1 / 2.0, b1, b1 / 2.0],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let Self { b, a, x: xs, y: ys } = self;
        let y = b[0] * x + b[1] * xs[0] + b[2] * xs[1] - a[0] * ys[0] - a[1] * ys[1];
        (*xs, *ys) = ([x, xs[0]], [y, ys[0]]);
        y
    }
}

/// A sound on the mixer: its samples (a loop's stretch repeating until it is stopped), through its
/// envelope, as two channels at its [`Control`]'s ear volumes (smoothed over about 20 ms), at its
/// rate × the pitch's speed; an effect (a bank sound) through the muffle's low-pass while
/// [`MUFFLED`] (the filter runs all along, so a switch finds it settled). ponytail: a loop's
/// `count` is not kept (no sound of the pack has one).
struct Playing {
    samples: Arc<[i16]>,
    channels: usize,
    rate: SampleRate,
    frame: usize,
    end: usize,
    loop_start: Option<usize>,
    env: Env,
    age: u32,
    /// The age and level the stop began its release at.
    release: Option<(u32, f32)>,
    ears: [f32; 2],
    smooth: f32,
    right: Option<f32>,
    control: Arc<Control>,
    muffle: Option<[Biquad; 2]>,
}

impl Playing {
    /// `sound` None: the background or a line (no envelope; `looped`: whole, the background).
    fn new(
        pcm: &Pcm,
        sound: Option<&Sound>,
        looped: bool,
        speed: f32,
        control: Arc<Control>,
    ) -> Self {
        let channels = usize::from(pcm.channels);
        let frames = pcm.samples.len() / channels;
        let rate = (pcm.rate as f32 * speed).max(1.0);
        let (looped, env) = match sound {
            Some(s) => (
                (s.looped.as_ref()).map(|l| (l.start as usize, (l.end as usize).min(frames))),
                Env::new(&s.envelope, rate / 1e6),
            ),
            None => (looped.then_some((0, frames)), Env::FLAT),
        };
        let looped = looped.filter(|l| l.0 < l.1);
        Self {
            samples: pcm.samples.clone(),
            channels,
            rate: SampleRate::new(rate.round() as u32).unwrap_or(SampleRate::MIN),
            frame: 0,
            end: looped.map_or(frames, |l| l.1),
            loop_start: looped.map(|l| l.0),
            env,
            age: 0,
            release: None,
            ears: control
                .ears
                .each_ref()
                .map(|e| f32::from_bits(e.load(Relaxed))),
            smooth: 1.0 - (-1.0 / (rate * 0.02)).exp(),
            right: None,
            control,
            muffle: sound.map(|_| [Biquad::low_pass(MUFFLE_HZ, rate); 2]),
        }
    }

    fn done(&mut self) -> Option<f32> {
        self.control.done.store(true, Relaxed);
        None
    }
}

impl Drop for Playing {
    fn drop(&mut self) {
        self.control.done.store(true, Relaxed);
    }
}

impl Iterator for Playing {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        if self.age == 0 {
            PULLED.store(true, Relaxed);
        }
        let mut level = self.env.level(self.age as f32);
        if self.release.is_none() && self.control.stop.load(Relaxed) {
            self.release = Some((self.age, level));
        }
        if let Some((at, from)) = self.release {
            let t = (self.age - at) as f32;
            if t >= self.env.release {
                return self.done();
            }
            level = from * (1.0 - t / self.env.release);
        }
        if self.frame >= self.end {
            match self.loop_start {
                Some(start) => self.frame = start,
                None => return self.done(),
            }
        }
        let i = self.frame * self.channels;
        let mut left = f32::from(self.samples[i]) / 32768.0;
        let mut right = match self.channels {
            1 => left,
            _ => f32::from(self.samples[i + 1]) / 32768.0,
        };
        if let Some([l, r]) = &mut self.muffle {
            let muffled = (l.run(left), r.run(right));
            if MUFFLED.load(Relaxed) {
                (left, right) = muffled;
            }
        }
        for (ear, target) in self.ears.iter_mut().zip(&self.control.ears) {
            *ear += (f32::from_bits(target.load(Relaxed)) - *ear) * self.smooth;
        }
        self.frame += 1;
        self.age += 1;
        self.right = Some(right * level * self.ears[1]);
        Some(left * level * self.ears[0])
    }
}

impl Source for Playing {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        STEREO
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(id: u16, time: f32, rate: f32) -> AnimPlayer {
        AnimPlayer {
            id,
            time,
            rate,
            ..default()
        }
    }

    /// The oracle's footsteps (e_w09: clip 2's event at 1.0 fired from 0.426 and from 0.0087, the
    /// one at 15.0 from 14.33, rate 1.73) and a clip's first tick.
    #[test]
    fn clip_events_fire_as_the_clock_passes_them() {
        let pass = |a: f32, b: f32, t: f32| passes(&clock(2, a, 1.73), &clock(2, b, 1.73), t);
        assert!(pass(0.426, 2.156, 1.0) && pass(0.0087, 1.7387, 1.0) && pass(14.33, 16.06, 15.0));
        assert!(!pass(1.0, 2.73, 1.0), "fired last tick");
        assert!(
            pass(27.5, 1.2, 1.0) && pass(27.5, 1.2, 28.0),
            "round the loop"
        );
        let new = |t: f32| passes(&clock(0x7F, 40.0, 1.0), &clock(0x58, 1.0, 1.0), t);
        assert!(new(0.0) && new(1.0) && !new(2.0));
    }

    #[test]
    fn a_wav_reads_back() {
        let samples: [i16; 4] = [0, -1, 32767, -32768];
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut b = b"RIFF\0\0\0\0WAVE".to_vec();
        b.extend(b"LIST\x03\0\0\0abc\0"); // odd length: padded
        b.extend(b"fmt \x10\0\0\0\x01\0\x02\0\x44\xAC\0\0\x10\xB1\x02\0\x04\0\x10\0");
        b.extend(b"data\x08\0\0\0");
        b.extend(&data);
        let pcm = wav(&b).unwrap();
        assert_eq!(
            (pcm.rate, pcm.channels, &pcm.samples[..]),
            (44100, 2, &samples[..])
        );
        b[46] = 8; // 8-bit
        assert!(wav(&b).is_none());
        assert!(wav(b"RIFF").is_none());
    }

    /// A loop after an intro (sound 719: 1569..14371), a stop's release, and the envelope.
    #[test]
    fn loops_keep_their_intro_and_stop_after_the_release() {
        let pcm = Pcm {
            rate: 1_000_000,
            channels: 1,
            samples: (0..6).map(|i| i * 1000).collect(),
        };
        let mut sound = Sound {
            file: String::new(),
            rate: 0,
            volume: 127,
            pan: 64,
            envelope: conker_assets::audio::Envelope {
                attack_us: 0,
                attack_volume: 127,
                decay_us: -1,
                decay_volume: 127,
                release_us: 2,
            },
            looped: Some(conker_assets::audio::Loop {
                start: 2,
                end: 5,
                count: None,
            }),
            detune_after_attack: None,
        };
        let control = Arc::new(Control::default());
        control
            .ears
            .iter()
            .for_each(|e| e.store(1f32.to_bits(), Relaxed));
        let mut p = Playing::new(&pcm, Some(&sound), false, 1.0, control.clone());
        let left: Vec<f32> = (0..9)
            .map(|_| (p.next().unwrap() * 32768.0).round())
            .collect();
        // Frames 0 1 2 3 4 2 3 4 2 …, each as left then right.
        assert_eq!(
            left.iter().step_by(2).copied().collect::<Vec<_>>(),
            [0.0, 1000.0, 2000.0, 3000.0, 4000.0]
        );
        control.stop.store(true, Relaxed);
        let rest: Vec<f32> = p.by_ref().collect();
        assert_eq!(
            rest.len(),
            1 + 2 * 2,
            "the pending right sample, then 2 frames of release"
        );
        assert!(control.done.load(Relaxed));
        // Sound 176: decays to 0 over 287.5 ms.
        sound.envelope.decay_volume = 0;
        sound.envelope.decay_us = 100;
        let env = Env::new(&sound.envelope, 1.0);
        assert_eq!(
            [0.0, 50.0, 100.0, 500.0].map(|a| env.level(a)),
            [1.0, 0.5, 0.0, 0.0]
        );
    }

    fn at(slot: u16, x: f32, weapon: u8) -> Seen {
        Seen {
            slot,
            body: conker_sim::movement::Side::Tediz as usize,
            pos: [x, 0.0, 0.0],
            clock: AnimPlayer::default(),
            weapon,
            drawn: true,
            health: 6,
            reaction: 0,
            scope: false,
            local: false,
            floor: None,
            speed: 0.0,
            carry: None,
            flag: None,
            bot: false,
            team: 0,
            freeze: 0,
            deadline: 0,
            masked: false,
        }
    }

    #[test]
    fn hits_name_their_weapon() {
        let victim = Seen {
            reaction: conker_anim::combat::HIT,
            ..at(0, 0.0, 0)
        };
        let all = [
            (at(1, 500.0, 0x39), at(1, 500.0, 0x39)),
            (at(2, 900.0, 0x09), at(2, 900.0, 0x09)),
        ];
        assert_eq!(
            hit_weapon(&victim, false, &all, false),
            0x39,
            "the nearest gun"
        );
        assert_eq!(hit_weapon(&victim, false, &all[1..], false), 0x09);
        assert_eq!(
            hit_weapon(&victim, true, &[], false),
            0x15,
            "a burning victim"
        );
        let blast = Seen {
            reaction: BLOWN_UP,
            ..victim
        };
        assert_eq!(
            (
                hit_weapon(&blast, false, &all, true),
                hit_weapon(&blast, false, &all, false)
            ),
            (ROCKETS, GRENADE)
        );
        let kill = Seen {
            reaction: DEATH_SHOT,
            ..victim
        };
        assert_eq!(hit_weapon(&kill, false, &all, false), 0x24);
        let flung = Seen {
            reaction: DEATH_BLAST,
            ..victim
        };
        let gunner = (
            at(3, 700.0, conker_camera::GUN),
            at(3, 700.0, conker_camera::GUN),
        );
        assert_eq!(hit_weapon(&flung, false, &all, false), 0x23);
        assert_eq!(
            hit_weapon(&flung, false, &[all[0], all[1], gunner], false),
            conker_camera::GUN,
            "the mounted gun's bullet"
        );
    }

    #[test]
    fn projectiles_gone_are_the_ones_not_carried_on() {
        let old = [[0.0, 0.0, 0.0], [1000.0, 0.0, 0.0], [5000.0, 0.0, 0.0]];
        let new = [[150.0, 0.0, 0.0], [5100.0, 0.0, 0.0], [0.0, 9.0, 0.0]];
        assert_eq!(
            gone(&old, &new, 400.0).collect::<Vec<_>>(),
            [[1000.0, 0.0, 0.0]]
        );
        assert_eq!(gone(&old, &[], 400.0).count(), 3);
    }
    /// The duck factor (`func_1000D758`): Beach's 18000 falls to 7690 in 19 frames of a line (V b3:
    /// 0x464F → 0x1E09) and comes back 2250 a frame (+0x8CA), all of it 5 frames after.
    #[test]
    fn the_music_ducks_by_the_factor_law() {
        let music = |f: f32| 18000.0 * f / DUCK[0];
        let mut f = DUCK[0];
        let down: Vec<f32> = (0..19)
            .map(|_| {
                music({
                    f = duck(f, true);
                    f
                })
            })
            .collect();
        assert!((down[0] - (18000.0 - 562.5)).abs() < 0.1 && down[17] > 7700.0);
        assert!((down[18] - 7690.0).abs() < 1.0);
        let up: Vec<f32> = (0..5)
            .map(|_| {
                music({
                    f = duck(f, false);
                    f
                })
            })
            .collect();
        assert!((up[0] - 7690.0 - 2250.0).abs() < 1.0 && up[3] < 18000.0 && up[4] == 18000.0);
    }

    /// The end phase of a War match leaves the music playing (through its outro and STATS), a
    /// Deathmatch's stops it. An outro's loop is silent at its start, full at its fade-in's end,
    /// half way down half way through its fade-out, and over at its stop; a one-shot plays on its
    /// frame.
    #[test]
    fn the_music_plays_through_the_outro_and_script_sounds_fade() {
        use conker_assets::outro::ScriptSound;
        let Some(pack) = colors_pack() else { return };
        let mut s = Sounds::default();
        let at = |phase, colors| MatchView {
            phase,
            colors,
            ..MatchView::default()
        };
        let music = |s: &Sounds| s.voices.iter().any(|v| v.id == BACKGROUND && !v.stopped);
        s.match_sounds(&cx(&pack, 1), &at(0, true), &[], &[]);
        s.match_sounds(&cx(&pack, 2), &at(2, true), &[], &[]);
        assert!(music(&s));
        s.match_sounds(&cx(&pack, 3), &at(2, false), &[], &[]);
        assert!(!music(&s));

        // The tanks' 0x4A8 (looped in the bank).
        let id = 0x4A8;
        let sound = |t, stop, fade_in, fade_out| ScriptSound {
            id,
            actor: 0,
            t,
            stop,
            fade_in,
            fade_out,
            cents: 0,
        };
        let o = Outro {
            length: 300,
            sounds: vec![
                sound(0, Some(100), Some(20), Some(60)),
                sound(50, None, None, None),
            ],
            ..Outro::default()
        };
        let mut s = Sounds::default();
        let mut heard = Vec::new();
        for k in 0..60 {
            let cx = cx(&pack, k);
            s.outro_sounds(&cx, Some((&o, k)));
            let volume = |i| {
                (s.voices.iter())
                    .find(|v| v.owner == Owner::Outro(i) && !v.stopped)
                    .map(|v| v.volume)
            };
            heard.push((volume(0), volume(1)));
            s.upkeep(&cx, &[], None);
        }
        let full = SCRIPT[0];
        assert!(
            s.voices
                .iter()
                .all(|v| v.owner != Owner::Outro(0) || v.looped)
        );
        assert_eq!(
            (heard[0].0, heard[10].0, heard[40].0),
            (Some(0.0), Some(full), Some(full / 2.0))
        );
        assert_eq!((heard[49].0.is_some(), heard[50].0), (true, None));
        let once: Vec<u32> = (0..60).filter(|&k| heard[k as usize].1.is_some()).collect();
        assert_eq!(once, [25]);
    }

    /// A line (no bank sound) plays once unless it is the background; an effect's muffle is a
    /// 520 Hz low-pass (V c1b: `func_10012020` mode 5 sets param 9 to 520).
    #[test]
    fn lines_end_and_the_muffle_cuts_at_520_hz() {
        let pcm = Pcm {
            samples: vec![1000; 4].into(),
            channels: 1,
            rate: 22050,
        };
        let play = |looped| Playing::new(&pcm, None, looped, 1.0, Arc::new(Control::default()));
        assert_eq!(
            play(false).count(),
            8,
            "4 frames, left and right, then done"
        );
        assert_eq!(play(true).take(100).count(), 100, "the background loops");
        let gain = |hz: f32| {
            let mut f = Biquad::low_pass(MUFFLE_HZ, 22050.0);
            let out: Vec<f32> = (0..22050)
                .map(|i| f.run((TAU * hz * i as f32 / 22050.0).sin()))
                .collect();
            out[11025..].iter().fold(0f32, |m, y| m.max(y.abs()))
        };
        assert!(gain(100.0) > 0.95, "{}", gain(100.0));
        assert!(
            (gain(MUFFLE_HZ) - FRAC_1_SQRT_2).abs() < 0.02,
            "{}",
            gain(MUFFLE_HZ)
        );
        assert!(gain(5000.0) < 0.02, "{}", gain(5000.0));
    }

    /// `func_15074664` (C): any player's put-on muffles; a take-off unmuffles only after that
    /// player's own put-on; a death (the mask dropped at 0 health) leaves it.
    #[test]
    fn the_mask_muffles_from_any_put_on() {
        let mut s = Sounds::default();
        let (bare, masked) = (
            at(1, 0.0, 0),
            Seen {
                masked: true,
                ..at(1, 0.0, 0)
            },
        );
        let other = |p: Seen| Seen { slot: 2, ..p };
        assert!(s.mask(&bare, &masked) && s.muffled, "a put-on (anyone's)");
        assert!(!s.mask(&other(masked), &other(bare)), "2 never put one on");
        assert!(s.mask(&masked, &bare) && !s.muffled, "1's take-off");
        assert!(s.mask(&other(bare), &other(masked)));
        let dead = Seen {
            health: 0,
            ..other(bare)
        };
        assert!(
            !s.mask(&other(masked), &dead) && s.muffled,
            "a death keeps it"
        );
    }

    /// The pack's audio with map 30 (War: Colors), or None without a pack.
    fn colors_pack() -> Option<Pack> {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let audio = Audio::load(|f| std::fs::read(format!("{dir}/{f}"))).ok();
        let Some(audio) = audio.filter(|a| a.events.maps.contains_key("30")) else {
            eprintln!("no War: Colors audio in the pack at {dir}: skipping");
            return None;
        };
        Some(Pack::new(audio, Vec::new(), Vec::new(), "30", &[]))
    }

    fn cx(pack: &Pack, tick: u32) -> Cx<'_> {
        let ear = Ear {
            slot: 0,
            pos: [0.0; 3],
            right: [1.0, 0.0],
        };
        Cx {
            pack,
            tick,
            read: -1,
            ear,
        }
    }

    /// The ids `run` started over ticks `ticks` (each tick: the sounds due, `run`, then as headless).
    fn heard(
        s: &mut Sounds,
        pack: &Pack,
        ticks: std::ops::Range<u32>,
        mut run: impl FnMut(&mut Sounds, &Cx),
    ) -> Vec<(u32, u16)> {
        let mut ids = Vec::new();
        for t in ticks {
            let cx = cx(pack, t);
            let before = s.voices.len();
            s.due(&cx);
            run(s, &cx);
            ids.extend(s.voices[before..].iter().map(|v| (t, v.id)));
            s.upkeep(&cx, &[], None);
        }
        ids
    }

    /// A flag taken (V c1a read 328: 0x5B0, the line 0x5A3 or 0x5A5 ten frames on, 0x5B1 after it);
    /// none again from that flag's slot within 300 frames; a bot's line is the odd id, clicked too,
    /// and heard centred at its volume however far (V `beach-environment.md` §6).
    #[test]
    fn a_flag_taken_calls_on_the_radio() {
        let Some(pack) = colors_pack() else { return };
        let mut s = Sounds::default();
        let squirrel = |bot| Seen {
            flag: Some((0, bot)),
            bot,
            ..at(1, 0.0, 0)
        };
        let (was, is) = (at(1, 0.0, 0), squirrel(false));
        let calls = heard(&mut s, &pack, 100..120, |s, cx| {
            if cx.tick == 100 {
                s.player(cx, &was, &is, &[]);
            }
            s.clicks_off(cx, &[(is, is)]);
        });
        let line = calls.iter().find(|c| c.1 != 0x5B0 && c.1 != 0x5B1);
        assert_eq!(calls.first(), Some(&(100, 0x5B0)));
        assert!(matches!(line, Some(&(110, 0x5A3 | 0x5A5))), "{calls:x?}");
        assert!(calls.contains(&(111, 0x5B1)), "{calls:x?}");
        let again = heard(&mut s, &pack, 399..400, |s, cx| {
            s.player(cx, &was, &is, &[])
        });
        assert_eq!(again, [], "busy till 300 frames on");
        let bot = Seen {
            pos: [20_000.0, 0.0, 0.0],
            ..squirrel(true)
        };
        let mut levels = Vec::new();
        let calls = heard(&mut s, &pack, 400..420, |s, cx| {
            if cx.tick == 400 {
                s.player(cx, &was, &bot, &[]);
            }
            s.clicks_off(cx, &[(bot, bot)]);
            let new = s.voices.iter().filter(|v| v.started == cx.tick);
            levels.extend(new.map(|v| v.level(&cx.ear)));
        });
        assert!(
            matches!(
                calls[..],
                [(400, 0x5B0), (410, 0x5A4 | 0x5A6), (411, 0x5B1)]
            ),
            "{calls:x?}"
        );
        assert_eq!(
            levels,
            [25000.0, 32700.0, 30000.0].map(|v| (v / 32767.0, 0.5))
        );
    }

    /// A gun's release (the trigger falling, V gcQ): one of 0x618-0x61A at the gunner, then the same
    /// one again 15 ticks on where the gunner was, not following it.
    #[test]
    fn a_gun_release_plays_twice() {
        let Some(pack) = colors_pack() else { return };
        let mut s = Sounds::default();
        let gunner = at(1, 100.0, 0);
        let gun = |trigger| GunView {
            state: gun::IDLE,
            gunner: Some(1),
            trigger,
            ..GunView::default()
        };
        let mut released = Vec::new();
        let calls = heard(&mut s, &pack, 100..130, |s, cx| {
            let moved = Seen {
                pos: [500.0, 0.0, 0.0],
                ..gunner
            };
            let p = if cx.tick < 102 { gunner } else { moved };
            s.guns(cx, &[gun(cx.tick < 101)], &[(p, p)]);
            let new = s.voices.iter().filter(|v| v.started == cx.tick);
            released.extend(new.map(|v| (v.pos, v.follows)));
        });
        assert!(
            matches!(calls[..], [(101, a), (116, b)] if a == b && (0x618..0x61B).contains(&a)),
            "{calls:x?}"
        );
        let here = gunner.pos;
        assert_eq!(released, [(here, Some(1)), (here, None)]);
    }

    /// The stem's gain (`beach-environment.md` §4: CC 0xFC = v·255/100): full within 3213 of its
    /// point, 0.99 at 3219, none from 4000. A zone's volume: full within near, half way between,
    /// silent below 401 (§5: trigger 0x23's 17331, 451, 1385).
    #[test]
    fn the_stem_follows_and_zones_fade_by_distance() {
        let f = Follow {
            at: [0, -377, 8227],
            near: 3000,
            far: 4000,
        };
        let gains =
            [0.0, 3000.0, 3213.0, 3219.0, 3500.0, 3999.0, 4000.0, 9000.0].map(|d| follow(&f, d));
        assert_eq!(gains, [1.0, 1.0, 1.0, 0.99, 0.63, 0.0, 0.0, 0.0]);
        let play = Play {
            sounds: vec![0x4A7],
            volume: 17331,
            own: None,
            near: 451,
            far: 1385,
            channel: Channel::Free,
            chance: 255,
            if_idle: false,
            pitch: [0, 0],
            ticks: None,
        };
        let v = [0.0, 451.0, 918.0, 1363.0, 1364.0, 1385.0].map(|d| zone_volume(&play, d).round());
        assert_eq!(v, [17331.0, 17331.0, 8666.0, 408.0, 0.0, 0.0]);
    }

    /// A capture: 30 frames after the score rises, the scorers' jingle (0x1A4 squirrels, 0x1A7
    /// Tediz) and a voice at each loser (Tediz 0x662-0x664; C script 0xBB, V c1a read 412).
    #[test]
    fn a_capture_cheers_30_frames_on() {
        let Some(pack) = colors_pack() else { return };
        let mut s = Sounds::default();
        let tedi = |slot| Seen {
            team: 1,
            ..at(slot, 0.0, 0)
        };
        let all = [
            (at(1, 0.0, 0), at(1, 0.0, 0)),
            (tedi(2), tedi(2)),
            (tedi(3), tedi(3)),
        ];
        let mut m = MatchView {
            colors: true,
            ..Default::default()
        };
        let calls = heard(&mut s, &pack, 10..50, |s, cx| {
            m.scores[0] = u16::from(cx.tick >= 12);
            s.colors(cx, &m, &all);
        });
        let (jingle, voices) = calls.split_first().expect("a jingle");
        assert_eq!(*jingle, (42, LINES[1].0));
        assert_eq!(voices.len(), 2, "{calls:x?}");
        assert!(
            voices
                .iter()
                .all(|v| v.0 == 42 && (0x662..=0x664).contains(&v.1))
        );
    }
}
