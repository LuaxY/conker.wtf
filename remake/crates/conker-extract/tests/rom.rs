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

    // The weapons and the grenade in the hand: 11 models (the twin guns are two), one mesh each.
    let (held, _) = parse_glb(&slice.held_glb);
    let names: Vec<&str> = held["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!((slice.held.len(), names.len()), (11, 11));
    for (i, h) in slice.held.iter().enumerate() {
        let node = held["scenes"][i]["nodes"][0].as_u64().unwrap() as usize;
        let mesh = held["nodes"][node]["mesh"].as_u64().unwrap() as usize;
        assert_eq!(names[mesh], format!("09-{:02x}-00", h.entry));
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
