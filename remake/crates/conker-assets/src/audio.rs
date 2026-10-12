//! The pack's sound: `audio/sounds.json` (every sound the match plays: its WAV file and how the
//! game's player plays it) and `audio/events.json` (which game event plays which sound, and how),
//! with the WAV files in `audio/common/`, `audio/characters/<id>/` and `audio/maps/<map>/`.
//!
//! Volumes are the game's, out of 32767; distances are game units; times are 30 Hz ticks. A sound
//! is *positional* when [`Play::own`] is `None` or its source is not the listener's own player: full
//! volume within `near` of the listener, silent from `far`, linear in between (`func_1000A420`).
//! The listener's own player's sounds with `own: Some(v)` play unpositioned (centred) at `v`.

use std::collections::BTreeMap;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, invalid, parse_json, to_json};

pub const SOUNDS: &str = "audio/sounds.json";
pub const EVENTS: &str = "audio/events.json";

/// `audio/sounds.json`: every sound the events play, by the game's sound id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sounds {
    /// The rate the game's synthesizer mixes at (22050 Hz): a sound plays at its WAV's own rate.
    pub mix_rate: u32,
    pub sounds: BTreeMap<u16, Sound>,
}

/// One sound: its file and the game's playback data for it (the `B1` bank's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    /// The WAV file, relative to `audio/`.
    pub file: String,
    /// The rate it was decoded at (for reference: the WAV header's rate is the one played).
    pub rate: u32,
    /// The sample's own volume, 0-127 (multiplies the event's), and pan, 0-127 (64 centre).
    pub volume: u8,
    pub pan: u8,
    pub envelope: Envelope,
    /// Loop points in samples: the stretch `start..end` repeats, `count` times (None: until the
    /// sound is stopped). None: plays once.
    #[serde(rename = "loop")]
    pub looped: Option<Loop>,
    /// Cents the pitch moves by once the attack is over (a few weapon sounds), else None.
    pub detune_after_attack: Option<i8>,
}

/// ADSR in microseconds and 0-127 levels: from 0 up to `attack_volume` over `attack_us`, then to
/// `decay_volume` over `decay_us` (-1: holds), fading out over `release_us` once stopped.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub attack_us: i32,
    pub attack_volume: u8,
    pub decay_us: i32,
    pub decay_volume: u8,
    pub release_us: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Loop {
    pub start: u32,
    pub end: u32,
    pub count: Option<u32>,
}

/// A player's sound slots: a sound on `a` or `b` stops that player's previous sound on the same
/// slot (`b` is the voice slot); `free` sounds overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    A,
    B,
    Free,
}

/// How a sound plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Play {
    /// Candidates, one picked at random (the game avoids repeats until each has played).
    pub sounds: Vec<u16>,
    /// Volume, out of 32767, before distance.
    pub volume: u16,
    /// The listener's own player: Some(volume) plays it unpositioned at that volume, None
    /// positional like everyone else's.
    pub own: Option<u16>,
    pub near: u16,
    pub far: u16,
    pub channel: Channel,
    /// Plays when `chance` >= a random byte: 255 always, 64 one time in four.
    pub chance: u8,
    /// Skipped while the channel already plays a sound.
    pub if_idle: bool,
    /// Pitch: a random offset in cents between the two (`[0, 0]`: as stored).
    pub pitch: [i16; 2],
    /// Stops after this many ticks (a looping sound), else None.
    pub ticks: Option<u16>,
}

/// What a clip's event does at `time` (ticks into the clip, as the clip clock passes it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipAction {
    Play(Play),
    /// A footstep of this kind (0-4) on the floor's surface ([`Footsteps`]).
    Footstep(u8),
    /// Stops the player's sound on the channel.
    Stop(Channel),
}

/// A sound event of a character's clip (the clip's logical animation id, as `character.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipSound {
    pub clip: u16,
    pub time: f32,
    #[serde(flatten)]
    pub action: ClipAction,
}

/// Footsteps (`func_10011EB8`): the floor triangle's surface kind (bits 0-4 of its word) and the
/// event's step kind pick the candidates; volume `volume` × (0.5 + 0.5 × min(speed, 30) / 30).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footsteps {
    pub play: Play,
    /// The speed (horizontal, units per tick at 2 VIs) at which a step is loudest.
    pub full_speed: f32,
    /// Surface kind → candidates for step kinds 0-4 (empty: silent).
    pub surfaces: BTreeMap<u8, [Vec<u16>; 5]>,
}

/// A map's background: a recording of the game's own output, looped whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Background {
    /// Relative to `audio/`.
    pub file: String,
    /// Out of 32767 (the recording is at the game's level: play at 1.0 for the original's mix).
    pub volume: u16,
    pub note: String,
}

