//! N64 texel formats to RGBA8, and RGBA8 to PNG.
//!
//! The TMEM odd-row order follows DevOldSchool/conkers-bfd-decomp (MIT, commit e2400b27dda6),
//! `docs/rzip-assets.md` and `scripts/texture_native.py`: data loaded with a zero-dxt LOADBLOCK
//! is stored TMEM-ready, so odd rows have the 32-bit halves of every 64-bit word exchanged
//! (RGBA32: the 64-bit halves of every 128-bit group). Conker stores images bottom-up: [`decode`]
//! keeps memory order (row 0 is t = 0); [`Material::texture_png`] writes the rows in reverse so
//! the art is upright, and [`crate::Texture::uv`] flips v to match (as DOS does).

use crate::{Error, Image, Material, Palette, Result, flat_data};

/// Palette entry format of a colour-indexed texture (othermode `G_MDSFT_TEXTLUT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tlut {
    Rgba16,
    Ia16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Rgba16,
    Rgba32,
    Ci4(Tlut),
    Ci8(Tlut),
    Ia4,
    Ia8,
    Ia16,
    I4,
    I8,
}

impl Format {
    pub fn bits(self) -> usize {
        match self {
            Format::Ci4(_) | Format::Ia4 | Format::I4 => 4,
            Format::Ci8(_) | Format::Ia8 | Format::I8 => 8,
            Format::Rgba16 | Format::Ia16 => 16,
            Format::Rgba32 => 32,
        }
    }
}

/// RGBA5551 with the RDP's bit replication for 5 to 8 bits (DOS uses `v * 255 / 31`; the two
/// differ by at most 1).
fn rgba5551(v: u16) -> [u8; 4] {
    let c = |shift: u16| {
        let x = (v >> shift & 0x1F) as u8;
        x << 3 | x >> 2
    };
    [c(11), c(6), c(1), if v & 1 != 0 { 255 } else { 0 }]
}

fn ia16(v: u16) -> [u8; 4] {
    let [i, a] = v.to_be_bytes();
    [i, i, i, a]
}

/// Decodes `width` x `height` texels from the start of `data` to RGBA8. Rows are `width * bits / 8`
/// bytes (pass the tile's line width, not a smaller visible width). `tlut` holds the big-endian
/// palette for CI formats and is ignored otherwise. `interleaved`: odd rows are TMEM-ready
/// (loaded by LOADBLOCK with dxt 0, see [`crate::Texture::interleaved`]).
pub fn decode(
    format: Format,
    width: usize,
    height: usize,
    data: &[u8],
    tlut: &[u8],
    interleaved: bool,
) -> Result<Vec<u8>> {
    let bits = format.bits();
    if !(width * bits).is_multiple_of(8) {
        return Err(Error::OddWidth(width));
    }
    let row = width * bits / 8;
    let need = row * height;
    let mut texels = data
        .get(..need)
        .ok_or(Error::Short {
            need,
            have: data.len(),
        })?
        .to_vec();
    if interleaved {
        let group = if format == Format::Rgba32 { 16 } else { 8 };
        for line in texels.chunks_exact_mut(row).skip(1).step_by(2) {
            for word in line.chunks_exact_mut(group) {
                let (a, b) = word.split_at_mut(group / 2);
                a.swap_with_slice(b);
            }
        }
    }

    let be16 = |b: &[u8], i: usize| u16::from_be_bytes([b[2 * i], b[2 * i + 1]]);
    let entry = |i: usize| {
        tlut.get(2 * i..2 * i + 2)
            .map(|e| u16::from_be_bytes([e[0], e[1]]))
            .ok_or(Error::PaletteIndex(i))
    };
    let mut out = Vec::with_capacity(width * height * 4);
    for i in 0..width * height {
        let v = match bits {
            4 => (texels[i / 2] >> (4 - i % 2 * 4) & 0xF) as usize,
            8 => texels[i] as usize,
            16 => be16(&texels, i) as usize,
            _ => u32::from_be_bytes(texels[4 * i..4 * i + 4].try_into().unwrap()) as usize,
        };
        let px = match format {
            Format::Rgba16 => rgba5551(v as u16),
            Format::Rgba32 => (v as u32).to_be_bytes(),
            Format::Ia16 => ia16(v as u16),
            Format::Ia8 => {
                let (i, a) = ((v >> 4) as u8 * 17, (v & 0xF) as u8 * 17);
                [i, i, i, a]
            }
            Format::Ia4 => {
                let i = (v >> 1) as u8;
                let i = i << 5 | i << 2 | i >> 1;
                [i, i, i, if v & 1 != 0 { 255 } else { 0 }]
            }
            Format::I8 => [v as u8; 4],
            Format::I4 => [v as u8 * 17; 4],
            Format::Ci4(Tlut::Rgba16) | Format::Ci8(Tlut::Rgba16) => rgba5551(entry(v)?),
            Format::Ci4(Tlut::Ia16) | Format::Ci8(Tlut::Ia16) => ia16(entry(v)?),
        };
        out.extend_from_slice(&px);
    }
    Ok(out)
}

