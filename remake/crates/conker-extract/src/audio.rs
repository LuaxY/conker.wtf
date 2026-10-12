//! The game's sound effects: the `B1` sound bank (bank 0x17: entry 0 the control graph, 1 the
//! external sound data, 2 the wavetable), its instrument 0's sounds (the game's sound ids 1..=1762:
//! sound `id` is the instrument's sound `id - 1`) and their VADPCM samples decoded to PCM.
//!
//! Formats from DevOldSchool/conkers-bfd-decomp (MIT; `scripts/audio_assets.py`'s graph parser and
//! VADPCM decoder, `lib/libultrare/.../n_sndplayer.c` for how the player uses each field), ported:
//! - control header: `u16` revision `B1`, `u16` bank count, `u32` bank offsets; a bank: `u16`
//!   instrument count, flags, pad, `u32` rate, `u32` percussion, `u32` instruments. Instrument 0 is
//!   in the control file: 12 bytes, `s16` bend range, `s16` sound count, `u32` sound pointers, each an
//!   external offset encoded as `(pointer >> 5) & !7`;
//! - a sound (external data): `u32` envelope, key map and wavetable pointers relative to the sound,
//!   `u8` pan, volume, flags, pad; an envelope: `s32` attack, decay, release (µs), `u8` attack and
//!   decay volumes; a key map: velocity min/max, key min/max, key base, `s8` detune; a wavetable:
//!   `u32` base in entry 2, `s32` length, `u8` type (0 VADPCM), flags, pad, `u32` loop and book
//!   pointers; a book: `s32` order and predictor count, `s16` coefficients; a loop: `u32` start,
//!   end, count, then 16 `s16` of decoder state at the start.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail, ensure};
use conker_assets::Gives;
use conker_assets::audio::{
    Background, Channel, ClipAction, ClipSound, Cue, Emitter, Footsteps, On, Play,
};
use extract_character::{ROUTE_BANK, animation_entry, due, parse_routes};
use extract_core::Rom;

/// The audio bank and its entries.
const BANK: usize = 0x17;
const CONTROL: usize = 0;
const EXTERNAL: usize = 1;
const WAVETABLE: usize = 2;
/// `B1`.
const REVISION: u16 = 0x4231;
/// The rate the synthesizer mixes at (the bank's): a sample whose key map asks for no pitch change
/// plays at this rate.
pub const OUTPUT_RATE: u32 = 22050;
/// Key-map flag (`keyMax & 0x20`): the detune applies once the attack is over, not from the start.
const DETUNE_AFTER_ATTACK: u8 = 0x20;

