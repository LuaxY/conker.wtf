//! The pack's HUD (`hud/`): the textures the original's Deathmatch HUD draws, from the texture blob
//! by id (`func_1510D0EC`'s ids in `func_150918EC`, `func_15093B58` and the overlay sprites; formats
//! as their display lists load them), its font (`func_15015920`: run-length glyphs in the ROM at the
//! range in `.game` data's table at 0x80082F80, the characters at 0x80085930), and `hud.json`, the
//! layout and rules found in the oracle's RAM dumps (`lab/hud`).

use anyhow::{Context, Result, ensure};
use conker_assets::hud::{
    Align, Anchor, Button, End, Glyph, Grenades, Health, Horizontal as H, Hud, Image, Iris, Lives,
    Placed, Radar, Rank, Results, Reticle, Scope, Text, Texture, Tv, Vertical as V, Weapon,
};
use extract_core::Rom;
use extract_gbi::{Format, Tlut, decode, png};

use crate::GAME_DATA;

/// The font table (`[start, end)` ROM offsets per font) and the characters of font 0's glyphs.
const FONTS_AT: usize = 0x8008_2F80 - GAME_DATA;
const CHARS_AT: usize = 0x8008_5930 - GAME_DATA;
const GLYPHS: usize = 95;

/// (name, blob id, format, width, height, smooth): every texture the HUD draws, loaded as its
/// display lists load it (by LOADBLOCK with dxt 0: odd rows stored TMEM-ready).
const TEXTURES: [(&str, usize, Format, usize, usize, bool); 12] = [
    ("atlas", 0xD16, Format::Ci4(Tlut::Rgba16), 64, 64, false),
    ("atlas2", 0xD17, Format::Ci4(Tlut::Rgba16), 64, 64, false),
    ("radar", 0xCF3, Format::Ia8, 32, 32, true),
    ("grenade", 0xD13, Format::Rgba16, 32, 32, false),
    ("reticle", 0x1013, Format::I8, 32, 32, true),
    ("scope", 0xFEE, Format::I8, 64, 64, true),
    ("scope_dot", 0xFF2, Format::I8, 8, 8, true),
    ("iris", 0x7B0, Format::I8, 64, 64, true),
    ("rank_st", 0x1081, Format::Rgba16, 16, 16, false),
    ("rank_nd", 0xD21, Format::Rgba16, 16, 16, false),
    ("rank_rd", 0xEA2, Format::Rgba16, 16, 16, false),
    ("rank_th", 0x11B7, Format::Rgba16, 16, 16, false),
];

/// The rank suffixes draw in the text's colour through their alpha (`G_CC` PRIM, PRIM_A × TEXEL0):
/// white, so the pack's PNG shows what the game shows.
const TEXT_TEXTURES: [&str; 4] = ["rank_st", "rank_nd", "rank_rd", "rank_th"];

/// The iris's radius by frames before the respawn (index 0: the respawn; V `lab/hud` iris run,
/// `func_1517FB9C`'s argument each frame of a death at read 60, respawned at 194).
const IRIS: [u16; 89] = [
    0, 0, 2, 5, 8, 11, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 15, 22,
    29, 36, 44, 51, 58, 65, 72, 80, 80, 80, 80, 80, 81, 81, 81, 79, 78, 79, 81, 83, 82, 80, 77, 77,
    80, 88, 96, 104, 112, 120, 128, 136, 144, 152, 160, 168, 176, 184, 192, 200, 208, 216, 224,
    232, 240, 248, 256, 264, 272, 280, 288, 296, 304, 312, 320, 328, 336, 344, 352, 360, 368, 376,
    384, 392,
];

/// A decoded glyph: width, height, x and y offsets, RGBA.
type Decoded = (u8, u8, i8, i8, Vec<u8>);

fn be32(b: &[u8], at: usize) -> Result<u32> {
    let w = b.get(at..at + 4).context("short data")?;
    Ok(u32::from_be_bytes(w.try_into()?))
}

