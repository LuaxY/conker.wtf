//! Display-list walker for Conker's F3DEX2 variant (F3DEX2CBFD).
//!
//! Re-derived from documentation, not from GLideN64 (GPL-2): `lab/research/renderer-options.md`
//! §2, `lab/research/cbfd-recompiled.md` §6 and the RT64 patch they describe
//! (CBFD-Recompiled `recomp/rt64.patch`, MIT). Conker's data contracts (flat texture references,
//! CBFD normals, matrix slots, TLUT loads) are ported from DevOldSchool/conkers-bfd-decomp (MIT,
//! commit e2400b27dda6): `docs/rzip-assets.md` and `scripts/model_assets.py`
//! (`parse_model_geometry`). Every other command behaves as in the F3DEX2 GBI.

use std::collections::BTreeMap;

use crate::texture::{Format, Tlut};
use crate::{Error, Result};

/// Vertex cache slots: G_TRI4 indices are 5 bits and the CBFD normal array is 32 x 2 bytes.
pub const VERTEX_CACHE: usize = 32;
/// F3DEX2's display-list call stack depth.
const MAX_DEPTH: usize = 18;
/// Bounds the walk on looping branches in bad data.
const MAX_COMMANDS: usize = 1 << 20;

/// A G_SETTIMG source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Image {
    /// Conker's flat texture reference (`func_1510CE60`): a w1 with segment byte 0 holds the
    /// texture id (blob index, bits 0-21) and a mode (bits 22-23) choosing the bytes, see
    /// [`flat_data`].
    Flat { index: u32, mode: u8 },
    /// A segmented address bound at runtime (e.g. character face slots in segments 6, 7, 10, 11).
    Segment(u32),
}

/// The bytes a flat reference reads from texture `payload`: mode 0 = pixels from the start,
/// mode 1 = the 256-entry TLUT in the last 0x200 bytes, mode 2 = the 16-entry TLUT in the last 0x20.
pub fn flat_data(payload: &[u8], mode: u8) -> Option<&[u8]> {
    let tail = match mode {
        0 => return Some(payload),
        1 => 0x200,
        2 => 0x20,
        _ => return None,
    };
    payload.get(payload.len().checked_sub(tail)?..)
}

/// An RDP tile descriptor (G_SETTILE + G_SETTILESIZE); index 0 is s, 1 is t.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tile {
    /// `G_IM_FMT_*`: 0 RGBA, 1 YUV, 2 CI, 3 IA, 4 I.
    pub format: u8,
    /// `G_IM_SIZ_*`: 0 4-bit, 1 8-bit, 2 16-bit, 3 32-bit.
    pub size: u8,
    /// Row stride in 64-bit TMEM words.
    pub line: u16,
    /// TMEM address in 64-bit words.
    pub tmem: u16,
    pub palette: u8,
    /// Clamp/mirror bits: 1 mirror, 2 clamp.
    pub cm: [u8; 2],
    pub mask: [u8; 2],
    pub shift: [u8; 2],
    /// G_SETTILESIZE bounds, 10.2 fixed point.
    pub ul: [u16; 2],
    pub lr: [u16; 2],
}

/// The TLUT a colour-indexed tile reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Palette {
    pub image: Image,
    /// First entry used, counted from the start of that TLUT load (a CI4 tile can select a
    /// 16-entry bank of a 256-entry load).
    pub offset: u16,
}

/// Texture state of a draw: the G_TEXTURE tile and what the loads put in TMEM for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Texture {
    pub tile: Tile,
    /// G_TEXTURE mipmap levels minus one.
    pub level: u8,
    /// G_TEXTURE s/t scale, 0.16 fixed point.
    pub scale: [u16; 2],
    /// The image whose load starts at the tile's TMEM address; None for runtime-loaded TMEM.
    // ponytail: exact TMEM start match; mip levels or tiles inside a larger load get None.
    pub image: Option<Image>,
    /// Loaded by LOADBLOCK with dxt 0: odd rows are stored TMEM-ready (see [`crate::decode`]).
    pub interleaved: bool,
    pub palette: Option<Palette>,
}

impl Texture {
    /// Image width and height in texels, the extent to decode and to normalise UVs by. Width is
    /// the TMEM row stride (`line` words; G_SETTILESIZE is often narrower, e.g. 62 of 64);
    /// height is the G_SETTILESIZE bounds; either falls back to the wrap mask when unset.
    // ponytail: a t axis wrapping past its bounds (mask period > bounds) keeps the bounds;
    // track LOADBLOCK sizes if such materials need the full period.
    pub fn size(&self) -> [u32; 2] {
        let t = &self.tile;
        std::array::from_fn(|i| {
            if i == 0 && t.line != 0 {
                // 32-bit texels split across TMEM halves, so their line counts 16-bit units.
                (t.line as u32) << (4 - t.size.min(2))
            } else if t.ul[i] == 0 && t.lr[i] == 0 {
                1 << t.mask[i]
            } else {
                (t.lr[i].saturating_sub(t.ul[i]) / 4) as u32 + 1
            }
        })
    }