/// A spot on the map that plays sounds by itself, in bursts (`func_151607A4`): each tick while
/// idle, a burst starts with probability `start_chance`; a burst is 1-5 sounds, each followed by
/// a wait of 1-29 ticks and a 8-20 tick rise before the next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Emitter {
    pub at: [i16; 3],
    pub play: Play,
    pub start_chance: f32,
}

/// What a match sound is played on, besides the clips' events (game code; README.md says how each
/// maps to the remake's events).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum On {
    /// A shot of the weapon (conker-rules `Event::Shot`), at the shooter.
    Shot { weapon: u8 },
    /// Starts when the shooter enters one of `clips`, at the shooter; a looping sound plays while
    /// it stays in them, and [`Cue::then`] plays when it leaves.
    Firing { weapon: u8, clips: Vec<u16> },
    /// A spent case, `delay` ticks after each shot, at the shooter.
    Casing { weapon: u8, delay: u16 },
    /// While the weapon's model is in the hand (from its create event to its remove event,
    /// `held.json`): a loop at the holder.
    Holding { weapon: u8 },
    /// The thrown weapon's fuse, its last 60 ticks, in the hand or not: a loop until the blast.
    Fuse { weapon: u8 },
    /// A hit by the weapon (conker-rules `Event::Hit`), at the victim; `lethal` None = any hit.
    Hit { weapon: u8, lethal: Option<bool> },
    /// An explosion of the weapon's projectile (`Event::Explosion`), at the blast.
    Explosion { weapon: u8 },
    /// Body parts landing after a gory death by the weapon: several, over about two seconds,
    /// near the victim.
    Gibs { weapon: u8 },
    /// The weapon's projectile in flight: [`Play::sounds`] in turn, the i-th `ticks[i]` ticks
    /// after the launch, or (`ticks` empty) one every tick, alternating.
    Flight { weapon: u8, ticks: Vec<u16> },
    /// The projectile hitting a wall, at the impact.
    Impact { weapon: u8 },
    /// The weapon's projectile bouncing on the floor (the grenade).
    Bounce { weapon: u8 },
    /// A burning player (the flamethrower's burn), looping at it while it burns.
    Burning,
    /// A pickup taken (`Event::Pickup`), at the player: `gives` as `pickups.json`.
    Pickup { gives: crate::Gives },
    /// The weapon's scope going on (R held), heard by its holder only.
    Scope { weapon: u8 },
    /// War: Total War's gas: a loop while it is out (`MatchView::gas`), at the listener.
    Gas,
    /// War: Total War: a player taking gas canister `canister` (its team's), at it; `bot`: a player
    /// without a camera.
    Carry { canister: u8, bot: bool },
    /// War: Colors: a player taking flag `flag` (0 the red, 1 the green), at it; `bot`: a player
    /// without a camera.
    Flag { flag: u8, bot: bool },
    /// A mounted gun (maps 0x30 and 0x34), at the gun: its state (gun+0x232: 1 free, 2 a player
    /// jumping on, 3 manned, 4 firing, 0xB stopped by B or by overheating, 5 left) going from
    /// `from` to `to`; a looping sound plays until the state changes again.
    Gun { from: u8, to: u8 },
    /// A mounted gun's firing stopped by its gunner (Z let go while it fires, B, or overheating),
    /// at the gunner.
    GunStop,
    /// A gunner's zoom (C-up, C-down), heard by the gunner: `step` 0 while the zoom moves (a
    /// loop), 1 when it stops, 2 when step 1's sound has ended.
    GunZoom { step: u8 },
    /// A gunner's aim (the yaw target times the pitch target), heard by the gunner: `step` 0
    /// while it keeps changing (from its 10th VI of change, a loop), 1 when it stops.
    GunAim { step: u8 },
    /// War: Beach: a villager walking into the plunger's voice trigger (0x25) while the plunger is
    /// ready: its radio voice, [`Cue::delay`] after [`On::RadioClick`]'s click on.
    PlungerReady,
    /// War: Beach: a villager setting the plunger off: its radio voice, as [`On::PlungerReady`].
    Plunger,
    /// War: Beach: a radio voice's click on its speaker's voice slot: when the voice is called
    /// (`end` false), and when it has played (`end` true).
    RadioClick { end: bool },
    /// War: Beach: the plunger's detonation, three blasts, each at its [`Cue::at`] after its
    /// [`Cue::delay`].
    Detonation,
    /// A piece of debris landing (the detonation's and the strike's), at it.
    Debris,
    /// War: Beach: the artillery strike called on a villager whose escape countdown reached 0, at
    /// the villager.
    StrikeCall,
    /// War: Beach: the strike's shell landing, 40 frames after the call, at the same spot.
    StrikeBlast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub on: On,
    /// Only for a player of this character (its folder); None: anyone.
    pub character: Option<String>,
    pub play: Play,
    /// Played when a [`On::Firing`] loop ends, from where it was, at its last volume.
    pub then: Vec<Play>,
    /// Plays at this world point instead of at the event's (absent: the event's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[i16; 3]>,
    /// Ticks after the event (absent: 0).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub delay: u16,
    /// Where it comes from in the game and how sure it is (V seen on the oracle, C read in the
    /// code, I inferred).
    pub note: String,
}

