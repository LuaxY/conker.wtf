//! Bank-02 skeletal animation and its bank-0x0F routes.
//!
//! Ported from DevOldSchool/conkers-bfd-decomp (MIT, commit e2400b27dda6): `docs/rzip-assets.md`
//! §"Character containers and animation", `docs/evidence/assets/models/us_asset_inventory.md`
//! and `scripts/model_assets.py` (`parse_character_animation_descriptor`,
//! `decode_character_animation_frame`, `character_animation_frame_span`,
//! `character_rotation_quaternion`, `character_animation_preview_times`,
//! `parse_character_animation_route_table`, `character_animation_group_routes`,
//! `resolve_character_animation_runtime_duration`).
//!
//! A bank-02 entry is a table of (descriptor, stream) segment pairs. `func_1505E0C4` copies a
//! descriptor into zeroed state, `func_1502D824` copies fixed-stride frames (descriptor byte 5)
//! and `func_150A8A18` decodes each frame: masked joint translations, then a big-endian bitstream
//! of root translation, Euler rotations and optional scales. The clock is 30 Hz.

use std::f64::consts::PI;

use crate::{Error, Result};

/// The runtime animation clock (`func_1507BDB0` advances by 1.0 per two NTSC retraces).
pub const FPS: f32 = 30.0;
/// Root translation units per model unit.
pub const ROOT_SCALE: f32 = 1024.0;
/// Masked joint-translation units per model unit.
pub const JOINT_SCALE: f32 = 16.0;
/// Scale channel value of 1.0 (0 also means 1.0).
pub const SCALE_ONE: f32 = 16384.0;

/// `func_1505E0C4` copies descriptors into zeroed state: reads past the stored bytes see zeros.
const RUNTIME_ZERO_FILL: usize = 15;
const ROUTE_SENTINEL: u16 = 0x03E7;

/// One Euler-angle channel: `(base + (bits << 5)) & 0xFFFF`, plus an optional scale channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    pub base: u16,
    pub width: u8,
    /// (base, width) of the scale channel that rotation word bit 0x10 introduces.
    pub scale: Option<(u16, u8)>,
}

/// A clip's frame layout, from its descriptor segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Animation channel slots (descriptor byte 1 minus one).
    pub joints: usize,
    /// Signed high parts of the root translation (descriptor bytes 0, 2, 4), times 0x1000.
    pub root_base: [i8; 3],
    pub root_width: [u8; 3],
    /// Joints with an s16 x/y/z translation at the start of every frame.
    pub translated: Vec<bool>,
    /// Three per joint (x, y, z).
    pub channels: Vec<Channel>,
    /// Bytes per stored frame (descriptor byte 5).
    pub stride: usize,
    /// Clip length in ticks (descriptor byte 3); 0xFF is replaced at run time.
    pub duration: u8,
    /// Ticks between stored frames (descriptor byte 8 plus one).
    pub step: u8,
    /// Root motion (descriptor byte 7 bit 0x40, its s16 at byte 10): the root z channel moves the
    /// character instead of the pose, and this is its value at the loop point (`func_150A81D0` keeps it
    /// in `D_80088558`).
    pub root_end: Option<i16>,
}

