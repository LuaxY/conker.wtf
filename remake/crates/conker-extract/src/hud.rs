//! The pack's HUD (`hud/`): the textures the original's Deathmatch HUD draws, from the texture blob
//! by id (`func_1510D0EC`'s ids in `func_150918EC`, `func_15093B58` and the overlay sprites; formats
//! as their display lists load them), its font (`func_15015920`: run-length glyphs in the ROM at the
//! range in `.game` data's table at 0x80082F80, the characters at 0x80085930), and `hud.json`, the
//! layout and rules found in the oracle's RAM dumps (`lab/hud`).

use anyhow::{Context, Result, ensure};
use conker_assets::hud::{
    Align, Anchor, Beach, Button, End, Flash, Glyph, Grenades, Gun, Haze, HazeLayer, Health,
    Horizontal as H, Hud, Image, Iris, Lives, Placed, Radar, Rank, Results, Reticle, Scope, Sprite,
    Strip, Tape, Teams, Text, Texture, Tv, Vertical as V, Weapon,
};
use extract_core::Rom;
use extract_gbi::{Format, Tlut, decode, png};

use crate::GAME_DATA;

/// The font table (`[start, end)` ROM offsets per font) and the characters of font 0's glyphs.
const FONTS_AT: usize = 0x8008_2F80 - GAME_DATA;
const CHARS_AT: usize = 0x8008_5930 - GAME_DATA;
const GLYPHS: usize = 95;