/// `audio/events.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Events {
    /// Map folder → its background and emitters.
    pub maps: BTreeMap<String, MapSounds>,
    /// Character folder → its clips' sound events, in clip then time order.
    pub characters: BTreeMap<String, Vec<ClipSound>>,
    pub footsteps: Footsteps,
    pub cues: Vec<Cue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapSounds {
    pub background: Vec<Background>,
    pub emitters: Vec<Emitter>,
    /// The voice lines the map's mode plays (the game's MP3s), by what plays them (absent: none).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lines: BTreeMap<String, Line>,
}

/// A voice line: `file` (relative to `audio/`) played unpositioned at `volume` (out of 32767).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub file: String,
    pub volume: u16,
    pub note: String,
}

fn is_zero(v: &u16) -> bool {
    *v == 0
}

/// `audio/sounds.json` and `audio/events.json`; the WAV files are loaded by path (Bevy's asset
/// loader), relative to `audio/`.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub sounds: Sounds,
    pub events: Events,
}

impl Events {
    /// Every [`Play`], for checks.
    fn plays(&self) -> impl Iterator<Item = &Play> {
        let maps = (self.maps.values()).flat_map(|m| m.emitters.iter().map(|e| &e.play));
        let clips = (self.characters.values().flatten()).filter_map(|c| match &c.action {
            ClipAction::Play(p) => Some(p),
            _ => None,
        });
        let cues = (self.cues.iter()).flat_map(|c| std::iter::once(&c.play).chain(c.then.iter()));
        maps.chain(clips)
            .chain(std::iter::once(&self.footsteps.play))
            .chain(cues)
    }

    /// Every sound id the events can play.
    pub fn sound_ids(&self) -> std::collections::BTreeSet<u16> {
        let steps = (self.footsteps.surfaces.values()).flat_map(|s| s.iter().flatten());
        (self.plays().flat_map(|p| p.sounds.iter()).chain(steps))
            .copied()
            .collect()
    }
}

/// A file path relative to its folder (`audio/`, `hud/`) that stays inside it.
pub(crate) fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|p| !p.is_empty() && p != "..")
}

impl Audio {
    /// The two data files (the WAVs are listed in [`Sounds::sounds`] and [`Background::file`]).
    pub fn files() -> [&'static str; 2] {
        [SOUNDS, EVENTS]
    }

    /// Reads both files through `read` (as [`crate::Slice::load`]) and checks that every sound an
    /// event plays is in `sounds.json` and every file stays inside `audio/`.
    pub fn load<E: Display>(mut read: impl FnMut(&str) -> Result<Vec<u8>, E>) -> Result<Self> {
        let mut bytes = |file: &str| {
            read(file).map_err(|e| Error::Read {
                file: file.into(),
                message: e.to_string(),
            })
        };
        let sounds: Sounds = parse_json(SOUNDS, &bytes(SOUNDS)?)?;
        let events: Events = parse_json(EVENTS, &bytes(EVENTS)?)?;
        for (id, s) in &sounds.sounds {
            if !safe(&s.file) {
                return Err(invalid(
                    SOUNDS,
                    format!("sounds.{id}.file"),
                    "a path inside audio/",
                ));
            }
        }
        for (map, m) in &events.maps {
            let files =
                (m.background.iter().map(|b| &b.file)).chain(m.lines.values().map(|l| &l.file));
            if let Some(file) = files.into_iter().find(|f| !safe(f)) {
                return Err(invalid(
                    EVENTS,
                    format!("maps.{map}"),
                    format!("{file:?}: a path inside audio/"),
                ));
            }
        }
        if let Some(id) =
            (events.sound_ids().into_iter()).find(|id| !sounds.sounds.contains_key(id))
        {
            return Err(invalid(
                EVENTS,
                "(sounds)",
                format!("sound {id} is not in {SOUNDS}"),
            ));
        }
        Ok(Self { sounds, events })
    }