    /// Normalised glTF texture coordinates of a vertex's S10.5 `st`: G_TEXTURE scale, tile
    /// shift, then the tile origin. v is flipped (v = 1 - t / height) to match the upright images
    /// of [`Material::texture_png`]: v = 0 is the last row in memory. `1 - v` maps the same
    /// texels under every wrap mode (repeat, mirror, clamp).
    pub fn uv(&self, st: [i16; 2]) -> [f32; 2] {
        let size = self.size();
        let [u, v]: [f32; 2] = std::array::from_fn(|i| {
            let s = st[i] as f32 / 32.0 * self.scale[i] as f32 / 65536.0;
            let shift = self.tile.shift[i];
            let s = if shift <= 10 {
                s / (1 << shift) as f32
            } else {
                s * (1 << (16 - shift)) as f32
            };
            (s - self.tile.ul[i] as f32 / 4.0) / size[i] as f32
        });
        [u, 1.0 - v]
    }
}

/// Render state recorded for each draw. Words the list never set are 0 (inherited at runtime).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Material {
    /// None while G_TEXTURE is off.
    pub texture: Option<Texture>,
    /// G_SETCOMBINE: w0 & 0xFFFFFF, w1.
    pub combine: [u32; 2],
    /// Othermode high and low words.
    pub other_mode: [u32; 2],
    pub geometry_mode: u32,
    pub prim_color: u32,
    pub env_color: u32,
    /// A G_DL the resolver could not supply since the last othermode write: Conker's
    /// segment-8 lists are runtime render-state slots (OtherMode + EndDL).
    pub state_call: Option<u32>,
}

impl Material {
    /// Texel format of the texture. A 4- or 8-bit RGBA tile is colour-indexed: Conker loads
    /// indexed pixels through an RGBA16 transfer image and samples them through a TLUT.
    pub fn texture_format(&self) -> Option<Format> {
        let tile = self.texture?.tile;
        let tlut = if self.other_mode[0] >> 14 & 3 == 3 {
            Tlut::Ia16
        } else {
            Tlut::Rgba16
        };
        Some(match (tile.format, tile.size) {
            (0 | 2, 0) => Format::Ci4(tlut),
            (0 | 2, 1) => Format::Ci8(tlut),
            (0, 2) => Format::Rgba16,
            (0, 3) => Format::Rgba32,
            (3, 0) => Format::Ia4,
            (3, 1) => Format::Ia8,
            (3, 2) => Format::Ia16,
            (4, 0) => Format::I4,
            (4, 1) => Format::I8,
            _ => return None,
        })
    }
}

/// One vertex as loaded by a G_VTX (a source vertex loaded twice appears twice).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [i16; 3],
    pub flag: u16,
    /// S10.5 texel coordinates, see [`Texture::uv`].
    pub uv: [i16; 2],
    /// RGBA. CBFD keeps the colour a colour even when lit; normals live apart.
    pub color: [u8; 4],
    /// CBFD normal (G_MOVEMEM 0x0E): s8 x, y at `base + slot * 2`, z = low byte of `flag`.
    pub normal: Option<[i8; 3]>,
    /// The modelview G_MTX address active at load. Conker's characters use segment 3 with one
    /// 0x40-byte matrix per joint: slot = `(matrix & 0xFFFFFF) / 0x40`.
    // ponytail: records the last loaded or multiplied matrix, not the product of a MUL chain.
    pub matrix: Option<u32>,
    /// Segmented address the vertex was loaded from.
    pub address: u32,
}

/// Consecutive triangles sharing one material, in display-list order.
#[derive(Clone, Debug, PartialEq)]
pub struct Draw {
    pub material: Material,
    /// Indices into [`Ir::vertices`].
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ir {
    pub vertices: Vec<Vertex>,
    pub draws: Vec<Draw>,
    /// G_DL targets and normal arrays the resolver could not supply, in order.
    pub unresolved: Vec<u32>,
    /// G_TRI4 triangles skipped because they repeat an index (unused slots).
    pub degenerate: u32,
    /// Opcodes the walker does not interpret, with counts.
    pub skipped: BTreeMap<u8, u32>,
}

#[derive(Default)]
struct State<'a> {
    cache: [Option<u32>; VERTEX_CACHE],
    normals: Option<&'a [u8]>,
    matrix: Option<u32>,
    matrices: Vec<Option<u32>>,
    /// Everything but the texture, which is assembled per draw.
    material: Material,
    texture_on: bool,
    texture_tile: usize,
    level: u8,
    scale: [u16; 2],
    tiles: [Tile; 8],
    /// When each tile was last described (G_SETTILE), counted with the loads: see [`State::texture`].
    described: [u32; 8],
    /// When G_TEXTURE last chose its tile.
    chosen: u32,
    order: u32,
    image: Option<Image>,
    /// (TMEM address, image, interleaved, when)
    loads: Vec<(u16, Image, bool, u32)>,
    /// (TMEM address, entries, image)
    tluts: Vec<(u16, u16, Image)>,
}