/// One of the game's sounds (instrument 0's sound `id - 1`).
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub id: u16,
    /// 0-127 (`sampleVolume`), and pan 0-127 (64 centre).
    pub volume: u8,
    pub pan: u8,
    pub envelope: Envelope,
    pub key_min: u8,
    pub key_max: u8,
    pub key_base: u8,
    pub detune: i8,
    pub wave: Wave,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    pub attack_us: i32,
    pub decay_us: i32,
    pub release_us: i32,
    pub attack_volume: u8,
    pub decay_volume: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Wave {
    /// Offset and stored length in the wavetable.
    pub base: usize,
    pub len: usize,
    pub book: Book,
    pub looped: Option<Loop>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    pub order: usize,
    pub predictors: usize,
    pub coefficients: Vec<i16>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loop {
    /// Sample indices; `count` 0xFFFF_FFFF loops forever.
    pub start: u32,
    pub end: u32,
    pub count: u32,
    /// The decoder's output for the 16-sample frame holding `start`, as stored.
    pub state: [i16; 16],
}

impl Sound {
    /// The pitch ratio its key map gives (`KEYMAP_PITCHCENTS_*`: key base 60 plays as stored).
    fn cents(&self) -> i32 {
        let detune = if self.key_max & DETUNE_AFTER_ATTACK != 0 {
            0
        } else {
            i32::from(self.detune)
        };
        i32::from(self.key_base) * 100 + detune - 6000
    }

    /// The rate the samples play at with no pitch change from the game.
    pub fn rate(&self) -> u32 {
        (f64::from(OUTPUT_RATE) * 2f64.powf(f64::from(self.cents()) / 1200.0)).round() as u32
    }

    /// Cents the pitch moves by once the attack is over (key-map flag 0x20), else None.
    pub fn detune_after_attack(&self) -> Option<i8> {
        (self.key_max & DETUNE_AFTER_ATTACK != 0).then_some(self.detune)
    }

    /// `keyMin & 0x1F`: the volume group (the game can scale each group, `sndp_set_volume_entry`).
    pub fn volume_group(&self) -> u8 {
        self.key_min & 0x1F
    }

    /// `(keyMax & 0xF) * 8`: added to the effect (reverb) mix.
    pub fn fx_mix(&self) -> u8 {
        (self.key_max & 0xF) * 8
    }
}

fn be16(d: &[u8], at: usize) -> Result<u16> {
    let b = d.get(at..at + 2).context("sound bank: short read")?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

fn be32(d: &[u8], at: usize) -> Result<u32> {
    let b = d.get(at..at + 4).context("sound bank: short read")?;
    Ok(u32::from_be_bytes(b.try_into().unwrap()))
}

/// Instrument 0's sounds, in id order (index `id - 1`), from the control graph and external data.
pub fn parse_sounds(control: &[u8], external: &[u8]) -> Result<Vec<Sound>> {
    ensure!(be16(control, 0)? == REVISION, "sound bank: not B1");
    let bank = be32(control, 4)? as usize;
    let instrument = be32(control, bank + 12)? as usize;
    let count = be16(control, instrument + 14)? as usize;
    (0..count)
        .map(|k| {
            let at = ((be32(control, instrument + 16 + 4 * k)? >> 5) & !7) as usize;
            let rel =
                |field: usize| -> Result<usize> { Ok(at + be32(external, at + field)? as usize) };
            let (env, key, wave) = (rel(0)?, rel(4)?, rel(8)?);
            let k8 = |o: usize| {
                external
                    .get(key + o)
                    .copied()
                    .context("sound bank: key map")
            };
            let wave_type = *external.get(wave + 8).context("sound bank: wavetable")?;
            if wave_type != 0 {
                bail!("sound {}: wavetable type {wave_type}, not VADPCM", k + 1);
            }
            let book_at = at + be32(external, wave + 16)? as usize;
            let order = be32(external, book_at)? as usize;
            let predictors = be32(external, book_at + 4)? as usize;
            let coefficients = (0..8 * order * predictors)
                .map(|i| Ok(be16(external, book_at + 8 + 2 * i)? as i16))
                .collect::<Result<_>>()?;
            let loop_ptr = be32(external, wave + 12)? as usize;
            let looped = if loop_ptr == 0 {
                None
            } else {
                let l = at + loop_ptr;
                let mut state = [0; 16];
                for (i, s) in state.iter_mut().enumerate() {
                    *s = be16(external, l + 12 + 2 * i)? as i16;
                }
                Some(Loop {
                    start: be32(external, l)?,
                    end: be32(external, l + 4)?,
                    count: be32(external, l + 8)?,
                    state,
                })
            };
            Ok(Sound {
                id: k as u16 + 1,
                pan: *external.get(at + 12).context("sound bank: sound")?,
                volume: *external.get(at + 13).context("sound bank: sound")?,
                envelope: Envelope {
                    attack_us: be32(external, env)? as i32,
                    decay_us: be32(external, env + 4)? as i32,
                    release_us: be32(external, env + 8)? as i32,
                    attack_volume: *external.get(env + 12).context("sound bank: envelope")?,
                    decay_volume: *external.get(env + 13).context("sound bank: envelope")?,
                },
                key_min: k8(2)?,
                key_max: k8(3)?,
                key_base: k8(4)?,
                detune: k8(5)? as i8,
                wave: Wave {
                    base: be32(external, wave)? as usize,
                    len: be32(external, wave + 4)? as usize,
                    book: Book {
                        order,
                        predictors,
                        coefficients,
                    },
                    looped,
                },
            })
        })
        .collect()
}

/// The ROM's sound bank and its wavetable (bank 0x17 entries 0-2).
pub fn sound_bank(rom: &Rom) -> Result<(Vec<Sound>, Vec<u8>)> {
    let sounds = parse_sounds(
        &rom.asset(&[BANK, CONTROL])?,
        &rom.asset(&[BANK, EXTERNAL])?,
    )?;
    Ok((sounds, rom.asset(&[BANK, WAVETABLE])?.into_owned()))
}

/// Decodes whole 9-byte VADPCM frames (a trailing partial frame is ignored, as the game does) to
/// 16-bit PCM, 16 samples a frame.
pub fn decode(data: &[u8], book: &Book) -> Result<Vec<i16>> {
    let (order, c) = (book.order, &book.coefficients);
    let mut history = vec![0i32; order];
    let mut out = Vec::with_capacity(data.len() / 9 * 16);
    for frame in data.as_chunks::<9>().0 {
        let scale = 1i32 << (frame[0] >> 4);
        let predictor = usize::from(frame[0] & 0xF);
        ensure!(
            predictor < book.predictors,
            "VADPCM frame selects predictor {predictor}"
        );
        let p = &c[predictor * order * 8..][..order * 8];
        let residuals: Vec<i32> = frame[1..]
            .iter()
            .flat_map(|b| [b >> 4, b & 0xF])
            .map(|n| (i32::from(n) ^ 8) - 8)
            .map(|n| n * scale)
            .collect();
        for vector in residuals.chunks(8) {
            let mut samples = [0i32; 8];
            for i in 0..8 {
                let mut acc: i32 = (0..order)
                    .map(|o| i32::from(p[o * 8 + i]) * history[o])
                    .sum();
                for k in 0..i {
                    for o in 1..order {
                        acc += vector[i - 1 - k] * i32::from(p[o * 8 + k]);
                    }
                }
                samples[i] = ((vector[i] * 2048 + acc) >> 11).clamp(-32768, 32767);
            }
            history.copy_from_slice(&samples[8 - order..]);
            out.extend(samples.iter().map(|&s| s as i16));
        }
    }
    Ok(out)
}

/// A mono 16-bit PCM WAV file.
pub fn wav(samples: &[i16], rate: u32) -> Vec<u8> {
    let data = 2 * samples.len() as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    for w in [16u32, 1 | 1 << 16, rate, 2 * rate, 2 | 16 << 16] {
        out.extend(w.to_le_bytes());
    }
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    out.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    out
}

/// An MP3 stream (bank 0x16's voice lines: MPEG-2 layer III, 22050 Hz mono) as a mono WAV
/// ([`wav`]), decoded with symphonia.
pub fn mp3_wav(mp3: &[u8]) -> Result<Vec<u8>> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::errors::Error as Decode;
    use symphonia::core::io::MediaSourceStream;
    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(mp3.to_vec())), default());
    let mut hint = symphonia::core::probe::Hint::new();
    hint.with_extension("mp3");
    let probed = symphonia::default::get_probe().format(&hint, source, &default(), &default())?;
    let mut format = probed.format;
    let track = format.default_track().context("an MP3 without a track")?;
    let (id, rate) = (track.id, track.codec_params.sample_rate.context("no rate")?);
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &default())?;
    let mut samples = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(Decode::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        ensure!(decoded.spec().channels.count() == 1, "a mono voice");
        let mut buf = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buf.samples());
    }
    Ok(wav(&samples, rate))
}