impl Material {
    /// PNG of the material's flat (ROM) texture, upright: the last row in memory comes first
    /// (see [`crate::Texture::uv`]). `payload(id)` returns texture `id`'s decompressed bytes
    /// (`extract_core::Textures::get`). None when the material is untextured, its image or
    /// palette is bound at runtime (a segment) or its format is not decoded; an error when a
    /// payload is missing or shorter than the display list's tile.
    pub fn texture_png(&self, payload: impl Fn(u32) -> Option<Vec<u8>>) -> Option<Result<Vec<u8>>> {
        let t = self.texture?;
        let Some(Image::Flat { index, mode }) = t.image else {
            return None;
        };
        let format = self.texture_format()?;
        let palette = match t.palette {
            Some(Palette {
                image: Image::Flat { index, mode },
                offset,
            }) => Some((
                payload(index).unwrap_or_default(),
                mode,
                offset as usize * 2,
            )),
            Some(_) => return None,
            None => None,
        };
        let tlut = match &palette {
            Some((p, mode, offset)) => flat_data(p, *mode)
                .and_then(|d| d.get(*offset..))
                .unwrap_or_default(),
            None => &[],
        };
        let pixels = payload(index).unwrap_or_default();
        let pixels = flat_data(&pixels, mode).unwrap_or_default();
        let [w, h] = t.size();
        Some(
            decode(format, w as usize, h as usize, pixels, tlut, t.interleaved).and_then(|rgba| {
                let rows: Vec<&[u8]> = rgba.chunks_exact(w as usize * 4).rev().collect();
                png(w, h, &rows.concat())
            }),
        )
    }
}

