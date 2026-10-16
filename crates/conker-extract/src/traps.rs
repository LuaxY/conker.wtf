//! Scene 0x34's traps' textures and sounds (`lab/research/mp-original/dm-maps-results.md` §2b;
//! the client's `traps.rs` draws and plays them): `maps/34/traps/*.png` and, through
//! [`crate::audio::pack_audio`], sounds [`SOUNDS`].

use anyhow::Result;
use extract_core::Rom;
use extract_gbi::{Format, decode, png};

/// The scene whose traps these are.
pub const SCENE: usize = 0x34;

/// (file under `maps/34/traps/`, blob id, format, width, height): what the original's display lists
/// draw them with, loaded by LOADBLOCK with dxt 0 (V: the RAM textures of oracle run fx34's DLs 170
/// and 300, byte for byte the blob's, `lab/hud/texfind`): a fire jet's flame quad (`func_151A6350`)
/// and the electric floor's arcs and bolts (ribbons tinted by the env colour). The jets' particles
/// (`func_151A6600`'s texrects, 0xBA6) are `hud/puff.png`, shared with the flamethrower's.
pub const TEXTURES: [(&str, usize, Format, usize, usize); 2] = [
    ("flame", 0x916, Format::Rgba32, 32, 32),
    ("arc", 0x753, Format::Ia16, 16, 16),
];

/// The fire jets' 0x1AA (`func_151A6068`) and the electric floor's 0x236 (`func_15104FF8`).
pub const SOUNDS: [u16; 2] = [0x1AA, 0x236];

/// `maps/34/traps/`'s PNGs ([`TEXTURES`]).
pub fn textures(rom: &Rom) -> Result<crate::Files> {
    let blob = rom.textures()?;
    TEXTURES
        .iter()
        .map(|&(name, id, format, w, h)| {
            let rgba = decode(format, w, h, &blob.get(id)?, &[], true)?;
            let file = format!("maps/{}/traps/{name}.png", crate::map_name(SCENE));
            Ok((file, png(w as u32, h as u32, &rgba)?))
        })
        .collect()
}