fn default<T: Default>() -> T {
    T::default()
}

/// Reads a mono 16-bit PCM WAV file back: its rate and samples (the pack test's round trip).
pub fn read_wav(bytes: &[u8]) -> Result<(u32, Vec<i16>)> {
    ensure!(
        bytes.len() >= 44 && &bytes[..4] == b"RIFF" && &bytes[8..16] == b"WAVEfmt ",
        "not a WAV file"
    );
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    ensure!(
        word(20) == 1 | 1 << 16 && word(32) == 2 | 16 << 16,
        "not mono 16-bit PCM"
    );
    ensure!(
        &bytes[36..40] == b"data",
        "WAV: data chunk expected after fmt"
    );
    let data = bytes
        .get(44..44 + word(40) as usize)
        .context("WAV: short data")?;
    Ok((
        word(24),
        data.as_chunks::<2>()
            .0
            .iter()
            .map(|&b| i16::from_le_bytes(b))
            .collect(),
    ))
}

// ---------------------------------------------------------------------------------------------
// The event map: which game event plays which sound (`audio/events.json`). Tags as in
// `../lab/audio/README.md`: C read in the recompiled code, V seen on the oracle's sound calls.

/// The clips a Deathmatch plays: every animation id (obj+0x84) of the oracle's Deathmatch traces
/// (`../lab/audio/tools/mp_anims.py` over `../lab/oracle/trajectories/*.raw`, 105 ids), and the
/// crossbow's and the shotgun's walks (0xEC, 0x1F5: conker-anim's, untraced), and item 0x40's
/// (the club's: `combat6_*` traces, `../lab/oracle/caveman_README.md`), and War: Total War's gas
/// mask put on and taken off (0x14E, 0x151: `../lab/research/mp-original/total-war-effects.md` §4).
pub const MATCH_CLIPS: [u16; 117] = [
    0x0, 0x1, 0x2, 0x9, 0xF, 0x13, 0x16, 0x17, 0x18, 0x1B, 0x21, 0x2B, 0x32, 0x36, 0x37, 0x38,
    0x4C, 0x58, 0x59, 0x69, 0x76, 0x77, 0x78, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F, 0xBD, 0xC5, 0xC6,
    0xCF, 0xD3, 0xE2, 0xE4, 0xE7, 0xEB, 0xEC, 0xF0, 0xF3, 0xF4, 0xF7, 0xF8, 0xF9, 0xFB, 0xFC,
    0x102, 0x103, 0x10A, 0x10C, 0x10F, 0x111, 0x112, 0x113, 0x119, 0x11A, 0x11B, 0x11C, 0x11D,
    0x120, 0x121, 0x122, 0x124, 0x129, 0x12A, 0x12B, 0x144, 0x14E, 0x151, 0x152, 0x153, 0x155,
    0x156, 0x157, 0x158, 0x159, 0x15A, 0x15B, 0x15C, 0x15D, 0x15E, 0x15F, 0x162, 0x163, 0x164,
    0x165, 0x166, 0x168, 0x16B, 0x16F, 0x176, 0x177, 0x17E, 0x18A, 0x19C, 0x19D, 0x1EA, 0x1EB,
    0x1ED, 0x1EE, 0x1EF, 0x1F3, 0x1F4, 0x1F5, 0x1F6, 0x1F7, 0x1F8, 0x20D, 0x21E, 0x22C, 0x235,
    0x238, 0x26E, 0x28E, 0x2C2, 0x2C3,
];
/// Bank 0x12 entry `class`: the character's voice table, sound words for event ids 2000.. (C
/// `func_15016850` loads it, `func_1506E46C` reads it; V: the RAM copy for class 117).
const VOICE_BANK: usize = 0x12;
const VOICE_IDS: u32 = 2000;
/// The footstep table (`func_10011EB8`): per surface kind, five (first id, count) pairs, one per
/// step kind, in the main code's data (ROM offset = address − 0x8000_0000; V: equal to the RAM).
const FOOTSTEPS_AT: usize = 0x2C240;
const SURFACE_KINDS: usize = 20;
/// Event ops (`func_1506AD30` dispatches through the table at 0x800863FC).
const OP_SOUND: u32 = 9;
const OP_PICK: u32 = 10;
const OP_STOP_B: u32 = 107;
const OP_STOP_A: u32 = 108;
/// Ops whose argument goes through `func_1506E46C` (voice table, random count, chance, busy flag):
/// (op, channel, near, far, volume for others, own volume). C: their handlers.
const VOICED: [(u32, Channel, u16, u16, u16, u16); 13] = [
    (53, Channel::A, 300, 1800, 28000, 24000),
    (55, Channel::B, 3000, 8000, 28000, 24000),
    (56, Channel::A, 3000, 8000, 28000, 24000),
    (76, Channel::A, 300, 1800, 28000, 24000),
    (77, Channel::A, 300, 1800, 14000, 12000),
    (80, Channel::A, 100, 800, 28000, 24000),
    (82, Channel::B, 300, 1800, 28000, 24000),
    (83, Channel::B, 100, 800, 28000, 24000),
    (84, Channel::B, 300, 3000, 14000, 12000),
    (109, Channel::A, 80, 800, 28000, 24000),
    (110, Channel::A, 300, 3000, 28000, 24000),
    (111, Channel::A, 300, 3000, 14000, 12000),
    // The shotgun's blast (`func_1506E6FC`: op 53's call, farther; V sounds 755/756 as e_w24's).
    (144, Channel::A, 700, 4000, 28000, 24000),
];

fn play(
    sounds: impl IntoIterator<Item = u16>,
    volume: u16,
    own: Option<u16>,
    near: u16,
    far: u16,
    channel: Channel,
) -> Play {
    Play {
        sounds: sounds.into_iter().collect(),
        volume,
        own,
        near,
        far,
        channel,
        chance: 255,
        if_idle: false,
        pitch: [0, 0],
        ticks: None,
    }
}

