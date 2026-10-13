//! The War victory outros ([`conker_assets::outro`]): War: Total War's on scene 0x2D (cutscene 2 if
//! the squirrels win, 4 if the Tediz win), War: Colors' on 0x30 (1 and 5) and War: Beach's on 0x2B
//! (4 if the villagers win, 3 if the Tediz win), decoded from `[6, scene, n]` (loader
//! `func_1501D348`; the format in `../lab/research/mp-original/total-war-outro.md` §3, the Colors
//! ones in `colors-outro.md`, Beach's in `beach-presentation.md` §4) for what those six use, with their
//! actors' characters (each with the clips it plays) and their voice lines (bank 0x16's MP3s,
//! decoded to WAV), and the models their attachments and props show (`outro.gltf`, [`models`]);
//! the head turns (op 0x11), talk (op 5 b=0x17), part masks (op 5 b=0x3C), attachments (op 0x0D),
//! clip blends, kind-3 props, script sounds (ops 6, 0x0A), op 8's song (with its map's recording,
//! [`crate::PackMap::song`]) and the near plane (op 5 b=0x25) as
//! `../lab/research/mp-original/outros-leftovers.md` reads them. No generic cutscene interpreter:
//! the rest is left out (the tanks' smoke, op 5 b=0x0A; op 9's texture swap).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use conker_assets::hud::{Anchor, Horizontal, Rgba, Vertical};
use conker_assets::outro::{
    Actor, Attach, CHATTER, Camera, Fade, Head, Key, Light, OPEN, Outro, Page, Particles, Place,
    Play, Prop, Quad, SHUT, Say, ScriptSound, Subtitles, Talk, Voice,
};
use extract_character::{Character, parse_attachment};
use extract_core::{Rom, Table};
use extract_level::{IDENTITY, Instance, Level, Mesh, scene_level};
use extract_scene::scene_placements;
use serde_json::{Value, json};

use crate::{GAME_DATA, map_name, model_lift};

/// Each scene's (winning team, cutscene): `func_1501D348(scene, n)`, the first n if team 0 ranks
/// 1st, else the second (table 0x800AB7BC, V).
const OUTROS: [(usize, [(u8, u8); 2]); 5] = [
    (0x2D, [(0, 2), (1, 4)]),
    (0x30, [(0, 1), (1, 5)]),
    (0x2B, [(0, 4), (1, 3)]),
    // Heist: cutscene 2 if some human's team ranks 1st, else 3 (`func_151E43DC`'s scenario-0 case;
    // `heist-presentation-scratch/outro`): "winner 0" here is that human win.
    (0x1A, [(0, 2), (1, 3)]),
    // War: Raptor: cutscene 2 if the cavemen (team 0) rank 1st, else 4 (V b2, b1; `raptor.md` §6).
    (0x3F, [(0, 2), (1, 4)]),
];
/// Op 5 b=1's colour table (`func_15029BB8`: 3 bytes per entry, indexed by the record's d).
const COLOURS: usize = 0x8008_4110 - GAME_DATA;
/// The scene's character records (bank 0x0E entry `scene`, 0x30 bytes each): model at +4, uid at
/// +0x28 (C/V).
const RECORD: usize = 0x30;
/// A character record's scale (f32 at +0x20): 0.5 for every War actor, 0.74 for Heist's 134
/// (V obj+0x14C/+0x150).
const RECORD_SCALE: usize = 0x20;
/// The voice lines' bank.
const MP3: usize = 0x16;
/// Each actor model's head joint (`func_15036F34`'s a0: 3 for 90, 4 for 135 and 88;
/// `func_15036CE8`'s 0xC for the villagers) and jaw (the model's special joint 0, header words
/// 12/13) (C, V lldb; `outros-leftovers-scratch/joints.py`).
const HEADS: [(usize, u8, u8); 7] = [
    // Heist's Don Weaso: jaw 6 (header words 12/13); no op 0x11 turns his head (joint unknown).
    (134, u8::MAX, 6),
    (90, 3, 6),
    (88, 4, 7),
    (135, 4, 7),
    (156, 12, 15),
    (157, 12, 15),
    // War: Raptor's cavemen: jaw 15 (header words 12/13, C); head 12, the villagers' on the same
    // Tedi rig (I: the caveman's call is not traced).
    (136, 12, 15),
];
/// The talkers whose jaw does not skew (`func_15034860`: classes 0x69/0x7C/0x87, 135's).
const NO_SKEW: usize = 135;
/// The attachment actions' headers (`func_15083568`: a records pointer, a u8 count at +4; 16-byte
/// records, the bank-09 entry at +0, the joint at +1, the kind at +3).
const ACTIONS: usize = 0x8008_6CC4;
/// A placement record's byte the kind-3 objects name it by (the prop's uid, +0x72 at run time).
pub(crate) const PLACEMENT_UID: usize = 0x33;

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
                // A command before 0 never runs: the clock starts at 0 (V: Colors cs1's officer's
                // head turn at −1, cs5's tank sound 0x4A9 at −3 and −1).
                if t >= 0 {
                    out.push(Command {
                        t,
                        op,
                        b: be16(r, 2),
                        c: r[4],
                        d: r[5],
                        e: be16(r, 6),
                    });
                }
                t += i32::from(r[1]);
            }
        }
    }
    (length, out)
}

