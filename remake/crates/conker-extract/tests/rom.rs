//! The exports against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.
//! Slice numbers are the decoder crates' verified ones for scene 0x2D and model 117.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use conker_extract::{Layer, Slice};
use extract_core::Rom;
use gltf_json::validation::Validate;
use serde_json::Value;

fn rom_bytes() -> Option<Vec<u8>> {
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return None;
    };
    Some(std::fs::read(path).expect("read CONKER_ROM"))
}

fn rom() -> Option<Rom> {
    Some(Rom::new(rom_bytes()?).expect("the US ROM"))
}

/// The JSON and BIN chunks of a binary glTF 2.0 file, after checking the container layout,
/// gltf-json's validation (indices in range, required fields), every buffer view inside the
/// buffer, and every image an embedded PNG (no external URIs).
fn parse_glb(glb: &[u8]) -> (Value, &[u8]) {
    let word = |at: usize| u32::from_le_bytes(glb[at..at + 4].try_into().unwrap()) as usize;
    assert_eq!((&glb[..4], word(4), word(8)), (&b"glTF"[..], 2, glb.len()));
    let text = word(12);
    assert_eq!((text % 4, word(16)), (0, 0x4E4F_534A));
    let (json_bytes, chunk) = (&glb[20..20 + text], 20 + text);
    assert_eq!(word(chunk + 4), 0x004E_4942);
    let bin = &glb[chunk + 8..];
    assert_eq!((word(chunk), bin.len() % 4), (bin.len(), 0));

    let root = gltf_json::Root::from_slice(json_bytes).unwrap();
    let mut errors = Vec::new();
    root.validate(&root, gltf_json::Path::new, &mut |path, e| {
        errors.push(format!("{}: {e}", path()))
    });
    assert!(errors.is_empty(), "{errors:?}");

    let json: Value = serde_json::from_slice(json_bytes).unwrap();
    assert_eq!(json["asset"]["version"], "2.0");
    let length = json["buffers"][0]["byteLength"].as_u64().unwrap() as usize;
    assert!(length <= bin.len() && bin.len() - length < 4);
    assert!(!std::str::from_utf8(json_bytes).unwrap().contains("\"uri\""));
    let view = |i: &Value| {
        let v = &json["bufferViews"][i.as_u64().unwrap() as usize];
        let start = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        &bin[start..start + v["byteLength"].as_u64().unwrap() as usize]
    };
    for i in 0..json["bufferViews"].as_array().unwrap().len() {
        view(&i.into()); // in range
    }
    for image in json["images"].as_array().unwrap() {
        assert_eq!(image["mimeType"], "image/png");
        assert!(view(&image["bufferView"]).starts_with(b"\x89PNG"));
    }
    (json, bin)
}