/// `func_1506E46C`: an event word to its candidates, chance and busy flag; ids from 2000 name the
/// character's voice table (a zero entry: nothing).
fn resolve(word: u32, voices: &[u8]) -> Option<(Vec<u16>, u8, bool)> {
    let mut w = word;
    let id = w & 0x7FFF;
    if id >= VOICE_IDS {
        let at = 4 * (id - VOICE_IDS) as usize;
        let entry = voices
            .get(at..at + 4)
            .map_or(0, |b| u32::from_be_bytes(b.try_into().unwrap()));
        if entry == 0 {
            return None;
        }
        let high = w & 0xFF20_0000;
        w = if high != 0 {
            high | (entry & 0xFF_FFFF)
        } else {
            entry
        };
    }
    let busy = word & 0x20_0000 != 0;
    if w >> 16 == 0 {
        return Some((vec![(w & 0x7FFF) as u16], 255, busy));
    }
    let first = (w & 0x7FFF) as u16;
    let count = ((w >> 16) & 0x1F).max(1) as u16;
    Some(((first..first + count).collect(), (w >> 24) as u8, busy))
}

/// An op-9 event (`func_1506BF5C`): the sound id in bits 0-10, the slot in bits 11-13 (0x2000 a
/// footstep of that kind), a character filter in bits 14-15 against the top byte.
fn sound_op(arg: u32, class: u8) -> Option<ClipAction> {
    let only = (arg >> 24) as u8;
    match arg & 0xC000 {
        0x4000 if class != only => return None,
        0x8000 if class == only => return None,
        _ => {}
    }
    let id = (arg & 0x7FF) as u16;
    let channel = match arg & 0x3800 {
        0x2000 => return Some(ClipAction::Footstep(id as u8)),
        0x1000 => Channel::B,
        0x3000 => Channel::Free,
        _ => Channel::A,
    };
    let mut p = play([id], 32000, Some(32000), 500, 2500, channel);
    p.if_idle = arg & 0x3800 == 0x3800;
    Some(ClipAction::Play(p))
}

/// The sound events of character `class`'s Deathmatch clips: its route table's events
/// (`[0x0F, route entry]`, as held.rs reads them) decoded as their op handlers do.
fn clip_sounds(rom: &Rom, game_data: &[u8], class: usize) -> Result<Vec<ClipSound>> {
    let entry = animation_entry(game_data, animation_entry(game_data, class)?)?;
    let table = rom.asset(&[ROUTE_BANK, entry])?;
    let voices = rom.asset(&[VOICE_BANK, class])?;
    let mut out = Vec::new();
    for (clip, route) in parse_routes(&table)?.iter().enumerate() {
        let clip = clip as u16;
        if route.aux_offset == 0 || !MATCH_CLIPS.contains(&clip) {
            continue;
        }
        let (mut at, mut deferred, mut last) = (route.aux_offset as usize + 0x10, None, 0.0);
        loop {
            let time = f32::from_bits(be32(&table, at)?);
            if time >= 900.0 {
                break;
            }
            let time = due(&mut last, time);
            let (op, arg) = (be32(&table, at + 4)? & 0xFF, be32(&table, at + 8)?);
            at += 12;
            let action = match op {
                OP_SOUND if arg & 0x1_0000 != 0 => {
                    deferred = Some(arg);
                    None
                }
                OP_SOUND => sound_op(arg, class as u8),
                // `func_1506C32C`: one of the deferred id and up to three more, packed 11 bits each.
                OP_PICK => deferred.take().and_then(|d| {
                    let ids = [d, arg, arg >> 11, arg >> 22].map(|w| (w & 0x7FF) as u16);
                    match sound_op(d & !0x1_07FF | u32::from(ids[0]), class as u8)? {
                        ClipAction::Play(mut p) => {
                            p.sounds = ids.into_iter().filter(|&i| i != 0).collect();
                            Some(ClipAction::Play(p))
                        }
                        other => Some(other),
                    }
                }),
                OP_STOP_A => Some(ClipAction::Stop(Channel::A)),
                OP_STOP_B => Some(ClipAction::Stop(Channel::B)),
                // The sniper rifle's shot (clip 0x156) is its `On::Shot` cue.
                144 if clip == 0x156 => None,
                _ => VOICED.iter().find(|v| v.0 == op).and_then(
                    |&(_, channel, near, far, volume, own)| {
                        let (sounds, chance, if_idle) = resolve(arg, &voices)?;
                        let mut p = play(sounds, volume, Some(own), near, far, channel);
                        (p.chance, p.if_idle) = (chance, if_idle);
                        Some(ClipAction::Play(p))
                    },
                ),
            };
            if let Some(action) = action {
                out.push(ClipSound { clip, time, action });
            }
        }
    }
    Ok(out)
}

/// The footstep table for the surface kinds `kinds`.
fn footsteps(rom: &Rom, kinds: &BTreeSet<u8>) -> Result<Footsteps> {
    let table = rom
        .bytes()
        .get(FOOTSTEPS_AT..FOOTSTEPS_AT + 0x14 * SURFACE_KINDS)
        .context("ROM: footstep table")?;
    let surfaces = (kinds.iter().filter(|&&k| usize::from(k) < SURFACE_KINDS))
        .map(|&k| {
            let row = &table[0x14 * usize::from(k)..];
            let steps = std::array::from_fn(|s| {
                let first = u16::from_be_bytes([row[4 * s], row[4 * s + 1]]);
                let count = u16::from_be_bytes([row[4 * s + 2], row[4 * s + 3]]);
                if first == 0 {
                    vec![]
                } else {
                    (first..first + count.max(1)).collect()
                }
            });
            (k, steps)
        })
        .collect();
    // `func_1506BF5C` 0x2000: channel A, 0x7FFF for one listener, 500/2500.
    Ok(Footsteps {
        play: play([], 32767, Some(32767), 500, 2500, Channel::A),
        full_speed: 30.0,
        surfaces,
    })
}