/// A command's time on the clock.
fn at(c: &Command) -> u16 {
    c.t.min(i32::from(u16::MAX)) as u16
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
        // Heist cs2: one place (`heist-presentation-scratch/outro`, DLs 500/587/657; the text's
        // centre fits left + floor(advance / 2), 194).
        (0x1A, 2) => vec![(
            top(Horizontal::Right),
            [194, 25],
            [
                ([110, 3, 282, 48], [256.0, 257.28125], [377, 1410]),
                ([160, 44, 202, 54], [271.0, 256.59375], [-377, 1410]),
                ([173, 47, 193, 57], [271.0, 257.125], [-756, 1442]),
                ([105, 8, 277, 52], [319.0, 257.0], [-378, 1442]),
            ],
        )],
        // Heist cs3: right (camera 0), left from 530 (camera 4), right from 800, left from 1300
        // (camera 4, 6 px wider), right from 1400 (DLs 205-880).
        (0x1A, _) => {
            let right = (
                top(Horizontal::Right),
                [196, 28],
                [
                    ([101, 0, 297, 58], [256.15625, 257.4375], [331, 1102]),
                    ([158, 52, 206, 65], [271.0, 256.875], [-331, 1102]),
                    ([173, 54, 196, 67], [271.0, 256.21875], [-665, 1122]),
                    ([96, 4, 292, 61], [319.0, 256.21875], [-332, 1122]),
                ],
            );
            let left = (
                top(Horizontal::Left),
                [77, 28],
                [
                    ([0, 0, 178, 58], [262.25, 257.4375], [331, 1102]),
                    ([38, 52, 86, 65], [271.0, 256.875], [-331, 1102]),
                    ([53, 54, 76, 67], [271.0, 256.21875], [-665, 1122]),
                    ([0, 4, 173, 61], [311.5625, 256.21875], [-332, 1122]),
                ],
            );
            let wider = (
                top(Horizontal::Left),
                [83, 28],
                [
                    ([0, 0, 184, 58], [260.34375, 257.4375], [331, 1102]),
                    ([44, 52, 92, 65], [271.0, 256.875], [-331, 1102]),
                    ([59, 54, 82, 67], [271.0, 256.21875], [-665, 1122]),
                    ([0, 4, 179, 61], [313.5, 256.21875], [-332, 1122]),
                ],
            );
            vec![right, left, right, wider, right]
        }
        // Raptor: no DLs of its bubbles yet (cs2 has two, texts b 0 and 1; cs4 one).
        (0x3F, _) => Vec::new(),
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

/// The song's recording under `audio/` ([`Outro::song`]).
pub fn song_file(scene: usize, entry: u16) -> String {
    format!("maps/{}/song-{entry:x}.wav", map_name(scene))
}

/// Whether scene `scene` has outros.
pub fn has_outros(scene: usize) -> bool {
    OUTROS.iter().any(|o| o.0 == scene)
}

/// Scene `scene`'s outros (none but on the scenes of `OUTROS`), each actor's clips named as in its
/// character's glTF ([`characters`]).
pub fn outros(rom: &Rom, scene: usize) -> Result<Vec<Outro>> {
    let Some((_, which)) = OUTROS.iter().find(|o| o.0 == scene) else {
        return Ok(Vec::new());
    };
    let game_data = rom.game_data()?;
    let placements = scene_placements(rom, scene)?;
    let records = rom.asset(&[0x0E, scene])?;
    let record_of = |uid: u8| {
        (records.as_chunks::<RECORD>().0.iter())
            .find(|r| r[0x28] == uid)
            .with_context(|| format!("no character record for uid {uid:#x}"))
    };
    let model_of = |uid: u8| record_of(uid).map(|r| usize::from(r[4]));
    let mut loaded: BTreeMap<usize, Character> = BTreeMap::new();
    let mut out = Vec::new();
    for &(winner, cutscene) in which {
        let asset = rom.asset(&[6, scene, usize::from(cutscene)])?;
        let table = Table::parse(&asset)?;
        let entry = |i: usize| table.raw(i);
        let counts = entry(0)?;
        let [objects, curves, n_scripts, _] = [0, 2, 4, 6].map(|o| be16(counts, o) as usize);
        let (kinds, first_curve, first_script) = (1 + objects, 1 + 2 * objects, 1 + 3 * objects);
        let kind = |o: usize| entry(kinds + o).map(|k| (be16(k, 0), k[2]));
        // Heist cs3's last object, a kind-3 prop, has no script (7 objects, 6 scripts).
        ensure!(
            curves == objects
                && (n_scripts == objects
                    || (n_scripts + 1 == objects && kind(objects - 1)?.0 == 3)),
            "a curve and a script per object"
        );
        let scripts: Vec<_> = (0..objects)
            .map(|o| match o < n_scripts {
                true => Ok(script(entry(first_script + o)?)),
                false => Ok((None, Vec::new())),
            })
            .collect::<Result<_>>()?;
        let all: Vec<(usize, Command)> = (scripts.iter().enumerate())
            .flat_map(|(o, s)| s.1.iter().map(move |c| (o, *c)))
            .collect();
        // `func_1501CE54`: object 0's 0xFE record (its record 2) ends the scene unless it is −9999
        // (Raptor's: cs2 1500, cs4 750, V 750 and 375 frames); else the longest 0xFF record.
        let cut = (entry(first_script)?.get(0x10..0x14))
            .filter(|r| r[0] == 0xFE && be16(r, 2) != 0xD8F1)
            .map(|r| be16(r, 2));
        let length =
            (cut.or_else(|| scripts.iter().filter_map(|s| s.0).max())).context("no length")?;
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
        let (mut actors, mut sounds, mut particles) = (Vec::new(), Vec::new(), Vec::new());
        for (n, o) in of_kind(2).into_iter().enumerate() {
            let model = model_of(kind(o)?.1)?;
            if let std::collections::btree_map::Entry::Vacant(e) = loaded.entry(model) {
                e.insert(crate::load_character(rom, model)?);
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
                    blend: match c.e {
                        0 => 5,
                        e => u8::try_from(e - 1)?,
                    },
                });
            }
            // Heist cs3's Wise Guy: its t-0 op 2 has c 2, which plays nothing (`func_15029BB8` acts
            // on c 0; V obj+0x84 0, no clip, till t 530): its first clip is held at frame 0 till then.
            if plays.first().is_some_and(|p| p.t > 0)
                && op2.first().is_some_and(|c| (c.t, c.c) == (0, 2))
            {
                let first = plays[0].clone();
                plays.insert(
                    0,
                    Play {
                        t: 0,
                        rate: 0.0,
                        once: true,
                        ..first
                    },
                );
            }
            ensure!(
                plays.first().is_some_and(|p| p.t == 0),
                "cutscene {cutscene} object {o}: a clip from t 0"
            );
            let first = plays.remove(0);
            let curve = entry(first_curve + o)?;
            let points = path(entry(1 + o)?)?;
            let yaw = keys(curve, 7);
            let turns = |k: &[Key]| k.windows(2).any(|w| w[0].value != w[1].value);
            let moving = |channel| Some(keys(curve, channel)).filter(|k| turns(k));
            actors.push(Actor {
                character: folder(model, &scripts[o].1)?,
                animation: first.animation,
                ticks: first.ticks,
                rate: first.rate,
                at: *points.first().context("the actor's place")?,
                facing: facing(yaw.first().map_or(0.0, |k| k.value)),
                lift: model_lift(rom, model)?,
                scale: {
                    let r = record_of(kind(o)?.1)?;
                    f32::from_be_bytes(r[RECORD_SCALE..RECORD_SCALE + 4].try_into()?)
                },
                once: first.once,
                along: match points.len() {
                    0 | 1 => Vec::new(),
                    _ => keys(curve, 10),
                },
                path: match points.len() {
                    0 | 1 => Vec::new(),
                    _ => points,
                },
                yaw: if turns(&yaw) { yaw } else { Vec::new() },
                plays,
                head: head(&scripts[o].1, model, curve)?,
                talk: talk(&scripts[o].1, model)?,
                mask: mask(&scripts[o].1)?,
                attach: attach(&scripts[o].1, &game_data)?,
                team: team(&scripts[o].1)?,
                roll: moving(6).unwrap_or_default(),
                pitch: moving(8).unwrap_or_default(),
            });
            sounds.extend(script_sounds(&scripts[o].1, n)?);
            particles.extend(particles_of(&scripts[o].1, n)?);
        }
        ensure!(
            (all.iter())
                .all(|(o, c)| !matches!(c.op, 6 | 0x0A) || kind(*o).is_ok_and(|k| k.0 == 2)),
            "cutscene {cutscene}: a sound on an object that is no actor"
        );
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
        let songs: Vec<_> = all.iter().map(|a| a.1).filter(|c| c.op == 8).collect();
        ensure!(songs.len() <= 1, "cutscene {cutscene}: songs");
        let song = songs.first().map(|c| Voice {
            t: at(c),
            entry: c.b,
            file: song_file(scene, c.b),
        });

        // The pages: op 7 c 0 opens, 2 turns the page, 1 closes; the text is the last entry, 0xBD
        // between pages, `\n` between lines.
        let carrier = (0..objects).find(|&o| scripts[o].1.iter().any(|c| c.op == 7));
        let subtitles = match carrier {
            // No bubble places (Raptor): no subtitles. Op 7's b picks the text, entry 1 + 3 ×
            // objects + scripts + b (C: Raptor's cs2 has b 0 and 1, two bubbles).
            Some(_) if bubbles(scene, cutscene).is_empty() => None,
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
        // The kind-3 objects: a hidden placement (its uid) shown and posed, rotated x by channel 8
        // and y by channel 7 (`func_15020EC4`).
        let mut props = Vec::new();
        for o in of_kind(3) {
            let uid = kind(o)?.1;
            let placement = (placements.iter())
                .position(|p| p.raw[PLACEMENT_UID] == uid)
                .with_context(|| format!("cutscene {cutscene}: no placement {uid:#x}"))?;
            let curve = entry(first_curve + o)?;
            let first = |ch| keys(curve, ch).first().map(|k| k.value);
            // Heist cs3's backdrop: channels 5 and 4 scale x and y in percent (V obj scale (37,
            // 37.01) for 3700, 3701); z keeps the placement's.
            let scale = match (first(5), first(4)) {
                (Some(x), Some(y)) => Some([x / 100.0, y / 100.0, placements[placement].scale[2]]),
                _ => None,
            };
            props.push(Prop {
                placement: u16::try_from(placement)?,
                at: *path(entry(1 + o)?)?.first().context("a prop's place")?,
                rotation: [first(8).unwrap_or(0.0), first(7).unwrap_or(0.0)],
                scale,
            });
        }
        // Heist's light: op 5 b=0x65 d 6 on a kind-0 object (`func_151619A0`), its colour channels
        // 0-2 and size channel 5 (V white, 8); its c 1 does not end it (V).
        let mut light = None;
        for o in of_kind(0) {
            for c in (scripts[o].1.iter()).filter(|c| (c.op, c.b, c.c) == (5, 0x65, 0)) {
                ensure!(
                    c.d == 6 && light.is_none(),
                    "cutscene {cutscene}: light {c:?}"
                );
                let curve = entry(first_curve + o)?;
                let first = |ch| keys(curve, ch).first().map_or(0.0, |k| k.value);
                light = Some(Light {
                    t: at(c),
                    at: *path(entry(1 + o)?)?.first().context("a light's place")?,
                    rgb: [0, 1, 2].map(|ch| first(ch) as u8),
                    size: first(5),
                });
            }
        }
        let near = (all.iter().map(|a| a.1))
            .find(|c| c.op == 5 && c.b == 0x25)
            .map(|c| f32::from(c.c));
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
            song,
            subtitles,
            props,
            sounds,
            near,
            light,
            particles,
        });
    }
    Ok(out)
}

