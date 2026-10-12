//! War: Total War's victory outros ([`conker_assets::outro`]): scene 0x2D's cutscenes 2 (the
//! squirrels win) and 4 (the Tediz win), decoded from `[6, 0x2D, n]` (loader `func_1501D348`; the
//! format in `../lab/research/mp-original/total-war-outro.md` §3) for what those two use, with
//! their actors' characters (each with its one clip) and their voice lines (bank 0x16's MP3s,
//! decoded to WAV). No generic cutscene interpreter: anything else they hold is left out (the head
//! channels 0-2, talk, cs4's attachment set 0x71).

use anyhow::{Context, Result, ensure};
use conker_assets::hud::{Anchor, Horizontal, Rgba, Vertical};
use conker_assets::outro::{Actor, Camera, Fade, Key, Outro, Page, Quad, Subtitles, Voice};
use extract_character::Character;
use extract_core::{Rom, Table};

use crate::{GAME_DATA, SLICE_SCENE, model_lift};

/// (winning team, cutscene): `func_1501D348(0x2D, n)`, n = 2 if team 0 ranks 1st, else 4 (V).
const OUTROS: [(u8, u8); 2] = [(0, 2), (1, 4)];
/// Op 5 b=1's colour table (`func_15029BB8`: 3 bytes per entry, indexed by the record's d).
const COLOURS: usize = 0x8008_4110 - GAME_DATA;
/// The scene's character records (bank 0x0E entry 0x2D, 0x30 bytes each): model at +4, uid at
/// +0x28 (C/V).
const RECORD: usize = 0x30;
/// Characters are drawn at half their model units (V obj+0x14C/+0x150, as players).
const SCALE: f32 = 0.5;
/// The voice lines' bank.
const MP3: usize = 0x16;

/// A script command at its time: `u8 op, u8 delay, u16 b, u8 c, u8 d, u16 e`.
#[derive(Clone, Copy, Debug)]
struct Command {
    t: u16,
    op: u8,
    b: u16,
    c: u8,
    d: u8,
    e: u16,
}

fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

/// A script's length (its 0xFF record) and timed commands: record 0's +2 is the start time, each
/// command fires at the clock and moves it on by its delay; op 0 sets it to `b`.
fn script(entry: &[u8]) -> Result<(u16, Vec<Command>)> {
    let records = entry.as_chunks::<8>().0;
    let mut t = be16(records.first().context("empty script")?, 2);
    let (mut length, mut out) = (None, Vec::new());
    for r in &records[1..] {
        match r[0] {
            0xFF => length = Some(be16(r, 2)),
            0xFD | 0xFE => {}
            op => {
                if op == 0 {
                    t = be16(r, 2);
                }
                out.push(Command {
                    t,
                    op,
                    b: be16(r, 2),
                    c: r[4],
                    d: r[5],
                    e: be16(r, 6),
                });
                t += u16::from(r[1]);
            }
        }
    }
    Ok((length.context("script without a 0xFF record")?, out))
}

/// An object's channel `channel`, from its curve entry's 8-byte keys.
fn keys(curve: &[u8], channel: u8) -> Vec<Key> {
    (curve.as_chunks::<8>().0.iter())
        .filter(|k| k[5] == channel)
        .map(|k| Key {
            t: be16(k, 6),
            value: f32::from_be_bytes([k[0], k[1], k[2], k[3]]),
            catmull: k[4] == 0,
        })
        .collect()
}

/// An object's path points (`s16 x, y, z, u16 flag`).
fn path(entry: &[u8]) -> Vec<[i16; 3]> {
    (entry.as_chunks::<8>().0.iter())
        .map(|p| [0, 2, 4].map(|o| be16(p, o) as i16))
        .collect()
}

/// A texture rectangle as the DL draws it (V `hudwalk`: corners, s/t at the top-left in 10.5
/// fixed point on a tile at 256, steps per pixel in 5.10) over HUD texture `texture`.
fn quad(texture: &str, [x0, y0, x1, y1]: [i16; 4], st: [f32; 2], step: [i16; 2], a: u8) -> Quad {
    let (s, t) = (st[0] - 256.0, st[1] - 256.0);
    let step = step.map(|d| f32::from(d) / 1024.0);
    let (w, h) = (f32::from(x1 - x0), f32::from(y1 - y0));
    let colour: Rgba = if a == 255 { [255; 4] } else { [0, 0, 0, a] };
    Quad {
        texture: texture.into(),
        rect: [x0, y0, x1 - x0, y1 - y0],
        uv: [s, t, s + w * step[0], t + h * step[1]],
        colour,
    }
}