/// The hit reactions (`func_1505E874`): the reaction each weapon's hit plays (the damage types'
/// byte 0, `../lab/oracle/combat_README.md` §4), whose entry in the shared reaction list
/// (`[0x0F, entry]`, header word 2; 0x18-byte entries, byte 0 the reaction, word 1 its sound word)
/// names the voice; `func_15060A30` plays it on slot B.
const REACTIONS: [(u8, u8); 18] = [
    (0x09, 0x21),
    (0x39, 0x21),
    (0x23, 0x31),
    (0x24, 0x21),
    (0x15, 0x10),
    (0x21, 0x33),
    (0x22, 0x2F),
    (0x14, 0x20),
    (0x26, 0x2B),
    (0x16, 0x2B),
    (0x14, 0x2E),
    (0x24, 0x2E),
    (0x18, 0x20),
    (0x18, 0x2E),
    (0x37, 0x21),
    (0x37, 0x2B),
    (0x40, 0x25),
    (0x40, 0x26),
];

fn hit_voices(rom: &Rom, game_data: &[u8], class: usize, character: &str) -> Result<Vec<Cue>> {
    let entry = animation_entry(game_data, animation_entry(game_data, class)?)?;
    let table = rom.asset(&[ROUTE_BANK, entry])?;
    let voices = rom.asset(&[VOICE_BANK, class])?;
    let list = be32(&table, 8)? as usize;
    let mut cues: Vec<Cue> = Vec::new();
    for (weapon, reaction) in REACTIONS {
        let word = (0..64)
            .map(|i| list + 0x18 * i)
            .find(|&at| table.get(at) == Some(&reaction))
            .map(|at| be32(&table, at + 4))
            .transpose()?
            .unwrap_or(0);
        let Some((sounds, chance, if_idle)) = (word != 0).then(|| resolve(word, &voices)).flatten()
        else {
            continue;
        };
        if cues.iter().any(|c| {
            c.on == (On::Hit {
                weapon,
                lethal: None,
            })
        }) {
            continue;
        }
        let mut p = play(sounds, 28000, Some(24000), 500, 2500, Channel::B);
        (p.chance, p.if_idle) = (chance, if_idle);
        cues.push(Cue {
            on: On::Hit { weapon, lethal: None },
            character: Some(character.into()),
            play: p,
            then: vec![],
            note: format!("the victim's voice: reaction {reaction:#x}'s word {word:#x} (C func_1505E874, ROM tables)"),
        });
    }
    Ok(cues)
}

/// (path, contents) of files to write.
pub type Files = Vec<(String, Vec<u8>)>;

/// A record (`func_1000FA64`): positional for every listener, the shooter's own included.
fn record(sounds: impl IntoIterator<Item = u16>, volume: u16, near: u16, far: u16) -> Play {
    play(sounds, volume, None, near, far, Channel::Free)
}

fn cue(on: On, play: Play, note: &str) -> Cue {
    Cue {
        on,
        character: None,
        play,
        then: vec![],
        note: note.into(),
    }
}

fn range(first: u16, count: u16) -> std::ops::Range<u16> {
    first..first + count
}

