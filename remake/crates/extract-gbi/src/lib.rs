//! Conker's display lists (F3DEX2 with Rare's "CBFD" changes) and N64 texture decoding.
//!
//! [`walk`] turns a display list into an IR of vertices and triangles grouped by material;
//! [`decode`] turns texels into RGBA8 and [`png`] into a PNG file, [`Material::texture_png`] does
//! both for a draw's texture, and [`glb`] packs the glTF writers' output. Library code only:
//! callers supply bytes and a segment resolver.

mod dl;
mod texture;

pub use dl::{
    Draw, Image, Ir, Material, Palette, Texture, Tile, VERTEX_CACHE, Vertex, flat_data, walk,
};
pub use texture::{Format, Tlut, decode, png};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the resolver has no data at {0:#010x}")]
    Unresolved(u32),
    #[error("display list runs past its data at {0:#010x}")]
    Truncated(u32),
    #[error("bad G_VTX at {0:#010x}")]
    BadVertexLoad(u32),
    #[error("triangle at {at:#010x} uses unloaded vertex slot {slot}")]
    Unloaded { at: u32, slot: usize },
    #[error("display-list call stack overflow at {0:#010x}")]
    TooDeep(u32),
    #[error("display list exceeds the command limit (a branch loop?)")]
    TooLong,
    #[error("texture data too short: need {need} bytes, have {have}")]
    Short { need: usize, have: usize },
    #[error("{0} texels wide is not a whole number of bytes per row")]
    OddWidth(usize),
    #[error("palette index {0} is outside the TLUT")]
    PaletteIndex(usize),
    #[error(transparent)]
    Png(#[from] ::png::EncodingError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Binary glTF: header, JSON chunk (space-padded) and BIN chunk (zero-padded), both 4-aligned.
pub fn glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
    let (text, data) = (
        json.len().next_multiple_of(4),
        bin.len().next_multiple_of(4),
    );
    let total = 12 + 8 + text + 8 + data;
    let mut out = Vec::with_capacity(total);
    out.extend(b"glTF");
    for word in [2, total, text, 0x4E4F_534A] {
        out.extend((word as u32).to_le_bytes());
    }
    out.extend(json);
    out.resize(20 + text, b' ');
    for word in [data, 0x004E_4942] {
        out.extend((word as u32).to_le_bytes());
    }
    out.extend(bin);
    out.resize(total, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glb_layout() {
        let glb = glb(br#"{"a":1}"#, &[1, 2, 3]);
        let word = |at: usize| u32::from_le_bytes(glb[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!(&glb[..4], b"glTF");
        assert_eq!((word(4), word(8)), (2, glb.len()));
        assert_eq!((word(12), word(16)), (8, 0x4E4F_534A));
        assert_eq!(&glb[20..28], br#"{"a":1} "#);
        assert_eq!((word(28), word(32)), (4, 0x004E_4942));
        assert_eq!(&glb[36..], [1, 2, 3, 0]);
    }
}