/// Where each outro's bubble and text go (V: the DLs at t 56 and on, `total-war-outro-scratch/hw/
/// cs{2,4}b_dl_*/draws.json`): its shadow (black, alpha 0x40) and the shadow of its tail, the tail,
/// the bubble (white); textures 0x1047 (`bubble`) and 0x1049 (`tail`), cs2's mirrored; the text's
/// centre x and middle line's top. The bubble's grow at the open and shrink at the close, and each
/// page's fade in and out, are not kept.
fn bubble(cutscene: u8) -> (Anchor, [i16; 2], Vec<Quad>) {
    match cutscene {
        2 => (
            Anchor(Horizontal::Right, Vertical::Top),
            [214, 25],
            vec![
                quad(
                    "bubble",
                    [130, 0, 304, 55],
                    [256.25, 260.34375],
                    [374, 1102],
                    0x40,
                ),
                quad(
                    "tail",
                    [157, 45, 199, 58],
                    [271.0, 256.3125],
                    [-374, 1102],
                    0x40,
                ),
                quad(
                    "tail",
                    [170, 48, 190, 61],
                    [271.0, 256.71875],
                    [-750, 1122],
                    255,
                ),
                quad(
                    "bubble",
                    [124, 1, 297, 58],
                    [319.0, 256.21875],
                    [-375, 1122],
                    255,
                ),
            ],
        ),
        _ => (
            Anchor(Horizontal::Left, Vertical::Top),
            [71, 28],
            vec![
                quad(
                    "bubble",
                    [0, 0, 152, 58],
                    [257.96875, 257.21875],
                    [418, 1102],
                    0x40,
                ),
                quad(
                    "tail",
                    [97, 48, 135, 61],
                    [256.3125, 256.46875],
                    [418, 1102],
                    0x40,
                ),
                quad(
                    "tail",
                    [104, 51, 122, 64],
                    [256.34375, 256.875],
                    [840, 1122],
                    255,
                ),
                quad(
                    "bubble",
                    [0, 4, 146, 61],
                    [260.09375, 256.21875],
                    [419, 1122],
                    255,
                ),
            ],
        ),
    }
}

/// The voice line's WAV under `audio/`.
fn voice_file(entry: u16) -> String {
    format!("maps/2d/voice-{entry:x}.wav")
}

