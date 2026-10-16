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

    // The weapons and the grenades in the hand: 18 models (the twin guns are two; the Tediz' grenade
    // and everyone else's; map 0x1A's bat and tommy gun; War: Raptor's bone crossbow and knives),
    // the gas mask in the hand and on the head, Race's kick, item 0x3A's two grenades (#137 on
    // 0x1A, #186 on 0x3F: child objects, no events), the head stumps (#0x59, #0x58) and the racer
    // classes' helmets (#0x5A, #0x5C, #0x5B; no events), one mesh each but the crossbows and the
    // grenades, child objects with a list per joint.
    let (held, _) = parse_glb(&slice.held_glb);
    let names: Vec<&str> = held["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(slice.held.len(), 29);
    // The grenades on the left hand from the pull's t19 to the throw's t15: the Tediz' #75 (0x176,
    // 0x16B), everyone else's #76 (0x175, 0x16C; ROM clip events, C `func_15065A5C`).
    let grenade = |entry| slice.held.iter().find(|h| h.entry == entry).unwrap();
    let ends = |h: &extract_character::Held| (h.joint, h.create.clone(), h.remove.clone());
    assert_eq!(
        ends(grenade(75)),
        (4, vec![(0x176, 19.0)], vec![(0x16B, 15.0)])
    );
    assert_eq!(
        ends(grenade(76)),
        (4, vec![(0x175, 19.0)], vec![(0x16C, 15.0)])
    );
    // Race's kick (clip 0x354): bank 09 #0x34 in the right hand from tick 5 to 30 (V k1 DL 250).
    let kick = slice.held.iter().find(|h| h.entry == 0x34).unwrap();
    assert_eq!(
        (kick.joint, &kick.create[..], &kick.remove[..]),
        (9, &[(0x354, 5.0)][..], &[(0x354, 30.0)][..])
    );
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
    // Every held material is textured but 117's stump's (#0x59) shade-only draw (its asset's DL
    // loads no texture for it): the twin guns' last three images (3790, 3796, 3112) are sampled
    // through the tile described for them, not G_TEXTURE's stale tile 1 (extract-gbi).
    let materials = held["materials"].as_array().unwrap();
    let untextured: Vec<_> = (materials.iter())
        .filter(|m| m["pbrMetallicRoughness"]["baseColorTexture"].is_null())
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(untextured, ["m72-untextured"]);
    let twin = names.iter().position(|&n| n == "09-1a-00").unwrap();
    let images: Vec<_> = (held["meshes"][twin]["primitives"]
        .as_array()
        .unwrap()
        .iter())
    .map(|p| &materials[p["material"].as_u64().unwrap() as usize]["extras"]["texture"])
    .map(|t| (t["image"].as_str().unwrap(), t["size"].clone()))
    .collect();
    for (image, size) in [("3790", [16, 16]), ("3796", [16, 16]), ("3112", [16, 16])] {
        let flat = format!("Some(Flat {{ index: {image}, mode: 0 }})");
        assert!(
            images.contains(&(flat.as_str(), serde_json::json!(size))),
            "{image}"
        );
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

/// War: Total War's outros against the oracle's frames (`lab/research/mp-original/
/// total-war-outro-scratch/outro_cs{2,4}.csv` and `fade_cs{2,4}.csv`, runs cs2a/cs4a: read 457 is
/// the outro's frame 0, a row the state after its read's frame): the camera's eye and look-at
/// within 0.05 units, its yaw within 0.001°, the actor's place, facing, clip and rate exact, its
/// clip time within 0.001 tick, the fade's alpha exact; and the voices' lengths (8.81 s, 12.24 s).
#[test]
fn outros_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let outros = conker_extract::outro::outros(&rom, 0x2D).unwrap();
    let characters: Vec<_> = (conker_extract::outro::characters(&rom, &outros).unwrap())
        .into_iter()
        .map(|c| c.1)
        .collect();
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../lab/research/mp-original/total-war-outro-scratch"
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

/// War: Colors' outros (scene 0x30's cutscenes 1 and 5) against the oracle's frames (`lab/
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
    let entries: Vec<_> = characters.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(entries, ["135", "40", "40-7-9", "88", "90"]);
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
        "/../../lab/research/mp-original/colors-outro-scratch"
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
                    || characters.iter().any(|(folder, c)| *folder == a.character
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
/// oracle's frames (`lab/research/mp-original/beach-presentation-scratch/outro_cs{3,4}_{camera,
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
    let entries: Vec<_> = characters.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(entries, ["156", "157", "90"]);
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
        "/../../lab/research/mp-original/beach-presentation-scratch"
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
                    || characters.iter().any(|(folder, c)| *folder == a.character
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

/// Heist's outros (scene 0x1A's cutscenes 2 and 3, `heist-presentation.md` §4) against the oracle's
/// frames (`lab/research/mp-original/heist-presentation-scratch/outro/outro_cs{2,3}_{camera,
/// actors}.csv`, `fade_cs{2,3}.csv`, runs cs2/cs3: read 459 (cs2) or 157 (cs3) is the outro's frame
/// 0), every frame from 1 to the STATS: camera, actors (place, facing, scale, clip, clip time),
/// fade. cs3's Wise Guy plays no clip before t 530 (its t-0 op 2 is c 2): the decoder holds its
/// first clip at frame 0 there, and those rows (anim 0) are not compared. Also what the decoder
/// adds for 0x1A: Don Weaso (134) at 0.74 with no head turn, the voices, the Wise Guy's red shirt,
/// roll, cement shoe and bubbles, the white light and the black backdrop.
#[test]
fn heist_outros_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let outros = conker_extract::outro::outros(&rom, 0x1A).unwrap();
    let characters = conker_extract::outro::characters(&rom, &outros).unwrap();
    let [cs2, cs3] = &outros[..] else {
        panic!("two outros")
    };
    assert_eq!((cs2.winner, cs2.cutscene, cs2.length), (0, 2, 650));
    assert_eq!((cs3.winner, cs3.cutscene, cs3.length), (1, 3, 1550));
    let entries: Vec<_> = characters.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(entries, ["134", "59"]);
    let from = |o: &conker_assets::outro::Outro| -> Vec<u16> {
        std::iter::once(&o.camera)
            .chain(&o.cuts)
            .map(|c| c.from)
            .collect()
    };
    assert_eq!(
        (from(cs2), from(cs3)),
        (vec![0], vec![0, 530, 800, 1300, 1400])
    );
    let voice = |o: &conker_assets::outro::Outro| {
        let v = o.voice.as_ref().unwrap();
        (v.t, v.entry, v.file.clone())
    };
    assert_eq!(voice(cs2), (30, 0xB5, "maps/1a/voice-b5.wav".into()));
    assert_eq!(voice(cs3), (30, 0xCE, "maps/1a/voice-ce.wav".into()));
    let don = &cs3.actor;
    assert_eq!((don.character.as_str(), don.scale), ("134", 0.74));
    assert!(don.head.is_none() && don.talk.as_ref().is_some_and(|t| t.jaw == 6));
    // The light (obj 2: op 5 b=0x65 d 6, channels 0-2 and 5), in both.
    let light = conker_assets::outro::Light {
        t: 0,
        at: [-644, 180, 4728],
        rgb: [255; 3],
        size: 8.0,
    };
    assert!(cs2.light == Some(light.clone()) && cs3.light == Some(light));
    // cs3's Wise Guy (actor 1): REDS' shirt, the cement shoe (action 0x69: bank-09 0x51 on joint
    // 19) from t 400, the roll's 4 keys, sounds 10 and 12, the bubbles' five windows.
    let wise_guy = &cs3.cast[0];
    assert_eq!(
        (wise_guy.character.as_str(), wise_guy.team),
        ("59", Some(0))
    );
    let shoe = conker_assets::outro::Attach {
        t: 400,
        entry: 0x51,
        joint: 19,
        on: true,
    };
    assert_eq!(wise_guy.attach, [shoe]);
    let roll: Vec<_> = wise_guy.roll.iter().map(|k| (k.t, k.value)).collect();
    assert_eq!(roll, [(530, 0.0), (601, -16.0), (700, 13.0), (800, 0.0)]);
    assert!(wise_guy.pitch.is_empty() && don.team.is_none() && don.roll.is_empty());
    let sounds: Vec<_> = cs3.sounds.iter().map(|s| (s.id, s.actor, s.t)).collect();
    assert_eq!(sounds, [(0x0A, 1, 594), (0x0C, 1, 726)]);
    let particles: Vec<_> = (cs3.particles.iter())
        .map(|p| (p.actor, p.emitter, p.t, p.until))
        .collect();
    assert_eq!(
        particles,
        [
            (1, 0x50, 530, 566),
            (1, 0x51, 552, 605),
            (1, 0x50, 624, 653),
            (1, 0x51, 700, 738),
            (1, 0x51, 1296, 1321)
        ]
    );
    // The black backdrop: placement 13 (uid 0xD) at (−1503, 20, 4345), turned 117° (ch7), scaled
    // (37, 37.01) by channels 5 and 4, its own z (2.195).
    let [backdrop] = &cs3.props[..] else {
        panic!("one prop")
    };
    assert_eq!(
        (backdrop.placement, backdrop.at, backdrop.rotation),
        (13, [-1503, 20, 4345], [0.0, 117.0])
    );
    let scale = backdrop.scale.unwrap();
    assert!((0..3).all(|i| (scale[i] - [37.0, 37.01, 2.195][i]).abs() < 1e-4));
    assert!(cs2.props.is_empty() && cs2.particles.is_empty() && cs2.sounds.is_empty());
    let (_, scenes) = conker_extract::outro::models(&rom, 0x1A, &outros)
        .unwrap()
        .unwrap();
    let names: Vec<_> = (scenes.as_array().unwrap().iter())
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["09-51", "prop-13"]);

    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../lab/research/mp-original/heist-presentation-scratch/outro"
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
    let Some(..) = rows("outro_cs2_camera.csv") else {
        eprintln!("{dir}: no oracle CSVs, skipping the frames");
        return;
    };
    for (o, n, first) in [(cs2, 2, 459), (cs3, 3, 157)] {
        let mut compared = 0;
        for row in &rows(&format!("outro_cs{n}_camera.csv")).unwrap() {
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let read: u32 = row[0].parse().unwrap();
            let k = read - first;
            if k == 0 || k > o.frames() {
                continue;
            }
            compared += 1;
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
        assert_eq!(compared, o.frames(), "cs{n} camera rows");
        let actors: Vec<_> = o.actors().collect();
        let (mut objects, mut held, mut compared) = (Vec::new(), 0, 0);
        for row in &rows(&format!("outro_cs{n}_actors.csv")).unwrap() {
            let obj: u32 = row[2].parse().unwrap();
            if !objects.contains(&obj) {
                objects.push(obj);
            }
            let a = actors[objects.iter().position(|&x| x == obj).unwrap()];
            let f = |c: usize| row[c].parse::<f32>().unwrap();
            let read: u32 = row[0].parse().unwrap();
            let k = read - first;
            if k == 0 || k > o.frames() {
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
            assert!((a.scale - f(22)).abs() < 1e-6, "cs{n} object {obj} scale");
            // obj+0x68, the team look's selector: the Wise Guy's 1 (op 5 b=0x6F), else 0.
            let selector: u8 = row[28].parse().unwrap();
            assert_eq!(a.team.map_or(0, |t| t + 1), selector, "cs{n} {obj}");
            let roll = conker_assets::outro::value(&a.roll, t);
            assert!(
                (roll - f(24)).abs() < 1e-3,
                "cs{n} read {read}: roll {roll}"
            );
            let logical: u16 = row[10].parse().unwrap();
            if logical == 0 {
                held += 1;
                continue;
            }
            compared += 1;
            let (clip, time) = a.clip(k);
            let (name, ticks) = a.clips().nth(clip).unwrap();
            assert!(
                name.ends_with(&format!("_anim_{logical:04}"))
                    || characters.iter().any(|(folder, c)| *folder == a.character
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
        // The Wise Guy's held frames, t 2 to 528 (V: anim 0 till its op 2 at 530).
        assert_eq!(
            (compared, held),
            [(325, 0), (2 * 775 - 264, 264)][n - 2],
            "cs{n} rows"
        );
        let mut fades = 0;
        for row in rows(&format!("fade_cs{n}.csv")).unwrap() {
            let read: u32 = row[0].parse().unwrap();
            if read > first && read - first <= o.frames() {
                let want: u8 = row[6].parse().unwrap();
                assert_eq!(o.fade(o.t(read - first)), want, "cs{n} read {read}");
                fades += 1;
            }
        }
        assert_eq!(fades, o.frames(), "cs{n} fade rows");
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
    for (model, per_list) in [(156, &[11, 1, 1, 1, 1, 1][..]), (157, &[14, 1, 1, 1, 1])] {
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
            12, 13, 13, 14, 12, 13, 13, 14, 12, 13, 13, 14, 16, 16, 16, 16
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

/// The projectiles' models in flight (E8, V the oracle's batches: the grenade 12 + 8 + 8 vertices,
/// the rocket 16 with its nose at +484 and tail at −316, the bolt 23, the bone knife 23 + 22)
/// and the quads 0x2D and 0x34 never draw (bank-03 #0x33), out of their levels.
#[test]
fn projectiles_and_undrawn_quads() {
    let Some(rom) = rom() else { return };
    let (level, scenes) = conker_extract::projectiles_level(&rom).unwrap();
    let counts: Vec<_> = (level.meshes.iter()).map(|m| m.ir.vertices.len()).collect();
    assert_eq!(counts, [28, 16, 34, 23, 16, 45, 46, 46]);
    let z: Vec<_> = (level.meshes[1].ir.vertices.iter())
        .map(|v| v.position[2])
        .collect();
    assert_eq!((z.iter().min(), z.iter().max()), (Some(&-316), Some(&484)));
    assert!(level.meshes.iter().all(|m| m.ir.unresolved.len() <= 1));
    assert_eq!(scenes.as_array().unwrap().len(), 8);
    // Tank's shell: a five-sided prism with its nose at +48 (V sh1 DL 42, drawn at 1.5).
    let z: Vec<_> = (level.meshes[4].ir.vertices.iter())
        .map(|v| v.position[2])
        .collect();
    assert_eq!((z.iter().min(), z.iter().max()), (Some(&-32), Some(&48)));
    for (scene, quads) in [(0x2D, 6), (0x34, 2)] {
        let all = extract_level::scene_level(&rom, scene).unwrap();
        let undrawn = |l: &extract_level::Level| {
            (l.instances.iter())
                .filter(|i| l.meshes[i.mesh].path.starts_with(&conker_extract::UNDRAWN))
                .count()
        };
        assert_eq!(undrawn(&all), quads);
        assert_eq!(undrawn(&conker_extract::map_level(&rom, scene).unwrap()), 0);
    }
}

/// What the six outros' other commands give (`lab/research/mp-original/outros-leftovers.md`):
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
        scale: None,
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

/// Tank's model and map 0x24 (`lab/research/mp-original/tank.md`): the tank (bank 01 #119):
/// 14 joints, the turret (2) on the hull, the gun (3) on the turret, the muzzle (4) 110 along the
/// gun, the antenna (6-11), the treads (13, 12: the game's two scroll lists of 28 vertices, RAM
/// `0x800C4020[119]` = vertices 187 and 215, V); its 8 clips. Scene 0x24: 34 placements, the
/// bunker doors `[4, 36, 4]` (0-7) in pairs at the bunkers, the gas bomb (12) on the centre post
/// (the terrain's top at −617 under it), the 20 spawn records (starts: 0-3 the bunkers, 0x10-0x13),
/// the 6 power-up spots 60 above their nodes (V p1, c1).
#[test]
fn tank_model_and_map() {
    let Some(rom) = rom() else { return };
    let tank = extract_character::Character::load(&rom, conker_extract::TANK).unwrap();
    let joints = &tank.model.joints;
    let parents: Vec<_> = joints.iter().map(|j| j.parent).collect();
    let p = |j: u8| Some(j);
    assert_eq!(
        parents,
        [
            None,
            p(0),
            p(1),
            p(2),
            p(3),
            p(2),
            p(2),
            p(6),
            p(7),
            p(8),
            p(9),
            p(10),
            p(0),
            p(0)
        ]
    );
    let at = |j: usize| joints[j].translation.map(|v| (v * 100.0).round() / 100.0);
    assert_eq!(
        [at(2), at(3), at(4)],
        [
            [0.0, 80.15, -5.26],
            [0.0, 26.42, 53.14],
            [0.0, 10.13, 110.4]
        ]
    );
    assert_eq!(
        (at(12), at(13)),
        ([46.16, 83.52, 0.0], [-44.57, 81.93, 0.0])
    );
    // The treads' vertices (texture 4607, the texture table's entry 4; address index = RAM index
    // + 3) by joint.
    let mut treads: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    let tread = Some(extract_gbi::Image::Flat {
        index: 4607,
        mode: 0,
    });
    for l in &tank.model.lists {
        for d in (l.draws.iter()).filter(|d| d.material.texture.and_then(|t| t.image) == tread) {
            for &i in d.triangles.iter().flatten() {
                let v = &l.vertices[i as usize];
                let joint = (v.matrix.unwrap_or(0) & 0xFF_FFFF) / 0x40;
                treads
                    .entry(joint)
                    .or_default()
                    .insert((v.address & 0xFF_FFFF) / 16);
            }
        }
    }
    assert_eq!(treads.len(), 2);
    assert_eq!(treads[&13], (190..218).collect());
    assert_eq!(treads[&12], (218..246).collect());
    let clips: Vec<_> = (tank.playable_clips())
        .map(|c| (c.pair, c.logical.clone(), c.duration))
        .collect();
    assert_eq!(
        clips,
        [
            (0, vec![1, 3], 52),
            (1, vec![0, 2], 30),
            (2, vec![4], 26),
            (3, vec![5], 30),
            (4, vec![6], 8),
            (5, vec![7], 39),
            (6, vec![8], 10),
            (7, vec![9], 25)
        ]
    );

    let placements = extract_scene::scene_placements(&rom, conker_extract::TANK_MAP).unwrap();
    assert_eq!(placements.len(), 34);
    let doors: Vec<_> = placements[..8].iter().map(|p| p.position).collect();
    let bunkers: [[i16; 3]; 8] = [
        [-7041, -900, -688],
        [-7041, -900, -688],
        [7041, -900, 686],
        [7041, -900, 686],
        [-425, -1545, -7039],
        [-425, -1545, -7039],
        [425, -1545, 7039],
        [425, -1545, 7039],
    ];
    assert_eq!(doors, bunkers);
    assert_eq!(placements[12].position, [-20, -532, -108]);
    let level = extract_level::scene_level(&rom, conker_extract::TANK_MAP).unwrap();
    let model = |i: usize| {
        let inst = level
            .instances
            .iter()
            .find(|n| n.placement == Some(i))
            .unwrap();
        level.meshes[inst.mesh].path.clone()
    };
    assert!((0..8).all(|i| model(i) == [4, 0x24, 4]));
    let props: Vec<_> = [16, 8, 10, 11, 12, 9, 17].map(model).to_vec();
    assert_eq!(
        props,
        [
            [3, 88],
            [3, 71],
            [3, 81],
            [3, 70],
            [3, 80],
            [3, 72],
            [3, 95]
        ]
        .map(|m| m.to_vec())
    );
    let collision =
        extract_scene::scene_collision(&rom, conker_extract::TANK_MAP, &placements).unwrap();
    let terrain: Vec<_> = (collision.triangles.iter())
        .filter(|t| t.layer == Layer::Terrain)
        .cloned()
        .collect();
    let post = extract_scene::floor_below(&terrain, [-20.0, -532.0, -108.0], 0.0);
    assert_eq!(post.map(|f| f.1.round()), Some(-617.0));
    let spawns = extract_scene::scene_spawns(&rom, conker_extract::TANK_MAP).unwrap();
    assert_eq!(spawns.len(), 20);
    let starts = [0, 1, 2, 3, 16, 17, 18, 19].map(|k| spawns[k].position);
    let records: [[i16; 3]; 8] = [
        [7156, -901, 683],
        [-7146, -901, -674],
        [517, -1546, 7029],
        [-545, -1546, -7031],
        [-3772, -1067, -5288],
        [3778, -1069, 5341],
        [2257, -138, 2336],
        [-2169, -145, -2462],
    ];
    assert_eq!(starts, records);
    let (pickups, spots) = conker_extract::pickup_table(&rom, conker_extract::TANK_MAP).unwrap();
    let placed: Vec<_> = pickups.iter().map(|p| p.placement).collect();
    assert_eq!(
        (placed, spots),
        (
            vec![16, 8, 10, 11],
            vec![
                [-2512, 2, -2450],
                [3736, 138, -1061],
                [-3920, 160, 942],
                [-3593, -974, -5799]
            ]
        )
    );
    // The props glTF: the hidden placements in place, `model@placement`.
    let (props, scenes) = conker_extract::props_level(&rom, conker_extract::TANK_MAP).unwrap();
    // Scene 1: the carried gas bomb at the origin (bank 09 #0x3F, one part).
    assert_eq!(props.instances.len(), 23);
    assert_eq!(scenes[1]["nodes"], serde_json::json!([22]));
    assert_eq!(props.meshes[props.instances[22].mesh].path, [9, 0x3F, 0]);
    let names: Vec<_> = (props.instances[..22].iter())
        .map(|i| (props.meshes[i.mesh].path.clone(), i.placement.unwrap()))
        .collect();
    assert_eq!(
        names[..5],
        [
            (vec![3, 72], 9),
            (vec![3, 80], 12),
            (vec![3, 72], 13),
            (vec![3, 80], 14),
            (vec![3, 71], 15)
        ]
    );
    assert!(names[5..].iter().all(|(m, _)| m == &[3, 95]));
}

/// War: Raptor's models and outros (`lab/research/mp-original/raptor.md`): the raptor (bank 01
/// #83): 39 joints, the skull (5) on the neck (4), the jaw (6) and the crest (7) on the skull; the
/// baby (165) on the raptor's rig plays its clips (V r3); the egg (171) one joint. The outros (C, no
/// oracle CSVs): cs2 if the cavemen win, three cavemen talking, two turning their heads, voice
/// 0x1C5; cs4 if the raptors win, three raptors in part mask 2 and the baby; their 0xFF lengths
/// 1,530 and 830 VIs (the oracle's 1,500 and 749); no subtitles (no DLs of their bubbles).
#[test]
fn raptor_models_and_outros() {
    let Some(rom) = rom() else { return };
    let raptor = extract_character::Character::load(&rom, conker_extract::RAPTOR).unwrap();
    let joints = &raptor.model.joints;
    let parents: Vec<_> = (4..8).map(|j| joints[j].parent).collect();
    assert_eq!(
        (joints.len(), parents),
        (39, vec![Some(3), Some(4), Some(5), Some(5)])
    );
    let clips = |c: &extract_character::Character| {
        (c.playable_clips())
            .map(|k| (k.pair, k.logical.clone(), k.duration))
            .collect::<Vec<_>>()
    };
    let baby = conker_extract::load_character(&rom, conker_extract::BABY).unwrap();
    assert_eq!(baby.model.joints.len(), 39);
    assert_eq!(clips(&baby), clips(&raptor));
    assert_eq!(clips(&raptor).len(), 37);
    let egg = conker_extract::load_character(&rom, conker_extract::EGG).unwrap();
    assert_eq!((egg.model.joints.len(), clips(&egg).len()), (1, 0));

    let outros = conker_extract::outro::outros(&rom, conker_extract::TEMPLE).unwrap();
    let summary: Vec<_> = (outros.iter())
        .map(|o| {
            let cast: Vec<_> = o.actors().map(|a| a.character.as_str()).collect();
            (o.winner, o.cutscene, o.length, cast, o.subtitles.is_none())
        })
        .collect();
    assert_eq!(
        summary,
        [
            (0, 2, 1500, vec!["136"; 3], true),
            (1, 4, 750, vec!["83", "83", "83", "165"], true)
        ]
    );
    let voice = outros[0]
        .voice
        .as_ref()
        .map(|v| (v.t, v.entry, v.file.as_str()));
    assert_eq!(voice, Some((30, 0x1C5, "maps/3f/voice-1c5.wav")));
    let heads: Vec<_> = (outros[0].actors())
        .map(|a| (a.head.is_some(), a.talk.is_some()))
        .collect();
    assert_eq!(heads, [(true, true), (true, true), (false, true)]);
    let masks: Vec<_> = outros[1].actors().map(|a| a.mask).collect();
    assert_eq!(masks, [Some(2), Some(2), Some(2), None]);
}

/// Race's map 0x33 (`lab/research/mp-original/race.md`, V runs L1, L2, k1, cls): 120 nav nodes
/// and a 33-node lap from node 0x13 on each route, as the oracle's race table walks them; the four
/// barriers by uid, two shown per scenario; the 7 power-ups; script 0xBC; the board on joint 23 and
/// the bots' helmets on joint 12 by their actions; model 144 on the Tedi's rig.
#[test]
fn race_map_and_racers() {
    let Some(rom) = rom() else { return };
    let nav = rom.asset(&[0x19, 0x33]).unwrap();
    assert_eq!(u16::from_be_bytes([nav[0], nav[1]]), 120);
    let race = conker_extract::race_mode(&rom).unwrap();
    let low: Vec<u8> = [19, 89, 20, 21, 22, 23, 24, 25, 84, 26, 86, 27, 28, 0, 1, 82]
        .into_iter()
        .chain(2..=18)
        .collect();
    let high: Vec<u8> = [19, 89, 20, 21, 22]
        .into_iter()
        .chain(58..=69)
        .chain(3..=18)
        .collect();
    let routes: Vec<_> = (race.routes.iter())
        .map(|r| (r.scenario, r.shown.clone(), r.lap.clone(), r.map.as_str()))
        .collect();
    assert_eq!(
        routes,
        [
            (11, vec![0, 3], low, "race_map_low"),
            (12, vec![1, 2], high, "race_map_high")
        ]
    );
    assert!(race.routes.iter().all(|r| r.lap.len() == 33));
    assert_eq!(race.barriers, [0, 1, 2, 3]);
    let placements = extract_scene::scene_placements(&rom, 0x33).unwrap();
    let uids: Vec<u8> = (0..4).map(|i| placements[i].raw[0x33]).collect();
    assert_eq!(uids, [0xFF, 0xFE, 0xFD, 0xFC]);
    // Bundle segment `[4, 0x33, 4]` (dispatch 2, segment 4); the power-ups `[3, 100]` and `[3, 101]`.
    assert!((0..4).all(|i| (placements[i].dispatch, placements[i].model) == (2, 4)));
    let (pickups, spots) = conker_extract::pickup_table(&rom, 0x33).unwrap();
    let kinds: Vec<_> = (pickups.iter())
        .map(|p| {
            let at = &placements[usize::from(p.placement)];
            (p.placement, p.gives, (at.dispatch, at.model))
        })
        .collect();
    let (canister, missiles) = ((0, 100), (0, 101));
    let (c, m) = (
        conker_rules::weapon::Gives::Weapon(0x31),
        conker_rules::weapon::Gives::Weapon(0x33),
    );
    assert_eq!(
        kinds,
        [
            (4, c, canister),
            (5, c, canister),
            (6, m, missiles),
            (7, m, missiles),
            (8, m, missiles),
            (9, c, canister),
            (10, m, missiles)
        ]
    );
    assert!(spots.is_empty());
    assert_eq!(conker_extract::ai_script(0x33), 0xBC);
    assert!(!rom.asset(&[0x14, 0xBC]).unwrap().is_empty());
    assert_eq!(extract_scene::scene_spawns(&rom, 0x33).unwrap().len(), 17);

    // The board (action 0x79) and the helmets (0x76, 0x78, 0x77), bank-09 attachments.
    let attached: Vec<_> = (conker_extract::race_attached(&rom).unwrap().iter())
        .map(|h| (h.entry, h.joint, h.object))
        .collect();
    assert_eq!(
        attached,
        [
            (0x5D, 23, false),
            (0x5A, 12, false),
            (0x5C, 12, false),
            (0x5B, 12, false)
        ]
    );
    let game_data = rom.game_data().unwrap();
    let board = extract_character::parse_attachment(&rom.asset(&[9, 0x5D]).unwrap(), &game_data);
    assert_eq!(board.unwrap()[0].vertices.len(), 64);
    let at = |scene, joint| conker_assets::race::Attached { scene, joint };
    assert_eq!(race.board, at(0, 23));
    assert_eq!(
        race.heads,
        BTreeMap::from([(0xD, at(1, 12)), (0xE, at(2, 12)), (0xF, at(3, 12))])
    );
    let racers: Vec<_> = race.racers.iter().map(|(c, n)| (*c, n.as_str())).collect();
    assert_eq!(
        racers,
        [
            (0xB, "136"),
            (0xC, "144"),
            (0xD, "144"),
            (0xE, "144"),
            (0xF, "144")
        ]
    );
    let tedi = extract_character::Character::load(&rom, 117).unwrap();
    let racer = conker_extract::on_tedi_rig(&rom, conker_extract::RACER, &tedi).unwrap();
    assert_eq!((racer.model.joints.len(), racer.model.lists.len()), (28, 2));
}

/// Race's crowd and effects (F3, `lab/research/mp-original/race-effects-scratch`): each crowd
/// segment's frames from the two `.game` tables, in the order the original draws them (V: the ids
/// segments 4-7 draw at the counter's k in cd DLs 142 and 200 and w1 DL 218); 0x33's level drawing
/// every segment's first frame (F4: material 54 kept apart, though segment 4 draws its texture);
/// `race.json`'s panels; the effects' blobs at the sizes their DLs draw (V: equal to the RAM's), the
/// trail a ramp up and down.
#[test]
fn race_crowd_and_effects_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let frames = conker_extract::crowd_frames(&rom).unwrap();
    let segments: Vec<u32> = frames.iter().map(|f| f.0).collect();
    assert_eq!(segments, [4, 5, 6, 7]);
    let seen = [
        (60, [0x1260, 0x1266, 0x125B, 0x1259]),
        (89, [0x125E, 0x1264, 0x125A, 0x125D]),
        (98, [0x1266, 0x1262, 0x1259, 0x125C]),
    ];
    for (k, ids) in seen {
        let drawn: Vec<u32> = frames.iter().map(|(_, f)| f[k % 5]).collect();
        assert_eq!(drawn, ids, "k {k}");
    }
    let sets: Vec<BTreeSet<u32>> = frames.iter().map(|f| f.1.into()).collect();
    assert_eq!(
        sets[0],
        BTreeSet::from([0x125E, 0x1260, 0x1262, 0x1264, 0x1266])
    );
    assert_eq!(sets[0], sets[1]);
    assert_eq!(sets[2], (0x1259..=0x125D).collect());
    assert_eq!(sets[2], sets[3]);

    let level = conker_extract::map_level(&rom, 0x33).unwrap();
    let first: BTreeMap<u32, u32> = frames.iter().map(|&(s, ids)| (s, ids[0])).collect();
    assert_eq!(level.segments, first);
    assert_eq!(drawn_segments(&level), first.keys().copied().collect());
    let textures = rom.textures().unwrap();
    let gltf = extract_level::to_gltf(&level, &textures, "level.bin").unwrap();
    let json: Value = serde_json::from_str(&gltf.json).unwrap();
    let names: Vec<&str> = (json["materials"].as_array().unwrap().iter())
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 73);
    let bound = [
        "m53-seg06000000",
        "m54-tex4704",
        "m55-seg07000000",
        "m56-seg04000000",
        "m57-seg05000000",
    ];
    assert_eq!(names[53..58], bound);

    let race = conker_extract::race_mode(&rom).unwrap();
    let crowd = race.crowd.unwrap();
    assert_eq!(crowd.step, 4);
    let panels: Vec<&str> = crowd.panels.iter().map(|p| p.material.as_str()).collect();
    assert_eq!(
        panels,
        [
            "m56-seg04000000",
            "m57-seg05000000",
            "m53-seg06000000",
            "m55-seg07000000"
        ]
    );
    for (p, (_, ids)) in crowd.panels.iter().zip(&frames) {
        let files: Vec<String> = (ids.iter())
            .map(|id| format!("textures/{id:04}-64x32-rgba16-i.png"))
            .collect();
        assert_eq!(p.frames, files);
        assert_eq!(drawn_uri(&json, &p.material), files[0]);
    }
    let (_, images) = conker_extract::race_crowd(&rom, &textures).unwrap();
    assert_eq!(images.len(), 10);

    // Lamp and boost flares I8 64 × 64, the exhaust's four frames RGBA32 32 × 32, the trail I8 128.
    for (id, bytes) in [
        (0x235, 4096),
        (0xE21, 4096),
        (0x923, 4096),
        (0x926, 4096),
        (0xB39, 128),
    ] {
        assert_eq!(textures.get(id).unwrap().len(), bytes, "{id:#x}");
    }
    let trail = textures.get(0xB39).unwrap();
    assert_eq!((trail[0], trail[127]), (0, 0));
    assert_eq!(trail[64], 0xFF);
    assert!(trail[..64].is_sorted() && trail[64..].iter().rev().is_sorted());
}

/// The segments `level`'s draws read their texture from.
fn drawn_segments(level: &extract_level::Level) -> BTreeSet<u32> {
    (level.meshes.iter().flat_map(|m| &m.ir.draws))
        .filter_map(|d| match d.material.texture?.image? {
            extract_gbi::Image::Segment(a) => Some(a >> 24),
            _ => None,
        })
        .collect()
}

/// The image uri a glTF's material `name` draws.
fn drawn_uri<'a>(gltf: &'a Value, name: &str) -> &'a str {
    let m = (gltf["materials"].as_array().unwrap().iter())
        .find(|m| m["name"] == name)
        .unwrap_or_else(|| panic!("no material {name}"));
    let texture = &gltf["textures"][m["pbrMetallicRoughness"]["baseColorTexture"]["index"]
        .as_u64()
        .unwrap() as usize];
    gltf["images"][texture["source"].as_u64().unwrap() as usize]["uri"]
        .as_str()
        .unwrap()
}

/// 0x1A's runtime segments (F4, `lab/research/mp-original/race-effects-scratch/seg1a.py`): the
/// 5 ids `func_15100464` binds (V: the ids segments 2-8 draw in hud1-3, bag_dl, bag_ap, bag_dl3,
/// dm3, dm15, cs2 and cs3), `level.gltf`'s 9 segment materials drawing their first frame (lamps lit)
/// without merging, the screens' and lamps' frames, a lamp per switching light; and no other packed
/// map draws a runtime segment.
#[test]
fn runtime_segments() {
    let Some(rom) = rom() else { return };
    let ids = conker_extract::vault_ids(&rom).unwrap();
    assert_eq!(ids, [0x60B, 0x60C, 0x60D, 0x16C, 0xC7D]);
    let level = conker_extract::map_level(&rom, 0x1A).unwrap();
    let lit = (4..=8).map(|s| (s, 0x16C));
    let first: BTreeMap<u32, u32> = [(2, 0x60B), (3, 0x60C)].into_iter().chain(lit).collect();
    assert_eq!(level.segments, first);
    assert_eq!(drawn_segments(&level), first.keys().copied().collect());
    let textures = rom.textures().unwrap();
    let gltf = extract_level::to_gltf(&level, &textures, "level.bin").unwrap();
    let json: Value = serde_json::from_str(&gltf.json).unwrap();
    let names: Vec<&str> = (json["materials"].as_array().unwrap().iter())
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 104);
    let bound: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| n.contains("-seg"))
        .collect();
    assert_eq!(bound.len(), 9);
    let (segments, images) = conker_extract::map_segments(&rom, 0x1A, &textures).unwrap();
    let file = |id: u32| format!("textures/{id:04}-32x64-rgba16-i.png");
    let screens = segments.screens.unwrap();
    assert_eq!(screens.step, 8);
    let panels: Vec<_> = (screens.panels.iter())
        .map(|p| (p.material.as_str(), p.frames.clone()))
        .collect();
    assert_eq!(
        panels,
        [
            ("m69-seg02000000", [0x60B, 0x60C, 0x60D].map(file).to_vec()),
            ("m71-seg03000000", [0x60C, 0x60D, 0x60B].map(file).to_vec()),
        ]
    );
    // Light commands 8-0xC in trigger order: lamps on segments 4-8.
    let lamps: Vec<Vec<&str>> = (segments.lamps.iter())
        .map(|l| l.materials.iter().map(String::as_str).collect())
        .collect();
    assert_eq!(
        lamps,
        [
            vec!["m30-seg04000000", "m31-seg04000000"],
            vec!["m75-seg05000000"],
            vec!["m68-seg06000000"],
            vec!["m70-seg07000000"],
            vec!["m73-seg08000000", "m74-seg08000000"],
        ]
    );
    for l in &segments.lamps {
        assert_eq!((&l.on, &l.off), (&file(0x16C), &file(0xC7D)));
        for m in &l.materials {
            assert_eq!(drawn_uri(&json, m), l.on);
        }
    }
    for p in &screens.panels {
        assert_eq!(drawn_uri(&json, &p.material), p.frames[0]);
    }
    let pngs: Vec<String> = images.iter().map(|i| i.0.clone()).collect();
    assert_eq!(pngs, [0x60B, 0x60C, 0x60D, 0x16C, 0xC7D].map(file));
    for scene in [0x2D, 0x30, 0x34, 0x3F, 0x2B, 0x24] {
        let level = conker_extract::map_level(&rom, scene).unwrap();
        assert!(
            level.segments.is_empty() && drawn_segments(&level).is_empty(),
            "{scene:#x}"
        );
        let (segments, images) = conker_extract::map_segments(&rom, scene, &textures).unwrap();
        assert_eq!(
            (segments, images.len()),
            (Default::default(), 0),
            "{scene:#x}"
        );
    }
}

/// The pickable and cheat characters (`lab/research/mp-original/characters.md` §3, M14a): the
/// picks on the Tedi's rig (its joints, animation slots and bank-02 entry), Conker drawn as #1, every
/// menu row's hitbox and push body × its class's scale, truncated in f32, as the oracle's obj+0xD2-D6
/// and +0xE4-E8 (V `bodies.py`, all 22 rows, the bats as data), and the voice tables' equalities.
#[test]
fn pick_characters() {
    let Some(rom) = rom() else { return };
    let tedi = extract_character::Character::load(&rom, 117).unwrap();
    let rig = |c: &extract_character::Character| -> Vec<_> {
        (c.model.joints.iter())
            .map(|j| (j.parent, j.matrix, j.anim, j.flags))
            .collect()
    };
    for pick in conker_extract::PICKS {
        let own = extract_character::Character::load(&rom, pick).unwrap();
        assert_eq!((own.anim_entry, rig(&own)), (0, rig(&tedi)), "{pick}");
        let packed = conker_extract::mesh_only(&rom, pick).unwrap();
        assert_eq!((packed.entry, rig(&packed)), (pick, rig(&tedi)), "{pick}");
        assert!(packed.clips.is_empty() && packed.gltf(&rom.textures().unwrap(), true).is_ok());
    }
    let vertices = |c: &extract_character::Character| -> usize {
        c.model.lists.iter().map(|l| l.vertices.len()).sum()
    };
    let conker = conker_extract::mesh_only(&rom, 0).unwrap();
    let rep = |n| extract_character::Character::load(&rom, n).unwrap();
    assert_eq!(
        (vertices(&conker), vertices(&rep(0)), vertices(&rep(1))),
        (701, 913, 701)
    );
    // `HIDDEN`'s lists: 150's 5-9 load 0-4's vertices again (its second list table); the zombies
    // have 11 lists.
    let addresses = |c: &extract_character::Character, k: usize| -> Vec<u32> {
        c.model.lists[k]
            .vertices
            .iter()
            .map(|v| v.address)
            .collect()
    };
    let neo = rep(150);
    assert_eq!(neo.model.lists.len(), 10);
    assert!((0..5).all(|k| addresses(&neo, k) == addresses(&neo, k + 5)));
    assert_eq!(
        (rep(159).model.lists.len(), rep(160).model.lists.len()),
        (11, 11)
    );
    // (model, class scale, hitbox, body) of `D_800AB57C`'s rows 0-0x15.
    let rows: [(usize, f32, [i16; 3], [i16; 3]); 22] = [
        (128, 0.5, [36, 64, 44], [30, 66, 33]),
        (176, 0.6, [43, 76, 52], [36, 79, 39]),
        (117, 0.5, [26, 43, 35], [23, 45, 33]),
        (177, 0.6, [43, 76, 52], [36, 79, 39]),
        (59, 0.5, [36, 64, 44], [30, 66, 33]),
        (152, 0.5, [36, 64, 44], [17, 66, 33]),
        (136, 0.5, [36, 64, 44], [30, 66, 33]),
        (144, 0.5, [46, 80, 44], [30, 66, 33]),
        (144, 0.5, [46, 80, 44], [30, 66, 33]),
        (144, 0.5, [46, 80, 44], [30, 66, 33]),
        (83, 0.7, [125, 202, 125], [124, 203, 126]),
        (156, 0.5, [36, 64, 44], [30, 66, 33]),
        (155, 0.5, [92, 92, 5], [44, 66, 33]),
        (154, 0.5, [60, 64, 44], [59, 60, 44]),
        (144, 0.5, [46, 80, 44], [30, 66, 33]),
        (0, 0.5, [24, 50, 43], [25, 48, 45]),
        (150, 0.5, [24, 50, 43], [25, 48, 45]),
        (157, 0.5, [36, 64, 44], [28, 66, 33]),
        (159, 0.5, [25, 56, 37], [20, 58, 32]),
        (160, 0.5, [23, 56, 44], [15, 57, 33]),
        (178, 0.5, [26, 43, 35], [23, 45, 33]),
        (180, 0.5, [26, 43, 35], [23, 45, 33]),
    ];
    for (model, scale, hitbox, body) in rows {
        let (h, b) = conker_extract::hitbox_body(&rom, model).unwrap();
        let at = |v: [i16; 3]| v.map(|x| (f32::from(x) * scale) as i16);
        assert_eq!((at(h), at(b)), (hitbox, body), "{model}");
    }
    // Voice tables `[0x12, model]` (events 2000 on): equal ones share their sounds. 178's and 180's
    // are all zero (no voice); Conker's reps 1-4 have none.
    let voices = |m: usize| rom.asset(&[0x12, m]).unwrap().into_owned();
    for (a, b) in [
        (176, 128),
        (177, 117),
        (152, 59),
        (150, 0),
        (178, 180),
        (144, 136),
    ] {
        assert_eq!(voices(a), voices(b), "{a} = {b}");
    }
    assert!(voices(178).iter().all(|&b| b == 0) && !voices(178).is_empty());
    assert!(voices(1).is_empty());
    let own = [117, 128, 59, 136, 156, 157, 0, 159, 160].map(voices);
    assert!((0..own.len()).all(|i| (i + 1..own.len()).all(|j| own[i] != own[j])));
}

/// The faces (`lab/research/mp-original/faces.md`): each model's bank-0x11 entries for the eye
/// segments 6 and 7 (open, half shut, shut) and the mouth's 0x0A and 0x0B, and the texture ids its
/// table gives them (V: the oracle's RAM at those entries equals these ROM textures, `texcheck.py`
/// on runs c4, c2 and b1). `bind_faces` binds each face material to the open look (no segment left
/// in the glTF) and gives the blinkers' eyes; Conker draws model 1's table.
#[test]
fn faces_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let ids = |model: usize, entries: &[usize]| -> Vec<u32> {
        let asset = rom.asset(&[1, model]).unwrap();
        let word = |at: usize| u32::from_be_bytes(asset[at..at + 4].try_into().unwrap());
        (entries.iter())
            .map(|&e| word(word(0x18) as usize + 12 * e))
            .collect()
    };
    // (drawn model, segment 6's and 7's entries, 0x0A's and 0x0B's, the eyes' texture ids).
    let soldier = [4200, 4203, 4201];
    let caveman = [4730, 4732, 4731];
    let table: [(usize, [usize; 3], [usize; 3], [usize; 2], [u32; 3]); 14] = [
        (1, [25, 26, 27], [25, 26, 27], [14, 14], [1937, 1939, 1940]),
        (150, [16, 17, 1], [16, 17, 1], [15, 15], [1937, 1939, 1946]),
        (128, [1, 6, 6], [1, 6, 6], [0, 0], [4200, 4201, 4201]),
        (176, [0, 14, 14], [0, 14, 14], [0, 0], [4200, 4203, 4203]),
        (156, [0, 8, 4], [0, 8, 4], [0, 0], soldier),
        (157, [0, 10, 9], [0, 10, 9], [0, 0], soldier),
        (135, [0, 16, 10], [0, 16, 10], [0, 0], soldier),
        (88, [1, 10, 12], [1, 10, 12], [0, 0], soldier),
        (136, [0, 2, 5], [0, 2, 5], [0, 0], caveman),
        (144, [0, 4, 7], [0, 4, 7], [0, 0], caveman),
        (83, [1, 4, 5], [1, 4, 5], [0, 0], [3694, 3692, 3690]),
        (165, [8, 9, 10], [8, 9, 10], [0, 0], [3695, 3693, 3691]),
        (134, [4, 10, 10], [4, 10, 10], [0, 0], [3271, 3272, 3272]),
        (22, [2, 9, 9], [0, 5, 8], [0, 0], [4734, 4735, 4735]),
    ];
    let entries = |m, s| conker_extract::face_entries(&rom, m, s).unwrap();
    for (model, six, seven, mouth, eyes) in table {
        assert_eq!(
            (entries(model, 6), entries(model, 7)),
            (six.to_vec(), seven.to_vec())
        );
        assert_eq!(
            [entries(model, 0x0A), entries(model, 0x0B)],
            mouth.map(|e| vec![e])
        );
        assert_eq!(ids(model, &six), eyes, "{model}");
    }
    assert_eq!(ids(22, &[0, 5, 8]), caveman);
    assert_eq!(ids(1, &[14]), ids(150, &[15]));
    assert_eq!(ids(1, &[14]), [3649]);
    // Conker's record is model 0's too, and the tank in the outros (40) keeps its entry 0.
    assert_eq!(entries(0, 6), entries(1, 6));
    assert_eq!((entries(40, 6), entries(40, 7)), (vec![0; 3], vec![0; 3]));

    let textures = rom.textures().unwrap();
    let images = |c: &extract_character::Character| -> BTreeSet<String> {
        (c.model.lists.iter().flat_map(|l| &l.draws))
            .filter_map(|d| d.material.texture.and_then(|t| t.image))
            .map(|i| format!("{i:?}"))
            .collect()
    };
    let bind = |c| conker_extract::bind_faces(&rom, c, &textures).unwrap();
    let face = |e: [usize; 3]| e.map(|e| format!("textures/face-{e:02}.png"));
    // Conker: both eyes on one material (12), open 1937; the mouth 3649 on 9-11.
    let (conker, faces, pngs) = bind(conker_extract::mesh_only(&rom, 0).unwrap());
    assert!(images(&conker).iter().all(|i| !i.contains("Segment")));
    assert!(images(&conker).contains("Flat { index: 1937, mode: 0 }"));
    let segments: Vec<_> = faces
        .iter()
        .map(|f| (f.segment, f.materials.clone()))
        .collect();
    assert_eq!(segments, [(6, vec![12]), (7, vec![12])]);
    assert!(faces.iter().all(|f| f.textures == face([25, 26, 27])));
    assert_eq!(pngs.len(), 3);
    // The body (22) blinks with its two eyes apart; the baby (165) and Don Weaso (134) never.
    let (_, faces, pngs) = bind(conker_extract::load_character(&rom, 22).unwrap());
    let segments: Vec<_> = faces
        .iter()
        .map(|f| (f.segment, f.textures.clone()))
        .collect();
    assert_eq!(segments, [(6, face([2, 9, 9])), (7, face([0, 5, 8]))]);
    assert_eq!(pngs.len(), 5);
    for (model, open) in [(165, 3695), (134, 3271), (40, 4216)] {
        let (c, faces, pngs) = bind(conker_extract::load_character(&rom, model).unwrap());
        assert!(faces.is_empty() && pngs.is_empty(), "{model}");
        assert!(images(&c).iter().all(|i| !i.contains("Segment")), "{model}");
        assert!(images(&c).contains(&format!("Flat {{ index: {open}, mode: 0 }}")));
    }
}

/// A textured part whose combiner reads no texel draws its vertex colours (F4; V the client
/// agent's a1, frame 100: Conker's head and tail, material 11 on texture 3649, orange): its glTF
/// material has no texture.
#[test]
fn parts_ignoring_their_texture_draw_vertex_colours() {
    let Some(rom) = rom() else { return };
    let textures = rom.textures().unwrap();
    let conker = conker_extract::mesh_only(&rom, 0).unwrap();
    let conker = conker_extract::bind_faces(&rom, conker, &textures)
        .unwrap()
        .0;
    let g = conker.gltf(&textures, false).unwrap().json;
    let m = &g["materials"][11];
    assert_eq!(m["extras"]["image"], "flat 3649 mode 0");
    assert_eq!(m["extras"]["combine"], "fffe8ff517f8ff");
    assert!(m["pbrMetallicRoughness"]["baseColorTexture"].is_null());
}

/// The characters' materials draw as the oracle draws them (`lab/research/mp-original/
/// alpha-scratch`, `check.py`: the render mode and culling at each texture's draws, V runs a1, r1,
/// and cs5b, c1b, x1): (model, texture, its glTF materials' (alphaMode, doubleSided)). Segment 8
/// is the renderer's render-state table, the lists start culling back faces. Colors cs5's lit tank
/// (40-7-9) draws segment 7 from entry 9 (4368), the other tank (40) from entry 0 (4216).
#[test]
fn character_materials_follow_the_oracle() {
    let Some(rom) = rom() else { return };
    let textures = rom.textures().unwrap();
    let modes = |g: &Value, id: u32| -> BTreeSet<(String, bool)> {
        (g["materials"].as_array().unwrap().iter())
            .filter(|m| m["extras"]["image"] == format!("flat {id} mode 0"))
            .map(|m| {
                let alpha = m["alphaMode"].as_str().unwrap();
                assert_eq!(m["alphaCutoff"].as_f64(), (alpha == "MASK").then_some(0.5));
                (alpha.to_string(), m["doubleSided"].as_bool().unwrap())
            })
            .collect()
    };
    let (o, b, m) = ("OPAQUE", "BLEND", "MASK");
    let table: [(usize, u32, &[(&str, bool)]); 14] = [
        (0, 1937, &[(b, false)]), // the eyes
        (0, 3095, &[(o, false)]),
        (150, 1937, &[(b, false)]),
        (150, 3763, &[(b, false)]),
        (117, 4402, &[(o, false)]),
        (90, 4545, &[(m, false)]),
        (40, 4364, &[(m, true)]),
        (40, 4216, &[(o, false), (b, false)]), // the lights (segments 6, 7) and a flat use
        (136, 4730, &[(o, false), (m, false)]), // the eyes (V the half look 4732)
        (136, 4725, &[(o, false), (m, false), (m, true)]),
        (169, 2293, &[(b, true)]), // the cloth
        (169, 1637, &[(m, true)]),
        (138, 988, &[(m, true)]), // the belt
        (157, 1356, &[(o, false), (o, true), (b, false)]),
    ];
    for (model, id, want) in table {
        let c = match model {
            0 | 150 => conker_extract::mesh_only(&rom, model).unwrap(),
            _ => conker_extract::load_character(&rom, model).unwrap(),
        };
        let c = conker_extract::bind_faces(&rom, c, &textures).unwrap().0;
        let g = c.gltf(&textures, false).unwrap().json;
        let want: BTreeSet<_> = want.iter().map(|&(a, d)| (a.to_string(), d)).collect();
        assert_eq!(modes(&g, id), want, "{model}: {id}");
    }
    let outros = conker_extract::outro::outros(&rom, 0x30).unwrap();
    let tanks: Vec<_> = (conker_extract::outro::characters(&rom, &outros).unwrap())
        .into_iter()
        .filter(|c| c.1.entry == 40)
        .map(|(folder, c)| {
            let c = conker_extract::bind_faces(&rom, c, &textures).unwrap().0;
            (folder, c.gltf(&textures, false).unwrap().json)
        })
        .collect();
    let [(plain, g40), (lit, g_lit)] = &tanks[..] else {
        panic!("two tanks")
    };
    assert_eq!((plain.as_str(), lit.as_str()), ("40", "40-7-9"));
    let blend: BTreeSet<_> = [(b.to_string(), false)].into();
    assert_eq!(
        (modes(g40, 4368).len(), modes(g_lit, 4368)),
        (0, blend.clone())
    );
    assert!(modes(g_lit, 4216).is_superset(&blend));
}

/// The child objects' rigs (`conker_assets::RIGS`; `lab/research/mp-original/held-leftovers.md`
/// §1): the crossbow (#110) has 3 joints and the clips the game poses it with, the bone crossbow
/// (#47) the same joints and clips, as item 0x3A's two grenades (#137, #186) have; each pair is one
/// glTF, one clip set, the second model's primitives on list 1.
#[test]
fn child_object_rigs() {
    use extract_character::{Character, Clip, JOINT_SCALE, SCALE_ONE};
    let Some(rom) = rom() else { return };
    let crossbow = Character::load(&rom, 110).unwrap();
    let joints = &crossbow.model.joints;
    assert_eq!(joints.len(), 3);
    // (pair, stored frames, ticks): the draw's bind pose (pair 0, empty), the idle's one-tick
    // pose, the fire and the holster.
    let clips: Vec<_> = (crossbow.clips.iter())
        .map(|c| (c.pair, c.frames.len(), c.duration))
        .collect();
    assert_eq!(clips, [(0, 0, 66), (1, 1, 1), (2, 9, 31), (3, 10, 36)]);
    let clip = |pair: usize| -> &Clip { crossbow.clips.iter().find(|c| c.pair == pair).unwrap() };
    let near = |a: f32, b: f32| (a - b).abs() < 0.001;
    // The idle: the nock (joint 1) drawn back to (13.015, -2.472, 67.389) in joint 0's frame.
    let a = usize::from(joints[1].anim);
    let delta = clip(1).frames[0].translations[a].unwrap();
    let nock: [f32; 3] =
        std::array::from_fn(|k| joints[1].translation[k] + f32::from(delta[k]) / JOINT_SCALE);
    assert!(
        near(nock[0], 13.015) && near(nock[1], -2.472) && near(nock[2], 67.389),
        "{nock:?}"
    );
    // The fire: the loaded bolt's joint 2 grows from nothing by t8 (a frame every 4 ticks).
    let a = usize::from(joints[2].anim);
    let scales: Vec<f32> = (clip(2).frames[..3].iter())
        .map(|f| f.scales[3 * a].map_or(1.0, |v| f32::from(v) / SCALE_ONE))
        .collect();
    assert!(
        near(scales[0], 0.002) && near(scales[1], 0.703) && near(scales[2], 1.0),
        "{scales:?}"
    );
    let textures = rom.textures().unwrap();
    for (name, [first, second]) in conker_assets::RIGS {
        let (a, b) = (
            Character::load(&rom, first).unwrap(),
            Character::load(&rom, second).unwrap(),
        );
        assert_eq!(
            (&a.model.joints, &a.clips),
            (&b.model.joints, &b.clips),
            "{name}"
        );
        let rig = conker_extract::rig(&rom, [first, second]).unwrap();
        let (json, _) = parse_glb(&rig.gltf(&textures, true).unwrap().glb());
        let animations: Vec<&str> = (json["animations"].as_array().unwrap().iter())
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            animations,
            [
                "pair_0000_anim_0000",
                "pair_0001_anim_0001",
                "pair_0002_anim_0002",
                "pair_0003_anim_0003"
            ]
        );
        let lists: BTreeSet<u64> = (json["meshes"][0]["primitives"].as_array().unwrap().iter())
            .map(|p| p["extras"]["list"].as_u64().unwrap())
            .collect();
        assert_eq!(lists, [0, 1].into(), "{name}");
    }
    let lob = Character::load(&rom, 137).unwrap();
    let clips: Vec<_> = (lob.clips.iter()).map(|c| (c.pair, c.duration)).collect();
    assert_eq!(
        (lob.model.joints.len(), clips),
        (4, vec![(0, 10), (1, 30), (2, 24), (3, 30)])
    );
    // The rigs' clip 0, an empty pair, is its descriptor's bases: the crossbow's rest, item 0x3A's
    // bent fuse (V held-client-scratch l1a DLs 133-220: joints 1-3 turned about z by these).
    let base = |entries| {
        let rig = conker_extract::rig(&rom, entries).unwrap();
        let clip = rig.clips.iter().find(|c| c.pair == 0).unwrap();
        assert_eq!(clip.frames.len(), 1);
        clip.frames[0].rotations.clone()
    };
    assert!(base([0x6E, 0x2F]).iter().all(|r| *r == [0, 0, 0]));
    let lob = base([0x89, 0xBA]);
    assert_eq!(lob[1..], [[0, 0, 63264], [0, 0, 61152], [0, 0, 60736]]);
    let q = extract_character::quaternion(lob[1]);
    assert!(
        (q[2] + 0.109).abs() < 0.001 && (q[3] - 0.994).abs() < 0.001,
        "{q:?}"
    );
}

/// The body-part spheres (`lab/research/mp-original/held-leftovers.md` §2, its `parts_rom.txt`
/// and `broad_rom.txt`): the table's 23 models with their set, sphere count and head (part 0xA)
/// spheres, in order; 117's and 83's spheres; no parts for the models off the table.
#[test]
fn body_part_spheres() {
    let Some(rom) = rom() else { return };
    let table: [(usize, u8, usize, usize); conker_extract::PART_ENTRIES] = [
        (83, 0, 7, 0),
        (0, 1, 42, 3),
        (1, 1, 42, 3),
        (2, 1, 42, 3),
        (3, 1, 42, 3),
        (4, 1, 42, 3),
        (59, 2, 43, 2),
        (90, 3, 21, 4),
        (116, 3, 21, 4),
        (122, 3, 21, 4),
        (117, 4, 35, 3),
        (128, 5, 42, 3),
        (136, 6, 31, 3),
        (144, 7, 27, 3),
        (150, 8, 37, 3),
        (159, 10, 37, 3),
        (160, 10, 37, 3),
        (156, 11, 37, 3),
        (157, 11, 37, 3),
        (178, 11, 37, 3),
        (180, 11, 37, 3),
        (176, 11, 37, 3),
        (177, 11, 37, 3),
    ];
    let models = conker_extract::part_models(&rom).unwrap();
    assert_eq!(models, table.map(|t| (t.0, t.1)));
    for (model, _, n, heads) in table {
        let parts = conker_extract::body_parts(&rom, model).unwrap().unwrap();
        let head = parts.spheres.iter().filter(|s| s[1] == 0xA).count();
        assert_eq!((parts.spheres.len(), head), (n, heads), "{model}");
    }
    let tedi = conker_extract::body_parts(&rom, 117).unwrap().unwrap();
    assert_eq!(
        (tedi.broad, tedi.spheres[0]),
        ([117, 95], [12, 0xA, 38, 0, 33, 5])
    );
    // The ears last, after the body's spheres.
    assert_eq!(
        tedi.spheres[33..],
        [[12, 0xA, 22, 20, 21, 9], [12, 0xA, 22, -22, 21, 6]]
    );
    let raptor = conker_extract::body_parts(&rom, 83).unwrap().unwrap();
    assert_eq!(
        (raptor.broad, raptor.spheres[4]),
        ([339, 227], [5, 0, 58, 0, 42, 115])
    );
    // The zombies' limbs: parts 4-9 besides the head and the body.
    let zombie = conker_extract::body_parts(&rom, 159).unwrap().unwrap();
    let parts: BTreeSet<i16> = zombie.spheres.iter().map(|s| s[1]).collect();
    assert_eq!(parts, [0, 4, 5, 6, 7, 8, 9, 0xA].into());
    for model in [152, 154, 155, 119, 138] {
        assert_eq!(
            conker_extract::body_parts(&rom, model).unwrap(),
            None,
            "{model}"
        );
    }
}