impl State<'_> {
    fn texture(&self) -> Option<Texture> {
        if !self.texture_on {
            return None;
        }
        // A G_TEXTURE tile left from an earlier image: bank-09 #0x1A's (the twin gun) first image
        // describes tiles 1 and 0, G_TEXTURE picks 1, and the later images, loaded at the same TMEM
        // address, only describe tile 0 again. The RDP samples tile 1 (C: F3DEX2's triangles carry
        // G_TEXTURE's tile, RT64 `textureTile`), a window over the new load and the old one's
        // leftovers; the list's tile for the new image is the one described since, so take it.
        // Not when G_TEXTURE chose its tile after that one was described (bank-01 #0x45 and #0x00
        // describe tile 5, then choose tile 0: theirs is meant).
        let own = self.texture_tile;
        let at = self.tiles[own].tmem;
        let load = self.loads.iter().find(|l| l.0 == at);
        let newer = (0..8)
            .filter(|&k| {
                let t = &self.tiles[k];
                let since = self.described[own].max(self.chosen);
                k != own && t.tmem == at && t.line != 0 && self.described[k] > since
            })
            .max_by_key(|&k| self.described[k])
            .filter(|_| load.is_some_and(|l| l.3 > self.described[own]));
        let tile = self.tiles[newer.unwrap_or(own)];
        let indexed = tile.size <= 1 && matches!(tile.format, 0 | 2);
        let palette = if indexed {
            let addr = 0x100
                + if tile.size == 0 {
                    tile.palette as u16 * 16
                } else {
                    0
                };
            self.tluts
                .iter()
                .find(|t| (t.0..t.0 + t.1).contains(&addr))
                .map(|t| Palette {
                    image: t.2,
                    offset: addr - t.0,
                })
        } else {
            None
        };
        Some(Texture {
            tile,
            level: self.level,
            scale: self.scale,
            image: load.map(|l| l.1),
            interleaved: load.is_some_and(|l| l.2),
            palette,
        })
    }

    fn load(&mut self, w1: u32, interleaved: bool) {
        if let Some(image) = self.image {
            let tmem = self.tiles[(w1 >> 24 & 7) as usize].tmem;
            self.order += 1;
            self.loads.retain(|l| l.0 != tmem);
            self.loads.push((tmem, image, interleaved, self.order));
        }
    }

    fn triangle(&self, ir: &mut Ir, at: u32, slots: [usize; 3]) -> Result<()> {
        let mut tri = [0; 3];
        for (v, slot) in tri.iter_mut().zip(slots) {
            *v = self
                .cache
                .get(slot)
                .copied()
                .flatten()
                .ok_or(Error::Unloaded { at, slot })?;
        }
        let material = Material {
            texture: self.texture(),
            ..self.material
        };
        match ir.draws.last_mut() {
            Some(d) if d.material == material => d.triangles.push(tri),
            _ => ir.draws.push(Draw {
                material,
                triangles: vec![tri],
            }),
        }
        Ok(())
    }
}

fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// G_TRI1 indices are byte-sized slot * 2 in bits 16-23, 8-15, 0-7.
fn tri1(w: u32) -> [usize; 3] {
    [16, 8, 0].map(|s| (w >> s & 0xFF) as usize / 2)
}

/// The four triangles of a G_TRI4 (opcodes 0x10-0x1F: the opcode's low nibble holds index bits).
fn tri4(w0: u32, w1: u32) -> [[usize; 3]; 4] {
    let f = |w: u32, s: u32| (w >> s & 0x1F) as usize;
    [
        [
            f(w0, 23),
            f(w0, 18),
            ((w0 >> 15 & 7) as usize) << 2 | (w1 >> 30) as usize,
        ],
        [f(w0, 10), f(w0, 5), f(w0, 0)],
        [f(w1, 25), f(w1, 20), f(w1, 15)],
        [f(w1, 10), f(w1, 5), f(w1, 0)],
    ]
}

