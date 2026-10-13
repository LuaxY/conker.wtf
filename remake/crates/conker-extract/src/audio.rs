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
/// mask put on and taken off (0x14E, 0x151: `../lab/research/mp-original/total-war-effects.md` §4),
/// and map 0x1A's bat (draw 0x10E, 0x110) and tommy gun (draw 0x1DD, idles 0x1DE/0x1DF, fire
/// 0x1E0, 0x253, 0x254) (C `func_15065A5C`; V `../lab/research/mp-original/heist.md` h3, h4), and
/// Heist's carrier's grab, idle, walk, run and throw (0x114-0x118; its hit 0x11B is already the
/// club's: V `../lab/research/mp-original/heist-presentation.md` §2), and War: Raptor's egg
/// carrier's grab, throw, walk, idle and run (0xD5-0xD8, 0x2E4: V `raptor.md` r4), a standing bite's
/// victim (0x170: V r2), the bone crossbow's draw and shot (0x2C8, 0x2C9: V r10) and the bone knives'
/// draw and throw (0x2CA, 0x2CC: V r9).
pub const MATCH_CLIPS: [u16; 140] = [
    0x0, 0x1, 0x2, 0x9, 0xF, 0x13, 0x16, 0x17, 0x18, 0x1B, 0x21, 0x2B, 0x32, 0x36, 0x37, 0x38,
    0x4C, 0x58, 0x59, 0x69, 0x76, 0x77, 0x78, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F, 0xBD, 0xC5, 0xC6,
    0xCF, 0xD3, 0xD5, 0xD6, 0xD7, 0xD8, 0xE2, 0xE4, 0xE7, 0xEB, 0xEC, 0xF0, 0xF3, 0xF4, 0xF7, 0xF8,
    0xF9, 0xFB, 0xFC, 0x102, 0x103, 0x10A, 0x10C, 0x10E, 0x10F, 0x110, 0x111, 0x112, 0x113, 0x114,
    0x115, 0x116, 0x117, 0x118, 0x119, 0x11A, 0x11B, 0x11C, 0x11D, 0x120, 0x121, 0x122, 0x124,
    0x129, 0x12A, 0x12B, 0x144, 0x14E, 0x151, 0x152, 0x153, 0x155, 0x156, 0x157, 0x158, 0x159,
    0x15A, 0x15B, 0x15C, 0x15D, 0x15E, 0x15F, 0x162, 0x163, 0x164, 0x165, 0x166, 0x168, 0x16B,
    0x16F, 0x170, 0x176, 0x177, 0x17E, 0x18A, 0x19C, 0x19D, 0x1DD, 0x1DE, 0x1DF, 0x1E0, 0x1EA,
    0x1EB, 0x1ED, 0x1EE, 0x1EF, 0x1F3, 0x1F4, 0x1F5, 0x1F6, 0x1F7, 0x1F8, 0x20D, 0x21E, 0x22C,
    0x235, 0x238, 0x253, 0x254, 0x26E, 0x28E, 0x2C2, 0x2C3, 0x2C8, 0x2C9, 0x2CA, 0x2CC, 0x2E4,
];
/// The racers' clips with sound events besides [`MATCH_CLIPS`]', for Race's models (136 the humans',
/// [`crate::RACER`] the bots'): the kick (0x354: sound 90 at tick 11). Riding (5), knocked up
/// (0x54) and dead (0x170) have none (V k1, b, the oracle's obj+0x84; C their route events).
const RACE_CLIPS: [u16; 1] = [0x354];
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
const VOICED: [(u32, Channel, u16, u16, u16, u16); 15] = [
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
    // The raptor's and the baby's calls and steps (`func_1506E998`, `func_1506BAD8(300, 3000)`) and
    // War: Raptor's body's (`func_1506E67C`, `func_1506BA4C(190, 1300)`).
    (141, Channel::B, 300, 3000, 28000, 24000),
    (145, Channel::A, 190, 1300, 28000, 24000),
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

/// The sound events of character `class`'s Deathmatch clips (Heist's bag, [`crate::CASH`], the
/// tank, [`crate::TANK`], and War: Raptor's raptor, baby and body: all their clips): its route
/// table's events (`[0x0F, route entry]`, as held.rs reads them; the baby's are the raptor's, as
/// [`crate::load_character`]'s clips) decoded as their op handlers do.
fn clip_sounds(rom: &Rom, game_data: &[u8], class: usize) -> Result<Vec<ClipSound>> {
    let animated = if class == crate::BABY {
        crate::RAPTOR
    } else {
        class
    };
    let entry = animation_entry(game_data, animation_entry(game_data, animated)?)?;
    let table = rom.asset(&[ROUTE_BANK, entry])?;
    let voices = rom.asset(&[VOICE_BANK, class])?;
    let mut out = Vec::new();
    for (clip, route) in parse_routes(&table)?.iter().enumerate() {
        let clip = clip as u16;
        let every = matches!(
            class,
            crate::CASH | crate::TANK | crate::RAPTOR | crate::BABY | crate::BODY
        );
        let racing = matches!(class, crate::CAVEMAN | crate::RACER) && RACE_CLIPS.contains(&clip);
        if route.aux_offset == 0 || (!every && !racing && !MATCH_CLIPS.contains(&clip)) {
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
/// names the voice; `func_15060A30` plays it on slot B. The mounted gun's (0x1B, its bullet entry
/// 0x2C) plays only when the hit does not kill (V `mounted-gun-scratch/gc`: gcQ the squirrel's 887
/// at 24000 on a hit, gcQK nothing on the kill). Map 0x1A's bat (0x19) swings as the club (the same
/// `func_150641D8(0x113, 0x113, 0x119)`, C); its tommy gun's (0x38) bullet has the pistol bullet's
/// numbers (I, `heist.md` §3.1). War: Raptor's bone knives (0x3F) and bolts (0x41) hurt the raptor
/// as weapon 0x14's knives and 0x18's bolts do, reaction 0x20 (I: V its hurt clip 12, which the
/// raptor's reactions 0x11, 0x1A and 0x20 all play, `raptor.md` r9, r10).
const REACTIONS: [(u8, u8); 24] = [
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
    (0x1B, 0x31),
    (0x19, 0x25),
    (0x19, 0x26),
    (0x38, 0x21),
    (0x3F, 0x20),
    (0x41, 0x20),
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
        let on = On::Hit {
            weapon,
            lethal: (weapon == 0x1B).then_some(false),
        };
        if cues.iter().any(|c| c.on == on) {
            continue;
        }
        let mut p = play(sounds, 28000, Some(24000), 500, 2500, Channel::B);
        (p.chance, p.if_idle) = (chance, if_idle);
        cues.push(Cue {
            character: Some(character.into()),
            ..cue(
                on,
                p,
                &format!("the victim's voice: reaction {reaction:#x}'s word {word:#x} (C func_1505E874, ROM tables)"),
            )
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
        at: None,
        delay: 0,
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
    // The tommy gun's firing clips: standing, moving, above speed 20 (the list at 0x800996C8).
    let tommy = || On::Firing {
        weapon: 0x38,
        clips: vec![0x1E0, 0x253, 0x254],
    };
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
                squirrel_fire.clone(),
                "a squirrel's battle cry, one start in four, cut when firing stops (C func_15065A5C: class 0x80; V sq_e_w09)",
            )
        },
        Cue {
            then: vec![
                record([727], 32767, 500, 1000),
                record(range(747, 3), 16000, 500, 1000),
            ],
            ..cue(
                tommy(),
                record([719], 32767, 500, 1000),
                "tommy gun: the twin guns' loop while firing (its clips, 0x800996C8), then the tail and a case (C func_15065A5C case 0x38, func_1000EFB4 by the sound 0x2CF)",
            )
        },
        Cue {
            character: Some("128".into()),
            ..cue(
                tommy(),
                squirrel_fire,
                "a squirrel's battle cry with the tommy gun, as with the twin guns (C func_15065A5C case 0x38: class 0x80)",
            )
        },
    ];
    // The Wise Guy's (0x3B) battle cry with the twin guns and the tommy gun: one of 0x543-0x546, one
    // start in four, cut when firing stops (C func_15065A5C cases 9 and 0x38, func_1000F568).
    let mut wise_guy_fire = record(range(1347, 4), 32767, 500, 1000);
    wise_guy_fire.chance = 63;
    for on in [
        On::Firing {
            weapon: 0x09,
            clips: vec![0x7C, 0x20D],
        },
        tommy(),
    ] {
        cues.push(Cue {
            character: Some("59".into()),
            ..cue(
                on,
                wise_guy_fire.clone(),
                "a Wise Guy's battle cry, one start in four, cut when firing stops (C func_15065A5C: class 0x3B)",
            )
        });
    }
    cues.extend([
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
            On::Head { weapon: 0x09 },
            splat(),
            "a head popped (C func_151D4DAC → func_15138C80; V e_w09 read 1004, a corpse's)",
        ),
        cue(
            On::Head { weapon: 0x39 },
            splat(),
            "a head popped (C func_151D4DAC → func_15138C80; V e_w39 read 970, a corpse's)",
        ),
        cue(
            On::FlyBy {
                chances: vec![(0x09, 8), (0x39, 1), (0x23, 255), (0x1B, 50)],
                leads: vec![40, 30, 60, 20, 15, 5, 5, 5],
            },
            play([807, 808, 809, 810, 811, 800, 801, 802], 26000, None, 10000, 9999, Channel::Free),
            "a bullet passing a camera: the table at 0x8008FBE8, 2D at 26000 panned by the bullet (C func_151C2734 → \
             func_10010F30, func_1000A420); the chances are the spawners' func_151C229C arg16 (twin guns func_151C62D0, \
             pistol func_150FED30, 0x23 func_150FE320, mounted gun func_150FB8C0; I: the tommy gun's func_150FE860, 0); \
             V e_w09 read 896: 802",
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
            On::Bounce { weapon: 0x26 },
            play([1447, 1448, 1449], 28000, None, 300, 1800, Channel::A),
            "a bounce leaving it faster than 5 upward, from the shuffle bag (C func_1505B5F8, func_1000F568; V e_gren, \
             sq_e_gren, ctlw_k*: the three each once, in every order)",
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
    ]);
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
        (0x38, 688),
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
    for weapon in [0x09, 0x39, 0x24, 0x16, 0x23, 0x37, 0x38] {
        cues.push(Cue {
            character: Some("128".into()),
            ..cue(
                On::Pickup { gives: Gives::Weapon(weapon) },
                play([1383], 26000, Some(26000), 100, 1000, Channel::Free),
                "a squirrel's voice on the pickup (C func_1511DD98: class 0x80; 0x23's 15 ticks later)",
            )
        });
    }
    // The Wise Guy's (0x3B): 0x542 on the twin guns, pistol, tommy gun and shotgun (classes 0x39,
    // 0x5C-0x5E); on the bazooka (class 0x3A) 0x53E or 0x53F in place of its 1546.
    for (weapon, ids) in [
        (0x09, &[1346][..]),
        (0x39, &[1346]),
        (0x38, &[1346]),
        (0x37, &[1346]),
        (0x16, &[1342, 1343]),
    ] {
        cues.push(Cue {
            character: Some("59".into()),
            ..cue(
                On::Pickup {
                    gives: Gives::Weapon(weapon),
                },
                play(
                    ids.iter().copied(),
                    26000,
                    Some(26000),
                    100,
                    1000,
                    Channel::Free,
                ),
                "a Wise Guy's voice on the pickup (C func_1511DD98: model 0x3B)",
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
    // Mode 4: `func_15103C14(id, who, n, slot, pairs)`, a radio line at the taker heard everywhere,
    // 20 VIs after its click on (`radio_click`), once per slot (its team's) in 600 VIs: one of n
    // variants (id + 2·rand % n; n 0: id), and a player without a camera (a bot) the next id,
    // louder.
    let radio = |on, ids: Vec<u16>, volume, note: &str| Cue {
        delay: 10,
        ..cue(on, record(ids, volume, 32766, 32767), note)
    };
    let variants =
        |id: u16, n: u16, bot: bool| (0..n.max(1)).map(move |k| id + 2 * k + u16::from(bot));
    for (canister, id) in [(0, 0x599), (1, 0x640)] {
        for (bot, volume) in [(false, 30000), (true, 32700)] {
            cues.push(radio(
                On::Carry { canister, bot },
                variants(id, 0, bot).collect(),
                volume,
                "a canister taken (C script 0xBA 0x4606/0x462E: obj(0x7599 / 0x7640, 4, 0, carrier, canister, 1))",
            ));
        }
    }
    // War: Colors' flags, the same mode 4 from script 0xBB's pickup: 2 squirrel lines, 7 Tedi ones
    // (C; V c1a read 328: (0x5A3, 0, 2, 0, 1), 0x5B0 at 328, 0x5B1 at 365).
    for (flag, id, n) in [(0, 0x5A3, 2), (1, 0x642, 7)] {
        for (bot, volume) in [(false, 30000), (true, 32700)] {
            cues.push(radio(
                On::Flag { flag, bot },
                variants(id, n, bot).collect(),
                volume,
                "a flag taken (C script 0xBB: obj(0x75A3, 4, 2, carrier, 0, 1) by a squirrel, obj(0x7642, 4, 7, carrier, 1, 1) by a Tedi)",
            ));
        }
    }
    cues.extend([
        cue(
            On::RadioClick { end: false },
            play([0x5B0], 25000, Some(25000), 100, 120, Channel::B),
            "the radio's click on, at the call, heard by everyone, centred (C func_15103910; V ev2 read 402, c1a read \
             328). A player without a camera clicks too: volume −25000 is 25000 with flag 0x100, centred and \
             everywhere (C func_15103AA0 → func_10010344; V radio read 102)",
        ),
        cue(
            On::RadioClick { end: true },
            play([0x5B1], 30000, Some(30000), 0x7FFE, 0x7FFF, Channel::B),
            "the radio's click off, when the voice has played (C func_15103910; V ev2 read 511, c1a read 365)",
        ),
    ]);
    // War: Colors: each loser's voice 30 frames after a capture (script 0xBB: obj(0x7657, 0, 0x7659,
    // 0, 0x5DC0, p | 0x2000, 800, 1000) → `func_1509F354`).
    for (team, id) in [(0, 0x657), (1, 0x662)] {
        cues.push(cue(
            On::Lose { team },
            record(range(id, 3), 24000, 800, 1000),
            "a capture's loser, 30 frames after it: one of its team's three, at it, unless it is voicing (C script \
             0xBB, func_1509F354: squirrels 0x657-0x659, Tediz 0x662-0x664; V c1a read 412: the Tedi bot's 0x664)",
        ));
    }
    cues.extend(gun_cues());
    cues
}

/// The mounted guns' sounds (`../lab/research/mp-original/mounted-gun.md` §4.4; V: the sound calls
/// of a gunner on 0x30's gun B, scenario 5, reads 83-693). There is no sound per shot: the gun's
/// class script loops 0x616 on its channel A while it fires.
fn gun_cues() -> Vec<Cue> {
    let gun = |from, to| On::Gun { from, to };
    // The gunner's own records at its position, 299/300: only the gunner hears them.
    let gunner = |id, volume| play([id], volume, Some(volume), 299, 300, Channel::Free);
    let mut cry = record(range(0x57C, 6), 32767, 500, 1000);
    cry.chance = 63;
    vec![
        cue(
            gun(3, 4),
            play([0x616], 28000, None, 700, 4000, Channel::A),
            "the firing, a loop (C func_1507BC14, the gun's class script; V: on each start of fire)",
        ),
        cue(
            gun(3, 4),
            play([0x60E], 28000, None, 300, 3000, Channel::B),
            "the firing's second sound, stopped when firing ends (V: the class script, channel B)",
        ),
        cue(
            gun(4, 3),
            play([0x617], 28000, None, 700, 4000, Channel::A),
            "firing ends, 20 frames after Z is let go (V: the class script, channel A)",
        ),
        cue(
            gun(4, 0xB),
            play([0x617], 28000, None, 300, 1800, Channel::A),
            "firing stopped (V: on overheating; I: none when B stops it, seen once)",
        ),
        cue(
            gun(0xB, 3),
            play([0x61B], 28000, None, 300, 3000, Channel::B),
            "the stop's end, 25 frames after B or overheating (V: the class script, channel B)",
        ),
        cue(
            On::GunStop,
            record(range(0x618, 3), 32000, 200, 500),
            "a record at the gunner (C func_15065A5C case 0x1B: 0x618 + rand % 3; V; its callback \
             func_1000EC24 plays it again unpositioned 15 frames later)",
        ),
        Cue {
            character: Some("128".into()),
            ..cue(
                On::Firing {
                    weapon: 0x1B,
                    clips: vec![0x1A4, 0x350],
                },
                cry,
                "a squirrel gunner's battle cry, one start of fire in four, cut when the gunner \
                 leaves these clips (C func_15065A5C: class 0x80, func_1000F568(0x57C, 6), list 0x800996D0)",
            )
        },
        cue(
            On::GunZoom { step: 0 },
            gunner(0x620, 26000),
            "the zoom record's loop (C func_15126378, func_15127520: table 0x800894E0 entry 1; V)",
        ),
        cue(
            On::GunZoom { step: 1 },
            gunner(0x621, 26000),
            "the zoom stops: the record's sound, and again unpositioned at 20000 (C func_15127520; V)",
        ),
        cue(
            On::GunZoom { step: 2 },
            gunner(0x622, 20000),
            "unpositioned once the record's sound has ended (C func_15127520, flag 0x80: I; V 16 frames after step 1, \
             never stopped: its sample loops but its envelope decays in 0.1 s, I)",
        ),
        cue(
            On::GunAim { step: 0 },
            gunner(0x61C, 16000),
            "the aim record's loop (C func_15126378, func_151277B0; V)",
        ),
        cue(
            On::GunAim { step: 1 },
            gunner(0x61D, 16000),
            "the aim stops (C func_151277B0; V)",
        ),
    ]
}

/// War: Beach's sounds (`../lab/research/mp-original/beach-presentation.md` §6; V: the oracle's sound
/// calls, `beach-presentation-scratch/snd_ev_calls.txt`, a record's args being volume, far, near).
/// Its two flickering lamps' buzz is the map's two emitters (`map_sounds`).
fn beach_cues() -> Vec<Cue> {
    // A radio voice (`func_15103C14`): a record at the speaker heard everywhere (30000, far
    // 0x7FFF, near 0x7FFE), its callback `func_15103910` clicking on the speaker's channel B first; a
    // player without a camera (a bot) says the next id at 32700 (`func_15103AA0`).
    let radio = |on: On, character: &str, id: u16, bot: bool, note: &str| Cue {
        character: Some(character.into()),
        delay: 10,
        ..cue(
            on,
            record(
                [id + u16::from(bot)],
                [30000, 32700][usize::from(bot)],
                0x7FFE,
                0x7FFF,
            ),
            note,
        )
    };
    let detonation = |id, at, delay| Cue {
        at: Some(at),
        delay,
        ..cue(
            On::Detonation,
            record([id], 28000, 7000, 9000),
            "one of the detonation's three records (C func_150FCA30; V ev2: at the call, 0x2B7 at once, \
             0x2B8 45 VIs later, 0x2B6 90 VIs later, heard in frames 617, 640, 662)",
        )
    };
    let mut cues = Vec::new();
    for bot in [false, true] {
        cues.extend([
            radio(
                On::PlungerReady { bot },
                "156",
                0x6A9,
                bot,
                "a male villager in trigger 0x25 while the plunger is ready (C func_1509EFF0; V ev2 read 413, 20 VIs after \
                 the click; a bot's 0x6AA by the rule)",
            ),
            radio(
                On::PlungerReady { bot },
                "157",
                0x6A5,
                bot,
                "a female villager in trigger 0x25 while the plunger is ready (C func_1509EFF0; V radio: a bot's 0x6A6 at \
                 32700, centred, 10 frames after its click)",
            ),
            radio(
                On::Plunger { bot },
                "156",
                0x6AB,
                bot,
                "a male villager on the plunger (C; not heard in ev3: the 600-VI cooldown per player, 0x800D9AB0)",
            ),
            radio(
                On::Plunger { bot },
                "157",
                0x6A7,
                bot,
                "a female villager on the plunger (C)",
            ),
        ]);
    }
    cues.extend([
        detonation(0x2B7, [-319, -273, -3419], 0),
        detonation(0x2B8, [-2038, 1560, -2631], 23),
        detonation(0x2B6, [1519, 947, -2633], 45),
        cue(
            On::Debris,
            play(range(0x1B9, 9), 24000, None, 500, 1000, Channel::Free),
            "each piece of debris landing (C func_15133D20; V ev2 655-694 after the detonation, ev3 1082-1109 after the strike)",
        ),
        cue(
            On::StrikeCall,
            record([0x69E], 32000, 500, 3000),
            "the strike's call, a record at the villager (C func_150FCFD4; V ev3 read 1004)",
        ),
        cue(
            On::StrikeBlast,
            play([0x69D], 32000, None, 500, 3000, Channel::Free),
            "the strike's blast, 40 frames after the call, at its spot (C func_150FD514; V ev3 read 1044)",
        ),
    ]);
    for character in ["156", "157"] {
        cues.push(Cue {
            character: Some(character.into()),
            ..cue(
                On::StrikeCall,
                play(range(1715, 12), 28000, Some(24000), 500, 2500, Channel::B),
                "the villager's voice at the call: its pain voice, as its hit voices (C func_1505E874 from \
                 func_1505D024, damage 0x39; V ev3 read 1004: 1725 at 24000, its own)",
            )
        });
    }
    cues
}

/// Heist's voices and the bag's throw on 0x1A (`../lab/research/mp-original/heist-presentation.md` §5,
/// `heist.md` §2.3; V `heist-presentation-scratch/bag/snd_bag_calls.txt`). The voices are fixed ids,
/// the same for every character: script 0xB7's `obj(id, 2, 0x7FBC, player, 500, 1000, 1)` →
/// `func_1509EFF0` → `func_1509F5F4` on the speaker's channel B, centred for it, at it for the others.
fn heist_cues() -> Vec<Cue> {
    let voice = |ids: &[u16]| {
        play(
            ids.iter().copied(),
            32700,
            Some(32700),
            500,
            1000,
            Channel::B,
        )
    };
    vec![
        cue(
            On::CashPickup,
            voice(&[1130, 1131]),
            "a pickup of the bag at rest (C script 0xB7 kind 1: 0x46A + random(2); V snd_bag 335, 566: 1130)",
        ),
        cue(
            On::CashSteal { rare: true },
            voice(&[1351]),
            "a steal within 240 frames of another's drop, one case in three (C script 0xB7 kind 0; V snd_bag 464)",
        ),
        cue(
            On::CashSteal { rare: false },
            record(range(1125, 3), 32700, 500, 1000),
            "a steal's other two cases: one of 1125-1127 as Colors' loser lines, a record at the picker (C script \
             0xB7 kind 0: obj(0x7465, 0, 0x7467, 0, 0x7FBC, picker, 500, 1000) → func_1509F354; not heard)",
        ),
        cue(
            On::CashDelivery,
            voice(&[1128, 1129]),
            "a delivery: 0x468 + random(2) at the carrier (C script 0xB7; V snd_bag 632: 1129)",
        ),
        cue(
            On::CashThrow,
            play([91], 28000, None, 500, 2500, Channel::B),
            "the throw's release (clip 0x118's op 54 → func_1506EA98 → func_15060A30): the whoosh on the bag's \
             channel B, at it (V snd_bag 394)",
        ),
    ]
}

/// Tank's sounds besides the tank's clips (its shell's fire is clip 1's event: 699) on 0x24
/// (`../lab/research/mp-original/tank.md` §8; C as named, V `../lab/audio/captures/tank_s1_calls.txt`).
fn tank_cues() -> Vec<Cue> {
    let tank = |on, play, note: &str| Cue {
        character: Some(crate::TANK.to_string()),
        ..cue(on, play, note)
    };
    let pickup = |bit, id, note: &str| {
        let gives = Gives::Weapon(bit);
        let play = play([id], 26000, Some(26000), 100, 1000, Channel::Free);
        tank(On::Pickup { gives }, play, note)
    };
    let blast = || Play {
        pitch: [0, 499],
        ..record([689, 693, 694, 695, 701, 702], 32700, 1000, 3000)
    };
    let mut gear = play(range(1187, 4), 28000, Some(28000), 300, 1800, Channel::A);
    gear.if_idle = true;
    vec![
        tank(
            On::TankEngine { moving: false },
            record([1191], 0x2000, 200, 800),
            "idling: 0x4A7 from the tank's creation, back to it when it neither speeds up nor coasts (C \
             func_151BB044, func_151BB0E0: a record following the tank, |speed| · obj+0xB8 / 2 a parameter \
             of it, I: the pitch; V s1 22, 58)",
        ),
        tank(
            On::TankEngine { moving: true },
            record([1192], 0x2EE0, 200, 800),
            "driving (speeding up, or faster than 5 and coasting): 0x4A8, its volume scaled by a fade of 8/256 \
             a frame; 900 or 1412 VIs on, 0x4A9 at 0x2EE0 (C func_151BB0E0; V s1 44, 351; 0x4A9 not heard: \
             not packed)",
        ),
        tank(
            On::TankGear { coast: false },
            gear.clone(),
            "a change of the engine's state: one of 0x4A3-0x4A6 on the tank's channel A when idle (C \
             func_151BB0E0: func_1000F568(0x4A3, 4); V s1 58, 99, 140, 221)",
        ),
        tank(
            On::TankGear { coast: true },
            Play {
                sounds: vec![1189],
                ..gear
            },
            "into coasting: 0x4A5 (C func_151BB0E0; V s1 44, 85, 126, 167, 207)",
        ),
        tank(
            On::TankBlast { ground: false },
            blast(),
            "a shell on a tank: the rocket's blast (C func_151BBA9C → func_15055A2C(1, …, 0); V s1 43, 84, \
             125, 166, 206, 247)",
        ),
        tank(
            On::TankBlast { ground: true },
            blast(),
            "a shell on the ground: the rocket's blast (C func_151BB61C → func_15055A2C(0, …, 0); V s1 296)",
        ),
        tank(
            On::TankBlast { ground: true },
            play([750], 30000, None, 500, 1000, Channel::A),
            "a shell on the ground, on the shell's channel A as well (C func_1504A730 → func_150538CC; V s1 \
             296)",
        ),
        Cue {
            delay: 22,
            ..tank(
                On::TankDeath { bomb: false },
                play([742], 12000, None, 500, 1000, Channel::Free),
                "a tank killed by a shell, at its clip 7's tick 23 (C func_151670C0 → func_15133C58; V s1 \
                 269, 22 frames after the killing blast at 247)",
            )
        },
        tank(
            On::TankDeath { bomb: true },
            play([735], 12000, None, 500, 1000, Channel::Free),
            "a tank killed by the bomb's gas, at its clip 6's tick 6 (C func_151670C0 → func_15133C58; V s1 \
             652; the delay after the detonation not measured)",
        ),
        tank(
            On::BombPickup,
            play([11, 12], 26000, Some(26000), 100, 1000, Channel::Free),
            "the gas bomb taken: 0xB + random(2) (C func_1511DD98, class 0x50; V s1 311: 12)",
        ),
        pickup(
            0x80,
            1296,
            "the shield taken (C func_1511DD98, class 0x58; V s1 271)",
        ),
        pickup(
            0x01,
            1187,
            "the boost's canisters taken (C func_1511DD98, class 0x47)",
        ),
        pickup(
            0x10,
            385,
            "the one-shot shells' container taken (C func_1511DD98, class 0x51)",
        ),
        pickup(
            0x02,
            752,
            "the scope's launcher taken (C func_1511DD98, class 0x46)",
        ),
    ]
}

/// Race's sounds besides the racers' clips (the kick's swing, 90, is clip 0x354's event) on 0x33
/// (`../lab/research/mp-original/race.md`; C as named, V `../lab/audio/captures/race_{a,b,c}_calls.txt`:
/// a 1 human and 3 bots with walls, deaths and kicks, b missiles fired and hitting, c the finish).
/// The finish, the results and a jump play nothing (V c, a).
fn race_cues() -> Vec<Cue> {
    let hum = |hum, id, note: &str| cue(On::RaceHum { hum }, record([id], 1000, 200, 1000), note);
    let beep = |stage, id| {
        cue(
            On::RaceCountdown { stage },
            play([id], 0x5DC0, Some(0x5DC0), 0, 0, Channel::Free),
            "the countdown's beep, unpositioned for everyone (C script 0xBC 0x1566: obj(0x7687 or 0x7688, 1, \
             0x5DC0) → func_10010F30; V a, b, c reads 141, 171: 1671, 196: 1672)",
        )
    };
    let pickup = |item| {
        cue(
            On::Pickup {
                gives: Gives::Weapon(item),
            },
            play([1577], 32767, Some(32767), 100, 1000, Channel::Free),
            "a power-up taken: 0x629 (C func_1511DD98; V a, b, c)",
        )
    };
    let death = |kind_e, sounds: &[u16], volume, near, far, note: &str| {
        let p = play(
            sounds.iter().copied(),
            volume,
            Some(volume),
            near,
            far,
            Channel::Free,
        );
        cue(On::RaceDeath { kind_e }, p, note)
    };
    let mut flight = play(range(750, 3), 30000, None, 500, 1000, Channel::A);
    flight.if_idle = true;
    let mut wall = play(range(778, 8), 28000, Some(24000), 300, 1800, Channel::A);
    wall.if_idle = true;
    vec![
        hum(
            0,
            1459,
            "hum 0x5B3 (C func_150D3360 → func_1000FA64: at the rider, volume 1000, near 200, far 1000, callback \
             func_150D5124; V a: three records a racer from read 22, made again at each respawn)",
        ),
        hum(1, 1460, "hum 0x5B4 (C, V as hum 0's)"),
        hum(2, 1468, "hum 0x5BC (C, V as hum 0's)"),
        beep(1, 1671),
        beep(2, 1671),
        beep(3, 1672),
        pickup(0x31),
        pickup(0x33),
        cue(
            On::Shot { weapon: 0x31 },
            play([1576], 32767, Some(32767), 500, 1000, Channel::A),
            "a canister used (Z, the boost): 0x628 on the racer's channel A (C func_150D7790; V a, w3)",
        ),
        cue(
            On::Shot { weapon: 0x33 },
            play([567], 30000, None, 500, 2000, Channel::A),
            "a missile fired (Z): 0x237 on the missile's channel A, at the racer (C func_150D7790 → \
             func_1506C460; V a, b)",
        ),
        cue(
            On::RaceMissile,
            flight,
            "a missile in flight: from 30 VIs (obj+0xA8), 0x2EE + random(3) when its channel A is idle (C \
             func_150538CC kind 0xD; V a, b)",
        ),
        cue(
            On::Hit {
                weapon: 0x33,
                lethal: None,
            },
            play([263], 28000, None, 300, 1800, Channel::Free),
            "a missile hitting a racer: 0x107 (attacks 0x0F and 0x10's sound) on the missile, which vanishes \
             (here on no player's channel) (C func_1505C7D8; V b 233, 269)",
        ),
        cue(
            On::RaceKick,
            play([1015], 28000, Some(24000), 300, 1800, Channel::A),
            "a kick hitting a racer: attack 0x15's sound word 0xFF0103F7 (1015) on the kicker's channel A \
             (C func_1505C7D8, the class data's attack table; V a)",
        ),
        cue(
            On::RaceWall,
            wall,
            "a scrape: word 0xFF08030A, one of 778-785 on the racer's channel A when idle (C func_1504CB98; V a \
             525, 971)",
        ),
        death(
            Some(false),
            &[1224],
            32767,
            700,
            2000,
            "the blow-up: 0x4C8, its volume passed as −0x7D00 (0x8300 to the player: I full) (C func_150D5A6C; \
             V a, b)",
        ),
        death(
            Some(true),
            &[1076],
            32000,
            700,
            2000,
            "the blow-up over surface kind 0xE (178 of 0x33's faces): 0x434 (C func_150D5A6C; V a)",
        ),
        death(
            None,
            &[694, 695],
            32000,
            1000,
            3000,
            "the blow-up's blast: 0x2B6 + random(2) (C func_150D5A6C; V a, b)",
        ),
        death(
            None,
            &[689, 690, 691, 692],
            32000,
            700,
            2000,
            "the blow-up's debris: 0x2B1 + random(4) (C func_150D5A6C; V a, b)",
        ),
    ]
}

/// A map's own sound: its emitters, and the sequence its background plays, its channels and volume.
/// 0x2D: its two flickering lamps' buzz (C func_151607A4 → func_15160954 → func_151618BC; V: these
/// two positions in every capture, 25000, 500/1500, one of ten), sequence 19, channels 0-3 (mute mask 0xFFF0). 0x30: no
/// emitter (V: no positional call but the players' in 25 s of 2 idle humans), channels 0-5 (mask
/// 0xFFC0, V). 0x34: no emitter, sequence 36, channels 0-5 (V, as 0x30). 0x3F: no emitter, sequence
/// 84, no channel muted, volume 24000 (`func_1000E40C`, set again every frame; V). 0x2B (War: Beach's
/// scenario 8, 2 idle humans 25 s): two lamps as 0x2D's by the villagers' starts (V: bursts of
/// 1100-1108 from `func_151618BC` at both), sequence 77, channels 0-9 (mute mask 0xFC00), volume 18000.
/// 0x1A (Heist and Deathmatch scenario 6, the same; V `../lab/audio/map1a/`): two lamps as 0x2D's (V
/// 98-99 buzzes each in 8,653 reads); its five switching lights (`lights.json` `switching`), each
/// switch back on one of 1099-1108 at 16000, 500/1000 (C func_15100340 → func_151616D0 →
/// func_151604A0, message 0x1C; V 191-219 each); sequence 30, no channel muted (mask 0), volume 8000
/// (C func_150000B0: the scene record's song, V). 0x24 (Tank, scenario 1, 2 humans): no emitter
/// (V `../lab/audio/captures/tank_s1_calls.txt`: only the tanks' and the match's sounds), sequence
/// 32, no channel muted, volume 18000 (C func_150000B0, V). 0x33 (Race): no emitter, no zone and no
/// song (V `../lab/audio/captures/race_*_calls.txt`: only the racers', the power-ups' and the
/// countdown's sounds, no song start; `../lab/audio/map33/quiet.wav`, the boards' hums muted: digital
/// silence between them), so no background.
fn map_sounds(map: &str) -> (Vec<Emitter>, u8, &'static str, u16) {
    let spots = match map {
        "2d" => &[[-1101, -1201, -229], [650, -1191, 486]][..],
        "2b" => &[[2616, -236, 8004], [-3032, -327, 8306]],
        "1a" => &[[5770, 299, -199], [-6631, 336, -167]],
        _ => &[],
    };
    let switching = match map {
        "1a" => &[
            [230, 306, -2040],
            [1887, 300, 739],
            [-528, 342, 5632],
            [-153, 283, -5940],
            [-2708, 329, -1110],
        ][..],
        _ => &[],
    };
    let buzz = |at, volume, far| Emitter {
        at,
        play: play(range(1099, 10), volume, None, 500, far, Channel::Free),
    };
    let lamps = spots.iter().map(|&at| buzz(at, 25000, 1500));
    let emitters = lamps
        .chain(switching.iter().map(|&at| buzz(at, 16000, 1000)))
        .collect();
    let (sequence, channels, volume) = match map {
        "2d" => (19, "0-3", 32000),
        "2b" => (77, "0-9", 18000),
        "1a" => (30, "0-15", 8000),
        "34" => (36, "0-5", 32000),
        "3f" => (84, "0-15", 24000),
        "24" => (32, "0-15", 18000),
        _ => (19, "0-5", 32000),
    };
    (emitters, sequence, channels, volume)
}

/// A map's sound zones: its trigger records of kind 2 (`+0x15 >> 2`, `func_150A11C4` → `func_150A23E4`,
/// C; V on 0x2B's 0x23 and 0x24, `../lab/research/mp-original/beach-environment.md` §5). Each plays
/// sound `+0x1C >> 16` at its centre at volume (`+0x17` & 0x7F)·0x147, near `+0x1C & 0xFFFF`, far
/// `+0x18`, centred (flag 0x100), by horizontal distance for shape 0 (bit 0x8000 of the far): 0x2B's
/// two, 0x1A's four (its bases' 1116; V `../lab/audio/map1a/`: they played while idle players stood
/// in 0x35 and 0x37). Every one stops on leave: `func_150A09D0`'s pass stops any kind-2 record no
/// player entered since its last pass (C; V on 0x1A, `heist-presentation.md` §3).
fn zones(triggers: &[u8]) -> Vec<conker_assets::audio::Zone> {
    let records = triggers.as_chunks::<0x34>().0.iter().enumerate();
    (records.filter(|(_, r)| r[0x15] >> 2 == 2))
        .map(|(i, r)| {
            let h = |o: usize| i16::from_be_bytes([r[o], r[o + 1]]);
            let w = |o: usize| u32::from_be_bytes(r[o..o + 4].try_into().unwrap());
            conker_assets::audio::Zone {
                trigger: i as u16,
                at: [h(0), h(2), h(4)],
                horizontal: r[0x15] & 3 == 0,
                play: play(
                    [(w(0x1C) >> 16) as u16],
                    u16::from(r[0x17] & 0x7F) * 0x147,
                    None,
                    w(0x1C) as u16,
                    w(0x18) as u16,
                    Channel::Free,
                ),
                stops_on_leave: true,
            }
        })
        .collect()
}

/// The raw stem's frames ahead of the south recording's ([`loop_cut`]'s source): it runs 552 or 736
/// late, by the dumps' 184-frame audio blocks, so the midpoint (V `beach-environment.md` §4).
pub const STEM_LAG: usize = 644;

/// `../lab/audio/map2b/background_loop.wav`'s cut of `background_south.wav` (V: byte for byte,
/// `loop_cut_is_the_backgrounds`): from frame 1,001,910 (45.5 s at 22,020 Hz) one period of song
/// 0x4D, 1,878,088 frames (85.290 s), its first 11,010 (0.5 s) cross-faded with the frames a period
/// on.
pub const LOOP_CUT: [usize; 3] = [1_001_910, 1_878_088, 11_010];

/// A 16-bit PCM WAV with the lab recordings' 44-byte header, its first `skip` frames dropped, cut as
/// [`LOOP_CUT`]: frame n of the loop is frame start + n, the first `fade` mixed with start + period + n
/// by weights n/fade and 1 − n/fade, rounded half to even.
pub fn loop_cut(wav: &[u8], skip: usize) -> Result<Vec<u8>> {
    ensure!(
        wav.len() >= 44 && &wav[..4] == b"RIFF" && &wav[36..40] == b"data",
        "not a WAV file"
    );
    let word = |at: usize| u32::from_le_bytes(wav[at..at + 4].try_into().unwrap());
    ensure!(
        word(20) & 0xFFFF == 1 && word(32) >> 16 == 16,
        "not 16-bit PCM"
    );
    let channels = (word(20) >> 16) as usize;
    let data = wav
        .get(44..44 + word(40) as usize)
        .context("WAV: short data")?;
    let samples: Vec<i16> = (data.as_chunks::<2>().0.iter())
        .map(|&b| i16::from_le_bytes(b))
        .collect();
    let [start, period, fade] = LOOP_CUT;
    ensure!(
        (skip + start + period + fade) * channels <= samples.len(),
        "WAV: too short for the loop's cut"
    );
    let at = |frame: usize, c: usize| f64::from(samples[(skip + frame) * channels + c]);
    let bytes = (period * channels * 2) as u32;
    let mut out = wav[..44].to_vec();
    out[4..8].copy_from_slice(&(36 + bytes).to_le_bytes());
    out[40..44].copy_from_slice(&bytes.to_le_bytes());
    for n in 0..period {
        for c in 0..channels {
            let v = match n < fade {
                true => {
                    let w = n as f64 / fade as f64;
                    (at(start + n, c) * w + at(start + period + n, c) * (1.0 - w)).round_ties_even()
                }
                false => at(start + n, c),
            };
            out.extend((v as i16).to_le_bytes());
        }
    }
    Ok(out)
}

/// The pack's sound: `audio/sounds.json`, `audio/events.json` and the WAV files, and the folders
/// under `audio/` that hold them. `surfaces`: the maps' terrain surface kinds (footsteps); `maps`:
/// each map's folder name and recordings of its background (`../lab/audio`'s), of its song's
/// positional channels ([`STEM_LAG`]) and of its song (0x24's bomb countdown), or None; `characters`: the bank-01 entries whose clip sounds
/// and hit voices play (each its own folder).
#[allow(clippy::type_complexity)]
pub fn pack_audio(
    rom: &Rom,
    surfaces: &BTreeSet<u8>,
    maps: &[(String, Option<&[u8]>, Option<&[u8]>, Option<&[u8]>)],
    characters: &[usize],
    outro_sounds: &[(String, BTreeSet<u16>)],
) -> Result<(Files, Vec<String>)> {
    use conker_assets::audio::{self as pack, Audio, Events, Follow, MapSounds};
    let game_data = rom.game_data()?;
    let entries = characters;
    let mut characters = BTreeMap::new();
    let mut cues = code_cues();
    for &class in entries {
        let folder = class.to_string();
        characters.insert(folder.clone(), clip_sounds(rom, &game_data, class)?);
        cues.extend(hit_voices(rom, &game_data, class, &folder)?);
    }
    // Heist's bag on 0x1A: its own clips' sounds (no voice), and the voices and the throw.
    if maps.iter().any(|m| m.0 == "1a") {
        let bag = clip_sounds(rom, &game_data, crate::CASH)?;
        characters.insert(crate::CASH.to_string(), bag);
        cues.extend(heist_cues());
    }
    if maps.iter().any(|m| m.0 == "24") {
        cues.extend(tank_cues());
    }
    if maps.iter().any(|m| m.0 == "33") {
        cues.extend(race_cues());
    }
    // War: Raptor's baby and body on 0x3F: their clips' sounds (the raptor's are a character's).
    if maps.iter().any(|m| m.0 == "3f") {
        for prop in [crate::BABY, crate::BODY] {
            characters.insert(prop.to_string(), clip_sounds(rom, &game_data, prop)?);
        }
        cues.extend([0x1A6, 0x434].map(|id| {
            cue(
                On::RaptorSplat,
                record([id], 0x7D00, 500, 1000),
                "an egg's splat: 0x1A6 and 0x434 at it (C script 0xBE 0x476A-0x48C4: obj(0x71A6 / 0x7434, 3, \
                 0x7D00, egg, 500, 1000); V raptor r4)",
            )
        }));
    }
    // No cues for a character the pack leaves out (the Wise Guy's without map 0x1A).
    cues.retain(|c| (c.character.as_ref()).is_none_or(|n| characters.contains_key(n)));
    let mut files = Vec::new();
    let mut map_events = BTreeMap::new();
    for (name, background, stem, song) in maps {
        let (emitters, sequence, channels, volume) = map_sounds(name);
        let scene = usize::from_str_radix(name, 16)?;
        let mut map = MapSounds {
            background: vec![],
            emitters,
            lines: BTreeMap::new(),
            zones: zones(&rom.asset(&[0x0C, scene, 5])?),
        };
        // War: Beach's escape: MP3 0x1A8 for a human escapee (`func_1001263C(0x1A8, 0x7FFF, 0x40)`).
        if name == "2b" {
            cues.extend(beach_cues());
            let file = format!("maps/{name}/voice-1a8.wav");
            files.push((
                format!("audio/{file}"),
                mp3_wav(&rom.asset(&[0x16, 0x1A8])?)?,
            ));
            let note = "a human villager's escape: the radio, centred, while the music's volume ducks from 0x4650 \
                        to 0x1E0A (−0x233 a frame) and comes back at +0x1000 a frame once it ends; not when an \
                        MP3 already plays (C func_1509E640; V e1 read 101, snd ev 103-121, 308-312)";
            map.lines.insert(
                "escape".into(),
                pack::Line {
                    file,
                    volume: 32767,
                    note: note.into(),
                },
            );
        }
        // War: Colors' capture jingles: MP3 0x1A4 when the squirrels score, 0x1A7 when the Tediz do.
        if name == "30" {
            for (team, id) in [("squirrels", 0x1A4), ("tediz", 0x1A7)] {
                let file = format!("maps/{name}/jingle-{id:x}.wav");
                files.push((format!("audio/{file}"), mp3_wav(&rom.asset(&[0x16, id])?)?));
                let note = format!(
                    "the capture jingle when the {team} score, for everyone 30 frames after the capture (the \
                     final one too): centred, full volume, the music ducked as for 0x2B's escape; an MP3 that \
                     starts later (an outro's voice) takes over (C script 0xBB command({:#x}, 0) → func_1509E640 \
                     → func_1001263C({id:#x}, 0x7FFF, 0x40); V c1a, c1b read 412)",
                    0x6000 | id
                );
                let line = pack::Line {
                    file,
                    volume: 32767,
                    note,
                };
                map.lines.insert(format!("jingle-{team}"), line);
            }
        }
        if let Some(wav) = background {
            let file = format!("maps/{name}/background.wav");
            map.background.push(Background {
                file: file.clone(),
                volume: 32767,
                note: format!("the game's own output (sequence {sequence}, channels {channels}, at volume {volume}), recorded on the oracle and looped whole"),
                follow: None,
            });
            files.push((format!("audio/{file}"), wav.to_vec()));
        }
        // 0x2B's song 0x4D: channels 13 and 14, which the south recording lacks, follow (0, −377, 8227).
        if let Some(wav) = stem.filter(|_| name == "2b") {
            let file = format!("maps/{name}/background_stem.wav");
            map.background.push(Background {
                file: file.clone(),
                volume: 32767,
                note: "song 0x4D's channels 13 and 14 alone, recorded on the oracle at 100 % (the others muted by \
                       the start's mask 0x9FFF in place of 0xFC00), cut as background.wav: played with it, its \
                       volume by the listener's 3D distance to (0, −377, 8227), full within 3000, none from 4000, \
                       clamped at 100 % (C func_1000BA18 → func_100114D0, func_1000E588: CC 0xFC on channels 13/14; \
                       V beach-environment.md §4)"
                    .into(),
                follow: Some(Follow {
                    at: [0, -377, 8227],
                    near: 3000,
                    far: 4000,
                }),
            });
            files.push((format!("audio/{file}"), loop_cut(wav, STEM_LAG)?));
        }
        // Tank's bomb countdown: song 50, recorded alone.
        if let Some(wav) = song.filter(|_| name == "24") {
            let file = format!("maps/{name}/countdown.wav");
            let note = "the bomb's plant: song 50 on music player 1 over the map's (script 0xB8 command \
                        0xA032 → func_1509E730 → func_1000D96C(50); V s1 read 331), once, 26 s, for everyone: \
                        recorded on the oracle in place of the map's song 32 (I: player 1's volume as player 0's)";
            let line = pack::Line {
                file: file.clone(),
                volume: 32767,
                note: note.into(),
            };
            map.lines.insert("countdown".into(), line);
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
    // A sound several maps' emitters play (0x2D's and 0x2B's bubbles) stays in the first map's folder.
    let mut first: BTreeMap<u16, &str> = BTreeMap::new();
    for (name, ..) in maps {
        for &id in events.maps[name]
            .emitters
            .iter()
            .flat_map(|e| &e.play.sounds)
        {
            if *first.entry(id).or_insert(name) == name {
                add(&mut std::iter::once(id), format!("maps/{name}"));
            }
        }
        for z in &events.maps[name].zones {
            add(&mut z.play.sounds.iter().copied(), format!("maps/{name}"));
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
    // The outros' script sounds (`outros.json`), in their map's folder.
    for (name, ids) in outro_sounds {
        add(&mut ids.iter().copied(), format!("maps/{name}"));
    }
    // Scene 0x34's traps' sounds (`crate::traps`), in its folder.
    let traps = crate::map_name(crate::traps::SCENE);
    if maps.iter().any(|m| m.0 == traps) {
        add(
            &mut crate::traps::SOUNDS.into_iter(),
            format!("maps/{traps}"),
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

    /// [`loop_cut`] makes `background_loop.wav` from `background_south.wav` byte for byte (the lab's
    /// recordings; skipped without them).
    #[test]
    fn loop_cut_is_the_backgrounds() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../lab/audio/map2b");
        let read = |f: &str| std::fs::read(format!("{dir}/{f}")).ok();
        let (Some(south), Some(cut)) = (read("background_south.wav"), read("background_loop.wav"))
        else {
            eprintln!("no 0x2B recordings in {dir}: skipping");
            return;
        };
        assert!(loop_cut(&south, 0).unwrap() == cut);
    }

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