impl Layout {
    pub fn parse(descriptor: &[u8]) -> Result<Self> {
        let d = descriptor;
        if d.len() < 10 || d[1] < 2 {
            return Err(Error::Malformed("animation descriptor too short"));
        }
        let mut rt = d.to_vec();
        rt.resize(d.len() + RUNTIME_ZERO_FILL, 0);
        let joints = d[1] as usize - 1;
        let flags = d[7] & 0xF0;
        let mut off = 10 + 2 * (flags & 0x40 != 0) as usize + 2 * (flags & 0x20 != 0) as usize;
        if off > d.len() {
            return Err(Error::Malformed("animation descriptor header truncated"));
        }
        let mut translated = vec![false; joints];
        if flags & 0x80 != 0 {
            let size = joints.div_ceil(16) * 2;
            let mask = rt
                .get(off..off + size)
                .ok_or(Error::Malformed("animation translation mask truncated"))?;
            for (i, t) in translated.iter_mut().enumerate() {
                *t = mask[i / 8] >> (7 - i % 8) & 1 != 0;
            }
            off += size;
        }
        let root_width = [d[6] >> 4, d[6] & 0xF, d[7] & 0xF];
        let mut bits: usize = root_width.iter().map(|&w| w as usize).sum();
        let word = |off: &mut usize| {
            let w = rt
                .get(*off..*off + 2)
                .map(|b| u16::from_be_bytes([b[0], b[1]]))
                .ok_or(Error::Malformed("animation channel table truncated"));
            *off += 2;
            w
        };
        let mut channels = Vec::with_capacity(joints * 3);
        for _ in 0..joints * 3 {
            let w = word(&mut off)?;
            let scale = if w & 0x10 != 0 {
                let s = word(&mut off)?;
                bits += (s & 0xF) as usize;
                Some((s & 0xFFF0, (s & 0xF) as u8))
            } else {
                None
            };
            bits += (w & 0xF) as usize;
            channels.push(Channel {
                base: w & 0xFFE0,
                width: (w & 0xF) as u8,
                scale,
            });
        }
        let padding = d.get(off..).unwrap_or_default();
        if padding.len() > 7 || padding.iter().any(|&b| b != 0) {
            return Err(Error::Malformed("animation descriptor has trailing data"));
        }
        let stride = d[5] as usize;
        if stride < translated.iter().filter(|&&t| t).count() * 6 + bits.div_ceil(8) {
            return Err(Error::Malformed(
                "animation frame stride smaller than its channels",
            ));
        }
        Ok(Self {
            joints,
            root_base: [d[0] as i8, d[2] as i8, d[4] as i8],
            root_width,
            translated,
            channels,
            stride,
            duration: d[3],
            step: d[8] + 1,
            root_end: (flags & 0x40 != 0).then(|| i16::from_be_bytes([d[10], d[11]])),
        })
    }

    /// Stored frames `func_1502D824` reads for a clip of `duration` ticks.
    pub fn frame_count(&self, duration: u16) -> usize {
        let (d, s) = (duration as usize, self.step as usize);
        d.saturating_sub(1).div_ceil(s) + 1
    }

    /// Decodes one stored frame (`data` is at least `stride` bytes; bytes past the channels are
    /// ignored).
    pub fn frame(&self, data: &[u8]) -> Result<Frame> {
        let data = data
            .get(..self.stride)
            .ok_or(Error::Malformed("animation frame truncated"))?;
        let mut off = 0;
        let translations = self
            .translated
            .iter()
            .map(|&t| {
                t.then(|| {
                    let v = std::array::from_fn(|k| {
                        i16::from_be_bytes([data[off + 2 * k], data[off + 2 * k + 1]])
                    });
                    off += 6;
                    v
                })
            })
            .collect();
        let mut bits = Bits {
            data: &data[off..],
            pos: 0,
        };
        let root = std::array::from_fn(|k| {
            self.root_base[k] as i32 * 0x1000 + (bits.read(self.root_width[k]) << 5) as i32
        });
        let mut rotations = Vec::with_capacity(self.joints);
        let mut scales = Vec::with_capacity(self.channels.len());
        let mut angle = [0u16; 3];
        for (i, c) in self.channels.iter().enumerate() {
            angle[i % 3] = (c.base as u32 + (bits.read(c.width) << 5)) as u16;
            scales.push(
                c.scale
                    .map(|(base, w)| (base as u32 + (bits.read(w) << 5)) as u16),
            );
            if i % 3 == 2 {
                rotations.push(angle);
            }
        }
        Ok(Frame {
            root,
            translations,
            rotations,
            scales,
        })
    }
}

/// MSB-first bit reader; reads past the end return zeros (the stride check in [`Layout::parse`]
/// guarantees the channels fit).
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn read(&mut self, width: u8) -> u32 {
        (0..width).fold(0, |v, _| {
            let bit = self
                .data
                .get(self.pos / 8)
                .map_or(0, |b| b >> (7 - self.pos % 8) & 1);
            self.pos += 1;
            v << 1 | bit as u32
        })
    }
}