/// The in-memory slice: both GLBs valid and self-contained, and the counts the decoder crates
/// verified for scene 0x2D and model 117. Prints the extraction time (run with `--release`).
#[test]
fn slice_in_memory() {
    let Some(bytes) = rom_bytes() else { return };
    let start = Instant::now();
    let slice = Slice::from_rom(bytes).unwrap();
    eprintln!(
        "Slice::from_rom: {:.0} ms; level GLB {} bytes, character GLB {} bytes",
        start.elapsed().as_secs_f64() * 1e3,
        slice.level_glb.len(),
        slice.character_glb.len()
    );

    let (level, _) = parse_glb(&slice.level_glb);
    let nodes = level["nodes"].as_array().unwrap();
    let placements: BTreeSet<u64> = nodes
        .iter()
        .filter_map(|n| n["extras"]["placement"].as_u64())
        .collect();
    assert_eq!((nodes.len(), placements.len()), (48, 46));
    assert_eq!(placements.last(), Some(&45));
    assert_eq!(level["meshes"].as_array().unwrap().len(), 27);
    assert_eq!(level["images"].as_array().unwrap().len(), 144);

    let (character, _) = parse_glb(&slice.character_glb);
    assert_eq!(
        character["skins"][0]["joints"].as_array().unwrap().len(),
        28
    );
    assert_eq!(character["images"].as_array().unwrap().len(), 4);
    let animations = character["animations"].as_array().unwrap();
    assert_eq!((animations.len(), slice.clips.len()), (764, 764));
    // The root-motion clips (the movement's creep, 0x16's attack, the shot death among them).
    let rooted: Vec<u16> = slice
        .root_clips
        .iter()
        .flat_map(|c| c.logical.clone())
        .collect();
    assert_eq!(slice.root_clips.len(), 10);
    assert!([0, 0x77, 0x17E].iter().all(|id| rooted.contains(id)));
    for (a, c) in animations.iter().zip(&slice.clips) {
        let e = &a["extras"];
        assert_eq!(
            (e["pair"].as_u64(), e["frames"].as_u64()),
            (Some(c.pair as u64), Some(c.frames as u64))
        );
        assert_eq!(e["durationTicks"], c.duration_ticks);
        assert_eq!(e["logicalIds"], serde_json::json!(c.logical));
    }

    // The weapons and the grenade in the hand: 15 models (the twin guns are two; map 0x1A's bat and
    // tommy gun), and the gas mask in the hand and on the head, one mesh each but the crossbow, a
    // child object with a list per joint.
    let (held, _) = parse_glb(&slice.held_glb);
    let names: Vec<&str> = held["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(slice.held.len(), 18);
    // The bat (bank 09 #0x38) in the right hand from its draw 0x10E (15 ticks in), away at 0x110
    // (its holster: the club's 0x2C3 is not it); the tommy gun (#0x5F) from 0x1DD at 11, away at
    // 0x1DE at 12 (ROM clip events, C).
    let find = |entry| slice.held.iter().find(|h| h.entry == entry).unwrap();
    let (bat, tommy) = (find(0x38), find(0x5F));
    assert_eq!((bat.joint, tommy.joint), (9, 9));
    assert_eq!(bat.create[0], (0x10E, 15.0));
    assert_eq!(bat.remove[0], (0x110, 14.0));
    assert_eq!(
        (tommy.create.clone(), tommy.remove.clone()),
        (vec![(0x1DD, 11.0)], vec![(0x1DE, 12.0)])
    );
    for (i, h) in slice.held.iter().enumerate() {
        let node = held["scenes"][i]["nodes"][0].as_u64().unwrap() as usize;
        let mesh = held["nodes"][node]["mesh"].as_u64().unwrap() as usize;
        let bank = if h.object { 1 } else { 9 };
        assert_eq!(names[mesh], format!("{bank:02x}-{:02x}-00", h.entry));
    }

    assert_eq!(slice.collision.len(), 4_299);
    let terrain: Vec<_> = slice
        .collision
        .iter()
        .filter(|t| t.layer == Layer::Terrain)
        .collect();
    assert_eq!(terrain.len(), 1_751);
    assert!(terrain.iter().all(|t| t.surface.is_some()));
    assert_eq!(
        slice
            .collision
            .iter()
            .filter(|t| t.surface == Some(5))
            .count(),
        865
    );
    assert_eq!(slice.spawns.len(), 20);
    // Player 1's record in the oracle's Deathmatch: (-2267, -521, 6016) is after a +16 jitter
    // along the facing, so only y is exact.
    assert_eq!(slice.spawns[1].position[1], -521);
}

#[test]
fn slice_files_and_counts() {
    let Some(rom) = rom() else { return };
    let export = conker_extract::slice(&rom).unwrap();
    let files: BTreeMap<&str, &[u8]> = export
        .files
        .iter()
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    assert_eq!(files.len(), export.files.len(), "duplicate paths");
    let json = |path: &str| -> Value { serde_json::from_slice(files[path]).unwrap() };

    assert_eq!(
        json("scene-2d/placements.json").as_array().unwrap().len(),
        46
    );
    let spawns = json("scene-2d/spawns.json");
    assert_eq!(spawns.as_array().unwrap().len(), 20);
    assert!(
        spawns
            .as_array()
            .unwrap()
            .iter()
            .all(|s| !s["floor"].is_null())
    );
    let collision = json("scene-2d/collision.json");
    assert_eq!(collision["faces"], 4_299);
    assert_eq!(collision["surface_histogram"]["0x00000005"], 865);
    let obj = std::str::from_utf8(files["scene-2d/collision.obj"]).unwrap();
    assert_eq!(obj.lines().filter(|l| l.starts_with("f ")).count(), 4_299);

    let level = json("level-2d/level-2d.gltf");
    assert_eq!(level["nodes"].as_array().unwrap().len(), 48);
    assert_eq!(level["meshes"].as_array().unwrap().len(), 27);
    let pngs = files
        .keys()
        .filter(|p| p.starts_with("level-2d/textures/"))
        .count();
    assert_eq!(pngs, level["images"].as_array().unwrap().len());
    assert_eq!(
        level["buffers"][0]["byteLength"],
        files["level-2d/level-2d.bin"].len()
    );

    let character = json("character-117/bank01-0117.gltf");
    assert_eq!(
        character["skins"][0]["joints"].as_array().unwrap().len(),
        28
    );
    assert_eq!(character["animations"].as_array().unwrap().len(), 764);
    assert_eq!(character["buffers"][0]["uri"], "bank01-0117.bin");
    let clips = json("character-117/clips.json");
    assert_eq!(clips["clips"].as_array().unwrap().len(), 764);
}

/// Measured 2026-10-08. The 3 warnings are draws whose own display list loads more than the
/// texture holds, not decoder bugs (checked in wave 2 against the raw commands):
/// - `[03,0x26]` (texture 882) and `[04,0x05,0]` (2814, also `[04,0x08|0x20|0x25,0]`, which share
///   the material): LOADBLOCK of 2048 16-bit texels and an RGBA16 32x64 tile (line 8, TLUT off,
///   no runtime state call) = 4,096 bytes. Both payloads are 2,560 bytes, the size of 2,405 CI8
///   64x32 textures + 256-entry palettes, and other lists draw both ids exactly so (CI8 32x64
///   tile, line 4, mode-1 palette: 882 in `[04,0x04,0x0B..0x0E]`, 2814 in 7 terrains including
///   0x2D's), where they decode.
/// - `[03,0x5a]` (texture 1637, placed only in scene 0x30): a CI8 16x16 tile with a 256-entry
///   TLUT from mode 1 (the last 0x200 bytes) = 256 + 512 bytes, but the payload is 160 bytes,
///   the CI4 16x16 + 16-entry palette layout of 3 other textures.
///
/// The game's loads read past (or before) these payloads; their textures stay off.
#[test]
fn textures_counts() {
    let Some(rom) = rom() else { return };
    let export = conker_extract::textures(&rom).unwrap();
    assert_eq!(export.files.len(), 2_894);
    assert!(
        export
            .files
            .iter()
            .all(|(p, png)| p.starts_with("textures/") && png.starts_with(b"\x89PNG"))
    );
    assert!(export.summary[0].contains("2894 of 7762 texture ids, from 1221 walked display lists; 0 models rejected, 3 warnings"), "{}", export.summary[0]);
}

/// The sound bank: 1,762 sounds, every one VADPCM, and the decoder reproduces the decoder state the
/// bank stores at each loop start (DevOldSchool's check: 359 of 362 frames exact, the rest within 2).
#[test]
fn sound_bank_decodes() {
    let Some(rom) = rom() else { return };
    let (sounds, wavetable) = conker_extract::audio::sound_bank(&rom).unwrap();
    assert_eq!(sounds.len(), 1762);
    let (mut looped, mut exact, mut worst, mut samples) = (0, 0, 0, 0);
    for s in &sounds {
        let w = &s.wave;
        let pcm =
            conker_extract::audio::decode(&wavetable[w.base..w.base + w.len], &w.book).unwrap();
        samples += pcm.len();
        if let Some(l) = w.looped {
            let frame = (l.start as usize / 16) * 16;
            let error = (pcm[frame..frame + 16].iter().zip(l.state))
                .map(|(&a, b)| (i32::from(a) - i32::from(b)).abs())
                .max()
                .unwrap();
            looped += 1;
            exact += usize::from(error == 0);
            worst = worst.max(error);
        }
    }
    eprintln!(
        "sounds: {} decoded samples, {looped} loops, {exact} exact, worst {worst}",
        samples
    );
    assert!(looped > 0 && worst <= 2, "{looped} loops, worst {worst}");
}

/// War: Total War's outros against the oracle's frames (`../lab/research/mp-original/
/// total-war-outro-scratch/outro_cs{2,4}.csv` and `fade_cs{2,4}.csv`, runs cs2a/cs4a: read 457 is
/// the outro's frame 0, a row the state after its read's frame): the camera's eye and look-at
/// within 0.05 units, its yaw within 0.001°, the actor's place, facing, clip and rate exact, its
/// clip time within 0.001 tick, the fade's alpha exact; and the voices' lengths (8.81 s, 12.24 s).
#[test]
fn outros_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let outros = conker_extract::outro::outros(&rom, 0x2D).unwrap();
    let characters = conker_extract::outro::characters(&rom, &outros).unwrap();
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../lab/research/mp-original/total-war-outro-scratch"
    );
    let rows = |file: &str| -> Option<Vec<Vec<String>>> {
        let text = std::fs::read_to_string(format!("{dir}/{file}")).ok()?;
        Some(
            text.lines()
                .skip(1)
                .map(|l| l.split(',').map(String::from).collect())
                .collect(),
        )
    };
    let ([cs2, cs4], [c2, c4]) = (&outros[..], &characters[..]) else {
        panic!("two outros, two characters")
    };
    assert_eq!((cs2.winner, cs2.cutscene, cs2.length), (0, 2, 650));
    assert_eq!((cs4.winner, cs4.cutscene, cs4.length), (1, 4, 800));
    assert_eq!((c2.entry, c2.model.joints.len(), c4.entry), (135, 40, 90));
    assert_eq!((c2.clips.len(), c4.clips.len()), (1, 1));
    assert_eq!(cs2.actor.animation, "pair_0000_anim_0000");
    assert_eq!(cs4.actor.animation, "pair_0045_anim_0056");
    assert!(cs2.cuts.is_empty() && cs2.cast.is_empty() && cs4.cuts.is_empty());
    let pages = |o: &conker_assets::outro::Outro| -> (Vec<u16>, u16) {
        let s = o.subtitles.as_ref().unwrap();
        (s.pages.iter().map(|p| p.t).collect(), s.close)
    };
    assert_eq!(pages(cs2), (vec![47, 159, 347, 475], 583));
    assert_eq!(pages(cs4), (vec![16, 195, 354, 601], 745));
    let lines = &cs4.subtitles.as_ref().unwrap().pages[0].lines;
    assert_eq!(lines[2], "      bears");
    let voice = |o: &conker_assets::outro::Outro| o.voice.as_ref().map(|v| (v.t, v.entry));
    assert_eq!(
        (voice(cs2), voice(cs4)),
        (Some((50, 0xB1)), Some((30, 0xB3)))
    );
    let voices = conker_extract::outro::voices(&rom, &[cs2.clone(), cs4.clone()]).unwrap();
    for ((_, wav), secs) in voices.iter().zip([8.81, 12.24]) {
        let (rate, samples) = conker_extract::audio::read_wav(wav).unwrap();
        let length = samples.len() as f32 / rate as f32;
        assert!(
            rate == 22050 && (length - secs).abs() < 0.08,
            "{rate} Hz {length} s"
        );
    }

    let (Some(..), Some(..)) = (rows("outro_cs2.csv"), rows("fade_cs4.csv")) else {
        eprintln!("{dir}: no oracle CSVs, skipping the frames");
        return;
    };
    for (o, n) in [(cs2, 2), (cs4, 4)] {
        let frames = rows(&format!("outro_cs{n}.csv")).unwrap();
        let last = frames.last().unwrap()[0].parse::<u32>().unwrap();
        for read in [458, 476, 556, 756, last] {
            let row = frames.iter().find(|r| r[0] == read.to_string()).unwrap();
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let k = read - 457;
            let t = o.t(k);
            assert_eq!(t, f(1), "cs{n} read {read}");
            let (eye, at) = o.camera(t);
            for i in 0..3 {
                assert!(
                    (eye[i] - f(4 + i)).abs() < 0.05,
                    "cs{n} read {read}: eye {eye:?}"
                );
                assert!(
                    (at[i] - f(7 + i)).abs() < 0.05,
                    "cs{n} read {read}: at {at:?}"
                );
            }
            let yaw = conker_assets::outro::value(&o.camera.yaw, t) + 90.0;
            assert!((yaw - f(10)).abs() < 1e-3, "cs{n} read {read}: yaw {yaw}");
            let a = &o.actor;
            assert_eq!(a.place(t), [f(17), f(18), f(19)]);
            assert_eq!(a.facing(t).to_string(), row[20]);
            let logical = if n == 2 { "0" } else { "56" };
            assert_eq!((row[21].as_str(), a.rate), (logical, f(22)));
            let time = o.clip_time(k);
            assert!(
                (time - f(23)).abs() < 1e-3,
                "cs{n} read {read}: clip {time}"
            );
        }
        for row in rows(&format!("fade_cs{n}.csv")).unwrap() {
            let read: u32 = row[0].parse().unwrap();
            if read > 457 {
                let want: u8 = row[6].parse().unwrap();
                assert_eq!(o.fade(o.t(read - 457)), want, "cs{n} read {read}");
            }
        }
    }
}