/// One font glyph (`func_15015A38`): `w - 1, h - 1, x offset, y offset`, the record's size (u32),
/// then rows 1..=h of run-length bytes (intensity in the high nibble, run - 1 in the low one) for
/// columns 1..w; column 0 and row 0 are blank. Returns (w, h, x, y, RGBA white with the intensity as
/// alpha, rows 1..=h by columns 0..w).
fn glyph(record: &[u8]) -> Result<Decoded> {
    let (w, h) = (record[0] + 1, record[1] + 1);
    let (cols, rows) = (usize::from(w), usize::from(h));
    let mut alpha = vec![0u8; cols * rows];
    let mut at = 8;
    for r in 0..rows {
        let mut c = 1;
        while c < cols {
            let b = *record.get(at).context("glyph runs past its record")?;
            at += 1;
            for k in c..(c + usize::from(b & 0xF) + 1).min(cols) {
                alpha[r * cols + k] = b & 0xF0;
            }
            c += usize::from(b & 0xF) + 1;
        }
    }
    let rgba = alpha.iter().flat_map(|&a| [255, 255, 255, a]).collect();
    Ok((w, h, record[2] as i8, record[3] as i8, rgba))
}

/// The font's glyphs: (character, glyph) in the game's order.
fn font(rom: &Rom, data: &[u8]) -> Result<Vec<(char, Decoded)>> {
    let (start, end) = (
        be32(data, FONTS_AT)? as usize,
        be32(data, FONTS_AT + 4)? as usize,
    );
    let bytes = rom.bytes().get(start..end).context("font range")?;
    let chars = data
        .get(CHARS_AT..CHARS_AT + GLYPHS)
        .context("font characters")?;
    let mut out = Vec::new();
    let mut at = 0;
    for &c in chars {
        let record = bytes
            .get(at..)
            .filter(|r| r.len() >= 8)
            .context("font records")?;
        out.push((char::from(c), glyph(record)?));
        at += be32(record, 4)? as usize;
    }
    ensure!(at <= bytes.len(), "font records past their range");
    Ok(out)
}

fn image(texture: &str, rect: [u16; 4]) -> Image {
    Image {
        texture: texture.into(),
        rect,
    }
}

/// The weapon icons (`func_150918EC`): cell `n` of the atlas is column `n % 3` (25 texels apart)
/// of row `n / 3` (its top and height from the tables at 0x800873B4 and 0x800873BC), 22 wide;
/// weapon 0x39's is cell 10 of the second atlas, 3 texels lower, and the shotgun's (0x37) its cell
/// 7; cell 1 (0x16) is a texel shorter.
fn weapon_icons() -> std::collections::BTreeMap<u8, Image> {
    const TOPS: [u16; 5] = [4, 16, 25, 39, 53];
    const HEIGHTS: [u16; 5] = [12, 9, 14, 12, 10];
    let cell = |atlas: &str, n: usize, down: u16| {
        let (col, row) = (n as u16 % 3, n / 3);
        let height = HEIGHTS[row] - u16::from(n == 1);
        image(atlas, [col * 25, TOPS[row] + down, 22, height])
    };
    [
        (0x09, cell("atlas", 0, 0)),
        (0x16, cell("atlas", 1, 0)),
        (0x14, cell("atlas", 3, 0)),
        (0x15, cell("atlas", 7, 0)),
        (0x22, cell("atlas", 9, 0)),
        (0x23, cell("atlas", 10, 0)),
        (0x21, cell("atlas", 12, 0)),
        (0x24, cell("atlas", 13, 0)),
        (0x39, cell("atlas2", 10, 3)),
        (0x18, cell("atlas", 6, 0)),
        (0x37, cell("atlas2", 7, 3)),
    ]
    .into()
}