/// (name, blob id, format, width, height, smooth): every texture the HUD draws, loaded as its
/// display lists load it (by LOADBLOCK with dxt 0: odd rows stored TMEM-ready); War: Total War's
/// gassed screen's (`func_151D2830`; V: white, alpha 1-7 of 15, equal to the RAM's at 0x8023B390,
/// `../lab/research/mp-original/total-war-effects.md` §2); its outros' speech bubble and its tail
/// (V: equal to the RAM's at 0x8025F250 and 0x8022E320 that the outro's DLs draw, `lab/hud/texfind`);
/// a mounted gun's gunner view's (V: the RAM's that a 1-view gunner's DLs draw, found by `texfind`;
/// its centre is the `reticle`); War: Beach's strip heads, and the explosions' sprites its
/// detonation and strike draw (screen-space texrects, not yet used by the remake: V the RAM's at
/// 0x801CE678, 0x801F7540, 0x801F6120, 0x801F7130 and 0x801E4910 that `../lab/research/mp-original/
/// beach-presentation-scratch`'s det1 DL 318 and ev3 DL 1044 draw, byte for byte the blob's; the
/// strike's red flash, 0x80202220, is `iris`).
const TEXTURES: [(&str, usize, Format, usize, usize, bool); 29] = [
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
    ("teams", 0xD10, Format::Rgba32, 32, 32, false),
    ("haze", 0xA48, Format::Ia8, 64, 64, true),
    ("bubble", 0x1047, Format::Ia8, 64, 64, true),
    ("tail", 0x1049, Format::Ia8, 16, 16, true),
    ("gun_handle", 0x3D1, Format::Ia16, 64, 32, true),
    ("gun_handle_lit", 0x3D2, Format::Ia16, 64, 32, true),
    ("gun_flash", 0x3D0, Format::Rgba32, 32, 32, true),
    ("gun_corner", 0x1127, Format::I8, 16, 16, true),
    ("gun_tick_side", 0x1198, Format::I4, 16, 16, true),
    ("gun_tick_end", 0x1197, Format::I4, 16, 16, true),
    ("gun_tape_heading", 0xFF3, Format::I8, 16, 8, true),
    ("gun_tape_elevation", 0xFF4, Format::I8, 8, 16, true),
    ("beach_teams", 0xD12, Format::Rgba32, 16, 32, false),
    ("explosion", 0x78A, Format::Rgba32, 32, 32, true),
    ("smoke", 0xE4A, Format::Rgba32, 32, 32, true),
    ("fire", 0x71F, Format::I8, 32, 32, true),
    ("shell", 0x1042, Format::Ia16, 16, 16, true),
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
/// 7; cell 1 (0x16) is a texel shorter; the club's (0x40) cell 4, 3 texels higher and 20 wide
/// (drawn a pixel right of the others: not kept).
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
        (0x40, image("atlas", [25, 13, 20, 9])),
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
            // The name colours 0x800AB62C by obj+0x328 − 1: 3 on team 0, 1 on team 1 (C
            // `func_15093B58`; V `sides_s9`, the gas run's DL 3000 drew 0x78FF16).
            team_colours: [[120, 255, 22, 255], [255, 55, 55, 255]],
            // War: Colors' flags not carried: obj+0x328 = 5, the name colour 4 (C; V e1 DL 325).
            flag_colour: [255, 255, 255, 255],
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
        strip: strip(),
        // War: Colors' carried-flag icons (HUD flag 0x8000; V e0 DL 360/364, e1 DL 325/360/382):
        // texture 0xD10's flags, sharp, the red (16, 16) at x 236 while a squirrel carries it, the
        // green (0, 16) at x 40 while a Tedi does.
        flags: [(236, 16), (40, 0)].map(|(x, s)| Placed {
            anchor: Anchor(H::Centre, V::Bottom),
            rect: [x, 196, 16, 16],
            image: image("teams", [s, 16, 16, 16]),
        }),
        // Command F 1 (`func_1509DFC4`, C): `func_15042D94(150, 86, 1, "%d", (t + 59) / 60)` in white,
        // flags 1: centred, no shadow (0x80), off the viewport's scale (0x40 clear: 1 in one view;
        // `func_15043384`).
        countdown: Text {
            anchor: Anchor(H::Centre, V::Middle),
            at: [150, 86],
            align: Align::Centre,
            colour: white,
            shadow: None,
        },
        // `func_151D2830` (C; V e1 DL 1080, `hudwalk`): two texrects of 0xA48 over the viewport,
        // bilinear, wrapping, combiner TEXEL0 × ENV: env (0, 255, 0, the strength), 1 texel a
        // pixel from t 32; env (255, 0, 0, half of it), half a texel a pixel from t 0.
        haze: Haze {
            image: image("haze", [0, 0, 64, 64]),
            layers: [
                HazeLayer {
                    colour: [0, 255, 0],
                    share: 1.0,
                    texel: 1.0,
                    t: 32.0,
                },
                HazeLayer {
                    colour: [255, 0, 0],
                    share: 0.5,
                    texel: 0.5,
                    t: 0.0,
                },
            ],
        },
        gun: gun(),
        beach: beach(),
        end: None,
    }
}

