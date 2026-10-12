//! The War victory outros ([`conker_assets::outro`]): War: Total War's on scene 0x2D (cutscene 2 if
//! the squirrels win, 4 if the Tediz win) and War: Colors' on 0x30 (1 and 5), decoded from
//! `[6, scene, n]` (loader `func_1501D348`; the format in `../lab/research/mp-original/
//! total-war-outro.md` §3, the Colors ones in `colors-outro.md`) for what those four use, with their
//! actors' characters (each with the clips it plays) and their voice lines (bank 0x16's MP3s,
//! decoded to WAV). No generic cutscene interpreter: anything else they hold is left out (the head
//! channels 0-2, talk, attachment sets (op 0x0D), sounds (ops 6, 0x0A), effects (op 5 b=0x0A),
//! op 9, the clips' blends).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use conker_assets::hud::{Anchor, Horizontal, Rgba, Vertical};
use conker_assets::outro::{
    Actor, Camera, Fade, Key, Outro, Page, Place, Play, Quad, Subtitles, Voice,
};
use extract_character::Character;
use extract_core::{Rom, Table};

use crate::{GAME_DATA, map_name, model_lift};

/// Each scene's (winning team, cutscene): `func_1501D348(scene, n)`, the first n if team 0 ranks
/// 1st, else the second (table 0x800AB7BC, V).
const OUTROS: [(usize, [(u8, u8); 2]); 2] = [(0x2D, [(0, 2), (1, 4)]), (0x30, [(0, 1), (1, 5)])];
/// Op 5 b=1's colour table (`func_15029BB8`: 3 bytes per entry, indexed by the record's d).
const COLOURS: usize = 0x8008_4110 - GAME_DATA;
/// The scene's character records (bank 0x0E entry `scene`, 0x30 bytes each): model at +4, uid at
/// +0x28 (C/V).
const RECORD: usize = 0x30;
/// Characters are drawn at half their model units (V obj+0x14C/+0x150, as players).
const SCALE: f32 = 0.5;
/// The voice lines' bank.
const MP3: usize = 0x16;

/// A script command at its time: `u8 op, u8 delay, u16 b, u8 c, u8 d, u16 e`.
#[derive(Clone, Copy, Debug)]
struct Command {
    t: i32,
    op: u8,
    b: u16,
    c: u8,
    d: u8,
    e: u16,
}