/// One decoded pose; vectors are indexed by animation channel slot ([`crate::Joint::anim`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Root translation in 1/[`ROOT_SCALE`] units, added to the root joint.
    pub root: [i32; 3],
    /// Per-joint translation in 1/[`JOINT_SCALE`] units, added to the joint's local translation.
    pub translations: Vec<Option<[i16; 3]>>,
    /// Per-joint encoded Euler angles x, y, z (see [`quaternion`]).
    pub rotations: Vec<[u16; 3]>,
    /// Per channel (joint * 3 + axis): None or 0 is unit scale, else value / [`SCALE_ONE`].
    pub scales: Vec<Option<u16>>,
}

/// A decoded clip: the (descriptor, stream) pair `pair` of a bank-02 entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub pair: usize,
    /// Logical animation ids routed to this pair (bank 0x0F); the game plays clips by these.
    pub logical: Vec<u16>,
    pub joints: usize,
    /// Length in 30 Hz ticks.
    pub duration: u16,
    /// Ticks between stored frames.
    pub step: u16,
    /// Empty when the stream segment is empty.
    pub frames: Vec<Frame>,
    /// Root motion: the root z channel's raw value per stored frame (without the descriptor's base) and
    /// its loop-point value ([`Layout::root_end`]); `None` for clips that move by speed.
    pub root_motion: Option<RootMotion>,
}

/// A clip's root-z track, in the channel's raw units (32 per model unit): the game moves the character
/// along its facing by the track's advance (`func_1502DB84`, `func_15059C84`).
#[derive(Clone, Debug, PartialEq)]
pub struct RootMotion {
    pub z: Vec<i32>,
    pub end: i16,
}

impl Clip {
    /// Decodes a pair. `ff_duration` is what descriptor duration 0xFF means for this character
    /// (see [`ff_duration`]).
    pub fn decode(
        pair: usize,
        logical: Vec<u16>,
        descriptor: &[u8],
        stream: &[u8],
        ff_duration: u16,
    ) -> Result<Self> {
        let layout = Layout::parse(descriptor)?;
        let duration = match layout.duration {
            0xFF => ff_duration,
            d => d as u16,
        };
        let frames = if stream.is_empty() {
            Vec::new()
        } else {
            if layout.stride == 0 || duration == 0 {
                return Err(Error::Malformed("animation stream without frames"));
            }
            (0..layout.frame_count(duration))
                .map(|i| layout.frame(stream.get(i * layout.stride..).unwrap_or_default()))
                .collect::<Result<_>>()?
        };
        let root_motion = layout.root_end.map(|end| RootMotion {
            z: frames
                .iter()
                .map(|f| (f.root[2] - layout.root_base[2] as i32 * 0x1000) >> 5)
                .collect(),
            end,
        });
        Ok(Self {
            pair,
            logical,
            joints: layout.joints,
            duration,
            step: layout.step as u16,
            frames,
            root_motion,
        })
    }

    /// Key times in seconds: frame i at tick `i * step`, the last clamped to `duration - 1`
    /// (the final interval is shortened). The clip lasts `duration / 30` s.
    pub fn times(&self) -> Vec<f32> {
        let n = self.frames.len();
        (0..n)
            .map(|i| {
                let tick = i * self.step as usize;
                let tick = if i + 1 == n {
                    tick.min(self.duration as usize - 1)
                } else {
                    tick
                };
                tick as f32 / FPS
            })
            .collect()
    }
}

/// glTF rotation (x, y, z, w) of encoded Euler angles: s16 full turns (low 5 bits dropped),
/// composed z * y * x as `func_150A8918` builds the row-vector matrix.
pub fn quaternion(angles: [u16; 3]) -> [f32; 4] {
    let half = angles.map(|a| (a & 0xFFE0) as i16 as f64 * PI / 65536.0);
    let [(sx, cx), (sy, cy), (sz, cz)] = half.map(f64::sin_cos);
    let q = [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ];
    let len = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| (v / len) as f32)
}