/// A mounted gun's gunner view in one view (`func_151C9BA0` case 0x1B, C; V the private oracle's
/// DLs 125-260 of a gunner on 0x30's gun B, `hudwalk`): overlay sprites centred off the view's
/// centre (146, 108), each its half size × 2 wide. The sight (`func_151CC840`): sprites 0x64 (the
/// reticle) 12, 0x9D at x ±55 and 0x9C at y ±55 7, 0x9E at (±44, ±44) 6, env (255, 0, 0, 0xB4).
/// The tapes (`func_151CC524`, sprites 0x9A at y −37, 0x9B at x −37, 28 × 4.2, the texture 5 times
/// along; `func_151CC77C`: offset 2 · ∓angle / 360 · 2560 in 1/32 texels, the angles of the view
/// direction s126+0x130; V at reads 127 and 242: −48 and −78.72 texels at headings 108° and
/// 177.19°). The handles (`func_151CB5FC`, sprites 0x7B / 0x7C at (±88, 91), 49 × 18: at rest
/// alpha 255 / 0; the side that fired kicks, the client's `hud::kick`). The zoom bar
/// (`func_151C9F38` type 3: x 37, w 1, h 30; colours 0x800AAC90 / 0x800AACE0 entry 3; its half
/// height 1 + 29 × zoom, I as the sniper's 2 + 58; `bar_flash` 0x800AAC80, the red it turns 16
/// frames after the zoom stops, `func_151CBC60` / `func_151CC2BC`). The flash
/// (`func_150FB8C0`: sprite 0x55 at (±59, 68), half size 40 + rand · 20 by 80 + rand · 20, alpha
/// 200 + rand % 56, life (rand & 3) + 5; V DL 221 and 223).
fn gun() -> Gun {
    let sprite = |rect, texture: &str, size: [u16; 2], flip| Sprite {
        rect,
        image: image(texture, [0, 0, size[0], size[1]]),
        flip,
    };
    let (no, x, y, xy) = ([false, false], [true, false], [false, true], [true, true]);
    Gun {
        anchor: Anchor(H::Centre, V::Middle),
        sight: vec![
            sprite([134.0, 96.0, 24.0, 24.0], "reticle", [32, 32], no),
            sprite([84.0, 101.0, 14.0, 14.0], "gun_tick_side", [16, 16], x),
            sprite([194.0, 101.0, 14.0, 14.0], "gun_tick_side", [16, 16], no),
            sprite([139.0, 46.0, 14.0, 14.0], "gun_tick_end", [16, 16], no),
            sprite([139.0, 156.0, 14.0, 14.0], "gun_tick_end", [16, 16], y),
            sprite([96.0, 58.0, 12.0, 12.0], "gun_corner", [16, 16], no),
            sprite([184.0, 58.0, 12.0, 12.0], "gun_corner", [16, 16], x),
            sprite([96.0, 146.0, 12.0, 12.0], "gun_corner", [16, 16], y),
            sprite([184.0, 146.0, 12.0, 12.0], "gun_corner", [16, 16], xy),
        ],
        colour: [255, 0, 0, 0xB4],
        tapes: [
            Tape {
                rect: [118.0, 66.75, 56.0, 8.25],
                image: image("gun_tape_heading", [0, 0, 16, 8]),
                repeats: 5.0,
                per_degree: -4.0 / 9.0,
            },
            Tape {
                rect: [104.75, 80.0, 8.25, 56.0],
                image: image("gun_tape_elevation", [0, 0, 8, 16]),
                repeats: 5.0,
                per_degree: -4.0 / 9.0,
            },
        ],
        tape_colour: [255, 0, 0, 0x82],
        handles: [
            sprite([9.0, 181.0, 98.0, 36.0], "gun_handle", [64, 32], no),
            sprite([185.0, 181.0, 98.0, 36.0], "gun_handle", [64, 32], x),
        ],
        lit: image("gun_handle_lit", [0, 0, 64, 32]),
        back: [180.0, 76.0, 6.0, 64.0],
        back_colour: [255, 0, 0, 0x41],
        bar_x: 182.0,
        bar_width: 2.0,
        bar_half: [1.0, 29.0],
        bar_colour: [255, 255, 0, 0x5A],
        bar_flash: [255, 0, 0, 255],
        flash: Flash {
            image: image("gun_flash", [0, 0, 32, 32]),
            centre: [205.0, 176.0],
            width: [80.0, 120.0],
            height: [160.0, 200.0],
            alpha: [200, 255],
            life: [5, 8],
        },
    }
}

/// Total War's team strip (`func_151E9D18`, V: the gas run's DL 3000 at `../lab/research/
/// mp-original/total-war-gas-scratch/run1`, timed, 2 and 4 kills): texture 0xD10's cells, 16 x
/// 16, sharp, the squirrels' at x 198 from texel x 0, the Tediz' at x 66 from 16; the flags (row
/// 16) on the clock, the heads (row 0) untimed (C, HUD flag 0x100); the number `" %d"` from x 211
/// and 79 (its space moves the pen 4), top 197, grey over a black shadow.
fn strip() -> Strip {
    let anchor = Anchor(H::Centre, V::Bottom);
    let number = |x| Text {
        anchor,
        at: [x, 197],
        align: Align::Left,
        colour: [192, 192, 192, 255],
        shadow: Some([0, 0, 0, 255]),
    };
    Strip {
        anchor,
        rects: [[198, 196, 16, 16], [66, 196, 16, 16]],
        kills: [
            image("teams", [0, 16, 16, 16]),
            image("teams", [16, 16, 16, 16]),
        ],
        lives: [
            image("teams", [0, 0, 16, 16]),
            image("teams", [16, 0, 16, 16]),
        ],
        numbers: [number(215), number(83)],
    }
}