/// Op 5 b=0x6F (`func_1502460C`): obj+0x68 + c = d; c 0 (every use) selects the team look d − 1.
/// The actor's character folder: its model, or `<model>-<segment>-<entry>` when op 9 draws a face
/// segment from a texture-table entry (obj+0x6C+c = b + 10, a code ≥ 10: segment 6 + c on entry
/// b; `outros-leftovers.md` §6). Colors cs5's lit tank is 40-7-9 (V cs5b: its segment 7 draws
/// 4368 and the other tank's 4216, both translucent); [`characters`] binds it. Its op is at t 1:
/// the folder draws it from t 0, a tick early.
fn folder(model: usize, script: &[Command]) -> Result<String> {
    let ops: Vec<_> = script.iter().filter(|c| c.op == 9).collect();
    Ok(match ops[..] {
        [] => model.to_string(),
        [c] if c.c < 2 => format!("{model}-{}-{}", 6 + c.c, c.b),
        _ => anyhow::bail!("model {model}: op 9 {ops:?}"),
    })
}

fn team(script: &[Command]) -> Result<Option<u8>> {
    let Some(c) = script.iter().find(|c| c.op == 5 && c.b == 0x6F) else {
        return Ok(None);
    };
    ensure!(c.c == 0 && c.d > 0, "a team look {c:?}");
    Ok(Some(c.d - 1))
}