/// What the game's code plays besides the clips' events (constants of the functions named; tags:
/// C read, V on the oracle's sound calls, `../lab/audio/README.md`).
fn code_cues() -> Vec<Cue> {
    let casing = || play(range(747, 3), 32000, Some(32000), 100, 120, Channel::B);
    let thud = |id| play([id], 28000, Some(28000), 300, 1800, Channel::Free);
    // A loop for 120 VIs, then faded out (C func_1000EBC4, the record's timer).
    let splat = || Play {
        ticks: Some(60),
        ..record([1278], 24000, 300, 600)
    };
    let mut squirrel_fire = record(range(1404, 6), 32767, 500, 1000);
    squirrel_fire.chance = 63;
    let mut cues = vec![
        Cue {
            then: vec![
                record([727], 32767, 500, 1000),
                record(range(747, 3), 16000, 500, 1000),
            ],
            ..cue(
                On::Firing {
                    weapon: 0x09,
                    clips: vec![0x7C, 0x20D],
                },
                record([719], 32767, 500, 1000),
                "twin guns: a loop while firing, then the tail and a case (C func_15065A5C case 9, func_1000EFB4; V e_w09)",
            )
        },
        Cue {
            character: Some("128".into()),
            ..cue(
                On::Firing {
                    weapon: 0x09,
                    clips: vec![0x7C, 0x20D],
                },
                squirrel_fire,
                "a squirrel's battle cry, one start in four, cut when firing stops (C func_15065A5C: class 0x80; V sq_e_w09)",
            )
        },
        cue(
            On::Shot { weapon: 0x39 },
            record([755, 756], 32000, 500, 1500),
            "pistol (C func_15065A5C case 0x39; V e_w39)",
        ),
        cue(
            On::Casing {
                weapon: 0x39,
                delay: 15,
            },
            casing(),
            "pistol case, 30 VIs after the shot (C func_10010558; V e_w39)",
        ),
        cue(
            On::Shot { weapon: 0x24 },
            play([755, 756], 28000, Some(24000), 300, 1800, Channel::A),
            "sniper rifle (V e_w24)",
        ),
        cue(
            On::Casing {
                weapon: 0x24,
                delay: 55,
            },
            casing(),
            "sniper case, 110 VIs after the shot (C func_15065A5C; V e_w24)",
        ),
        cue(
            On::Hit {
                weapon: 0x09,
                lethal: Some(true),
            },
            splat(),
            "the killing bullet (C func_15138120; V e_w09)",
        ),
        cue(
            On::Hit {
                weapon: 0x39,
                lethal: Some(true),
            },
            splat(),
            "the killing bullet (V e_w39)",
        ),
        cue(
            On::Hit {
                weapon: 0x24,
                lethal: Some(true),
            },
            splat(),
            "a head shot (C func_15081E78; V e_w24)",
        ),
        cue(
            On::Hit {
                weapon: 0x24,
                lethal: Some(true),
            },
            play([170], 28000, Some(24000), 500, 2500, Channel::B),
            "a head shot (C func_15081E78 → func_15060A30(0xAA); V e_w24)",
        ),
        cue(
            On::Hit {
                weapon: 0x14,
                lethal: Some(true),
            },
            splat(),
            "a head shot (C func_15081E78)",
        ),
        cue(
            On::Hit {
                weapon: 0x14,
                lethal: Some(true),
            },
            play([170], 28000, Some(24000), 500, 2500, Channel::B),
            "a head shot (C func_15081E78)",
        ),
        cue(
            On::Hit {
                weapon: 0x14,
                lethal: Some(false),
            },
            play([263], 28000, Some(24000), 500, 2500, Channel::B),
            "a body hit (C func_15081E78 → func_15060A30(0x107); V w14)",
        ),
        cue(
            On::Hit {
                weapon: 0x18,
                lethal: Some(true),
            },
            splat(),
            "a head shot (C func_15081E78, as weapon 0x14's)",
        ),
        cue(
            On::Hit {
                weapon: 0x18,
                lethal: Some(true),
            },
            play([170], 28000, Some(24000), 500, 2500, Channel::B),
            "a head shot (C func_15081E78, as weapon 0x14's)",
        ),
        cue(
            On::Hit {
                weapon: 0x18,
                lethal: Some(false),
            },
            play([263], 28000, Some(24000), 500, 2500, Channel::B),
            "a body hit (C func_15081E78 → func_15060A30(0x107), as weapon 0x14's)",
        ),
        cue(
            On::Hit {
                weapon: 0x21,
                lethal: None,
            },
            thud(263),
            "the damage's sound word 0x107 (C func_1505B9C4, class attack table; V e_w21)",
        ),
        cue(
            On::Hit {
                weapon: 0x22,
                lethal: None,
            },
            thud(1098),
            "word 0x44A (C func_1505B9C4; V e_w22)",
        ),
        cue(
            On::Hit {
                weapon: 0x22,
                lethal: None,
            },
            splat(),
            "(V e_w22)",
        ),
        cue(
            On::Hit {
                weapon: 0x15,
                lethal: None,
            },
            thud(163),
            "flame, damage type 0x1C's word 0xA3 (C func_1505B9C4; V e_w15)",
        ),
        cue(
            On::Burning,
            record([77], 15000, 800, 2000),
            "the burn, a loop while it burns (C func_1518F5D0; V e_w15)",
        ),
        Cue {
            ..cue(
                On::Firing {
                    weapon: 0x15,
                    clips: vec![0xE7],
                },
                play([426], 32767, Some(32767), 500, 2500, Channel::Free),
                "the flame at each start of fire, unpositioned for the shooter (V e_w15: func_1519BF8C → func_10010F30; others' I)",
            )
        },
        cue(
            On::Holding { weapon: 0x22 },
            record([1096], 32000, 500, 1000),
            "the chainsaw running, a loop while it is held (C func_150339C8, func_15033BDC; V e_w22)",
        ),
        cue(
            On::Firing {
                weapon: 0x22,
                clips: vec![0x15F],
            },
            play(range(1092, 4), 18000, Some(24000), 500, 1500, Channel::Free),
            "each swing (C func_15033BDC on clip 0x15F → func_10010FFC: ¾ volume and a far distance by size for others, I; V e_w22)",
        ),
        cue(
            On::Holding { weapon: 0x26 },
            record([1299], 32000, 500, 1000),
            "the grenade's fuse hissing, a loop while it is in the hand (C func_15033AD8, func_15033BDC; V e_gren)",
        ),
        cue(
            On::Fuse { weapon: 0x26 },
            play([929], 32000, None, 500, 1000, Channel::A),
            "the fuse's last 120 VIs (C func_15033BDC: cook timer < 0x78 in the hand; func_15054F74 on the grenade; V e_gren 60 ticks before the blast)",
        ),
        cue(
            On::Flight {
                weapon: 0x26,
                ticks: vec![4],
            },
            play([1447], 28000, None, 300, 1800, Channel::A),
            "the grenade in the air (its own clip 0's event at tick 4; V e_gren)",
        ),
        cue(
            On::Bounce { weapon: 0x26 },
            play([1448, 1449], 28000, None, 300, 1800, Channel::A),
            "bouncing, the two in turn (its clip 1's events at ticks 15 and 23; V e_gren)",
        ),
        Cue {
            play: Play {
                pitch: [0, 499],
                ..record([1549], 32700, 100, 3000)
            },
            ..cue(
                On::Explosion { weapon: 0x26 },
                record([], 0, 0, 0),
                "grenade (C func_15055A2C kind 1: 0x60D, pitch + rand % 500 cents; V e_gren)",
            )
        },
        Cue {
            play: Play {
                pitch: [0, 499],
                ..record([689, 693, 694, 695, 701, 702], 32700, 1000, 3000)
            },
            ..cue(
                On::Explosion { weapon: 0x16 },
                record([], 0, 0, 0),
                "rocket (C func_15055A2C: table 0x8009919C; V w16, ctlw_k7)",
            )
        },
        cue(
            On::Gibs { weapon: 0x16 },
            record(range(440, 10), 24000, 500, 1000),
            "body parts landing (C func_15133D20; V w16)",
        ),
        cue(
            On::Gibs { weapon: 0x22 },
            record(range(440, 10), 24000, 500, 1000),
            "body parts landing (V e_w22)",
        ),
        cue(
            On::Flight {
                weapon: 0x14,
                ticks: vec![],
            },
            play([559, 560], 30000, None, 500, 1000, Channel::A),
            "the aimed shot in flight, one a tick (C func_150538CC; V ctlw_k4, w14)",
        ),
        Cue {
            play: Play {
                pitch: [-200, 200],
                ..record([176, 1113, 1114], 26000, 500, 800)
            },
            ..cue(
                On::Impact { weapon: 0x14 },
                record([], 0, 0, 0),
                "on a wall, by surface (C func_151D3480; V ctlw_k4 176, w14 1113/1114; which surface I)",
            )
        },
        cue(
            On::Scope { weapon: 0x24 },
            play([1541], 26000, Some(26000), 299, 300, Channel::Free),
            "the scope coming up (C func_15126378, camera code; V e_w24, ctlw_k10)",
        ),
    ];
    // Pickups (C func_1511DD98, by the pickup's class; V pick57 688, pick87 81, the bot's 1454; the
    // crossbow's class 0x3D and the shotgun's 0x5E, scene 0x34's placements: C).
    for (weapon, id) in [
        (0x09, 688),
        (0x39, 688),
        (0x21, 1296),
        (0x22, 1092),
        (0x14, 177),
        (0x23, 687),
        (0x15, 385),
        (0x24, 1454),
        (0x16, 1546),
        (0x18, 1090),
        (0x37, 688),
    ] {
        let mut c = cue(
            On::Pickup {
                gives: Gives::Weapon(weapon),
            },
            play([id], 26000, Some(26000), 100, 1000, Channel::Free),
            "pickup (C func_1511DD98)",
        );
        if weapon == 0x16 {
            c.character = Some("117".into());
        }
        cues.push(c);
    }
    cues.push(cue(
        On::Pickup {
            gives: Gives::Grenades,
        },
        play([81], 9000, Some(9000), 100, 1000, Channel::Free),
        "pickup (C func_1511DD98; V pick87)",
    ));
    for weapon in [0x09, 0x39, 0x24, 0x16, 0x23, 0x37] {
        cues.push(Cue {
            character: Some("128".into()),
            ..cue(
                On::Pickup { gives: Gives::Weapon(weapon) },
                play([1383], 26000, Some(26000), 100, 1000, Channel::Free),
                "a squirrel's voice on the pickup (C func_1511DD98: class 0x80; 0x23's 15 ticks later)",
            )
        });
    }
    // War: Total War's gas, the AI script 0xBA's sound objects (`obj(0x7nnn, mode, ..)`: native 3's
    // type 7, `func_1509EFF0`, sound id 0xnnn). Mode 1: `func_10010F30`, unpositioned for everyone.
    cues.push(cue(
        On::Gas,
        play([0x4CF], 0x6590, Some(0x6590), 0, 0, Channel::Free),
        "the gas, from 29 frames before the kill until the masks come off (C script 0xBA 0x402C: obj(0x74CF, 1, 0x6590); V its handle)",
    ));
    // Mode 4: `func_15103C14`, a record at the carrier heard everywhere; a player without a camera
    // (a bot) plays the next id, louder.
    for (canister, id) in [(0, 0x599), (1, 0x640)] {
        for (bot, id, volume) in [(false, id, 30000), (true, id + 1, 32700)] {
            cues.push(cue(
                On::Carry { canister, bot },
                record([id], volume, 32766, 32767),
                "a canister taken (C script 0xBA 0x4606/0x462E: obj(0x7599 / 0x7640, 4, 0, carrier, canister, 1))",
            ));
        }
    }
    // War: Colors' flags, the same mode 4 from script 0xBB's pickup (I: the third argument, 2 and 7
    // here, 0 for the canisters, is unread).
    for (flag, id) in [(0, 0x5A3), (1, 0x642)] {
        for (bot, id, volume) in [(false, id, 30000), (true, id + 1, 32700)] {
            cues.push(cue(
                On::Flag { flag, bot },
                record([id], volume, 32766, 32767),
                "a flag taken (C script 0xBB: obj(0x75A3, 4, 2, carrier, 0, 1) by a squirrel, obj(0x7642, 4, 7, carrier, 1, 1) by a Tedi)",
            ));
        }
    }
    cues
}