/// One bank-0x0F route: logical animation id (its index) to a bank-02 pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub pair: usize,
    /// Index into the runtime metadata table `func_1505E0C4` reads.
    pub runtime_index: u16,
    pub aux_offset: u32,
}

/// Parses a route table (`func_1505E650`): 8-byte records from 0x10 until segment 0x3E7.
pub fn parse_routes(data: &[u8]) -> Result<Vec<Route>> {
    let mut routes = Vec::new();
    for r in data.get(0x10..).unwrap_or_default().as_chunks::<8>().0 {
        let segment = u16::from_be_bytes([r[0], r[1]]);
        if segment == ROUTE_SENTINEL {
            return Ok(routes);
        }
        if segment & 1 != 0 {
            return Err(Error::Malformed("route selects an odd segment"));
        }
        routes.push(Route {
            pair: segment as usize / 2,
            runtime_index: u16::from_be_bytes([r[2], r[3]]),
            aux_offset: u32::from_be_bytes([r[4], r[5], r[6], r[7]]),
        });
    }
    Err(Error::Malformed("route table has no sentinel"))
}

/// `.game` data addresses (loaded at [`crate::GAME_DATA_VRAM`]).
const GROUP_COUNTS: usize = 0x8009_8888;
const GROUP_POINTERS: usize = 0x8008_4410;
const DURATIONS_0X92: usize = 0x8009_93F8;
const DURATION_0XB7: usize = 0x8009_953C;

pub(crate) fn data_at(game_data: &[u8], vram: usize, len: usize) -> Result<&[u8]> {
    crate::get(game_data, vram.wrapping_sub(crate::GAME_DATA_VRAM), len)
}

/// The bank-02 / bank-0x0F entry that animates bank-01 character `model` (`func_1503D5F0`): the
/// first member of the one of five shared groups that holds it, else the model itself. The game
/// keeps it in character byte +6 (read by `func_1505E0C4`).
pub fn animation_entry(game_data: &[u8], model: usize) -> Result<usize> {
    let counts = data_at(game_data, GROUP_COUNTS, 5)?;
    for (g, &count) in counts.iter().enumerate() {
        if count == 0 {
            continue; // `blez` skips the group without reading its pointer
        }
        let ptr = crate::be32(data_at(game_data, GROUP_POINTERS + 4 * g, 4)?, 0)? as usize;
        let members = data_at(game_data, ptr, count as usize)?;
        if members.iter().any(|&m| m as usize == model) {
            return Ok(members[0] as usize);
        }
    }
    Ok(model)
}