/// Actor `actor`'s particle emitters: op 5 b=0x50 / 0x51 c 0 starts one, its next c 1 stops it.
fn particles_of(script: &[Command], actor: usize) -> Result<Vec<Particles>> {
    let mut out = Vec::new();
    for (i, c) in script.iter().enumerate() {
        if c.op == 5 && matches!(c.b, 0x50 | 0x51) && c.c == 0 {
            let stop = (script[i + 1..].iter())
                .find(|x| (x.op, x.b, x.c) == (5, c.b, 1))
                .with_context(|| format!("emitter {:#x}: no stop", c.b))?;
            out.push(Particles {
                actor: u8::try_from(actor)?,
                emitter: c.b,
                t: at(c),
                until: at(stop),
            });
        }
    }
    Ok(out)
}

/// Op 0x11 b=1 c=1 (on; every use): the head turn, channels 1 and 2 from its time.
fn head(script: &[Command], model: usize, curve: &[u8]) -> Result<Option<Head>> {
    let Some(on) = script.iter().find(|c| c.op == 0x11 && c.c == 1) else {
        return Ok(None);
    };
    ensure!(on.b == 1, "op 0x11 b {}", on.b);
    let (_, joint, _) = (HEADS.iter().find(|h| h.0 == model))
        .with_context(|| format!("model {model}: no head joint"))?;
    Ok(Some(Head {
        t: at(on),
        joint: *joint,
        pitch: keys(curve, 1),
        yaw: keys(curve, 2),
    }))
}