/// Encodes RGBA8 pixels as a PNG file.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = ::png::Encoder::new(&mut out, width, height);
    encoder.set_color(::png::ColorType::Rgba);
    encoder.set_depth(::png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(format: Format, data: &[u8], tlut: &[u8]) -> Vec<[u8; 4]> {
        let n = data.len() * 8 / format.bits();
        decode(format, n, 1, data, tlut, false)
            .unwrap()
            .as_chunks::<4>()
            .0
            .to_vec()
    }

    #[test]
    fn rgba16() {
        // 0xF801 = red, opaque; 0x07C0 = green, transparent; 0x0843 = r1 g1 b1, opaque.
        assert_eq!(
            px(Format::Rgba16, &[0xF8, 0x01, 0x07, 0xC0, 0x08, 0x43], &[]),
            [[255, 0, 0, 255], [0, 255, 0, 0], [8, 8, 8, 255]]
        );
    }

    #[test]
    fn rgba32() {
        assert_eq!(px(Format::Rgba32, &[1, 2, 3, 4], &[]), [[1, 2, 3, 4]]);
    }

    #[test]
    fn intensity_formats() {
        assert_eq!(px(Format::I8, &[0x7F], &[]), [[0x7F; 4]]);
        assert_eq!(px(Format::I4, &[0xF1], &[]), [[255; 4], [17; 4]]);
        assert_eq!(
            px(Format::Ia16, &[0x80, 0x40], &[]),
            [[0x80, 0x80, 0x80, 0x40]]
        );
        assert_eq!(px(Format::Ia8, &[0xF3], &[]), [[255, 255, 255, 51]]);
        // IA4: 3-bit intensity, 1-bit alpha; 0b1111 = white opaque, 0b0110 = 3/7 grey, clear.
        assert_eq!(
            px(Format::Ia4, &[0xF6], &[]),
            [[255, 255, 255, 255], [109, 109, 109, 0]]
        );
    }

    #[test]
    fn color_indexed() {
        let tlut = [0x00, 0x00, 0xF8, 0x01, 0x80, 0xFF];
        // CI4 is high nibble first.
        assert_eq!(
            px(Format::Ci4(Tlut::Rgba16), &[0x10], &tlut),
            [[255, 0, 0, 255], [0, 0, 0, 0]]
        );
        assert_eq!(
            px(Format::Ci8(Tlut::Ia16), &[2], &tlut),
            [[0x80, 0x80, 0x80, 0xFF]]
        );
        assert!(matches!(
            decode(Format::Ci8(Tlut::Rgba16), 1, 1, &[3], &tlut, false),
            Err(Error::PaletteIndex(3))
        ));
    }

    #[test]
    fn interleaved_odd_rows() {
        // I8, 8 x 3: odd row 1 has its 4-byte halves swapped in memory.
        let data: Vec<u8> = (0..8).chain([4, 5, 6, 7, 0, 1, 2, 3]).chain(0..8).collect();
        let rows = decode(Format::I8, 8, 3, &data, &[], true).unwrap();
        let red: Vec<u8> = rows.iter().step_by(4).copied().collect();
        assert_eq!(red, [0, 1, 2, 3, 4, 5, 6, 7].repeat(3));
        // RGBA32, 4 x 2: the odd row swaps 8-byte halves of each 16-byte group.
        let even: Vec<u8> = (0..16).collect();
        let odd: Vec<u8> = (8..16).chain(0..8).collect();
        let rgba = decode(
            Format::Rgba32,
            4,
            2,
            &[even.clone(), odd].concat(),
            &[],
            true,
        )
        .unwrap();
        assert_eq!(rgba, [even.clone(), even].concat());
    }

    #[test]
    fn size_errors() {
        assert!(matches!(
            decode(Format::Rgba16, 4, 4, &[0; 31], &[], false),
            Err(Error::Short { need: 32, have: 31 })
        ));
        assert!(matches!(
            decode(Format::I4, 3, 1, &[0; 2], &[], false),
            Err(Error::OddWidth(3))
        ));
    }

    fn unpng(file: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut reader = ::png::Decoder::new(std::io::Cursor::new(file))
            .read_info()
            .unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        (info.width, info.height, buf)
    }

    #[test]
    fn png_round_trip() {
        let rgba: Vec<u8> = (0..2 * 3 * 4).map(|i| i as u8 * 10).collect();
        assert_eq!(unpng(&png(2, 3, &rgba).unwrap()), (2, 3, rgba));
    }

    #[test]
    fn material_texture_png() {
        use crate::{Texture, Tile};
        // A 2 x 2 tile (bounds, no line stride) of texture 7, untinted.
        let material = |format: u8, size: u8, palette: Option<Palette>| Material {
            texture: Some(Texture {
                tile: Tile {
                    format,
                    size,
                    lr: [4, 4],
                    ..Tile::default()
                },
                level: 0,
                scale: [0xFFFF; 2],
                image: Some(Image::Flat { index: 7, mode: 0 }),
                interleaved: false,
                palette,
            }),
            ..Material::default()
        };
        // I8: memory rows [1, 2], [3, 4] come out upright as [3, 4], [1, 2].
        let i8 = material(4, 1, None);
        let file = i8.texture_png(|id| (id == 7).then(|| vec![1, 2, 3, 4]));
        let (w, h, rgba) = unpng(&file.unwrap().unwrap());
        let red: Vec<u8> = rgba.iter().step_by(4).copied().collect();
        assert_eq!((w, h, red), (2, 2, vec![3, 4, 1, 2]));
        // A short payload is an error; untextured or runtime-bound images are not textures.
        assert!(matches!(
            i8.texture_png(|_| Some(vec![1, 2, 3])),
            Some(Err(Error::Short { need: 4, have: 3 }))
        ));
        assert!(Material::default().texture_png(|_| None).is_none());
        let mut segment = i8;
        segment.texture.as_mut().unwrap().image = Some(Image::Segment(0x0600_0000));
        assert!(segment.texture_png(|_| None).is_none());

        // CI4 with its 16-entry TLUT in the payload's last 0x20 bytes (mode 2), from entry 2:
        // index 0 -> entry 2 (red), index 1 -> entry 3 (green).
        let palette = Palette {
            image: Image::Flat { index: 7, mode: 2 },
            offset: 2,
        };
        let mut payload = vec![0x01, 0x10];
        payload.extend([0u8; 0x20]);
        payload[2 + 4..2 + 8].copy_from_slice(&[0xF8, 0x01, 0x07, 0xC1]);
        let ci4 = material(2, 0, Some(palette));
        let (_, _, rgba) = unpng(&ci4.texture_png(|_| Some(payload.clone())).unwrap().unwrap());
        let (red, green) = ([255, 0, 0, 255], [0, 255, 0, 255]);
        assert_eq!(rgba, [green, red, red, green].concat());
        let mut runtime_palette = ci4;
        runtime_palette.texture.as_mut().unwrap().palette = Some(Palette {
            image: Image::Segment(0x0700_0000),
            offset: 0,
        });
        assert!(
            runtime_palette
                .texture_png(|_| Some(payload.clone()))
                .is_none()
        );
    }
}