/// The layout and rules (numbers from the RAM dumps and the HUD code, `lab/hud/README.md`).
fn layout(textures: std::collections::BTreeMap<String, Texture>, font: Vec<Glyph>) -> Hud {
    let white = [255, 255, 255, 255];
    let green = |a| [0, 255, 0, a];
    Hud {
        frame: [292, 216],
        tv: Tv {
            size: [320, 240],
            offset: [16, 12],
        },
        textures,
        font,
        health: Health {
            anchor: Anchor(H::Left, V::Top),
            right: 29,
            rows: [5, 10],
            piece: image("atlas", [49, 5, 5, 4]),
        },
        weapon: Weapon {
            anchor: Anchor(H::Right, V::Top),
            at: [260, 5],
            icons: weapon_icons(),
        },
        grenades: Grenades {
            anchor: Anchor(H::Right, V::Top),
            at: [275, 5],
            step: -11,
            icon: image("grenade", [6, 8, 5, 20]),
        },
        score: Text {
            anchor: Anchor(H::Centre, V::Top),
            at: [138, 6],
            align: Align::Centre,
            colour: white,
            shadow: None,
        },
        rank: Rank {
            text: Text {
                anchor: Anchor(H::Centre, V::Top),
                at: [162, 6],
                align: Align::Centre,
                colour: white,
                shadow: None,
            },
            suffixes: TEXT_TEXTURES.map(String::from),
            offset: [0, -4],
            texels: [1, 1, 15, 15],
            advance: 12,
            width: 16,
        },
        clock: Text {
            anchor: Anchor(H::Centre, V::Bottom),
            at: [135, 198],
            align: Align::Left,
            colour: white,
            shadow: Some([0, 0, 0, 255]),
        },
        radar: Radar {
            anchor: Anchor(H::Right, V::Top),
            centre: [259.0, 45.0],
            size: 48.0,
            disc: image("radar", [0, 0, 32, 32]),
            colour: [255, 255, 255, 128],
            scale: 0.020_25 / 4.0,
            range: 22.0,
            blip: 2.0,
            blip_colour: [255, 55, 55, 255],
            levels: vec![-690.0, -130.5, 828.5],
            level_mark: [0, 0, 0, 255],
            hidden_with: vec![0x21],
        },
        reticle: Reticle {
            anchor: Anchor(H::Centre, V::Middle),
            rect: [134, 96, 24, 24],
            image: image("reticle", [0, 0, 32, 32]),
            colour: [255, 200, 0, 130],
            weapons: vec![0x09, 0x14, 0x15, 0x39],
        },
        scope: Scope {
            anchor: Anchor(H::Centre, V::Middle),
            centre: [146.0, 108.0],
            ring: image("scope", [0, 0, 64, 64]),
            quadrant: 50.0,
            dot: image("scope_dot", [0, 0, 8, 8]),
            dots: 12,
            dot_size: 6.0,
            radius: 55.0,
            colour: green(130),
            back: [52.0, 46.0, 8.0, 124.0],
            back_colour: green(65),
            bar_x: 54.0,
            bar_width: 4.0,
            bar_half: [2.0, 58.0],
        },
        iris: Iris {
            anchor: Anchor(H::Centre, V::Middle),
            centre: [150.0, 91.0],
            image: image("iris", [0, 0, 64, 64]),
            colour: [0, 0, 0, 255],
            radius: IRIS.into(),
        },
        end: None,
    }
}

/// `hud/`'s files: `hud.json`, a PNG per texture and one per glyph (`font/<n>.png`, in the game's
/// order: its character list names two glyphs `ê` and two `ô`, the first of each is the one drawn).
pub fn pack_hud(rom: &Rom) -> Result<(crate::Files, Hud)> {
    let blob = rom.textures()?;
    let data = rom.game_data()?;
    let mut files = Vec::new();
    let mut textures = std::collections::BTreeMap::new();
    for (name, id, format, w, h, smooth) in &TEXTURES {
        let bytes = blob.get(*id)?;
        let tlut = bytes.get(w * h / 2..).unwrap_or_default();
        let mut rgba = decode(*format, *w, *h, &bytes, tlut, true)?;
        if TEXT_TEXTURES.contains(name) {
            for p in rgba.as_chunks_mut::<4>().0 {
                p[..3].fill(255);
            }
        }
        let file = format!("{name}.png");
        files.push((format!("hud/{file}"), png(*w as u32, *h as u32, &rgba)?));
        let size = [*w as u16, *h as u16];
        let smooth = *smooth;
        textures.insert(name.to_string(), Texture { file, size, smooth });
    }
    let mut glyphs = Vec::new();
    for (n, (ch, (w, h, x, y, rgba))) in font(rom, &data)?.into_iter().enumerate() {
        let file = format!("font/{n:02}.png");
        files.push((format!("hud/{file}"), png(w.into(), h.into(), &rgba)?));
        glyphs.push(Glyph {
            ch,
            file,
            size: [w, h],
            offset: [x, y],
        });
    }
    let hud = layout(textures, glyphs);
    files.push((conker_assets::hud::HUD.into(), hud.to_json()));
    Ok((files, hud))
}

/// (name, first texture id, tiles across, smooth): what the end of a match draws, RGBA32 32 x 32
/// tiles stored for a LOADBLOCK without dxt (odd rows' texels at x ^ 2); a sprite of several tiles
/// (`.game` data's sprite records at 0x800859E0: tiles across, the first id) is one PNG. The stick
/// is 0x7E8, the tile drawn (its record names 0x7E7). V: `lab/research/mp-original` (STATS screen).
const END_TEXTURES: [(&str, usize, usize, bool); 11] = [
    ("lives", 0xD14, 1, false),
    ("skull", 0xD0F, 1, true),
    ("stats", 0x816, 2, false),
    ("restart", 0x80E, 2, false),
    ("restart_lit", 0x810, 2, false),
    ("quit", 0x7FC, 2, false),
    ("quit_lit", 0x7FE, 2, false),
    ("stick", 0x7E8, 1, false),
    ("stat_icons", 0x888, 1, false),
    ("stat_deaths", 0x887, 1, false),
    ("players", 0x843, 1, false),
];