/// Op 5 b=0x17 (`func_1502460C`): c 0 talks (d 0 chatter, d 1 open) at e degrees (0: 45), c 1 shuts
/// (the degrees kept).
fn talk(script: &[Command], model: usize) -> Result<Option<Talk>> {
    let mut degrees = 45;
    let mut says = Vec::new();
    for c in script.iter().filter(|c| c.op == 5 && c.b == 0x17) {
        let state = match (c.c, c.d) {
            (0, 0) => CHATTER,
            (0, 1) => OPEN,
            (1, _) => SHUT,
            other => anyhow::bail!("talk {other:?}"),
        };
        if c.c == 0 {
            degrees = if c.e == 0 { 45 } else { u8::try_from(c.e)? };
        }
        says.push(Say {
            t: at(c),
            state,
            degrees,
        });
    }
    if says.is_empty() {
        return Ok(None);
    }
    let (_, _, jaw) =
        (HEADS.iter().find(|h| h.0 == model)).with_context(|| format!("model {model}: no jaw"))?;
    Ok(Some(Talk {
        jaw: *jaw,
        skew: model != NO_SKEW,
        says,
    }))
}

/// Op 5 b=0x3C: obj+0x94 = (c << 16) | e (d 0, every use, at t 0).
fn mask(script: &[Command]) -> Result<Option<u8>> {
    let Some(c) = script.iter().find(|c| c.op == 5 && c.b == 0x3C) else {
        return Ok(None);
    };
    ensure!((c.c, c.d, c.t) == (0, 0, 0), "a part mask {c:?}");
    Ok(Some(u8::try_from(c.e)?))
}

