//! The asset pack against the ROM path (path in env `CONKER_ROM`; the ROM test skips when it is
//! unset): `pack` of both maps (0x2D, 0x30) exported to a temporary folder and read back with
//! conker-assets gives, bit for bit, what the client (`Slice::from_rom`) and the server
//! (`conker-server`'s `scene`) build from the ROM, and its glTF files are valid, self-consistent
//! and draw the same as the in-memory GLBs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use conker_assets::{Gives, Layer as PackLayer};
use conker_extract::{Layer, PACK_CHARACTERS, PACK_MAP, PACK_WEAPONS, PackMap, SLICE_SCENE, Slice};
use conker_rules::game::Arena;
use conker_rules::spawn::Spawn;
use conker_rules::weapon::{self, Pickup};
use conker_sim::trig::AcosTable;
use extract_core::Rom;
use gltf_json::validation::Validate;
use serde_json::Value;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("conker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The JSON and BIN chunks of a GLB.
fn glb(bytes: &[u8]) -> (Value, &[u8]) {
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let text = word(12);
    let json = serde_json::from_slice(&bytes[20..20 + text]).unwrap();
    (json, &bytes[20 + text + 8..])
}

/// A glTF's content without its storage: the JSON less its buffers, buffer views and images (and
/// the accessors' view indices), every accessor's bytes, and every image's PNG bytes.
type Content = (Value, Vec<Vec<u8>>, Vec<Vec<u8>>);

fn content(mut json: Value, bin: &[u8], file: impl Fn(&str) -> Vec<u8>) -> Content {
    let views = json["bufferViews"].take();
    let view = |i: &Value| {
        let v = &views[i.as_u64().unwrap() as usize];
        let start = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        bin[start..start + v["byteLength"].as_u64().unwrap() as usize].to_vec()
    };
    let accessors = (json["accessors"].as_array_mut().unwrap().iter_mut())
        .map(|a| view(&a["bufferView"].take()))
        .collect();
    let images = (json["images"].take().as_array().into_iter().flatten())
        .map(|i| match i["uri"].as_str() {
            Some(uri) => file(uri),
            None => view(&i["bufferView"]),
        })
        .collect();
    json["buffers"].take();
    (json, accessors, images)
}

/// A `.gltf` of the pack: valid glTF 2.0 (gltf-json's validation), its buffer and every image a
/// relative URI to a file next to it, and its content as `glb`'s.
fn check_gltf(dir: &Path, path: &str, glb_bytes: &[u8]) {
    let text = std::fs::read(dir.join(path)).unwrap();
    let root = gltf_json::Root::from_slice(&text).unwrap();
    let mut errors = Vec::new();
    root.validate(&root, gltf_json::Path::new, &mut |p, e| {
        errors.push(format!("{}: {e}", p()))
    });
    assert!(errors.is_empty(), "{path}: {errors:?}");

    let json: Value = serde_json::from_slice(&text).unwrap();
    assert_eq!(json["asset"]["version"], "2.0");
    let folder = dir.join(path).parent().unwrap().to_path_buf();
    let file = |uri: &str| {
        assert!(
            !uri.contains("..") && !uri.contains(':') && !uri.starts_with('/'),
            "{uri}"
        );
        std::fs::read(folder.join(uri)).unwrap_or_else(|e| panic!("{path}: {uri}: {e}"))
    };
    let buffers = json["buffers"].as_array().unwrap();
    assert_eq!(buffers.len(), 1);
    let bin = file(buffers[0]["uri"].as_str().unwrap());
    assert_eq!(buffers[0]["byteLength"], bin.len());
    let images = json["images"].as_array().unwrap();
    assert!(!images.is_empty());
    for image in images {
        assert!(file(image["uri"].as_str().unwrap()).starts_with(b"\x89PNG"));
    }

    let (glb_json, glb_bin) = glb(glb_bytes);
    let pack = content(json, &bin, file);
    let memory = content(glb_json, glb_bin, |_| {
        unreachable!("GLB images are embedded")
    });
    assert_eq!(pack.0, memory.0, "{path}: JSON");
    assert!(pack.1 == memory.1, "{path}: accessor bytes");
    assert!(pack.2 == memory.2, "{path}: images");
}

fn bits<const N: usize>(v: [f32; N]) -> [u32; N] {
    v.map(f32::to_bits)
}

#[test]
fn pack_reads_back_as_the_rom_path() {
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let rom = Rom::new(bytes.clone()).unwrap();
    let start = Instant::now();
    let maps = [0x2D, 0x30].map(|scene| PackMap {
        scene,
        background: None,
    });
    let export = conker_extract::pack(&rom, &maps).unwrap();
    let dir = temp_dir("pack-test");
    for (path, data) in &export.files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }
    let size: usize = export.files.iter().map(|f| f.1.len()).sum();
    eprintln!(
        "pack: {} files, {size} bytes in {:.0} ms; {}",
        export.files.len(),
        start.elapsed().as_secs_f64() * 1e3,
        export.summary.join("; ")
    );
    let files: BTreeMap<&str, &[u8]> = (export.files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    assert_eq!(files.len(), export.files.len(), "duplicate paths");
    let names = conker_assets::Slice::files(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS);
    assert!(names.iter().all(|n| files.contains_key(n.as_str())));
    assert!(files.contains_key("README.md"));

    let read = |p: &str| std::fs::read(dir.join(p));
    let pack = conker_assets::Slice::load(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS, read).unwrap();

    // The client's slice.
    let slice = Slice::from_rom(bytes).unwrap();
    assert_eq!(pack.collision.len(), slice.collision.len());
    for (p, s) in pack.collision.iter().zip(&slice.collision) {
        assert_eq!(p.points.map(bits), s.points.map(bits));
        let layer = match s.layer {
            Layer::Terrain => PackLayer::Terrain,
            Layer::Secondary => PackLayer::Secondary,
            Layer::Placement(i) => PackLayer::Placement(i),
        };
        assert_eq!((p.layer, p.surface), (layer, s.surface));
    }
    assert_eq!(pack.spawns.len(), slice.spawns.len());
    for (p, s) in pack.spawns.iter().zip(&slice.spawns) {
        let fields = |b6, yaw, b14, b15| (b6, yaw, b14, b15);
        assert_eq!(p.position, s.position);
        assert_eq!(bits(p.camera), bits(s.camera));
        assert_eq!(
            fields(p.byte_06, p.yaw, p.byte_14, p.byte_15),
            fields(s.byte_06, s.yaw, s.byte_14, s.byte_15)
        );
    }
    // The squirrel soldier (128) plays the Tedi's (117) clips: the same 764, the same root motion.
    let [squirrel, tedi] = &pack.characters[..] else {
        panic!("two characters")
    };
    assert_eq!((tedi.clips.len(), tedi.root_clips.len()), (764, 10));
    assert_eq!(
        (&tedi.clips, &tedi.root_clips),
        (&slice.clips, &slice.root_clips)
    );
    assert_eq!(
        (&squirrel.clips, &squirrel.root_clips),
        (&tedi.clips, &tedi.root_clips)
    );
    // The client finds each clip's animation by name.
    for name in PACK_CHARACTERS {
        let character: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        let names: Vec<&str> = (character["animations"].as_array().unwrap().iter())
            .map(|a| a["name"].as_str().unwrap())
            .collect();
        let expected: Vec<String> = tedi.clips.iter().map(|c| c.animation()).collect();
        assert_eq!(names, expected, "{name}");
    }
    assert_eq!(pack.held.len(), slice.held.len());
    for (p, s) in pack.held.iter().zip(&slice.held) {
        let events = |e: &[(u16, f32)]| {
            e.iter()
                .map(|&(id, t)| (id, t.to_bits()))
                .collect::<Vec<_>>()
        };
        assert_eq!((p.entry, p.joint), (s.entry, s.joint));
        assert_eq!(events(&p.create), events(&s.create));
        assert_eq!(events(&p.remove), events(&s.remove));
    }

    // The server's reads: pickups on their placements, the bots' four assets, the acos table.
    let placements = extract_scene::scene_placements(&rom, SLICE_SCENE).unwrap();
    let spawns = (slice.spawns.iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let arena = Arena::deathmatch(spawns, |n| {
        placements[usize::from(n)].position.map(f32::from)
    });
    let pickups: Vec<(Pickup, [u32; 3])> = (pack.pickups.iter())
        .map(|p| {
            let gives = match p.gives {
                Gives::Weapon(id) => weapon::Gives::Weapon(id),
                Gives::Grenades => weapon::Gives::Grenades,
            };
            let pickup = Pickup {
                placement: p.placement,
                gives,
            };
            (pickup, bits(p.position))
        })
        .collect();
    let rom_pickups: Vec<_> = arena.pickups.iter().map(|&(p, at)| (p, bits(at))).collect();
    assert_eq!(pickups, rom_pickups);
    let [nav, triggers, ai, class] =
        conker_bots::Assets::paths(SLICE_SCENE).map(|p| rom.asset(&p).unwrap().into_owned());
    assert_eq!(
        [&pack.nav, &pack.triggers, &pack.ai, &pack.class],
        [&nav, &triggers, &ai, &class]
    );
    let game_data = rom.game_data().unwrap();
    assert_eq!(
        AcosTable::new(pack.acos.clone()),
        AcosTable::from_game_data(&game_data)
    );

    // What the runtime compiles in: the sink table (conker-rules' copy, the ROM's bytes at
    // 0x80099140) and the model lifts (obj+0xC8 in the game's RAM).
    assert_eq!(pack.sink, conker_rules::surface::SINK);
    let sink: Vec<u8> = pack.sink.iter().flatten().map(|&b| b as u8).collect();
    let at = 0x8009_9140 - 0x8008_2B20;
    assert_eq!(&game_data[at..at + 96], &sink[..]);
    assert_eq!((squirrel.model_lift, tedi.model_lift), (42, 43));

    // The glTF files.
    let textures = rom.textures().unwrap();
    let tedi = extract_character::Character::load(&rom, 117).unwrap();
    let squirrel = conker_extract::squirrel(&rom, &tedi).unwrap();
    assert_eq!(squirrel.model.joints.len(), 28);
    let squirrel_glb = squirrel.gltf(&textures, true).unwrap().glb();
    for (path, glb) in [
        (conker_assets::level_gltf(PACK_MAP), &slice.level_glb),
        (
            conker_assets::character_gltf(PACK_CHARACTERS[0]),
            &squirrel_glb,
        ),
        (
            conker_assets::character_gltf(PACK_CHARACTERS[1]),
            &slice.character_glb,
        ),
        (conker_assets::held_gltf(PACK_WEAPONS), &slice.held_glb),
    ] {
        check_gltf(&dir, &path, glb);
    }
    let manifest: conker_assets::Manifest = serde_json::from_slice(files["pack.json"]).unwrap();
    assert_eq!(manifest.source_rom_sha1, extract_core::US_SHA1);
    assert_eq!(manifest.maps, ["2d", "30"]);

    // Map 0x30 against the ROM path: the same reads, its pickups on the oracle's spots (their
    // models moved there in the level).
    let names = conker_assets::Slice::files("30", &PACK_CHARACTERS, PACK_WEAPONS);
    assert!(names.iter().all(|n| files.contains_key(n.as_str())));
    let map = conker_assets::Slice::load("30", &PACK_CHARACTERS, PACK_WEAPONS, read).unwrap();
    let placements = extract_scene::scene_placements(&rom, 0x30).unwrap();
    let collision = extract_scene::scene_collision(&rom, 0x30, &placements).unwrap();
    assert_eq!(map.collision.len(), collision.triangles.len());
    for (p, s) in map.collision.iter().zip(&collision.triangles) {
        assert_eq!(p.points.map(bits), s.points.map(bits));
        assert_eq!(p.surface, s.surface);
    }
    let spawns = extract_scene::scene_spawns(&rom, 0x30).unwrap();
    assert_eq!(spawns.len(), 25);
    for (p, s) in map.spawns.iter().zip(&spawns) {
        assert_eq!(
            (p.position, bits(p.camera), p.yaw),
            (s.position, bits(s.camera), s.yaw)
        );
    }
    let gives: Vec<_> = (map.pickups.iter())
        .map(|p| (p.placement, p.position.map(|v| v as i16)))
        .collect();
    let table: Vec<_> = (weapon::PICKUPS_30.iter().zip(weapon::SPOTS_30))
        .map(|(p, spot)| (p.placement, spot))
        .collect();
    assert_eq!(gives, table);
    let [nav, triggers, ..] =
        conker_bots::Assets::paths(0x30).map(|p| rom.asset(&p).unwrap().into_owned());
    assert_eq!([&map.nav, &map.triggers], [&nav, &triggers]);
    let level = conker_extract::map_level(&rom, 0x30).unwrap();
    let glb = extract_level::to_gltf(&level, &textures, "").unwrap().glb();
    check_gltf(&dir, &conker_assets::level_gltf("30"), &glb);
    std::fs::remove_dir_all(dir).unwrap();
}

/// `conker-extract pack` keeps an existing pack (edits included) unless `--force`, and never
/// writes into the source tree. Both refusals come before the ROM is read.
#[test]
fn pack_refuses_to_overwrite_or_to_write_in_the_source_tree() {
    let dir = temp_dir("pack-refusal");
    std::fs::write(dir.join("pack.json"), "{}").unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_conker-extract"))
            .args(["--rom", "/nonexistent.z64"])
            .args(args)
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let out = dir.to_str().unwrap();
    let (ok, err) = run(&["pack", "--out", out]);
    assert!(!ok && err.contains("pack.json exists"), "{err}");
    // With --force it goes on, to the ROM read.
    let (ok, err) = run(&["pack", "--out", out, "--force"]);
    assert!(!ok && err.contains("/nonexistent.z64"), "{err}");
    let inside = concat!(env!("CARGO_MANIFEST_DIR"), "/../../no-pack-here");
    let (ok, err) = run(&["pack", "--out", inside]);
    assert!(!ok && err.contains("stays out of the source tree"), "{err}");
    assert!(!Path::new(inside).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

/// The pack's sound (`audio/`): `Audio::load` reads it back; every WAV it names is there and holds
/// the bank's sound decoded sample for sample at the bank's rate, with its loop, volume and pan;
/// no other file is under `audio/`; and the event map holds what the oracle's sound calls showed.
#[test]
fn pack_audio_reads_back() {
    use conker_assets::audio::{Audio, Channel, ClipAction, On};
    use conker_extract::audio::{MATCH_CLIPS, decode, read_wav, sound_bank, wav};
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let background = wav(&[0, 1, -1, 2], 22020);
    let other = wav(&[3, 4], 22020);
    let maps = [(0x2D, &background), (0x30, &other)].map(|(scene, wav)| PackMap {
        scene,
        background: Some(wav),
    });
    let export = conker_extract::pack(&rom, &maps).unwrap();
    let files: BTreeMap<&str, &[u8]> = (export.files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let read = |p: &str| files.get(p).map(|d| d.to_vec()).ok_or("missing");
    let audio = Audio::load(read).unwrap();
    let manifest: conker_assets::Manifest = serde_json::from_slice(files["pack.json"]).unwrap();
    assert_eq!(
        manifest.audio,
        [
            "characters/117",
            "characters/128",
            "common",
            "maps/2d",
            "maps/30"
        ]
    );

    let (bank, wavetable) = sound_bank(&rom).unwrap();
    let mut listed: Vec<String> = Audio::files().map(String::from).into();
    let mut bytes = 0;
    for (&id, s) in &audio.sounds.sounds {
        let b = &bank[usize::from(id) - 1];
        let file = format!("audio/{}", s.file);
        let (rate, pcm) = read_wav(files[file.as_str()]).unwrap();
        let w = &b.wave;
        assert_eq!(
            pcm,
            decode(&wavetable[w.base..w.base + w.len], &w.book).unwrap(),
            "{id}"
        );
        assert_eq!(
            (rate, s.rate, s.volume, s.pan),
            (b.rate(), b.rate(), b.volume, b.pan),
            "{id}"
        );
        assert_eq!(
            s.looped.map(|l| (l.start, l.end)),
            w.looped.map(|l| (l.start, l.end)),
            "{id}"
        );
        bytes += pcm.len() * 2;
        listed.push(file);
    }
    for (name, wav) in [("2d", &background), ("30", &other)] {
        let file = format!("audio/{}", audio.events.maps[name].background[0].file);
        assert_eq!(files[file.as_str()], &wav[..]);
        listed.push(file);
    }
    // Scene 0x30's own surface kind 0x11 has footsteps; 0x2D's bubbles are its only emitters.
    assert!(audio.events.footsteps.surfaces.contains_key(&0x11));
    assert_eq!(audio.events.maps["2d"].emitters.len(), 2);
    assert!(audio.events.maps["30"].emitters.is_empty());
    listed.sort();
    let under: Vec<&str> = files
        .keys()
        .copied()
        .filter(|p| p.starts_with("audio/"))
        .collect();
    assert_eq!(under, listed);
    eprintln!(
        "audio: {} sounds ({bytes} PCM bytes), {} + {} clip events, {} cues",
        audio.sounds.sounds.len(),
        audio.events.characters["117"].len(),
        audio.events.characters["128"].len(),
        audio.events.cues.len()
    );

    // Every event is on a Deathmatch clip the character has (character.json's logical ids).
    let pack = conker_assets::Slice::load(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS, read).unwrap();
    for (name, character) in PACK_CHARACTERS.iter().zip(&pack.characters) {
        for e in &audio.events.characters[*name] {
            assert!(MATCH_CLIPS.contains(&e.clip), "{name} {:#x}", e.clip);
            assert!(
                character.clips.iter().any(|c| c.logical.contains(&e.clip)),
                "{name} {:#x}",
                e.clip
            );
        }
    }
    // What the oracle's sound calls showed (V, ../lab/audio/README.md).
    let play_at = |character: &str, clip: u16, time: f32| {
        audio.events.characters[character]
            .iter()
            .find(|e| e.clip == clip && e.time == time)
            .map(|e| e.action.clone())
    };
    let Some(ClipAction::Play(death)) = play_at("117", 0x58, 21.0) else {
        panic!("the Tedi's death voice")
    };
    assert_eq!(
        (death.sounds, death.channel, death.volume, death.own),
        (vec![1254], Channel::B, 28000, Some(24000))
    );
    assert_eq!(
        play_at("128", 0x58, 21.0),
        None,
        "a squirrel has no voice there"
    );
    assert_eq!(play_at("117", 2, 1.0), Some(ClipAction::Footstep(1)));
    assert_eq!(audio.events.footsteps.surfaces[&5][1], [577, 578, 579, 580]);
    let swing = play_at("117", 0x12A, 0.0);
    assert!(matches!(swing, Some(ClipAction::Play(p)) if p.sounds == [1314, 1315, 1316]));
    let cue = |on: &On, character: Option<&str>| {
        (audio.events.cues.iter())
            .find(|c| &c.on == on && c.character.as_deref() == character)
            .map(|c| c.play.sounds.clone())
    };
    let hit = On::Hit {
        weapon: 0x09,
        lethal: None,
    };
    assert_eq!(cue(&hit, Some("128")), Some((882..=887).collect()));
    assert_eq!(cue(&hit, Some("117")), None);
    let firing = On::Firing {
        weapon: 0x09,
        clips: vec![0x7C, 0x20D],
    };
    assert_eq!(cue(&firing, None), Some(vec![719]));
}

/// The pack's HUD (`hud/`): `Hud::load` reads `hud.json` back and writes it out the same, as
/// `End::load` does `end.json`; every texture and glyph they name is a PNG of its size under
/// `hud/`, and nothing else is there; the
/// font is the game's (95 glyphs from "0123456789ABC…", their metrics as the game decodes them,
/// V: `lab/hud/tools/verify_pack.py` compares every glyph with the RAM's).
#[test]
fn pack_hud_reads_back() {
    use conker_assets::hud::{END, End, HUD, Hud};
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let maps = [PackMap {
        scene: SLICE_SCENE,
        background: None,
    }];
    let export = conker_extract::pack(&rom, &maps).unwrap();
    let files: BTreeMap<&str, &[u8]> = (export.files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let hud = Hud::load(|p| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    assert_eq!(hud.to_json(), files[HUD]);
    let end = End::load(|p| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    assert_eq!(end.to_json(), files[END]);
    let manifest: conker_assets::Manifest = serde_json::from_slice(files["pack.json"]).unwrap();
    assert!(manifest.hud);
    let png_size = |file: &str| {
        let png = files[format!("hud/{file}").as_str()];
        let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
        [word(16), word(20)]
    };
    let mut listed = vec![HUD.to_string(), END.to_string()];
    for t in hud.textures.values().chain(end.textures.values()) {
        assert_eq!(png_size(&t.file), t.size.map(u32::from), "{}", t.file);
        listed.push(format!("hud/{}", t.file));
    }
    for g in &hud.font {
        assert_eq!(png_size(&g.file), g.size.map(u32::from), "{}", g.file);
        listed.push(format!("hud/{}", g.file));
    }
    listed.sort();
    let under: Vec<&str> = files
        .keys()
        .copied()
        .filter(|p| p.starts_with("hud/"))
        .collect();
    assert_eq!(under, listed);
    let chars: String = hud.font.iter().take(13).map(|g| g.ch).collect();
    assert_eq!((hud.font.len(), chars.as_str()), (95, "0123456789ABC"));
    let colon = hud.glyph(':').unwrap();
    assert_eq!((colon.size, colon.offset), ([4, 7], [0, 2]));
    assert_eq!(hud.weapon.icons[&0x09].rect, [0, 4, 22, 12]);
    assert_eq!(hud.weapon.icons[&0x39].texture, "atlas2");
    // The results' place, accuracy and labels are drawn with these.
    assert!(['%', 'P'].iter().all(|&c| hud.glyph(c).is_some()));
}