/// Walks the display list at segmented address `start`. `resolve` maps a segmented address to
/// the bytes from there to the end of its buffer. An unresolved G_DL is recorded and skipped (an
/// unresolved branch ends its list); an unresolved G_VTX is an error.
pub fn walk<'a>(start: u32, resolve: impl Fn(u32) -> Option<&'a [u8]>) -> Result<Ir> {
    let mut ir = Ir::default();
    let mut st = State::default();
    let mut stack: Vec<(u32, &'a [u8])> = Vec::new();
    let (mut pc, mut dl) = (start, resolve(start).ok_or(Error::Unresolved(start))?);
    for _ in 0..MAX_COMMANDS {
        let at = pc;
        let cmd = dl.get(..8).ok_or(Error::Truncated(at))?;
        let (w0, w1) = (be32(cmd, 0), be32(cmd, 4));
        dl = &dl[8..];
        pc = pc.wrapping_add(8);
        let mut end = false;
        match (w0 >> 24) as u8 {
            0x01 => {
                let n = (w0 >> 12 & 0xFF) as usize;
                let last = (w0 >> 1 & 0x7F) as usize;
                if n > last || last > VERTEX_CACHE {
                    return Err(Error::BadVertexLoad(at));
                }
                let src = resolve(w1).ok_or(Error::Unresolved(w1))?;
                let src = src.get(..n * 16).ok_or(Error::BadVertexLoad(at))?;
                for (i, v) in src.as_chunks::<16>().0.iter().enumerate() {
                    let slot = last - n + i;
                    let flag = be16(v, 6);
                    let normal = st
                        .normals
                        .and_then(|b| b.get(slot * 2..slot * 2 + 2))
                        .map(|b| [b[0] as i8, b[1] as i8, flag as u8 as i8]);
                    st.cache[slot] = Some(ir.vertices.len() as u32);
                    ir.vertices.push(Vertex {
                        position: [0, 2, 4].map(|o| be16(v, o) as i16),
                        flag,
                        uv: [8, 10].map(|o| be16(v, o) as i16),
                        color: [v[12], v[13], v[14], v[15]],
                        normal,
                        matrix: st.matrix,
                        address: w1.wrapping_add(16 * i as u32),
                    });
                }
            }
            0x05 => st.triangle(&mut ir, at, tri1(w0))?,
            0x06 | 0x07 => {
                st.triangle(&mut ir, at, tri1(w0))?;
                st.triangle(&mut ir, at, tri1(w1))?;
            }
            0x10..=0x1F => {
                for t in tri4(w0, w1) {
                    if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                        ir.degenerate += 1;
                    } else {
                        st.triangle(&mut ir, at, t)?;
                    }
                }
            }
            0xD7 => {
                st.texture_on = w0 >> 1 & 0x7F != 0;
                st.texture_tile = (w0 >> 8 & 7) as usize;
                st.chosen = st.order;
                st.level = (w0 >> 11 & 7) as u8;
                st.scale = [(w1 >> 16) as u16, w1 as u16];
            }
            0xD8 => {
                let keep = st.matrices.len().saturating_sub((w1 / 64) as usize);
                if keep < st.matrices.len() {
                    st.matrix = st.matrices[keep];
                    st.matrices.truncate(keep);
                }
            }
            0xD9 => st.material.geometry_mode = st.material.geometry_mode & (w0 & 0xFF_FFFF) | w1,
            0xDA => {
                // F3DEX2 stores the params with the push bit inverted.
                let params = (w0 & 0xFF) ^ 1;
                if params & 4 == 0 {
                    if params & 1 != 0 {
                        st.matrices.push(st.matrix);
                    }
                    st.matrix = Some(w1);
                }
            }
            0xDC if w0 & 0xFF == 0x0E => {
                st.normals = resolve(w1);
                if st.normals.is_none() {
                    ir.unresolved.push(w1);
                }
            }
            0xDE => {
                let call = w0 >> 16 & 0xFF == 0;
                match resolve(w1) {
                    Some(target) => {
                        if call {
                            if stack.len() == MAX_DEPTH {
                                return Err(Error::TooDeep(at));
                            }
                            stack.push((pc, dl));
                        }
                        (pc, dl) = (w1, target);
                    }
                    None => {
                        ir.unresolved.push(w1);
                        st.material.state_call = Some(w1);
                        end = !call;
                    }
                }
            }
            0xDF => end = true,
            op @ (0xE2 | 0xE3) => {
                let len = ((w0 & 0xFF) + 1).min(32);
                let shift = 32u32.saturating_sub((w0 >> 8 & 0xFF) + len);
                let mask = (u32::MAX >> (32 - len)) << shift;
                let word = &mut st.material.other_mode[(op == 0xE2) as usize];
                *word = *word & !mask | w1 & mask;
                st.material.state_call = None;
            }
            0xEF => {
                st.material.other_mode = [w0 & 0xFF_FFFF, w1];
                st.material.state_call = None;
            }
            0xF0 => {
                if let Some(image) = st.image {
                    let tmem = st.tiles[(w1 >> 24 & 7) as usize].tmem;
                    st.tluts.retain(|t| t.0 != tmem);
                    st.tluts.push((tmem, (w1 >> 14 & 0x3FF) as u16 + 1, image));
                }
            }
            0xF2 => {
                let t = &mut st.tiles[(w1 >> 24 & 7) as usize];
                t.ul = [(w0 >> 12 & 0xFFF) as u16, (w0 & 0xFFF) as u16];
                t.lr = [(w1 >> 12 & 0xFFF) as u16, (w1 & 0xFFF) as u16];
            }
            0xF3 => st.load(w1, w1 & 0xFFF == 0),
            0xF4 => st.load(w1, false),
            0xF5 => {
                st.order += 1;
                st.described[(w1 >> 24 & 7) as usize] = st.order;
                let t = &mut st.tiles[(w1 >> 24 & 7) as usize];
                *t = Tile {
                    format: (w0 >> 21 & 7) as u8,
                    size: (w0 >> 19 & 3) as u8,
                    line: (w0 >> 9 & 0x1FF) as u16,
                    tmem: (w0 & 0x1FF) as u16,
                    palette: (w1 >> 20 & 0xF) as u8,
                    cm: [(w1 >> 8 & 3) as u8, (w1 >> 18 & 3) as u8],
                    mask: [(w1 >> 4 & 0xF) as u8, (w1 >> 14 & 0xF) as u8],
                    shift: [(w1 & 0xF) as u8, (w1 >> 10 & 0xF) as u8],
                    ..*t
                };
            }
            0xFA => st.material.prim_color = w1,
            0xFB => st.material.env_color = w1,
            0xFC => st.material.combine = [w0 & 0xFF_FFFF, w1],
            0xFD => {
                st.image = Some(if w1 >> 24 == 0 {
                    Image::Flat {
                        index: w1 & 0x3F_FFFF,
                        mode: (w1 >> 22) as u8,
                    }
                } else {
                    Image::Segment(w1)
                });
            }
            0x00 | 0xE6..=0xE9 => {} // no-op and syncs
            op => *ir.skipped.entry(op).or_default() += 1,
        }
        if end {
            match stack.pop() {
                Some(ret) => (pc, dl) = ret,
                None => return Ok(ir),
            }
        }
    }
    Err(Error::TooLong)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(cmds: &[(u32, u32)]) -> Vec<u8> {
        cmds.iter()
            .flat_map(|&(w0, w1)| [w0.to_be_bytes(), w1.to_be_bytes()].concat())
            .collect()
    }

    /// Walks `segs[0]` from address 0; segment n of an address indexes `segs[n]`.
    fn run(segs: &[&[u8]]) -> Result<Ir> {
        walk(0, |a| {
            segs.get(a as usize >> 24)?.get(a as usize & 0xFF_FFFF..)
        })
    }

    /// `n` vertices: vertex i has x = i, flag = 0x100 + i, s = 32 * i, colour [i; 4].
    fn verts(n: usize) -> Vec<u8> {
        (0..n)
            .flat_map(|i| {
                let i = i as i16;
                let mut v = [0; 16];
                v[..2].copy_from_slice(&i.to_be_bytes());
                v[2..4].copy_from_slice(&(-i).to_be_bytes());
                v[4..6].copy_from_slice(&1000i16.to_be_bytes());
                v[6..8].copy_from_slice(&(0x100 + i).to_be_bytes());
                v[8..10].copy_from_slice(&(32 * i).to_be_bytes());
                v[10..12].copy_from_slice(&(-32 * i).to_be_bytes());
                v[12..].copy_from_slice(&[i as u8; 4]);
                v
            })
            .collect()
    }

    fn vtx(n: u32, dst: u32, addr: u32) -> (u32, u32) {
        (0x0100_0000 | n << 12 | (dst + n) << 1, addr)
    }

    fn tri(a: u32, b: u32, c: u32) -> u32 {
        (a * 2) << 16 | (b * 2) << 8 | (c * 2)
    }

    const END: (u32, u32) = (0xDF00_0000, 0);

    #[test]
    fn g_vtx_decode() {
        let v = verts(4);
        let dl = list(&[vtx(3, 2, 0x0100_0010), (0x0500_0000 | tri(2, 3, 4), 0), END]);
        let ir = run(&[&dl, &v]).unwrap();
        assert_eq!(ir.vertices.len(), 3);
        // Vertex 1 of the source lands in slot 2.
        assert_eq!(
            ir.vertices[0],
            Vertex {
                position: [1, -1, 1000],
                flag: 0x101,
                uv: [32, -32],
                color: [1; 4],
                normal: None,
                matrix: None,
                address: 0x0100_0010,
            }
        );
        assert_eq!(ir.vertices[2].address, 0x0100_0030);
        assert_eq!(ir.draws[0].triangles, [[0, 1, 2]]);

        // Past the 32-slot cache, past the source data, or unresolved.
        let bad = list(&[vtx(2, 31, 0x0100_0000), END]);
        assert!(matches!(run(&[&bad, &v]), Err(Error::BadVertexLoad(0))));
        let short = list(&[vtx(5, 0, 0x0100_0000), END]);
        assert!(matches!(run(&[&short, &v]), Err(Error::BadVertexLoad(0))));
        let missing = list(&[vtx(1, 0, 0x0700_0000), END]);
        assert!(matches!(
            run(&[&missing]),
            Err(Error::Unresolved(0x0700_0000))
        ));
    }

    #[test]
    fn g_tri1_tri2() {
        let v = verts(4);
        let dl = list(&[
            vtx(4, 0, 0x0100_0000),
            (0x0500_0000 | tri(0, 1, 2), 0),
            (0x0600_0000 | tri(1, 2, 3), tri(3, 2, 0)),
            (0x0700_0000 | tri(0, 1, 3), tri(0, 3, 2)), // G_QUAD: two triangles like G_TRI2
            END,
        ]);
        let ir = run(&[&dl, &v]).unwrap();
        assert_eq!(ir.draws.len(), 1);
        assert_eq!(
            ir.draws[0].triangles,
            [[0, 1, 2], [1, 2, 3], [3, 2, 0], [0, 1, 3], [0, 3, 2]]
        );

        let unloaded = list(&[vtx(2, 0, 0x0100_0000), (0x0500_0000 | tri(0, 1, 5), 0), END]);
        assert!(matches!(
            run(&[&unloaded, &v]),
            Err(Error::Unloaded { at: 8, slot: 5 })
        ));
    }

    #[test]
    fn g_tri4_packing() {
        // Hand-packed: tri 0 = (17, 2, 31) uses the opcode nibble (17 << 23) and splits 31 as
        // w0[17:15] = 7, w1[31:30] = 3; then (3, 4, 5) in w0, (6, 7, 8) and (9, 10, 11) in w1.
        let (w0, w1) = (0x188B_8C85, 0xCC74_254B);
        assert_eq!(
            tri4(w0, w1),
            [[17, 2, 31], [3, 4, 5], [6, 7, 8], [9, 10, 11]]
        );
        let v = verts(32);
        let dl = list(&[vtx(32, 0, 0x0100_0000), (w0, w1), END]);
        let ir = run(&[&dl, &v]).unwrap();
        assert_eq!(
            ir.draws[0].triangles,
            [[17, 2, 31], [3, 4, 5], [6, 7, 8], [9, 10, 11]]
        );
        assert_eq!(ir.degenerate, 0);

        // A repeated index marks an unused slot: (9, 9, 9) in the last position.
        let dl = list(&[vtx(32, 0, 0x0100_0000), (w0, 0xCC74_2529), END]);
        let ir = run(&[&dl, &v]).unwrap();
        assert_eq!(ir.draws[0].triangles.len(), 3);
        assert_eq!(ir.degenerate, 1);
    }

    #[test]
    fn cbfd_normals() {
        let mut v = verts(2);
        v[7] = 0xFE; // vertex 0 flag low byte: nz = -2
        let normals: Vec<u8> = (0..32u8).flat_map(|k| [k, k.wrapping_neg()]).collect();
        let dl = list(&[
            vtx(1, 0, 0x0100_0000),
            (0xDC38_000E, 0x0200_0000),
            vtx(2, 5, 0x0100_0000),
            (0xDC38_000E, 0x0800_0000),
            END,
        ]);
        let ir = run(&[&dl, &v, &normals]).unwrap();
        assert_eq!(ir.vertices[0].normal, None);
        assert_eq!(ir.vertices[1].normal, Some([5, -5, -2]));
        assert_eq!(ir.vertices[2].normal, Some([6, -6, 1]));
        assert_eq!(ir.unresolved, [0x0800_0000]);
    }

    #[test]
    fn matrices() {
        let v = verts(1);
        let dl = list(&[
            vtx(1, 0, 0x0100_0000),
            (0xDA38_0003, 0x0300_0040), // load, no push
            vtx(1, 0, 0x0100_0000),
            (0xDA38_0002, 0x0300_0080), // load + push
            vtx(1, 0, 0x0100_0000),
            (0xDA38_0007, 0x0400_0000), // projection: ignored
            (0xD838_0002, 0x40),        // pop one
            vtx(1, 0, 0x0100_0000),
            (0xD838_0002, 0x40), // pop on an empty stack: no-op
            vtx(1, 0, 0x0100_0000),
            END,
        ]);
        let ir = run(&[&dl, &v]).unwrap();
        let m: Vec<_> = ir.vertices.iter().map(|v| v.matrix).collect();
        assert_eq!(
            m,
            [
                None,
                Some(0x0300_0040),
                Some(0x0300_0080),
                Some(0x0300_0040),
                Some(0x0300_0040)
            ]
        );
    }

    #[test]
    fn calls_branches_and_unresolved() {
        let v = verts(3);
        // Segment 2: a callee that draws (0, 1, 2) and returns.
        let callee = list(&[(0x0500_0000 | tri(0, 1, 2), 0), END]);
        // Segment 3: a branch target that draws (2, 1, 0) and ends the walk.
        let target = list(&[(0x0500_0000 | tri(2, 1, 0), 0), END]);
        let dl = list(&[
            vtx(3, 0, 0x0100_0000),
            (0xDE00_0000, 0x0200_0000), // call
            (0xDE00_0000, 0x0800_0010), // unresolved call: recorded, skipped
            (0x0500_0000 | tri(0, 2, 1), 0),
            (0xEF00_0000, 0), // othermode write clears the pending state call
            (0x0500_0000 | tri(1, 0, 2), 0),
            (0xDE01_0000, 0x0300_0000), // branch: never returns
            (0x0500_0000 | tri(0, 0, 0), 0),
            END,
        ]);
        let ir = run(&[&dl, &v, &callee, &target]).unwrap();
        let tris: Vec<_> = ir.draws.iter().flat_map(|d| d.triangles.clone()).collect();
        assert_eq!(tris, [[0, 1, 2], [0, 2, 1], [1, 0, 2], [2, 1, 0]]);
        assert_eq!(ir.unresolved, [0x0800_0010]);
        let calls: Vec<_> = ir.draws.iter().map(|d| d.material.state_call).collect();
        assert_eq!(calls, [None, Some(0x0800_0010), None]);

        // An unresolved branch ends its list.
        let dl = list(&[(0xDE01_0000, 0x0800_0000), (0xFF00_0000, 0), END]);
        let ir = run(&[&dl]).unwrap();
        assert!(ir.skipped.is_empty());

        let recursive = list(&[(0xDE00_0000, 0), END]);
        assert!(matches!(run(&[&recursive]), Err(Error::TooDeep(0))));
        let looping = list(&[(0xDE01_0000, 0), END]);
        assert!(matches!(run(&[&looping]), Err(Error::TooLong)));
        assert!(matches!(
            run(&[&list(&[vtx(0, 0, 0)])]),
            Err(Error::Truncated(8))
        ));
    }

    #[test]
    fn material_state() {
        let v = verts(3);
        let draw = (0x0500_0000 | tri(0, 1, 2), 0);
        let dl = list(&[
            vtx(3, 0, 0x0100_0000),
            (0xFD10_0000, 0x0000_00BB), // SETTIMG: flat 0xBB, mode 0 (pixels)
            (0xF510_0000, 0x0700_0000), // SETTILE 7 (load tile), tmem 0
            (0xF300_0000, 0x077F_F000), // LOADBLOCK 2048 texels, dxt 0
            (0xFD10_0000, 0x0080_00BB), // SETTIMG: flat 0xBB, mode 2 (16-entry TLUT)
            (0xF500_0100, 0x0700_0000), // SETTILE 7, tmem 0x100
            (0xF000_0000, 0x0703_C000), // LOADTLUT 16 entries
            (0xF500_0800, 0x0019_8160), // SETTILE 0: RGBA 4b, line 4, palette 1, cm 1/2, masks 6/6
            (0xF200_0000, 0x000F_C0FC), // SETTILESIZE 0: 64 x 64
            (0xD700_0002, 0xFFFF_FFFF), // G_TEXTURE on, tile 0
            (0xFC12_1824, 0xFF33_FFFF), // SETCOMBINE
            (0xEF00_2C00, 0x0055_2078), // RDPSETOTHERMODE
            (0xE300_0E02, 0x0000_8000), // othermode H: TEXTLUT (shift 14, len 2) = RGBA16
            (0xD9FF_FFFF, 0x0000_0405), // geometry: set
            (0xFA00_0000, 0x1122_3344), // prim colour
            (0xFB00_0000, 0x5566_7788), // env colour
            draw,
            draw,             // same material: merged
            (0xD9FF_FBFF, 0), // clear G_CULL_BACK
            draw,
            (0xD700_0000, 0), // texturing off
            draw,
            (0xDB06_0000, 0), // G_MOVEWORD: not interpreted
            END,
        ]);
        let ir = run(&[&dl, &v]).unwrap();
        assert_eq!(ir.draws.len(), 3);
        assert_eq!(ir.draws[0].triangles.len(), 2);
        assert_eq!(ir.skipped, BTreeMap::from([(0xDB, 1)]));

        let m = ir.draws[0].material;
        let t = m.texture.unwrap();
        assert_eq!(
            t.image,
            Some(Image::Flat {
                index: 0xBB,
                mode: 0
            })
        );
        assert!(t.interleaved);
        // The tile's palette 1 is not covered by a 16-entry TLUT at 0x100.
        assert_eq!(t.palette, None);
        assert_eq!(
            t.tile,
            Tile {
                format: 0,
                size: 0,
                line: 4,
                tmem: 0,
                palette: 1,
                cm: [1, 2],
                mask: [6, 6],
                shift: [0, 0],
                ul: [0, 0],
                lr: [0xFC, 0xFC],
            }
        );
        assert_eq!(t.size(), [64, 64]);
        assert_eq!(t.scale, [0xFFFF, 0xFFFF]);
        assert_eq!(m.texture_format(), Some(Format::Ci4(Tlut::Rgba16)));
        let ia16 = Material {
            other_mode: [0xC000, 0],
            ..m
        }; // TEXTLUT = 3
        assert_eq!(ia16.texture_format(), Some(Format::Ci4(Tlut::Ia16)));
        assert_eq!(m.combine, [0x12_1824, 0xFF33_FFFF]);
        // EF set the high word to 0x2C00; E3 then wrote TEXTLUT (bits 14-15) = 2.
        assert_eq!(m.other_mode, [0xAC00, 0x0055_2078]);
        assert_eq!(m.geometry_mode, 0x405);
        assert_eq!((m.prim_color, m.env_color), (0x1122_3344, 0x5566_7788));
        assert_eq!(ir.draws[1].material.geometry_mode, 0x005);
        assert_eq!(ir.draws[2].material.texture, None);
    }

    /// Bank-09 #0x1A's sequence (the twin gun): G_TEXTURE tile 1, tiles 1 and 0 described for a 64 x
    /// 32 CI8 image, then 16 x 16 images loaded at the same TMEM address with only tile 0 described
    /// again (once, for the first of them): those draws take tile 0.
    #[test]
    fn a_stale_texture_tile_gives_way_to_the_newer_one() {
        let v = verts(3);
        let draw = (0x0500_0000 | tri(0, 1, 2), 0);
        let tile_64x32 = |t: u32| (0xF508_1000, t << 24 | 0x0001_4060); // CI8, line 8, masks 6/5
        let load = |image: u32, texels: u32| {
            [
                (0xFD10_0000, image),                            // SETTIMG flat, mode 0
                (0xF300_0000, 0x0700_0000 | (texels - 1) << 12), // LOADBLOCK tile 7, dxt 0
                (0xFD10_0000, 0x0040_0000 | image),              // SETTIMG flat, mode 1 (TLUT)
                (0xF000_0000, 0x063F_C000),                      // LOADTLUT tile 6, 256 entries
            ]
        };
        let mut cmds = vec![
            vtx(3, 0, 0x0100_0000),
            (0xD700_0902, 0xFFFF_FFFF), // G_TEXTURE level 1, tile 1, on
            tile_64x32(1),
            (0xF510_0000, 0x0700_0000), // SETTILE 7: the load tile, line 0, tmem 0
            (0xF500_0100, 0x0600_0000), // SETTILE 6: the TLUT's, tmem 0x100
        ];
        cmds.extend(load(0xAAA, 1024));
        cmds.extend([tile_64x32(0), draw]);
        cmds.extend(load(0xBBB, 128));
        cmds.extend([(0xF508_0400, 0x0001_0040), draw]); // SETTILE 0: CI8, line 2, masks 4/4
        cmds.extend(load(0xCCC, 128));
        cmds.extend([draw, END]);
        let ir = run(&[&list(&cmds), &v]).unwrap();
        let t: Vec<_> = (ir.draws.iter())
            .map(|d| d.material.texture.unwrap())
            .collect();
        let image = |index| Some(Image::Flat { index, mode: 0 });
        assert_eq!(t.len(), 3);
        assert_eq!(
            (t[0].image, t[0].tile.line, t[0].size()),
            (image(0xAAA), 8, [64, 32])
        );
        for (t, index) in t[1..].iter().zip([0xBBB, 0xCCC]) {
            assert_eq!(
                (t.image, t.tile.line, t.size()),
                (image(index), 2, [16, 16])
            );
            assert_eq!(
                t.palette.map(|p| p.image),
                Some(Image::Flat { index, mode: 1 })
            );
        }
    }

    #[test]
    fn load_tile_and_othermode_low() {
        let v = verts(3);
        let dl = list(&[
            vtx(3, 0, 0x0100_0000),
            (0xFD10_0000, 0x0600_0000), // SETTIMG: segment 6 (runtime-bound)
            (0xF510_1000, 0x0600_0000), // SETTILE 6: RGBA16, line 8, tmem 0
            (0xF400_0000, 0x0607_C07C), // LOADTILE: linear, never interleaved
            (0xF510_1000, 0),           // SETTILE 0: RGBA16, line 8 (32 texels)
            (0xD700_0002, 0xFFFF_FFFF),
            (0xEF00_0000, 0xFFFF_FFFF), // RDPSETOTHERMODE
            (0xE200_1803, 0x0000_0050), // othermode L: shift 4, len 4 := 5
            (0x0500_0000 | tri(0, 1, 2), 0),
            END,
        ]);
        let ir = run(&[&dl, &v]).unwrap();
        let m = ir.draws[0].material;
        let t = m.texture.unwrap();
        assert_eq!(t.image, Some(Image::Segment(0x0600_0000)));
        assert!(!t.interleaved);
        assert_eq!(m.texture_format(), Some(Format::Rgba16));
        assert_eq!(t.size()[0], 32);
        assert_eq!(m.other_mode, [0, 0xFFFF_FF5F]);
    }

    #[test]
    fn palettes() {
        let v = verts(3);
        let setup = |tile_w1: u32, tlut_count: u32| {
            list(&[
                vtx(3, 0, 0x0100_0000),
                (0xFD10_0000, 0x0040_0007), // flat 7, mode 1 (256-entry TLUT)
                (0xF500_0100, 0x0700_0000), // SETTILE 7, tmem 0x100
                (0xF000_0000, 0x0700_0000 | (tlut_count - 1) << 14),
                (0xF540_0800, tile_w1), // SETTILE 0: 4-bit
                (0xD700_0002, 0xFFFF_FFFF),
                (0x0500_0000 | tri(0, 1, 2), 0),
                END,
            ])
        };
        let palette = |dl: Vec<u8>| {
            run(&[&dl, &v]).unwrap().draws[0]
                .material
                .texture
                .unwrap()
                .palette
        };
        // A CI4 tile with palette 3 reads bank 3 of a 256-entry load.
        assert_eq!(
            palette(setup(0x0030_0000, 256)),
            Some(Palette {
                image: Image::Flat { index: 7, mode: 1 },
                offset: 48
            })
        );
        assert_eq!(palette(setup(0x0030_0000, 16)), None);
        assert_eq!(
            palette(setup(0, 16)),
            Some(Palette {
                image: Image::Flat { index: 7, mode: 1 },
                offset: 0
            })
        );
    }

    #[test]
    fn texture_uv() {
        let mut t = Texture {
            tile: Tile {
                shift: [1, 15],
                ul: [4, 0],
                lr: [4 + 31 * 4, 15 * 4],
                ..Tile::default()
            },
            level: 0,
            scale: [0x8000, 0x8000],
            image: None,
            interleaved: false,
            palette: None,
        };
        assert_eq!(t.size(), [32, 16]);
        // s: 64 texels * 0.5 scale / 2 (shift 1) - 1 (ul) = 15 of 32; t: 4 * 0.5 * 2 (shift 15) = 4
        // of 16, flipped.
        assert_eq!(t.uv([64 * 32, 4 * 32]), [15.0 / 32.0, 1.0 - 4.0 / 16.0]);
        // Without G_SETTILESIZE the masks give the size.
        t.tile = Tile {
            mask: [5, 6],
            ..Tile::default()
        };
        assert_eq!(t.size(), [32, 64]);
        // Width is the row stride: CI4 with 4 words per row is 64 wide even with 63-texel bounds;
        // RGBA32 lines count 16-bit units (8 words = 32 texels).
        t.tile = Tile {
            line: 4,
            lr: [62 * 4, 62 * 4],
            ..Tile::default()
        };
        assert_eq!(t.size(), [64, 63]);
        t.tile = Tile {
            line: 8,
            size: 3,
            lr: [31 * 4, 31 * 4],
            ..Tile::default()
        };
        assert_eq!(t.size(), [32, 32]);
    }

    #[test]
    fn flat_references() {
        let payload: Vec<u8> = (0..0x220u32).map(|i| i as u8).collect();
        assert_eq!(flat_data(&payload, 0).unwrap().len(), 0x220);
        assert_eq!(flat_data(&payload, 1).unwrap(), &payload[0x20..]);
        assert_eq!(flat_data(&payload, 2).unwrap(), &payload[0x200..]);
        assert_eq!(flat_data(&payload[..0x10], 2), None);
        assert_eq!(flat_data(&payload, 3), None);
    }
}