/// Op 0x0D: c 0 adds action b + 1's attachments, c 1 removes them (`func_15083568`).
fn attach(script: &[Command], game_data: &[u8]) -> Result<Vec<Attach>> {
    let mut out = Vec::new();
    for c in script.iter().filter(|c| c.op == 0x0D) {
        let header = ACTIONS - GAME_DATA + 8 * usize::from(c.b);
        let h = game_data.get(header..header + 8).context("action header")?;
        let at_ = (u32::from_be_bytes(h[..4].try_into()?) as usize)
            .checked_sub(GAME_DATA)
            .context("action records")?;
        let records =
            (game_data.get(at_..at_ + 16 * usize::from(h[4]))).context("action records")?;
        for r in records.as_chunks::<16>().0 {
            ensure!(
                matches!(r[3], 1 | 2),
                "action {:#x}: kind {}",
                c.b + 1,
                r[3]
            );
            out.push(Attach {
                t: at(c),
                entry: u16::from(r[0]),
                joint: r[1],
                on: c.c == 0,
            });
        }
    }
    Ok(out)
}

/// Actor `actor`'s script sounds: op 6 plays once; op 0x0A c 0 loops till the next c 1 of its id,
/// fading in up to a c 2 and out from a c 3 between; d is the pitch in 50 cents (`func_15029BB8`,
/// `func_150238D8`). A c 1-3 with no c 0 before it (its start before t 0) plays nothing.
fn script_sounds(script: &[Command], actor: usize) -> Result<Vec<ScriptSound>> {
    let mut out = Vec::new();
    for (i, c) in script.iter().enumerate() {
        let later = |k: u8| {
            script[i + 1..]
                .iter()
                .find(|x| x.op == 0x0A && x.b == c.b && x.c == k)
                .map(at)
        };
        let sound = ScriptSound {
            id: c.b,
            actor: u8::try_from(actor)?,
            t: at(c),
            stop: None,
            fade_in: None,
            fade_out: None,
            cents: i16::from(c.d) * 50,
        };
        match (c.op, c.c) {
            (6, _) => out.push(sound),
            (0x0A, 0) => {
                let stop = later(1).with_context(|| format!("loop {:#x}: no stop", c.b))?;
                let before = |t: Option<u16>| t.filter(|&t| t <= stop);
                out.push(ScriptSound {
                    stop: Some(stop),
                    fade_in: before(later(2)),
                    fade_out: before(later(3)),
                    ..sound
                });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The outros' attachments and props on `scene` as one pseudo-level (each part or mesh at the
/// origin: an attachment in its joint's frame, a prop where its outro poses it) and its glTF
/// scenes, named `09-<entry>` (hex) and `prop-<placement>`; none without any.
pub fn models(rom: &Rom, scene: usize, outros: &[Outro]) -> Result<Option<(Level, Value)>> {
    let attached: BTreeSet<u16> = (outros.iter().flat_map(Outro::actors))
        .flat_map(|a| a.attach.iter().map(|a| a.entry))
        .collect();
    let props: BTreeSet<u16> = (outros.iter().flat_map(|o| &o.props))
        .map(|p| p.placement)
        .collect();
    if attached.is_empty() && props.is_empty() {
        return Ok(None);
    }
    let game_data = rom.game_data()?;
    let (mut level, mut scenes) = (Level::default(), Vec::new());
    let add = |level: &mut Level, mesh: Mesh| {
        level.instances.push(Instance {
            mesh: level.meshes.len(),
            placement: None,
            matrix: IDENTITY,
        });
        level.meshes.push(mesh);
    };
    for e in attached {
        let first = level.instances.len();
        let parts = parse_attachment(&rom.asset(&[0x09, usize::from(e)])?, &game_data)?;
        for (part, ir) in parts.into_iter().enumerate() {
            let path = vec![0x09, usize::from(e), part];
            add(&mut level, Mesh { path, ir });
        }
        let nodes: Vec<usize> = (first..level.instances.len()).collect();
        scenes.push(json!({ "name": format!("09-{e:02x}"), "nodes": nodes }));
    }
    let placed = match props.is_empty() {
        true => Level::default(),
        false => scene_level(rom, scene)?,
    };
    for p in props {
        let first = level.instances.len();
        for i in (placed.instances.iter()).filter(|i| i.placement == Some(usize::from(p))) {
            add(&mut level, placed.meshes[i.mesh].clone());
        }
        ensure!(
            level.instances.len() > first,
            "scene {scene:#x}: no placement {p}"
        );
        let nodes: Vec<usize> = (first..level.instances.len()).collect();
        scenes.push(json!({ "name": format!("prop-{p}"), "nodes": nodes }));
    }
    Ok(Some((level, scenes.into())))
}

/// The outros' characters, each once with only the clips they play, by folder (an actor's
/// `character`: a `<model>-<segment>-<entry>` has that face segment bound to that entry).
pub fn characters(rom: &Rom, outros: &[Outro]) -> Result<Vec<(String, Character)>> {
    let mut played: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for a in outros.iter().flat_map(Outro::actors) {
        played
            .entry(&a.character)
            .or_default()
            .extend(a.clips().map(|c| c.0));
    }
    (played.into_iter())
        .map(|(folder, names)| {
            let mut parts = folder.split('-').map(str::parse::<usize>);
            let model = parts.next().context("a model")??;
            let mut character = crate::load_character(rom, model)?;
            if let (Some(segment), Some(entry)) = (parts.next(), parts.next()) {
                let id = crate::texture_id(rom, model, entry?)?;
                character = crate::bind_segment(character, segment? as u32, id);
            }
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
            Ok((folder.to_string(), character))
        })
        .collect()
}

/// The songs of a map's outros with its `recording` (path under `audio/`, file); without one, their
/// [`Outro::song`] goes.
pub fn songs(outros: &mut [Outro], recording: Option<&[u8]>) -> crate::Files {
    let mut files = Vec::new();
    for o in outros {
        match (&o.song, recording) {
            (Some(s), Some(wav)) => files.push((format!("audio/{}", s.file), wav.to_vec())),
            _ => o.song = None,
        }
    }
    files
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