/// A map's own sound: its emitters, and the sequence its background plays, its channels and volume.
/// 0x2D: two bubbling spots (C func_151607A4 → func_151618BC; V: these two positions in every
/// capture, 25000, 500/1500, one of ten), sequence 19, channels 0-3 (mute mask 0xFFF0). 0x30: no
/// emitter (V: no positional call but the players' in 25 s of 2 idle humans), channels 0-5 (mask
/// 0xFFC0, V). 0x34: no emitter, sequence 36, channels 0-5 (V, as 0x30). 0x3F: no emitter, sequence
/// 84, no channel muted, volume 24000 (`func_1000E40C`, set again every frame; V).
fn map_sounds(map: &str) -> (Vec<Emitter>, u8, &'static str, u16) {
    let emitters = match map {
        "2d" => [[-1101, -1201, -229], [650, -1191, 486]]
            .map(|at| Emitter {
                at,
                play: play(range(1099, 10), 25000, None, 500, 1500, Channel::Free),
                start_chance: 0.005,
            })
            .into(),
        _ => vec![],
    };
    let (sequence, channels, volume) = match map {
        "2d" => (19, "0-3", 32000),
        "34" => (36, "0-5", 32000),
        "3f" => (84, "0-15", 24000),
        _ => (19, "0-5", 32000),
    };
    (emitters, sequence, channels, volume)
}