/// War: Colors' outros (scene 0x30's cutscenes 1 and 5) against the oracle's frames (`../lab/
/// research/mp-original/colors-outro-scratch/outro_cs{1,5}_{camera,actors}.csv`, `fade_cs{1,5}.csv`,
/// runs cs1/cs5: read 439 (cs1) or 438 (cs5) is the outro's frame 0, a row the state after its
/// read's frame), every frame from 1 through the STATS: the camera (its cuts) within 0.05 units, its
/// yaw within 0.001°; every actor's place within 0.05 (its path by length), facing within 1 (u16),
/// clip exact, clip time within 0.01 tick (on the loop); the fade's alpha exact; the length, characters, pages,
/// voice (cs1: 0xD0 at t 20, none in cs5).
#[test]
fn colors_outros_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let outros = conker_extract::outro::outros(&rom, 0x30).unwrap();
    let characters = conker_extract::outro::characters(&rom, &outros).unwrap();
    let [cs1, cs5] = &outros[..] else {
        panic!("two outros")
    };
    assert_eq!((cs1.winner, cs1.cutscene, cs1.length), (0, 1, 1110));
    assert_eq!((cs5.winner, cs5.cutscene, cs5.length), (1, 5, 1100));
    let entries: Vec<_> = characters.iter().map(|c| c.entry).collect();
    assert_eq!(entries, [135, 40, 88, 90]);
    let from = |o: &conker_assets::outro::Outro| -> Vec<u16> {
        std::iter::once(&o.camera)
            .chain(&o.cuts)
            .map(|c| c.from)
            .collect()
    };
    assert_eq!(
        (from(cs1), from(cs5)),
        (vec![0, 100, 260, 450], vec![0, 300, 800])
    );
    assert_eq!((cs1.actors().count(), cs5.actors().count()), (5, 9));
    let s = cs1.subtitles.as_ref().unwrap();
    let pages: Vec<u16> = s.pages.iter().map(|p| p.t).collect();
    assert_eq!((pages, s.close), (vec![36, 89, 245, 431, 606], 694));
    assert_eq!(s.pages[0].lines, ["look at this scum."]);
    let moves: Vec<u16> = s.moves.iter().map(|m| m.t).collect();
    assert_eq!(moves, [100, 260, 450]);
    assert!(cs5.subtitles.is_none() && cs5.voice.is_none());
    let v = cs1.voice.as_ref().unwrap();
    assert_eq!(
        (v.t, v.entry, v.file.as_str()),
        (20, 0xD0, "maps/30/voice-d0.wav")
    );

    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../lab/research/mp-original/colors-outro-scratch"
    );
    let rows = |file: &str| -> Option<Vec<Vec<String>>> {
        let text = std::fs::read_to_string(format!("{dir}/{file}")).ok()?;
        Some(
            text.lines()
                .skip(1)
                .map(|l| l.split(',').map(String::from).collect())
                .collect(),
        )
    };
    let Some(..) = rows("outro_cs1_camera.csv") else {
        eprintln!("{dir}: no oracle CSVs, skipping the frames");
        return;
    };
    for (o, n, first) in [(cs1, 1, 439), (cs5, 5, 438)] {
        for row in rows(&format!("outro_cs{n}_camera.csv")).unwrap() {
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let read: u32 = row[0].parse().unwrap();
            let k = read - first;
            if k == 0 {
                continue;
            }
            let t = o.t(k);
            assert_eq!(t, f(1), "cs{n} read {read}");
            let (eye, at) = o.camera(t);
            for i in 0..3 {
                assert!(
                    (eye[i] - f(4 + i)).abs() < 0.05,
                    "cs{n} read {read}: eye {eye:?}"
                );
                assert!(
                    (at[i] - f(7 + i)).abs() < 0.05,
                    "cs{n} read {read}: at {at:?}"
                );
            }
            let yaw = conker_assets::outro::value(&o.view(t).yaw, t) + 90.0;
            let want = f(10);
            assert!(
                ((yaw - want + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-3,
                "cs{n} read {read}: yaw {yaw}"
            );
        }
        let actors: Vec<_> = o.actors().collect();
        // The script objects of the actors, in order.
        let mut objects: Vec<u32> = Vec::new();
        for row in rows(&format!("outro_cs{n}_actors.csv")).unwrap() {
            let obj: u32 = row[2].parse().unwrap();
            if !objects.contains(&obj) {
                objects.push(obj);
            }
            let a = actors[objects.iter().position(|&x| x == obj).unwrap()];
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let read: u32 = row[0].parse().unwrap();
            let k = read - first;
            if k == 0 {
                continue;
            }
            let t = o.t(k);
            let place = a.place(t);
            for i in 0..3 {
                assert!(
                    (place[i] - f(6 + i)).abs() < 0.05,
                    "cs{n} read {read} object {obj}: place {place:?}"
                );
            }
            let facing: u16 = row[9].parse().unwrap();
            assert!(
                a.facing(t).abs_diff(facing) <= 1,
                "cs{n} read {read} object {obj}: facing {}",
                a.facing(t)
            );
            let (clip, time) = a.clip(k);
            let (name, ticks) = a.clips().nth(clip).unwrap();
            let logical: u16 = row[10].parse().unwrap();
            assert!(
                name.ends_with(&format!("_anim_{logical:04}"))
                    || characters.iter().any(|c| c.entry.to_string() == a.character
                        && c.clips.iter().any(|x| x.logical.contains(&logical)
                            && name.starts_with(&format!("pair_{:04}", x.pair)))),
                "cs{n} read {read} object {obj}: clip {name}, want {logical}"
            );
            // On the loop: the oracle's sum can stop a hair under the end where the remake wraps.
            let off = (time - f(12)).abs();
            assert!(
                off.min(f32::from(ticks) - off) < 0.01,
                "cs{n} read {read} object {obj}: clip time {time}, want {}",
                f(12)
            );
        }
        assert_eq!(objects.len(), actors.len());
        for row in rows(&format!("fade_cs{n}.csv")).unwrap() {
            let read: u32 = row[0].parse().unwrap();
            if read > first {
                let want: u8 = row[6].parse().unwrap();
                assert_eq!(o.fade(o.t(read - first)), want, "cs{n} read {read}");
            }
        }
    }
}

/// War: Beach's outros (scene 0x2B's cutscenes 4 and 3, `beach-presentation.md` §4) against the
/// oracle's frames (`../lab/research/mp-original/beach-presentation-scratch/outro_cs{3,4}_{camera,
/// actors}.csv`, `fade_cs{3,4}.csv`, runs cs4/cs3: read 298 (cs4) or 147 (cs3) is the outro's frame
/// 0), every frame from 1 through the STATS, as Colors' are checked; the lengths, cameras, actors,
/// characters (156 and 157 play clips the pack's villagers carry: the Tedi's), voices (cs4 0x163 at
/// t 200, cs3 0x161 at 40), no subtitles. Left out by the decoder: song 0x8A (op 8). The rest
/// (`outros-leftovers.md`) is [`outro_commands_follow_the_research`].
#[test]
fn beach_outros_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let outros = conker_extract::outro::outros(&rom, 0x2B).unwrap();
    let characters = conker_extract::outro::characters(&rom, &outros).unwrap();
    let [cs4, cs3] = &outros[..] else {
        panic!("two outros")
    };
    assert_eq!((cs4.winner, cs4.cutscene, cs4.length), (0, 4, 650));
    assert_eq!((cs3.winner, cs3.cutscene, cs3.length), (1, 3, 1150));
    let entries: Vec<_> = characters.iter().map(|c| c.entry).collect();
    assert_eq!(entries, [156, 157, 90]);
    let from = |o: &conker_assets::outro::Outro| -> Vec<u16> {
        std::iter::once(&o.camera)
            .chain(&o.cuts)
            .map(|c| c.from)
            .collect()
    };
    assert_eq!((from(cs4), from(cs3)), (vec![0, 400, 550], vec![0]));
    assert_eq!((cs4.actors().count(), cs3.actors().count()), (4, 8));
    assert!(cs4.subtitles.is_none() && cs3.subtitles.is_none());
    let voice = |o: &conker_assets::outro::Outro| {
        let v = o.voice.as_ref().unwrap();
        (v.t, v.entry, v.file.clone())
    };
    assert_eq!(voice(cs4), (200, 0x163, "maps/2b/voice-163.wav".into()));
    assert_eq!(voice(cs3), (40, 0x161, "maps/2b/voice-161.wav".into()));
    // The pack draws 156 and 157 with the Tedi's clips (the same bank-02 entry): every clip these
    // outros play is there under its name.
    let tedi = extract_character::Character::load(&rom, 117).unwrap();
    let names: BTreeSet<String> = (tedi.playable_clips())
        .map(|c| {
            conker_assets::ClipInfo {
                pair: c.pair,
                logical: c.logical.clone(),
                frames: c.frames.len(),
                duration_ticks: c.duration,
            }
            .animation()
        })
        .collect();
    for a in outros.iter().flat_map(|o| o.actors()) {
        if a.character != "90" {
            assert!(a.clips().all(|c| names.contains(c.0)), "{}", a.character);
        }
    }

    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../lab/research/mp-original/beach-presentation-scratch"
    );
    let rows = |file: &str| -> Option<Vec<Vec<String>>> {
        let text = std::fs::read_to_string(format!("{dir}/{file}")).ok()?;
        Some(
            text.lines()
                .skip(1)
                .map(|l| l.split(',').map(String::from).collect())
                .collect(),
        )
    };
    let Some(..) = rows("outro_cs4_camera.csv") else {
        eprintln!("{dir}: no oracle CSVs, skipping the frames");
        return;
    };
    for (o, n, first) in [(cs4, 4, 298), (cs3, 3, 147)] {
        let camera = rows(&format!("outro_cs{n}_camera.csv")).unwrap();
        for row in &camera {
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let read: u32 = row[0].parse().unwrap();
            let k = read - first;
            if k == 0 {
                continue;
            }
            let t = o.t(k);
            assert_eq!(t, f(1), "cs{n} read {read}");
            let (eye, at) = o.camera(t);
            for i in 0..3 {
                assert!(
                    (eye[i] - f(4 + i)).abs() < 0.05,
                    "cs{n} read {read}: eye {eye:?}"
                );
                assert!(
                    (at[i] - f(7 + i)).abs() < 0.05,
                    "cs{n} read {read}: at {at:?}"
                );
            }
            let yaw = conker_assets::outro::value(&o.view(t).yaw, t) + 90.0;
            assert!(
                ((yaw - f(10) + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-3,
                "cs{n} read {read}: yaw {yaw}"
            );
        }
        let actors: Vec<_> = o.actors().collect();
        let mut objects: Vec<u32> = Vec::new();
        let rows_actors = rows(&format!("outro_cs{n}_actors.csv")).unwrap();
        for row in &rows_actors {
            let obj: u32 = row[2].parse().unwrap();
            if !objects.contains(&obj) {
                objects.push(obj);
            }
            let a = actors[objects.iter().position(|&x| x == obj).unwrap()];
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let read: u32 = row[0].parse().unwrap();
            let k = read - first;
            if k == 0 {
                continue;
            }
            let t = o.t(k);
            let place = a.place(t);
            for i in 0..3 {
                assert!(
                    (place[i] - f(6 + i)).abs() < 0.05,
                    "cs{n} read {read} object {obj}: place {place:?}"
                );
            }
            let facing: u16 = row[9].parse().unwrap();
            assert!(
                a.facing(t).abs_diff(facing) <= 1,
                "cs{n} read {read} object {obj}: facing {}",
                a.facing(t)
            );
            let (clip, time) = a.clip(k);
            let (name, ticks) = a.clips().nth(clip).unwrap();
            let logical: u16 = row[10].parse().unwrap();
            assert!(
                name.ends_with(&format!("_anim_{logical:04}"))
                    || characters.iter().any(|c| c.entry.to_string() == a.character
                        && c.clips.iter().any(|x| x.logical.contains(&logical)
                            && name.starts_with(&format!("pair_{:04}", x.pair)))),
                "cs{n} read {read} object {obj}: clip {name}, want {logical}"
            );
            let off = (time - f(12)).abs();
            assert!(
                off.min(f32::from(ticks) - off) < 0.01,
                "cs{n} read {read} object {obj}: clip time {time}, want {}",
                f(12)
            );
        }
        assert_eq!(objects.len(), actors.len());
        // V: 325 + 575 camera rows, 1300 + 4600 actor rows.
        let frames = [(4, 325), (3, 575)].iter().find(|r| r.0 == n).unwrap().1;
        assert_eq!(
            (camera.len(), rows_actors.len()),
            (frames + 1, (frames + 1) * actors.len()),
            "cs{n} rows"
        );
        for row in rows(&format!("fade_cs{n}.csv")).unwrap() {
            let read: u32 = row[0].parse().unwrap();
            if read > first {
                let want: u8 = row[6].parse().unwrap();
                assert_eq!(o.fade(o.t(read - first)), want, "cs{n} read {read}");
            }
        }
    }
}

/// War: Beach's villagers (`beach-presentation.md` §5): classes 0x11-0x20 give 12 looks of model
/// 156 and 2 of 157, each hiding display lists by its mask; the glTF keeps each list in its own
/// primitives (`extras.list`), so a look draws what the original's mask leaves (V: e1 DL 270, 156
/// with mask 0x0E, calls lists 0, 4 and 5; e1 DL 99, 157 with 0x0A, lists 0, 2 and 4).
#[test]
fn villager_looks_hide_their_lists() {
    use conker_extract::villager_look;
    let masks: Vec<_> = (0x11..=0x20).map(|c| villager_look(c).unwrap()).collect();
    let male = [
        0x3A, 0x38, 0x1A, 0x18, 0x36, 0x34, 0x16, 0x14, 0x2E, 0x2C, 0x0E, 0x0C,
    ];
    let want: Vec<_> = (male.iter().map(|&m| (156, m)))
        .chain([0x0A, 0x0A, 0x14, 0x14].map(|m| (157, m)))
        .collect();
    assert_eq!(masks, want);
    assert_eq!((villager_look(0x10), villager_look(0x21)), (None, None));
    let Some(rom) = rom() else { return };
    let textures = rom.textures().unwrap();
    let tedi = extract_character::Character::load(&rom, 117).unwrap();
    let mut drawn = Vec::new();
    for (model, per_list) in [(156, &[11, 1, 1, 1, 1, 1][..]), (157, &[15, 1, 1, 1, 1])] {
        let c = conker_extract::on_tedi_rig(&rom, model, &tedi).unwrap();
        assert_eq!(c.model.lists.len(), per_list.len());
        let g = c.gltf(&textures, false).unwrap();
        let prims = g.json["meshes"][0]["primitives"].as_array().unwrap();
        let lists: Vec<u64> = (prims.iter())
            .map(|p| p["extras"]["list"].as_u64().unwrap())
            .collect();
        let counts: Vec<usize> = (0..per_list.len() as u64)
            .map(|k| lists.iter().filter(|&&l| l == k).count())
            .collect();
        assert_eq!(counts, per_list, "{model}");
        // Each list's triangles are all in its primitives.
        for (k, ir) in c.model.lists.iter().enumerate() {
            let triangles: usize = ir.draws.iter().map(|d| d.triangles.len()).sum();
            let indices: u64 = (prims.iter().filter(|p| p["extras"]["list"] == k))
                .map(|p| {
                    g.json["accessors"][p["indices"].as_u64().unwrap() as usize]["count"]
                        .as_u64()
                        .unwrap()
                })
                .sum();
            assert_eq!(indices, 3 * triangles as u64, "{model} list {k}");
        }
        for &(_, mask) in masks.iter().filter(|l| l.0 == model) {
            drawn.push(lists.iter().filter(|&&l| mask >> l & 1 == 0).count());
        }
    }
    // The primitives each class draws.
    assert_eq!(
        drawn,
        [
            12, 13, 13, 14, 12, 13, 13, 14, 12, 13, 13, 14, 17, 17, 17, 17
        ]
    );
    let shown = |mask: u8, n: u8| (0..n).filter(|k| mask >> k & 1 == 0).collect::<Vec<_>>();
    assert_eq!(
        (shown(0x0E, 6), shown(0x0A, 5)),
        (vec![0, 4, 5], vec![0, 2, 4])
    );
}

/// The mounted gun, bank 01 #138 (`mounted-gun.md` §5.2): its seat, pivot and barrels, and its
/// one clip, the fire loop.
#[test]
fn mounted_gun_model() {
    let Some(rom) = rom() else { return };
    let gun = extract_character::Character::load(&rom, conker_extract::MOUNTED_GUN).unwrap();
    let joints = &gun.model.joints;
    assert_eq!(joints.len(), 12);
    let at = |j: usize| joints[j].translation.map(|v| (v * 100.0).round() / 100.0);
    assert_eq!(at(3), [0.0, 144.29, -125.92]);
    assert_eq!(at(4), [0.0, 104.53, 122.84]);
    assert_eq!(
        (at(5), at(7)),
        ([31.35, 115.88, 49.24], [-31.35, 115.88, 49.24])
    );
    let parents: Vec<_> = (3..9).map(|j| joints[j].parent).collect();
    assert_eq!(
        parents,
        [Some(0), Some(0), Some(4), Some(5), Some(4), Some(7)]
    );
    let clips: Vec<_> = (gun.playable_clips())
        .map(|c| (c.pair, c.logical.clone(), c.duration))
        .collect();
    assert_eq!(clips, [(0, vec![1], 12)]);
}

/// What the six outros' other commands give (`../lab/research/mp-original/outros-leftovers.md`):
/// heads (op 0x11: Colors cs1's officer's at t −1 never runs), talk (op 5 b=0x17), part masks (op
/// 5 b=0x3C), attachments (op 0x0D through the action table), blends (op 2's e − 1), props (kind
/// 3: the placement by its uid, posed by channels 7/8), script sounds (no 0x4A9: its starts are
/// before t 0) and cs3's near plane.
#[test]
fn outro_commands_follow_the_research() {
    use conker_assets::outro::{Attach, CHATTER, Outro, Prop, SHUT};
    let Some(rom) = rom() else { return };
    let get = |scene| conker_extract::outro::outros(&rom, scene).unwrap();
    let (tw, colors, beach) = (get(0x2D), get(0x30), get(0x2B));
    let actors = |o: &Outro| o.actors().cloned().collect::<Vec<_>>();
    let head = |o: &Outro| -> Vec<Option<(u16, u8)>> {
        o.actors()
            .map(|a| a.head.as_ref().map(|h| (h.t, h.joint)))
            .collect()
    };
    // Heads: TW cs2's 135 (joint 4) and cs4's 90 (joint 3) at 0; cs5's 90s; none for cs1.
    assert_eq!(head(&tw[0]), [Some((0, 4))]);
    assert_eq!(head(&tw[1]), [Some((0, 3))]);
    assert!(head(&colors[0]).iter().all(Option::is_none));
    let cs5_turns: Vec<_> = head(&colors[1])
        .into_iter()
        .map(|h| h.map(|h| h.0))
        .collect();
    let none = None;
    assert_eq!(
        cs5_turns,
        [
            Some(198),
            Some(196),
            Some(198),
            Some(198),
            none,
            Some(300),
            Some(303),
            none,
            Some(697)
        ]
    );
    assert_eq!(head(&beach[0])[2..], [Some((0, 12)), Some((0, 12))]);
    let h = tw[1].actor.head.as_ref().unwrap();
    assert!(!h.yaw.is_empty() && !h.pitch.is_empty());
    // Talk: TW cs4's 90 chatters from 36 at 45° (e 0), shuts at 153; cs2's and cs1's 135 at 20°.
    let talk = tw[1].actor.talk.as_ref().unwrap();
    assert_eq!((talk.jaw, talk.skew, talk.says.len()), (6, true, 14));
    assert_eq!(
        (talk.says[0].t, talk.says[0].state, talk.says[0].degrees),
        (36, CHATTER, 45)
    );
    assert_eq!((talk.says[1].t, talk.says[1].state), (153, SHUT));
    let cs1_talk = colors[0].actor.talk.as_ref().unwrap();
    assert_eq!(
        (cs1_talk.jaw, cs1_talk.skew, cs1_talk.says[0].degrees),
        (7, false, 20)
    );
    assert!(tw[0].actor.talk.is_some() && actors(&colors[1]).iter().all(|a| a.talk.is_none()));
    // Masks: cs3's villagers (uids 0x14-0x17) and cs4's (0x14 has none).
    let masks = |o: &Outro| -> Vec<Option<u8>> {
        o.actors()
            .filter(|a| a.character != "90")
            .map(|a| a.mask)
            .collect()
    };
    assert_eq!(
        masks(&beach[1]),
        [Some(0x0E), Some(0x14), Some(0x36), Some(0x0A)]
    );
    assert_eq!(masks(&beach[0]), [None, Some(0x16), Some(0x2C), Some(0x0A)]);
    // Attachments: 0x57 on joint 14 for the Tediz (cs4, cs5 but the standing officer, cs3); the
    // 88s' 0x92 on 18, then obj 4's 0x64 on 23.
    let on = |t, entry, joint, on| Attach {
        t,
        entry,
        joint,
        on,
    };
    assert_eq!(tw[1].actor.attach, [on(0, 0x57, 14, true)]);
    let cs5 = actors(&colors[1]);
    for (n, a) in cs5.iter().enumerate() {
        let want = match n {
            1 => vec![on(0, 0x57, 14, true), on(799, 0x57, 14, false)],
            4 | 7 | 8 => vec![],
            _ => vec![on(0, 0x57, 14, true), on(1150, 0x57, 14, false)][..1].to_vec(),
        };
        assert_eq!(a.attach, want, "cs5 actor {n}");
    }
    let cs1 = actors(&colors[0]);
    assert_eq!(
        cs1[1].attach,
        [
            on(0, 0x92, 18, true),
            on(502, 0x92, 18, false),
            on(599, 0x64, 23, true)
        ]
    );
    assert_eq!(cs1[2].attach, [on(0, 0x92, 18, true)]);
    let tediz: Vec<_> = beach[1].actors().filter(|a| a.character == "90").collect();
    assert!(tediz.iter().all(|a| a.attach[0] == on(0, 0x57, 14, true)));
    // Blends: cs1's officer's e 0xA (9) and e 0 (5).
    let blends: Vec<u8> = cs1[0].plays.iter().map(|p| p.blend).collect();
    assert_eq!(blends[..2], [9, 5]);
    // Props: Beach cs4's 0xFC (placement 3), cs3's 0xFD (2), Colors cs5's 0xFF (0).
    let prop = |placement, at, rotation| Prop {
        placement,
        at,
        rotation,
    };
    assert_eq!(beach[0].props, [prop(3, [1076, -29, 257], [7.0, 174.0])]);
    assert_eq!(beach[1].props.len(), 1);
    assert_eq!(
        (beach[1].props[0].placement, beach[1].props[0].at),
        (2, [1929, -634, -1468])
    );
    assert_eq!(
        (colors[1].props[0].placement, colors[1].props[0].at),
        (0, [-52, -350, 3692])
    );
    assert_eq!(
        (beach[1].props[0].rotation, colors[1].props[0].rotation),
        ([0.0, -85.0], [0.0, -108.0])
    );
    assert!(tw.iter().chain(&colors[..1]).all(|o| o.props.is_empty()));
    // Sounds: cs5's tanks' 0x4A8 from 0 (in to 297, out from 788, stop 1045; in to 612, stop
    // 1091), the march 0x6DE from 1 and from 908 at +150 cents to 1100, 0x633 and 0x631 once.
    let s: Vec<_> = (colors[1].sounds.iter())
        .map(|s| (s.id, s.actor, s.t, s.stop, s.fade_in, s.fade_out, s.cents))
        .collect();
    assert_eq!(
        s,
        [
            (0x6DE, 1, 1, Some(1100), None, None, 0),
            (0x6DE, 1, 908, Some(1100), None, None, 150),
            (0x4A8, 4, 0, Some(1045), Some(297), Some(788), 0),
            (0x4A8, 7, 0, Some(1091), Some(612), None, 0),
            (0x633, 7, 1028, None, None, None, 0),
            (0x631, 7, 1085, None, None, None, 0),
        ]
    );
    assert!(
        tw.iter()
            .chain(&beach)
            .chain(&colors[..1])
            .all(|o| o.sounds.is_empty())
    );
    // The near plane: Beach cs3 only.
    let near: Vec<_> = tw
        .iter()
        .chain(&colors)
        .chain(&beach)
        .map(|o| o.near)
        .collect();
    assert_eq!(near, [None, None, None, None, None, Some(30.0)]);
    // The models: the three attachments and Beach's two props, Colors' one.
    let names = |scene, outros: &[Outro]| -> Vec<String> {
        let (_, scenes) = conker_extract::outro::models(&rom, scene, outros)
            .unwrap()
            .unwrap();
        scenes
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(names(0x2D, &tw), ["09-57"]);
    assert_eq!(names(0x30, &colors), ["09-57", "09-64", "09-92", "prop-0"]);
    assert_eq!(names(0x2B, &beach), ["09-57", "prop-2", "prop-3"]);
}