/// `end.json`'s layout, the original's draws (V, the oracle's display lists: `lab/research/
/// mp-original/menus-and-flow.md`, "Match end and STATS").
fn end_layout(textures: std::collections::BTreeMap<String, Texture>) -> End {
    let centre = Anchor(H::Centre, V::Middle);
    let placed = |rect, texture, texels| Placed {
        anchor: centre,
        rect,
        image: image(texture, texels),
    };
    let button = |x, name: &str| Button {
        anchor: centre,
        rect: [x, 164, 64, 32],
        image: image(name, [0, 0, 64, 32]),
        lit: image(&format!("{name}_lit"), [0, 0, 64, 32]),
    };
    End {
        textures,
        // The digit n at s {1, 8, 15, 21}[n & 3], t {0, 8, 16}[n >> 2] (0x800873C4, 0x800873C8).
        lives: Lives {
            anchor: Anchor(H::Left, V::Top),
            at: [7, 6],
            digits: (0..10)
                .map(|n| image("lives", [[1, 8, 15, 21][n & 3], [0, 8, 16][n >> 2], 5, 7]))
                .collect(),
        },
        skull: placed([130, 92, 32, 32], "skull", [0, 0, 32, 32]),
        results: Results {
            anchor: centre,
            header: placed([116, 4, 64, 32], "stats", [0, 0, 64, 32]),
            // Accuracy, kills, headshots, deaths.
            icons: vec![
                placed([129, 49, 16, 16], "stat_icons", [16, 0, 16, 16]),
                placed([158, 49, 16, 16], "stat_icons", [0, 0, 16, 16]),
                placed([177, 49, 32, 16], "stat_icons", [0, 16, 32, 16]),
                placed([218, 49, 16, 15], "stat_deaths", [0, 16, 16, 15]),
            ],
            labels: [[0, 0], [16, 0], [0, 16], [16, 16]]
                .map(|[x, y]| image("players", [x, y, 16, 16]))
                .into(),
            label: [60, 64],
            rows: [66, 15],
            columns: [112, 140, 166, 196, 226],
            // P1 red, P2 blue (V); P3 green, P4 yellow (I: their labels' colours).
            colours: vec![
                [255, 0, 0, 255],
                [0, 0, 255, 255],
                [0, 255, 0, 255],
                [255, 255, 0, 255],
            ],
            shadow: [0, 0, 0, 255],
            restart: button(51, "restart"),
            quit: button(181, "quit"),
            stick: placed([132, 164, 32, 32], "stick", [0, 0, 32, 32]),
        },
    }
}

/// `hud/end.json` and its PNGs (files `hud.json` does not name).
pub fn pack_end(rom: &Rom) -> Result<(crate::Files, End)> {
    let blob = rom.textures()?;
    let mut files = Vec::new();
    let mut textures = std::collections::BTreeMap::new();
    for (name, id, across, smooth) in END_TEXTURES {
        let mut tiles = Vec::new();
        for t in id..id + across {
            tiles.push(decode(Format::Rgba32, 32, 32, &blob.get(t)?, &[], true)?);
        }
        let rgba: Vec<u8> = (0..32)
            .flat_map(|y| tiles.iter().flat_map(move |t| &t[y * 128..(y + 1) * 128]))
            .copied()
            .collect();
        let file = format!("{name}.png");
        files.push((format!("hud/{file}"), png(32 * across as u32, 32, &rgba)?));
        let size = [32 * across as u16, 32];
        textures.insert(name.to_string(), Texture { file, size, smooth });
    }
    let end = end_layout(textures);
    files.push((conker_assets::hud::END.into(), end.to_json()));
    Ok((files, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_runs_fill_columns_after_the_blank_one() {
        // 3 wide, 2 tall: row 1 = blank, 0xF0, 0x80; row 2 = blank, 0x40 twice (one run of 2).
        let record = [2, 1, 0, 1, 0, 0, 0, 11, 0xF0, 0x80, 0x41];
        let (w, h, x, y, rgba) = glyph(&record).unwrap();
        assert_eq!((w, h, x, y), (3, 2, 0, 1));
        let alpha: Vec<u8> = rgba.chunks(4).map(|p| p[3]).collect();
        assert_eq!(alpha, [0, 0xF0, 0x80, 0, 0x40, 0x40]);
    }
}