/// The pack's sound: `audio/sounds.json`, `audio/events.json` and the WAV files, and the folders
/// under `audio/` that hold them. `surfaces`: the maps' terrain surface kinds (footsteps); `maps`:
/// each map's folder name and a recording of its background (`../lab/audio`'s), or None.
pub fn pack_audio(
    rom: &Rom,
    surfaces: &BTreeSet<u8>,
    maps: &[(String, Option<&[u8]>)],
) -> Result<(Files, Vec<String>)> {
    use conker_assets::audio::{self as pack, Audio, Events, MapSounds};
    let game_data = rom.game_data()?;
    let mut characters = BTreeMap::new();
    let mut cues = code_cues();
    for (folder, class) in [("128", 128), ("117", 117), ("136", 136)] {
        characters.insert(folder.to_string(), clip_sounds(rom, &game_data, class)?);
        cues.extend(hit_voices(rom, &game_data, class, folder)?);
    }
    let mut files = Vec::new();
    let mut map_events = BTreeMap::new();
    for (name, background) in maps {
        let (emitters, sequence, channels, volume) = map_sounds(name);
        let mut map = MapSounds {
            background: vec![],
            emitters,
        };
        if let Some(wav) = background {
            let file = format!("maps/{name}/background.wav");
            map.background.push(Background {
                file: file.clone(),
                volume: 32767,
                note: format!("the game's own output (sequence {sequence}, channels {channels}, at volume {volume}), recorded on the oracle and looped whole"),
            });
            files.push((format!("audio/{file}"), wav.to_vec()));
        }
        map_events.insert(name.clone(), map);
    }
    let events = Events {
        maps: map_events,
        characters,
        footsteps: footsteps(rom, surfaces)?,
        cues,
    };

    // Each sound in the folder of what plays it: one character's, the map's emitters', or common.
    let (bank, wavetable) = sound_bank(rom)?;
    let mut owner: BTreeMap<u16, BTreeSet<String>> = BTreeMap::new();
    let mut add = |ids: &mut dyn Iterator<Item = u16>, scope: String| {
        for id in ids {
            owner.entry(id).or_default().insert(scope.clone());
        }
    };
    for (folder, clips) in &events.characters {
        for c in clips {
            if let ClipAction::Play(p) = &c.action {
                add(
                    &mut p.sounds.iter().copied(),
                    format!("characters/{folder}"),
                );
            }
        }
    }
    for (name, map) in &events.maps {
        for e in &map.emitters {
            add(&mut e.play.sounds.iter().copied(), format!("maps/{name}"));
        }
    }
    for c in &events.cues {
        let scope = c
            .character
            .as_ref()
            .map_or("common".to_string(), |ch| format!("characters/{ch}"));
        add(
            &mut c
                .play
                .sounds
                .iter()
                .chain(c.then.iter().flat_map(|p| &p.sounds))
                .copied(),
            scope,
        );
    }
    for id in events.sound_ids() {
        owner
            .entry(id)
            .or_insert_with(|| BTreeSet::from(["common".into()])); // footsteps
    }
    let mut sounds = BTreeMap::new();
    for (id, scopes) in owner {
        let scope = if scopes.len() == 1 {
            scopes.into_iter().next().unwrap()
        } else {
            "common".into()
        };
        let s = bank
            .get(usize::from(id).wrapping_sub(1))
            .with_context(|| format!("no sound {id}"))?;
        let pcm = decode(
            &wavetable[s.wave.base..s.wave.base + s.wave.len],
            &s.wave.book,
        )?;
        let file = format!("{scope}/{id:04}.wav");
        files.push((format!("audio/{file}"), wav(&pcm, s.rate())));
        let e = s.envelope;
        sounds.insert(
            id,
            pack::Sound {
                file,
                rate: s.rate(),
                volume: s.volume,
                pan: s.pan,
                envelope: pack::Envelope {
                    attack_us: e.attack_us,
                    attack_volume: e.attack_volume,
                    decay_us: e.decay_us,
                    decay_volume: e.decay_volume,
                    release_us: e.release_us,
                },
                looped: s.wave.looped.map(|l| pack::Loop {
                    start: l.start,
                    end: l.end,
                    count: (l.count != u32::MAX).then_some(l.count),
                }),
                detune_after_attack: s.detune_after_attack(),
            },
        );
    }
    let audio = Audio {
        sounds: pack::Sounds {
            mix_rate: OUTPUT_RATE,
            sounds,
        },
        events,
    };
    let folders: BTreeSet<String> = (files.iter())
        .filter_map(|(p, _)| {
            p.strip_prefix("audio/")?
                .rsplit_once('/')
                .map(|d| d.0.to_string())
        })
        .collect();
    files.extend(audio.write());
    Ok((files, folders.into_iter().collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trips() {
        let samples = [0, 1, -1, i16::MAX, i16::MIN];
        let bytes = wav(&samples, 22050);
        assert_eq!(bytes.len(), 44 + 10);
        assert_eq!(read_wav(&bytes).unwrap(), (22050, samples.to_vec()));
    }

    #[test]
    fn a_silent_frame_decodes_to_silence_and_residuals_scale() {
        let book = Book {
            order: 2,
            predictors: 1,
            coefficients: vec![0; 16],
        };
        assert_eq!(decode(&[0; 9], &book).unwrap(), vec![0; 16]);
        // Scale 2^1, first nibble 1 then -1 (0xF): with a zero book the output is the residual.
        let out = decode(&[0x10, 0x1F, 0, 0, 0, 0, 0, 0, 0], &book).unwrap();
        assert_eq!(&out[..3], &[2, -2, 0]);
        // A partial trailing frame is ignored.
        assert_eq!(decode(&[0; 13], &book).unwrap().len(), 16);
    }
}