/// War: Beach's HUD (`../lab/research/mp-original/beach-presentation.md` §1; V det1 DL 318, e1 DL
/// 99, cs4/cs3 DL 700/800): Total War's strip (`func_151E9D18`) with texture 0xD12's heads, the
/// villagers' (0, 16) at x 198 and the Tediz' (0, 0) at 66, the team scores as the numbers; the
/// escape countdown (`func_1508A6FC`: `"%d"` of the timer / 60, white, centred on the view's
/// centre x, its top 6 below the view's).
fn beach() -> Beach {
    let mut strip = strip();
    let heads = [
        image("beach_teams", [0, 16, 16, 16]),
        image("beach_teams", [0, 0, 16, 16]),
    ];
    (strip.kills, strip.lives) = (heads.clone(), heads);
    Beach {
        strip,
        escape: Text {
            anchor: Anchor(H::Centre, V::Top),
            at: [146, 6],
            align: Align::Centre,
            colour: [255, 255, 255, 255],
            shadow: None,
        },
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
const END_TEXTURES: [(&str, usize, usize, bool); 18] = [
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
    // Total War's header: sprites 0x50 SQUIRRELS, 0x51 TEDIZ, 0x52 YOU, 0x56 WIN, 0x57 LOSE, 0x58
    // DRAW (`func_151E43DC`).
    ("squirrels", 0x866, 3, false),
    ("tediz", 0x869, 2, false),
    ("you", 0x872, 2, false),
    ("win", 0x86D, 2, false),
    ("lose", 0x85B, 2, false),
    ("draw", 0x853, 2, false),
    // War: Beach's: sprite 0x55 FRENCHYS (its team 0's name, id 9: 0x4C + 9).
    ("frenchys", 0x855, 3, false),
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
        // `func_151E43DC`'s sprites centred on y 50 (V: its calls in `../lab/research/mp-original/
        // total-war-teams.md` §6); `func_151EA15C` draws from its argument, 106, Deathmatch's
        // 40 higher (V the rows, C the icons and labels), each row in `colours[team]` (C: 0x800ABA90
        // + 4·team, red, blue, green, yellow in the RAM dumps).
        teams: Teams {
            down: 40,
            draw: placed([116, 34, 64, 32], "draw", [0, 0, 64, 32]),
            won: [
                placed([86, 34, 64, 32], "you", [0, 0, 64, 32]),
                placed([146, 34, 64, 32], "win", [0, 0, 64, 32]),
            ],
            lost: [
                placed([86, 34, 64, 32], "you", [0, 0, 64, 32]),
                placed([146, 34, 64, 32], "lose", [0, 0, 64, 32]),
            ],
            team_won: [
                [
                    placed([77, 34, 96, 32], "squirrels", [0, 0, 96, 32]),
                    placed([172, 34, 64, 32], "win", [0, 0, 64, 32]),
                ],
                [
                    placed([81, 34, 64, 32], "tediz", [0, 0, 64, 32]),
                    placed([151, 34, 64, 32], "win", [0, 0, 64, 32]),
                ],
            ],
            // V cs4 DL 700 (FRENCHYS's tiles at 65, 97, 129), cs3 DL 800: no shift like SQUIRRELS'.
            beach_won: [
                [
                    placed([65, 34, 96, 32], "frenchys", [0, 0, 96, 32]),
                    placed([151, 34, 64, 32], "win", [0, 0, 64, 32]),
                ],
                [
                    placed([81, 34, 64, 32], "tediz", [0, 0, 64, 32]),
                    placed([151, 34, 64, 32], "win", [0, 0, 64, 32]),
                ],
            ],
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