/// What descriptor duration 0xFF means (`func_1505E0C4`): a per-logical-id u16 table for animation
/// entry 0x92, an f32 constant for 0xB7-0xB9, otherwise 255 ticks.
pub fn ff_duration(game_data: &[u8], anim_entry: usize, logical: &[u16]) -> Result<u16> {
    match anim_entry {
        0x92 => {
            let mut durations = Vec::new();
            for &id in logical {
                let b = data_at(game_data, DURATIONS_0X92 + 2 * id as usize, 2)?;
                durations.push(u16::from_be_bytes([b[0], b[1]]));
            }
            match durations.split_first() {
                Some((&d, rest)) if rest.iter().all(|&r| r == d) => Ok(d),
                Some(_) => Err(Error::Malformed("0x92 logical ids disagree on duration")),
                None => Err(Error::Malformed("0x92 duration needs a logical id")),
            }
        }
        0xB7..=0xB9 => {
            let b = data_at(game_data, DURATION_0XB7, 4)?;
            let d = f32::from_be_bytes(b.try_into().unwrap());
            if d.fract() != 0.0 || !(1.0..=65535.0).contains(&d) {
                return Err(Error::Malformed("0xB7 duration is not a tick count"));
            }
            Ok(d as u16)
        }
        _ => Ok(255),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packs `(value, width)` fields MSB-first.
    fn pack(fields: &[(u32, u8)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut pos = 0;
        for &(v, w) in fields {
            for b in (0..w).rev() {
                if pos % 8 == 0 {
                    out.push(0);
                }
                *out.last_mut().unwrap() |= ((v >> b & 1) as u8) << (7 - pos % 8);
                pos += 1;
            }
        }
        out
    }

    /// Two joints, translation mask on joint 1, scale channel on joint 0's y.
    fn descriptor() -> Vec<u8> {
        // root bases x=-1, y=0, z=2; duration 10; stride 12; root widths 3, 0, 2;
        // flags 0x80 (mask); step 4.
        let mut d = vec![0xFF, 3, 0, 10, 2, 12, 0x30, 0x82, 3, 0];
        d.extend([0x40, 0x00]); // mask: joint 1
        let words: [u16; 6] = [0x0002, 0x8011, 0x0000, 0xFFE1, 0x0000, 0x0000];
        for (i, w) in words.iter().enumerate() {
            d.extend(w.to_be_bytes());
            if i == 1 {
                d.extend(0x4002u16.to_be_bytes()); // scale base 0x4000, width 2
            }
        }
        d
    }

    #[test]
    fn layout() {
        let l = Layout::parse(&descriptor()).unwrap();
        assert_eq!(l.joints, 2);
        assert_eq!(l.root_base, [-1, 0, 2]);
        assert_eq!(l.root_width, [3, 0, 2]);
        assert_eq!(l.translated, [false, true]);
        assert_eq!(
            l.channels[0],
            Channel {
                base: 0,
                width: 2,
                scale: None
            }
        );
        assert_eq!(
            l.channels[1],
            Channel {
                base: 0x8000,
                width: 1,
                scale: Some((0x4000, 2))
            }
        );
        assert_eq!(
            l.channels[3],
            Channel {
                base: 0xFFE0,
                width: 1,
                scale: None
            }
        );
        assert_eq!((l.stride, l.duration, l.step), (12, 10, 4));
        // A 10-tick clip keys ticks 0, 4, 8 and 12 clamped to 9; 9 ticks: 0, 4, 8; 8: 0, 4, 7.
        assert_eq!([10, 9, 8, 1].map(|d| l.frame_count(d)), [4, 3, 3, 1]);
    }

    #[test]
    fn frame_decode() {
        let l = Layout::parse(&descriptor()).unwrap();
        // Joint 1 translation, then root x (3 bits), z (2), channels: x0 (2), y0 (1) + scale (2),
        // z0 (0), x1 (1), y1 (0), z1 (0).
        let mut data = [0i16, -16, 32]
            .iter()
            .flat_map(|v| v.to_be_bytes())
            .collect::<Vec<_>>();
        data.extend(pack(&[(5, 3), (3, 2), (2, 2), (1, 1), (3, 2), (1, 1)]));
        data.resize(12, 0xAA); // tail bytes are ignored
        let f = l.frame(&data).unwrap();
        assert_eq!(f.translations, [None, Some([0, -16, 32])]);
        assert_eq!(f.root, [-0x1000 + (5 << 5), 0, 0x2000 + (3 << 5)]);
        assert_eq!(f.rotations, [[2 << 5, 0x8020, 0], [0, 0, 0]]); // 0xFFE0 + 0x20 wraps to 0
        assert_eq!(f.scales, [None, Some(0x4060), None, None, None, None]);
        assert!(l.frame(&data[..11]).is_err());
    }

    #[test]
    fn rejects_bad_descriptors() {
        assert!(Layout::parse(&[0; 9]).is_err());
        let mut d = descriptor();
        d.extend([0, 1]); // non-zero trailing data
        assert!(Layout::parse(&d).is_err());
        let mut d = descriptor();
        d[5] = 4; // stride below 6 translation bytes + 2 packed bytes
        assert!(Layout::parse(&d).is_err());
        // Channel words past the stored bytes come from the runtime zero fill.
        let mut d = descriptor();
        d.truncate(d.len() - 4);
        assert_eq!(
            Layout::parse(&d).unwrap().channels[5],
            Channel {
                base: 0,
                width: 0,
                scale: None
            }
        );
    }

    #[test]
    fn clip_frames_and_times() {
        let d = descriptor();
        let stream = vec![0; 12 * 4 + 4]; // four frames and alignment padding
        let clip = Clip::decode(7, vec![3, 9], &d, &stream, 255).unwrap();
        assert_eq!(
            (clip.pair, clip.duration, clip.step, clip.frames.len()),
            (7, 10, 4, 4)
        );
        assert_eq!(clip.times(), [0.0, 4.0 / 30.0, 8.0 / 30.0, 9.0 / 30.0]);
        let mut short = d.clone();
        short[3] = 8; // last key clamped to tick 7
        assert_eq!(
            Clip::decode(0, vec![], &short, &stream, 255)
                .unwrap()
                .times()[2],
            7.0 / 30.0
        );
        let mut ff = d.clone();
        ff[3] = 0xFF;
        assert_eq!(
            Clip::decode(0, vec![], &ff, &[0; 12 * 2], 5)
                .unwrap()
                .frames
                .len(),
            2
        );
        assert!(Clip::decode(0, vec![], &ff, &stream, 255).is_err()); // 65 frames > stream
        assert!(
            Clip::decode(0, vec![], &d, &[], 255)
                .unwrap()
                .frames
                .is_empty()
        );
    }

    #[test]
    fn quaternions() {
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(quaternion([0, 0, 0]), [0.0, 0.0, 0.0, 1.0]);
        // A quarter turn is 0x4000; the low five bits are ignored.
        assert!(close(quaternion([0x401F, 0, 0]), [h, 0.0, 0.0, h]));
        assert!(close(quaternion([0, 0x4000, 0]), [0.0, h, 0.0, h]));
        assert!(close(quaternion([0, 0, 0xC000]), [0.0, 0.0, -h, h]));
        // x then y then z: q = qz * qy * qx.
        let q = quaternion([0x4000, 0x4000, 0]);
        assert!(close(q, [0.5, 0.5, -0.5, 0.5]));
    }

    #[test]
    fn routes() {
        let mut t = vec![0; 0x10];
        for (seg, idx, aux) in [(0x32u16, 1u16, 0u32), (0, 2, 0x40), (ROUTE_SENTINEL, 0, 0)] {
            t.extend(seg.to_be_bytes());
            t.extend(idx.to_be_bytes());
            t.extend(aux.to_be_bytes());
        }
        let r = parse_routes(&t).unwrap();
        assert_eq!(
            r,
            [
                Route {
                    pair: 25,
                    runtime_index: 1,
                    aux_offset: 0
                },
                Route {
                    pair: 0,
                    runtime_index: 2,
                    aux_offset: 0x40
                }
            ]
        );
        assert!(parse_routes(&t[..0x20]).is_err()); // no sentinel
        t[0x11] = 1;
        assert!(parse_routes(&t).is_err()); // odd segment
    }

    #[test]
    fn groups_and_durations() {
        let base = crate::GAME_DATA_VRAM;
        let mut g = vec![0u8; 0x17000];
        let mut put = |vram: usize, bytes: &[u8]| {
            g[vram - base..vram - base + bytes.len()].copy_from_slice(bytes)
        };
        put(GROUP_COUNTS, &[2, 0, 1, 0, 0]);
        put(GROUP_POINTERS, &0x8009_8860u32.to_be_bytes());
        put(GROUP_POINTERS + 8, &0x8009_8870u32.to_be_bytes());
        put(0x8009_8860, &[0, 117]);
        put(0x8009_8870, &[58]);
        put(DURATIONS_0X92 + 2 * 3, &40u16.to_be_bytes());
        put(DURATIONS_0X92 + 2 * 4, &40u16.to_be_bytes());
        put(DURATION_0XB7, &90f32.to_be_bytes());
        assert_eq!(
            [117, 0, 58, 5].map(|m| animation_entry(&g, m).unwrap()),
            [0, 0, 58, 5]
        );
        assert_eq!(ff_duration(&g, 0x92, &[3, 4]).unwrap(), 40);
        assert!(ff_duration(&g, 0x92, &[3, 5]).is_err());
        assert!(ff_duration(&g, 0x92, &[]).is_err());
        assert_eq!(ff_duration(&g, 0xB8, &[]).unwrap(), 90);
        assert_eq!(ff_duration(&g, 0, &[]).unwrap(), 255);
    }
}