fn be16(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

/// A script's length (its 0xFF record) and timed commands: record 0's +2 is the start time (s16:
/// the Colors officer's is −1, V), each command fires at the clock and moves it on by its delay;
/// op 0 sets it to `b`. Empty for an object without one (cs5's prop).
fn script(entry: &[u8]) -> (Option<u16>, Vec<Command>) {
    let records = entry.as_chunks::<8>().0;
    let Some(first) = records.first() else {
        return (None, Vec::new());
    };
    let mut t = i32::from(be16(first, 2) as i16);
    let (mut length, mut out) = (None, Vec::new());
    for r in &records[1..] {
        match r[0] {
            0xFF => length = Some(be16(r, 2)),
            0xFD | 0xFE => {}
            op => {
                if op == 0 {
                    t = i32::from(be16(r, 2));
                }
                out.push(Command {
                    t,
                    op,
                    b: be16(r, 2),
                    c: r[4],
                    d: r[5],
                    e: be16(r, 6),
                });
                t += i32::from(r[1]);
            }
        }
    }
    (length, out)
}

/// A command's time on the clock (one before 0 acts at once).
fn at(c: &Command) -> u16 {
    c.t.clamp(0, i32::from(u16::MAX)) as u16
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

/// An object's path points (`s16 x, y, z, u16 flag`): all Catmull-Rom (flag 0) in these outros.
fn path(entry: &[u8]) -> Result<Vec<[i16; 3]>> {
    let points = entry.as_chunks::<8>().0;
    ensure!(
        points.iter().all(|p| be16(p, 6) == 0),
        "a linear path segment"
    );
    Ok(points
        .iter()
        .map(|p| [0, 2, 4].map(|o| be16(p, o) as i16))
        .collect())
}

/// A u16 game angle from channel 7 (degrees): ch7 + 90°, truncated (V).
fn facing(ch7: f32) -> u16 {
    ((ch7 + 90.0) * 65536.0 / 360.0) as i32 as u16
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

/// A bubble as the DL draws it: its shadow (black, alpha 0x40) and the shadow of its tail, the
/// tail, the bubble (white); textures 0x1047 (`bubble`) and 0x1049 (`tail`).
type Bubble = [([i16; 4], [f32; 2], [i16; 2]); 4];

fn bubble_quads(b: Bubble) -> Vec<Quad> {
    let [shadow, tail_shadow, tail, white] = b;
    vec![
        quad("bubble", shadow.0, shadow.1, shadow.2, 0x40),
        quad("tail", tail_shadow.0, tail_shadow.1, tail_shadow.2, 0x40),
        quad("tail", tail.0, tail.1, tail.2, 255),
        quad("bubble", white.0, white.1, white.2, 255),
    ]
}

/// Where each outro's bubble and text go, in turn (V: the DLs at its pages, `total-war-outro-
/// scratch/hw/cs{2,4}b_dl_*/draws.json`, `colors-outro-scratch/hw/cs1_dl_*/draws.json`): the
/// bubble's quads, the text's centre x and middle line's top. Colors' cs1 moves its bubble where
/// the carrier's channel 8 changes (at its camera cuts). The bubble's grow at the open and shrink
/// at the close, and each page's fade in and out, are not kept.
fn bubbles(scene: usize, cutscene: u8) -> Vec<(Anchor, [i16; 2], Bubble)> {
    let top = |h| Anchor(h, Vertical::Top);
    match (scene, cutscene) {
        (0x2D, 2) => vec![(
            top(Horizontal::Right),
            [214, 25],
            [
                ([130, 0, 304, 55], [256.25, 260.34375], [374, 1102]),
                ([157, 45, 199, 58], [271.0, 256.3125], [-374, 1102]),
                ([170, 48, 190, 61], [271.0, 256.71875], [-750, 1122]),
                ([124, 1, 297, 58], [319.0, 256.21875], [-375, 1122]),
            ],
        )],
        (0x2D, _) => vec![(
            top(Horizontal::Left),
            [71, 28],
            [
                ([0, 0, 152, 58], [257.96875, 257.21875], [418, 1102]),
                ([97, 48, 135, 61], [256.3125, 256.46875], [418, 1102]),
                ([104, 51, 122, 64], [256.34375, 256.875], [840, 1122]),
                ([0, 4, 146, 61], [260.09375, 256.21875], [419, 1122]),
            ],
        )],
        // cs1: right (cameras 0), left (camera 3), its tail turned (camera 8), right again.
        _ => {
            let right = (
                top(Horizontal::Right),
                [197, 23],
                [
                    ([105, 0, 293, 53], [256.0, 262.65625], [345, 1102]),
                    ([135, 43, 181, 56], [271.0, 256.5], [-345, 1102]),
                    ([150, 46, 172, 59], [271.0, 256.90625], [-693, 1122]),
                    ([100, 0, 288, 57], [319.0, 257.3125], [-346, 1122]),
                ],
            );
            let left = (
                top(Horizontal::Left),
                [112, 23],
                [
                    ([21, 0, 209, 53], [256.21875, 262.65625], [345, 1102]),
                    ([51, 43, 97, 56], [271.0, 256.5], [-345, 1102]),
                    ([65, 46, 87, 59], [271.0, 256.90625], [-693, 1122]),
                    ([16, 0, 204, 57], [319.0, 257.3125], [-346, 1122]),
                ],
            );
            let turned = (
                top(Horizontal::Left),
                [79, 21],
                [
                    ([0, 0, 177, 51], [260.34375, 265.34375], [345, 1102]),
                    ([96, 43, 142, 56], [256.0, 256.5], [345, 1102]),
                    ([106, 46, 128, 59], [256.40625, 256.90625], [693, 1122]),
                    ([0, 0, 171, 55], [262.25, 259.53125], [346, 1122]),
                ],
            );
            vec![right, left, turned, right]
        }
    }
}

/// The voice line's WAV under `audio/`.
fn voice_file(scene: usize, entry: u16) -> String {
    format!("maps/{}/voice-{entry:x}.wav", map_name(scene))
}

/// Whether scene `scene` has outros.
pub fn has_outros(scene: usize) -> bool {
    OUTROS.iter().any(|o| o.0 == scene)
}

/// Scene `scene`'s outros (none but on 0x2D and 0x30), each actor's clips named as in its
/// character's glTF ([`characters`]).
pub fn outros(rom: &Rom, scene: usize) -> Result<Vec<Outro>> {
    let Some((_, which)) = OUTROS.iter().find(|o| o.0 == scene) else {
        return Ok(Vec::new());
    };
    let game_data = rom.game_data()?;
    let records = rom.asset(&[0x0E, scene])?;
    let model_of = |uid: u8| {
        (records.as_chunks::<RECORD>().0.iter())
            .find(|r| r[0x28] == uid)
            .map(|r| usize::from(r[4]))
            .with_context(|| format!("no character record for uid {uid:#x}"))
    };
    let mut loaded: BTreeMap<usize, Character> = BTreeMap::new();
    let mut out = Vec::new();
    for &(winner, cutscene) in which {
        let asset = rom.asset(&[6, scene, usize::from(cutscene)])?;
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
        let scripts: Vec<_> = (0..objects)
            .map(|o| Ok(script(entry(first_script + o)?)))
            .collect::<Result<_>>()?;
        let all: Vec<(usize, Command)> = (scripts.iter().enumerate())
            .flat_map(|(o, s)| s.1.iter().map(move |c| (o, *c)))
            .collect();
        let length = scripts
            .iter()
            .filter_map(|s| s.0)
            .max()
            .context("no length")?;
        let of_kind = |want: u16| {
            (0..objects)
                .filter(|&o| kind(o).is_ok_and(|k| k.0 == want))
                .collect::<Vec<_>>()
        };

        // The cameras: op 3 cuts to object b (`func_1502460C`; to the one already used: nothing).
        let camera = |o: usize, from: u16| -> Result<Camera> {
            ensure!(kind(o)?.0 == 1, "cutscene {cutscene}: op 3 to a non-camera");
            ensure!(
                kind(o)?.1 == 0,
                "cutscene {cutscene}: a camera looks by angles"
            );
            let curve = entry(first_curve + o)?;
            Ok(Camera {
                path: path(entry(1 + o)?)?,
                along: keys(curve, 10),
                yaw: keys(curve, 7),
                pitch: keys(curve, 8),
                from,
            })
        };
        let mut cuts: Vec<_> = (all.iter()).filter(|(_, c)| c.op == 3).collect();
        cuts.sort_by_key(|(_, c)| c.t);
        let (mut shown, mut cameras) = (None, Vec::new());
        for (_, c) in cuts {
            let o = usize::from(c.b);
            if shown != Some(o) && at(c) < length {
                shown = Some(o);
                cameras.push(camera(o, at(c))?);
            }
        }
        ensure!(
            cameras.first().is_some_and(|c| c.from == 0),
            "cutscene {cutscene}: a camera from t 0"
        );
        let camera = cameras.remove(0);

        // The actors: op 2 plays a clip (c 0) at rate 2 × its ticks / VIs to the object's next op 2
        // (a nonzero d: still), once if that next one plays a clip too (else it ends this one: a
        // loop; `func_15029BB8`'s flag to `func_1505E650`, V cs1's soldier's clip 37 holds at 24).
        let mut actors = Vec::new();
        for o in of_kind(2) {
            let model = model_of(kind(o)?.1)?;
            if let std::collections::btree_map::Entry::Vacant(e) = loaded.entry(model) {
                e.insert(Character::load(rom, model)?);
            }
            let character = &loaded[&model];
            let op2: Vec<_> = scripts[o].1.iter().filter(|c| c.op == 2).collect();
            let mut plays = Vec::new();
            for (n, c) in op2.iter().enumerate().filter(|(_, c)| c.c == 0) {
                let next = op2.get(n + 1).with_context(|| {
                    format!("cutscene {cutscene} object {o}: no op 2 after clip {}", c.b)
                })?;
                let clip = (character.playable_clips())
                    .find(|k| k.logical.contains(&c.b))
                    .with_context(|| format!("model {model}: no clip {}", c.b))?;
                let ticks = clip.duration;
                let rate = match c.d {
                    0 => 2.0 * f32::from(ticks) / (next.t - c.t) as f32,
                    _ => 0.0,
                };
                let animation = conker_assets::ClipInfo {
                    pair: clip.pair,
                    logical: clip.logical.clone(),
                    frames: clip.frames.len(),
                    duration_ticks: ticks,
                }
                .animation();
                plays.push(Play {
                    t: at(c),
                    animation,
                    ticks,
                    rate,
                    once: next.c == 0 || (next.c == 1 && next.d == 1),
                });
            }
            ensure!(
                plays.first().is_some_and(|p| p.t == 0),
                "cutscene {cutscene} object {o}: a clip from t 0"
            );
            let first = plays.remove(0);
            let curve = entry(first_curve + o)?;
            let points = path(entry(1 + o)?)?;
            let yaw = keys(curve, 7);
            let turns = yaw.windows(2).any(|w| w[0].value != w[1].value);
            actors.push(Actor {
                character: model.to_string(),
                animation: first.animation,
                ticks: first.ticks,
                rate: first.rate,
                at: *points.first().context("the actor's place")?,
                facing: facing(yaw.first().map_or(0.0, |k| k.value)),
                lift: model_lift(rom, model)?,
                scale: SCALE,
                once: first.once,
                along: match points.len() {
                    0 | 1 => Vec::new(),
                    _ => keys(curve, 10),
                },
                path: match points.len() {
                    0 | 1 => Vec::new(),
                    _ => points,
                },
                yaw: if turns { yaw } else { Vec::new() },
                plays,
            });
        }
        ensure!(!actors.is_empty(), "cutscene {cutscene}: no actor");
        let actor = actors.remove(0);

        // The fade in (op 5 b=1, its e's high byte 0), to the next op 5 b=1.
        let fades: Vec<_> = (all.iter().map(|a| a.1))
            .filter(|c| c.op == 5 && c.b == 1)
            .collect();
        ensure!(
            fades.len() == 2 && fades[0].c == 0 && fades[0].e >> 8 == 0,
            "cutscene {cutscene}: one fade in"
        );
        let colour = COLOURS + 3 * usize::from(fades[0].d);
        let rgb = game_data.get(colour..colour + 3).context("fade colour")?;
        let fade = Fade {
            rgb: rgb.try_into()?,
            duration: at(&fades[1]) - at(&fades[0]),
        };

        let lines: Vec<_> = (all.iter().map(|a| a.1))
            .filter(|c| c.op == 0x0B && c.c == 0)
            .collect();
        ensure!(lines.len() <= 1, "cutscene {cutscene}: voice lines");
        let voice = lines.first().map(|line| Voice {
            t: at(line),
            entry: line.b,
            file: voice_file(scene, line.b),
        });

        // The pages: op 7 c 0 opens, 2 turns the page, 1 closes; the text is the last entry, 0xBD
        // between pages, `\n` between lines.
        let carrier = (0..objects).find(|&o| scripts[o].1.iter().any(|c| c.op == 7));
        let subtitles = match carrier {
            None => None,
            Some(carrier) => {
                let ops: Vec<_> = (scripts[carrier].1.iter()).filter(|c| c.op == 7).collect();
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
                // The bubble moves where the carrier's channel 8 changes value.
                let ch8 = keys(entry(first_curve + carrier)?, 8);
                let moves: Vec<u16> = (ch8.windows(2))
                    .filter(|w| w[0].value != w[1].value)
                    .map(|w| w[1].t)
                    .collect();
                let mut places = bubbles(scene, cutscene).into_iter();
                let (anchor, text, quads) = places.next().context("a bubble")?;
                let moves: Vec<_> = (moves.into_iter().zip(places))
                    .map(|(t, (anchor, text, quads))| Place {
                        t,
                        anchor,
                        text,
                        quads: bubble_quads(quads),
                    })
                    .collect();
                ensure!(
                    moves.len() + 1 == bubbles(scene, cutscene).len(),
                    "cutscene {cutscene}: a bubble per place"
                );
                Some(Subtitles {
                    pages: (ops.iter().zip(pages))
                        .map(|(c, lines)| Page { t: at(c), lines })
                        .collect(),
                    close: at(ops[ops.len() - 1]),
                    anchor,
                    text,
                    quads: bubble_quads(quads),
                    moves,
                })
            }
        };
        out.push(Outro {
            winner,
            cutscene,
            length,
            camera,
            cuts: cameras,
            actor,
            cast: actors,
            fade,
            voice,
            subtitles,
        });
    }
    Ok(out)
}

/// The outros' characters, each once with only the clips they play.
pub fn characters(rom: &Rom, outros: &[Outro]) -> Result<Vec<Character>> {
    let mut played: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for a in outros.iter().flat_map(Outro::actors) {
        played
            .entry(&a.character)
            .or_default()
            .extend(a.clips().map(|c| c.0));
    }
    (played.into_iter())
        .map(|(model, names)| {
            let mut character = Character::load(rom, model.parse()?)?;
            character.clips.retain(|c| {
                let name = conker_assets::ClipInfo {
                    pair: c.pair,
                    logical: c.logical.clone(),
                    frames: c.frames.len(),
                    duration_ticks: c.duration,
                }
                .animation();
                names.contains(name.as_str())
            });
            Ok(character)
        })
        .collect()
}

/// The outros' voice lines as WAVs (path under `audio/`, file).
pub fn voices(rom: &Rom, outros: &[Outro]) -> Result<crate::Files> {
    (outros.iter().filter_map(|o| o.voice.as_ref()))
        .map(|v| {
            let mp3 = rom.asset(&[MP3, usize::from(v.entry)])?;
            Ok((format!("audio/{}", v.file), crate::audio::mp3_wav(&mp3)?))
        })
        .collect()
}