    /// The two data files, as [`Audio::load`] reads them.
    pub fn write(&self) -> Vec<(String, Vec<u8>)> {
        vec![
            (SOUNDS.into(), to_json(&self.sounds, 2)),
            (EVENTS.into(), to_json(&self.events, 3)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Audio {
        let play = |sounds: Vec<u16>| Play {
            sounds,
            volume: 28000,
            own: Some(24000),
            near: 300,
            far: 1800,
            channel: Channel::B,
            chance: 64,
            if_idle: true,
            pitch: [-200, 200],
            ticks: Some(60),
        };
        let sound = |file: &str| Sound {
            file: file.into(),
            rate: 15592,
            volume: 127,
            pan: 64,
            envelope: Envelope {
                attack_us: 0,
                attack_volume: 127,
                decay_us: -1,
                decay_volume: 127,
                release_us: 5750,
            },
            looped: Some(Loop {
                start: 0,
                end: 10,
                count: None,
            }),
            detune_after_attack: Some(-5),
        };
        Audio {
            sounds: Sounds {
                mix_rate: 22050,
                sounds: BTreeMap::from([
                    (577, sound("common/0577.wav")),
                    (1254, sound("characters/117/1254.wav")),
                ]),
            },
            events: Events {
                maps: BTreeMap::from([(
                    "2d".into(),
                    MapSounds {
                        background: vec![Background {
                            file: "maps/2d/background.wav".into(),
                            volume: 32767,
                            note: "recorded".into(),
                        }],
                        emitters: vec![Emitter {
                            at: [-1101, -1201, -229],
                            play: play(vec![577]),
                            start_chance: 0.005,
                        }],
                        lines: BTreeMap::from([(
                            "escape".into(),
                            Line {
                                file: "maps/2b/voice-1a8.wav".into(),
                                volume: 32767,
                                note: "V".into(),
                            },
                        )]),
                    },
                )]),
                characters: BTreeMap::from([(
                    "117".into(),
                    vec![
                        ClipSound {
                            clip: 0x58,
                            time: 21.0,
                            action: ClipAction::Play(play(vec![1254])),
                        },
                        ClipSound {
                            clip: 2,
                            time: 1.5,
                            action: ClipAction::Footstep(1),
                        },
                        ClipSound {
                            clip: 0x38,
                            time: 0.0,
                            action: ClipAction::Stop(Channel::A),
                        },
                    ],
                )]),
                footsteps: Footsteps {
                    play: play(vec![]),
                    full_speed: 30.0,
                    surfaces: BTreeMap::from([(5, [vec![577], vec![], vec![], vec![], vec![]])]),
                },
                cues: vec![Cue {
                    on: On::Pickup {
                        gives: crate::Gives::Weapon(9),
                    },
                    character: Some("128".into()),
                    play: play(vec![577]),
                    then: vec![play(vec![1254])],
                    at: Some([-319, -273, -3419]),
                    delay: 23,
                    note: "V".into(),
                }],
            },
        }
    }

    fn files(audio: &Audio) -> BTreeMap<String, Vec<u8>> {
        audio.write().into_iter().collect()
    }

    fn load(files: &BTreeMap<String, Vec<u8>>) -> Result<Audio> {
        Audio::load(|p| files.get(p).cloned().ok_or("missing"))
    }

    #[test]
    fn write_then_load_gives_the_same_audio() {
        let audio = sample();
        let f = files(&audio);
        assert_eq!(f.len(), Audio::files().len());
        assert_eq!(load(&f).unwrap(), audio);
        let events = String::from_utf8(f[EVENTS].clone()).unwrap();
        assert!(
            events.contains(r#"{"clip":2,"time":1.5,"footstep":1}"#),
            "{events}"
        );
        assert!(
            events.contains(r#""on": {"pickup":{"gives":{"weapon":9}}}"#),
            "{events}"
        );
        assert_eq!(
            audio.events.sound_ids().into_iter().collect::<Vec<_>>(),
            [577, 1254]
        );
    }

    #[test]
    fn missing_sounds_and_outside_paths_are_refused() {
        let mut audio = sample();
        audio.sounds.sounds.remove(&1254);
        let e = load(&files(&audio)).unwrap_err().to_string();
        assert_eq!(
            e,
            "audio/events.json: (sounds): sound 1254 is not in audio/sounds.json"
        );
        let mut audio = sample();
        audio.sounds.sounds.get_mut(&577).unwrap().file = "../pack.json".into();
        let e = load(&files(&audio)).unwrap_err().to_string();
        assert_eq!(
            e,
            "audio/sounds.json: sounds.577.file: a path inside audio/"
        );
        let mut audio = sample();
        let lines = &mut audio.events.maps.get_mut("2d").unwrap().lines;
        lines.get_mut("escape").unwrap().file = "/x.wav".into();
        assert_eq!(
            load(&files(&audio)).unwrap_err().to_string(),
            r#"audio/events.json: maps.2d: "/x.wav": a path inside audio/"#
        );
        let mut f = files(&sample());
        f.remove(SOUNDS);
        assert_eq!(
            load(&f).unwrap_err().to_string(),
            "audio/sounds.json: missing"
        );
    }
}