/// Both outros, each with its actor's character holding only the clip it plays.
pub fn outros(rom: &Rom) -> Result<Vec<(Outro, Character)>> {
    let game_data = rom.game_data()?;
    let records = rom.asset(&[0x0E, SLICE_SCENE])?;
    let model_of = |uid: u8| {
        (records.as_chunks::<RECORD>().0.iter())
            .find(|r| r[0x28] == uid)
            .map(|r| usize::from(r[4]))
            .with_context(|| format!("no character record for uid {uid:#x}"))
    };
    let mut out = Vec::new();
    for (winner, cutscene) in OUTROS {
        let asset = rom.asset(&[6, SLICE_SCENE, usize::from(cutscene)])?;
        let table = Table::parse(&asset)?;
        let entry = |i: usize| table.raw(i);
        let counts = entry(0)?;
        let [objects, curves, scripts, _] = [0, 2, 4, 6].map(|o| be16(counts, o) as usize);
        ensure!(
            curves == objects && scripts == objects,
            "a curve and a script per object"
        );
        let (kinds, first_curve, first_script) = (1 + objects, 1 + 2 * objects, 1 + 3 * objects);
        let kind = |o: usize| entry(kinds + o).map(|k| (be16(k, 0), k[2]));
        let script_of = |o: usize| script(entry(first_script + o)?);
        let find = |want: u16| {
            (0..objects)
                .find(|&o| kind(o).is_ok_and(|k| k.0 == want))
                .with_context(|| format!("cutscene {cutscene}: no object of kind {want}"))
        };
        let (camera, actor) = (find(1)?, find(2)?);
        ensure!(
            kind(camera)?.1 == 0,
            "cutscene {cutscene}: the camera looks by angles"
        );
        let carrier = (0..objects)
            .find(|&o| script_of(o).is_ok_and(|s| s.1.iter().any(|c| c.op == 7)))
            .context("no subtitle carrier")?;
        let (length, camera_script) = script_of(camera)?;
        let (_, actor_script) = script_of(actor)?;

        let eye = path(entry(1 + camera)?);
        let curve = entry(first_curve + camera)?;
        let camera = Camera {
            path: eye[..].try_into().context("a two-point camera path")?,
            along: keys(curve, 10),
            yaw: keys(curve, 7),
            pitch: keys(curve, 8),
        };

        // The actor: op 2 plays its clip; the next op 2 (the same clip, played on) sets the rate.
        let model = model_of(kind(actor)?.1)?;
        let plays: Vec<_> = actor_script.iter().filter(|c| c.op == 2).collect();
        ensure!(
            plays.len() == 2 && plays[1].b == plays[0].b && plays[0].d == 0,
            "cutscene {cutscene}: one clip played on"
        );
        let mut character = Character::load(rom, model)?;
        character.clips.retain(|c| c.logical.contains(&plays[0].b));
        let clip = (character.playable_clips().next())
            .with_context(|| format!("model {model}: no clip {}", plays[0].b))?;
        let ticks = clip.duration;
        let animation = conker_assets::ClipInfo {
            pair: clip.pair,
            logical: clip.logical.clone(),
            frames: clip.frames.len(),
            duration_ticks: ticks,
        }
        .animation();
        let facing = keys(entry(first_curve + actor)?, 7);
        let facing = facing.first().context("the actor's facing")?.value;
        let at = path(entry(1 + actor)?);
        let actor = Actor {
            character: model.to_string(),
            animation,
            ticks,
            rate: 2.0 * f32::from(ticks) / f32::from(plays[1].t - plays[0].t),
            at: *at.first().context("the actor's place")?,
            facing: ((facing + 90.0) * 65536.0 / 360.0) as i32 as u16,
            lift: model_lift(rom, model)?,
            scale: SCALE,
        };

        // The fade in (op 5 b=1, its e's high byte 0), to the next op 5 b=1.
        let fades: Vec<_> = (camera_script.iter())
            .filter(|c| c.op == 5 && c.b == 1)
            .collect();
        ensure!(
            fades.len() == 2 && fades[0].c == 0 && fades[0].e >> 8 == 0,
            "cutscene {cutscene}: one fade in"
        );
        let at = COLOURS + 3 * usize::from(fades[0].d);
        let rgb = game_data.get(at..at + 3).context("fade colour")?;
        let fade = Fade {
            rgb: rgb.try_into()?,
            duration: fades[1].t - fades[0].t,
        };

        let line = (actor_script.iter())
            .find(|c| c.op == 0x0B && c.c == 0)
            .context("no voice line")?;
        let voice = Voice {
            t: line.t,
            entry: line.b,
            file: voice_file(line.b),
        };

        // The pages: op 7 c 0 opens, 2 turns the page, 1 closes; the text is the last entry, 0xBD
        // between pages, `\n` between lines.
        let (_, carrier_script) = script_of(carrier)?;
        let ops: Vec<_> = carrier_script.iter().filter(|c| c.op == 7).collect();
        let text = entry(table.entries.len() - 1)?;
        let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
        let pages: Vec<Vec<String>> = (text.split(|&b| b == 0xBD))
            .map(|p| {
                (p.split(|&b| b == b'\n')
                    .map(|l| String::from_utf8_lossy(l).into()))
                .collect()
            })
            .collect();
        ensure!(
            ops.len() == pages.len() + 1 && ops.last().is_some_and(|c| c.c == 1),
            "cutscene {cutscene}: a command per page and a close"
        );
        let (anchor, text, quads) = bubble(cutscene);
        let subtitles = Subtitles {
            pages: (ops.iter().zip(pages))
                .map(|(c, lines)| Page { t: c.t, lines })
                .collect(),
            close: ops[ops.len() - 1].t,
            anchor,
            text,
            quads,
        };
        let outro = Outro {
            winner,
            cutscene,
            length,
            camera,
            actor,
            fade,
            voice,
            subtitles,
        };
        out.push((outro, character));
    }
    Ok(out)
}

/// The outros' voice lines as WAVs (path under `audio/`, file).
pub fn voices(rom: &Rom, outros: &[Outro]) -> Result<crate::Files> {
    (outros.iter())
        .map(|o| {
            let mp3 = rom.asset(&[MP3, usize::from(o.voice.entry)])?;
            Ok((
                format!("audio/{}", o.voice.file),
                crate::audio::mp3_wav(&mp3)?,
            ))
        })
        .collect()
}
