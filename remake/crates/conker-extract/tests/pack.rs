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
    let maps = [0x2D, 0x30, 0x34, 0x3F, 0x2B, 0x1A, 0x24].map(|scene| PackMap {
        scene,
        background: None,
        song: None,
        stem: None,
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
    let solid = conker_extract::solid_placements(0x2D);
    let collision: Vec<_> = (slice.collision.iter())
        .filter(|t| !matches!(t.layer, Layer::Placement(i) if !solid.contains(&i)))
        .collect();
    assert_eq!(pack.collision.len(), collision.len());
    for (p, s) in pack.collision.iter().zip(collision) {
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
    // The squirrel soldier (128) and the caveman (136) play the Tedi's (117) clips: the same 764,
    // the same root motion.
    let [squirrel, tedi, caveman] = &pack.characters[..] else {
        panic!("three characters")
    };
    assert_eq!(caveman.model_lift, 49);
    assert_eq!(
        (&caveman.clips, &caveman.root_clips),
        (&tedi.clips, &tedi.root_clips)
    );
    assert_eq!((tedi.clips.len(), tedi.root_clips.len()), (764, 10));
    assert_eq!(
        (&tedi.clips, &tedi.root_clips),
        (&slice.clips, &slice.root_clips)
    );
    assert_eq!(
        (&squirrel.clips, &squirrel.root_clips),
        (&tedi.clips, &tedi.root_clips)
    );
    // The Beach's villagers (packed with 0x2B) play them too, with their own model lifts (V: the
    // bank-0x11 records, as the others').
    let mut names = PACK_CHARACTERS.to_vec();
    names.extend(["156", "157"]);
    let beach = conker_assets::Slice::load("2b", &names, PACK_WEAPONS, read).unwrap();
    let lifts: Vec<i16> = beach.characters.iter().map(|c| c.model_lift).collect();
    assert_eq!(lifts, [42, 43, 49, 41, 40]);
    for villager in &beach.characters[3..] {
        assert_eq!(
            (&villager.clips, &villager.root_clips),
            (&tedi.clips, &tedi.root_clips)
        );
    }
    // Their looks (War: Beach's classes 0x11-0x20, `villager_look`): 12 on 156, 4 classes on 157.
    for (k, model) in [(3, 156), (4, 157)] {
        let looks: BTreeMap<u8, u8> = (0x11..=0x20)
            .filter_map(|c| {
                conker_extract::villager_look(c)
                    .filter(|l| l.0 == model)
                    .map(|l| (c, l.1))
            })
            .collect();
        assert_eq!(beach.characters[k].looks, looks);
        assert_eq!(looks.len(), [12, 4][k - 3]);
    }
    assert!(beach.characters[..3].iter().all(|c| c.looks.is_empty()));
    // The client finds each clip's animation by name.
    for name in names {
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
        .filter(|p| p.gives != Gives::Mask)
        .map(|p| {
            let gives = match p.gives {
                Gives::Weapon(id) => weapon::Gives::Weapon(id),
                Gives::Grenades => weapon::Gives::Grenades,
                Gives::Mask => weapon::Gives::Mask,
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
    let squirrel = conker_extract::on_tedi_rig(&rom, 128, &tedi).unwrap();
    assert_eq!(squirrel.model.joints.len(), 28);
    let squirrel_glb = squirrel.gltf(&textures, true).unwrap().glb();
    // The slice's level less 0x2D's hidden placements.
    let level = conker_extract::map_level(&rom, 0x2D).unwrap();
    let level_glb = extract_level::to_gltf(&level, &textures, "").unwrap().glb();
    for (path, glb) in [
        (conker_assets::level_gltf(PACK_MAP), &level_glb),
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
    // 0x2D's Total War props: its doors, masks and canisters in place, then the canister at the
    // origin, a glTF scene each.
    let (props, scenes) = conker_extract::props_level(&rom, 0x2D).unwrap();
    let mut gltf = extract_level::to_gltf(&props, &textures, "").unwrap();
    let mut json: Value = serde_json::from_str(&gltf.json).unwrap();
    let names: Vec<_> = (json["nodes"].as_array().unwrap().iter())
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names[..3], ["04-2d-04@0", "04-2d-05@1", "03-54@21"]);
    assert_eq!(names[9..], ["03-55@28", "03-55"]);
    json["scenes"] = scenes;
    gltf.json = json.to_string();
    check_gltf(&dir, &conker_assets::props_gltf(PACK_MAP), &gltf.glb());
    let manifest: conker_assets::Manifest = serde_json::from_slice(files["pack.json"]).unwrap();
    assert_eq!(manifest.source_rom_sha1, extract_core::US_SHA1);
    assert_eq!(manifest.maps, ["2d", "30", "34", "3f", "2b", "1a", "24"]);
    assert_eq!(
        manifest.characters[..7],
        ["128", "117", "136", "156", "157", "59", "119"]
    );

    // Each map's solid placements (the oracle's RAM) are those with collision flags less its
    // pickups (`+0x4F |= 0x20`) and its hidden ones: the game's layer-2 filter on ROM data. On 0x1A
    // the match sets 0x20 on every placement but 0, so its drawn 13 is not solid (V,
    // `lab/oracle/map1a/props_pickups_lights.md` §1).
    for scene in [0x2D, 0x30, 0x34, 0x3F, 0x2B, 0x1A, 0x24] {
        let (pickups, _) = conker_extract::pickup_table(&rom, scene).unwrap();
        let mut hidden = conker_extract::hidden_placements(scene).to_vec();
        if scene == 0x1A {
            hidden.push(13);
        }
        let placements = extract_scene::scene_placements(&rom, scene).unwrap();
        let solid: Vec<usize> = (placements.iter().enumerate())
            .filter(|(i, p)| p.has_collision() && !hidden.contains(i))
            .filter(|(i, _)| pickups.iter().all(|k| usize::from(k.placement) != *i))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            solid,
            extract_scene::deathmatch_solid(scene),
            "scene {scene:#x}"
        );
    }

    // Maps 0x30, 0x34, 0x3F, 0x2B and 0x1A against the ROM path: the same reads (only their solid
    // placements), their pickups on the oracle's spots (0x30's, 0x2B's and 0x1A's, their models
    // moved there in the level) or on their placements (0x34's, 0x3F's).
    let maps = [
        ("30", 0x30, 25),
        ("34", 0x34, 12),
        ("3f", 0x3F, 16),
        ("2b", 0x2B, 12),
        ("1a", 0x1A, 32),
        ("24", 0x24, 20),
    ];
    for (name, scene, records) in maps {
        let names = conker_assets::Slice::files(name, &PACK_CHARACTERS, PACK_WEAPONS);
        assert!(names.iter().all(|n| files.contains_key(n.as_str())));
        let map = conker_assets::Slice::load(name, &PACK_CHARACTERS, PACK_WEAPONS, read).unwrap();
        let placements = extract_scene::scene_placements(&rom, scene).unwrap();
        let collision = extract_scene::scene_collision(&rom, scene, &placements).unwrap();
        let solid = conker_extract::solid_placements(scene);
        let shown: Vec<_> = (collision.triangles.iter())
            .filter(|t| !matches!(t.layer, Layer::Placement(i) if !solid.contains(&i)))
            .collect();
        assert_eq!(map.collision.len(), shown.len());
        for (p, s) in map.collision.iter().zip(shown) {
            assert_eq!(p.points.map(bits), s.points.map(bits));
            assert_eq!(p.surface, s.surface);
        }
        let spawns = extract_scene::scene_spawns(&rom, scene).unwrap();
        assert_eq!(spawns.len(), records);
        for (p, s) in map.spawns.iter().zip(&spawns) {
            assert_eq!(
                (p.position, bits(p.camera), p.yaw),
                (s.position, bits(s.camera), s.yaw)
            );
        }
        let gives: Vec<_> = (map.pickups.iter())
            .map(|p| (p.placement, p.position.map(|v| v as i16)))
            .collect();
        let (pickups, spots) = conker_extract::pickup_table(&rom, scene).unwrap();
        let table: Vec<_> = (pickups.iter().enumerate())
            .map(|(k, p)| {
                let at = placements[usize::from(p.placement)].position;
                (p.placement, spots.get(k).copied().unwrap_or(at))
            })
            .collect();
        assert_eq!(gives, table);
        let [nav, triggers, _, _] =
            conker_bots::Assets::paths(scene).map(|p| rom.asset(&p).unwrap().into_owned());
        let ai = rom
            .asset(&[0x14, conker_extract::ai_script(scene)])
            .unwrap();
        assert_eq!([&map.nav, &map.triggers, &map.ai], [&nav, &triggers, &*ai]);
        let level = conker_extract::map_level(&rom, scene).unwrap();
        let glb = extract_level::to_gltf(&level, &textures, "").unwrap().glb();
        check_gltf(&dir, &conker_assets::level_gltf(name), &glb);
    }

    // Map 0x1A, the Vault (`lab/research/mp-original/heist.md`, `lab/oracle/map1a/`): script 0xB7,
    // four lifts and no kill gate; k0-k3 roam spots 0-7 and every pickup is back after 300 VIs (V);
    // its 34 lights (4 pulsing bases, 2 flickering lamps, 5 switching); the Wise Guy (59) on the
    // Tedi's clips, model lift 49 (V), its team look the texture of material 2 (V: REDS 7246, BLUES
    // 7230, GREENS 7231, YELLOWS 7232).
    let mut names = PACK_CHARACTERS.to_vec();
    names.push("59");
    let vault = conker_assets::Slice::load("1a", &names, PACK_WEAPONS, read).unwrap();
    assert_eq!(conker_extract::ai_script(0x1A), 0xB7);
    assert_eq!(conker_rules::lift::parse(&vault.triggers, 0x1A).len(), 4);
    assert!(conker_rules::lift::kill_gates(&vault.triggers).is_empty());
    let roams = &vault.pickups[0].roams;
    assert_eq!(roams.len(), 8);
    for (k, p) in vault.pickups.iter().enumerate() {
        assert_eq!(p.respawn, Some(300));
        assert_eq!(p.roams.len(), if k < 4 { 8 } else { 0 });
        assert!(k >= 4 || &p.roams == roams);
    }
    assert_eq!(vault.pickups[6].position, [-338.0, 510.0, 2396.0]);
    assert_eq!(vault.lights.ambient, [94, 94, 96]);
    assert_eq!(vault.lights.lights.len(), 34);
    assert_eq!(vault.lights.switching, [10, 11, 12, 20, 29]);
    let flicker = vault.lights.lights.iter().filter(|l| l.flicker).count();
    let pulse = vault
        .lights
        .lights
        .iter()
        .filter(|l| l.swing != 0.0)
        .count();
    assert_eq!((flicker, pulse), (2, 4));
    let wise_guy = &vault.characters[3];
    assert_eq!(wise_guy.model_lift, 49);
    assert_eq!(
        (&wise_guy.clips, &wise_guy.root_clips),
        (&pack.characters[1].clips, &pack.characters[1].root_clips)
    );
    let teams = wise_guy.teams.as_ref().unwrap();
    let gltf: Value = serde_json::from_slice(files["characters/59/character.gltf"]).unwrap();
    let material = &gltf["materials"][teams.material];
    assert_eq!(material["extras"]["image"], "flat 7246 mode 0");
    let image = material["pbrMetallicRoughness"]["baseColorTexture"]["index"].as_u64();
    let uri = gltf["images"][image.unwrap() as usize]["uri"]
        .as_str()
        .unwrap();
    let pngs: Vec<&[u8]> = (teams.textures.iter())
        .map(|t| files[format!("characters/59/{t}").as_str()])
        .collect();
    assert_eq!(pngs[0], files[format!("characters/59/{uri}").as_str()]);
    assert!((1..4).all(|t| !pngs[..t].contains(&pngs[t])));
    assert!(pack.characters.iter().all(|c| c.teams.is_none()));
    // The money bag (87) is drawn on its own rig: 5 joints, 12 clips, no character.json.
    let cash: Value = serde_json::from_slice(files["characters/87/character.gltf"]).unwrap();
    assert_eq!(cash["skins"][0]["joints"].as_array().unwrap().len(), 5);
    assert_eq!(cash["animations"].as_array().unwrap().len(), 12);
    assert!(!files.contains_key("characters/87/character.json"));

    // Map 0x24, Tank (`lab/research/mp-original/tank.md`): script 0xB8, no lift nor kill gate, the
    // bunkers' plant zones (triggers 0x11-0x14); its 8 doors solid at their placements (6 faces
    // each); the 4 power-ups roam the 6 spots and are back after 300 VIs; 16 static lights over
    // ambient (19, 68, 219); the tank (119) on its own rig: 8 clips, model lift 0, its parts, its
    // glTF drawing the brake lights unlit (4355), the headlights lit (4368) and flag 0 (4364), its
    // other flags as team looks; the props glTF of its hidden placements.
    let mut names = PACK_CHARACTERS.to_vec();
    names.push("119");
    let tank_map = conker_assets::Slice::load("24", &names, PACK_WEAPONS, read).unwrap();
    assert_eq!(conker_extract::ai_script(0x24), 0xB8);
    assert!(conker_rules::lift::parse(&tank_map.triggers, 0x24).is_empty());
    assert!(conker_rules::lift::kill_gates(&tank_map.triggers).is_empty());
    let h = |r: &[u8; 0x34], o: usize| i16::from_be_bytes([r[o], r[o + 1]]);
    let bunkers: Vec<_> = (tank_map.triggers.as_chunks::<0x34>().0[0x11..0x15].iter())
        .map(|r| (r[0x15], [0, 2, 4, 6, 8, 10].map(|o| h(r, o))))
        .collect();
    assert_eq!(
        bunkers,
        [
            (0x0E, [-830, -1555, -7062, 401, 128, 372]),
            (0x0E, [-7415, -952, -707, 388, 205, 331]),
            (0x0E, [779, -1562, 7030, 394, 224, 403]),
            (0x0E, [7393, -905, 704, 377, 198, 352]),
        ]
    );
    let doors = (tank_map.collision.iter())
        .filter(|t| matches!(t.layer, PackLayer::Placement(0..8)))
        .count();
    assert_eq!((tank_map.collision.len(), doors), (1694, 48));
    let gives: Vec<_> = (tank_map.pickups.iter())
        .map(|p| (p.placement, p.gives, p.respawn, p.roams.len()))
        .collect();
    let power = |placement, bit| (placement, Gives::Weapon(bit), Some(300), 6);
    assert_eq!(
        gives,
        [power(16, 0x80), power(8, 1), power(10, 0x10), power(11, 2)]
    );
    assert_eq!(tank_map.pickups[0].roams[0], [2504.0, -48.0, 2585.0]);
    assert_eq!(tank_map.lights.ambient, [19, 68, 219]);
    assert_eq!(tank_map.lights.lights.len(), 16);
    assert!((tank_map.lights.lights.iter()).all(|l| l.swing == 0.0 && !l.flicker));
    let tank = &tank_map.characters[3];
    assert_eq!(
        (tank.model_lift, tank.clips.len(), tank.root_clips.len()),
        (0, 8, 0)
    );
    let parts = conker_assets::Tank {
        turret: 2,
        gun: 3,
        muzzle: 4,
        brake: 11,
        brake_lit: "textures/brake-lit.png".into(),
        treads: 13,
        tread_joints: [13, 12],
    };
    assert_eq!(tank.tank.as_ref(), Some(&parts));
    let gltf: Value = serde_json::from_slice(files["characters/119/character.gltf"]).unwrap();
    assert_eq!(gltf["skins"][0]["joints"].as_array().unwrap().len(), 14);
    let image = |m: usize| gltf["materials"][m]["extras"]["image"].clone();
    let teams = tank.teams.as_ref().unwrap();
    assert_eq!(
        [image(11), image(12), image(13), image(teams.material)],
        [
            "flat 4355 mode 0",
            "flat 4368 mode 0",
            "flat 4607 mode 0",
            "flat 4364 mode 0"
        ]
    );
    let pngs: Vec<&[u8]> = (teams.textures.iter().chain([&parts.brake_lit]))
        .map(|t| files[format!("characters/119/{t}").as_str()])
        .collect();
    assert!((1..5).all(|t| !pngs[..t].contains(&pngs[t])));
    let props: Value = serde_json::from_slice(files["maps/24/props.gltf"]).unwrap();
    assert_eq!(props["nodes"].as_array().unwrap().len(), 22);
    assert_eq!(props["nodes"][1]["name"], "03-50@12");
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
/// no other file is under `audio/` but the outros' voices; and the event map holds what the
/// oracle's sound calls showed.
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
    let third = wav(&[5], 22020);
    let fourth = wav(&[6, 7], 22020);
    let fifth = wav(&[8], 22020);
    let song = wav(&[9], 22020);
    let sixth = wav(&[10, 11], 22020);
    let maps = [
        (0x2D, &background),
        (0x30, &other),
        (0x34, &third),
        (0x3F, &fourth),
        (0x2B, &fifth),
        (0x1A, &sixth),
    ]
    .map(|(scene, wav)| PackMap {
        scene,
        background: Some(wav),
        song: (scene == 0x2B).then_some(song.as_slice()),
        stem: None,
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
            "characters/136",
            "characters/156",
            "characters/157",
            "characters/59",
            "characters/87",
            "common",
            "maps/1a",
            "maps/2b",
            "maps/2d",
            "maps/30",
            "maps/34",
            "maps/3f"
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
    for (name, wav) in [
        ("2d", &background),
        ("30", &other),
        ("34", &third),
        ("3f", &fourth),
        ("2b", &fifth),
        ("1a", &sixth),
    ] {
        let file = format!("audio/{}", audio.events.maps[name].background[0].file);
        assert_eq!(files[file.as_str()], &wav[..]);
        listed.push(file);
    }
    // The War outros' voices and song (`maps/<map>/outros.json`: Total War's on 2d, Colors' on
    // 30, Beach's on 2b) and the maps' voice lines (Beach's escape).
    let lines = audio.events.maps.values().flat_map(|m| m.lines.values());
    listed.extend(lines.map(|l| format!("audio/{}", l.file)));
    for map in ["2d", "30", "2b", "1a"] {
        let file = format!("maps/{map}/outros.json");
        let outros: Vec<conker_assets::outro::Outro> =
            serde_json::from_slice(files[file.as_str()]).unwrap();
        let voices = outros.iter().flat_map(|o| o.voice.iter().chain(&o.song));
        listed.extend(voices.map(|v| format!("audio/{}", v.file)));
    }
    // Scene 0x30's own surface kind 0x11, 0x34's 6 and 0xE and 0x3F's 7 have footsteps; 0x2D's
    // bubbles are the only emitters; 0x34's and 0x3F's backgrounds are other sequences.
    for kind in [0x11, 6, 0xE, 7] {
        assert!(
            audio.events.footsteps.surfaces.contains_key(&kind),
            "{kind}"
        );
    }
    assert_eq!(audio.events.maps["2d"].emitters.len(), 2);
    // 0x2B's two bubbling spots play 0x2D's sounds, which stay in 0x2D's folder.
    let bubbles = &audio.events.maps["2b"].emitters;
    assert_eq!(bubbles.len(), 2);
    assert_eq!(bubbles[0].play, audio.events.maps["2d"].emitters[0].play);
    assert_eq!(audio.sounds.sounds[&1099].file, "maps/2d/1099.wav");
    // Colors cs5's script sounds (the tanks' 0x4A8, the march 0x6DE, 0x631, 0x633) in 30's folder,
    // and the outros' attachments and props in each map's `outro.gltf`.
    for id in [0x4A8, 0x6DE, 0x631, 0x633] {
        assert_eq!(
            audio.sounds.sounds[&id].file,
            format!("maps/30/{id:04}.wav")
        );
    }
    for map in ["2d", "30", "2b"] {
        let gltf: Value =
            serde_json::from_slice(files[format!("maps/{map}/outro.gltf").as_str()]).unwrap();
        assert!(gltf["scenes"][0]["name"] == "09-57", "{map}");
    }
    // 0x2B's detonation ring (bank 09 #0x157): one draw of 112 triangles, alpha-blended.
    let ring: Value = serde_json::from_slice(files["maps/2b/effects.gltf"]).unwrap();
    assert_eq!(ring["scenes"][0]["name"], "09-157");
    assert_eq!(ring["materials"][0]["alphaMode"], "BLEND");
    let indices = &ring["meshes"][0]["primitives"][0]["indices"];
    assert_eq!(
        ring["accessors"][indices.as_u64().unwrap() as usize]["count"],
        336
    );
    assert!(!files.contains_key("maps/2d/effects.gltf"));
    // Beach cs4's song 0x8A with 2b's recording; no other outro has one.
    assert_eq!(files["audio/maps/2b/song-8a.wav"], song.as_slice());
    let songs = |map: &str| {
        let outros: Vec<conker_assets::outro::Outro> =
            serde_json::from_slice(files[format!("maps/{map}/outros.json").as_str()]).unwrap();
        outros
            .into_iter()
            .map(|o| o.song.map(|s| (s.t, s.entry)))
            .collect::<Vec<_>>()
    };
    assert_eq!(songs("2b"), [Some((1, 0x8A)), None]);
    assert_eq!(songs("30"), [None, None]);
    assert_eq!(songs("1a"), [None, None]);
    assert!(
        audio.events.maps["2b"].background[0]
            .note
            .contains("sequence 77, channels 0-9, at volume 18000")
    );
    assert!(
        ["30", "34", "3f"]
            .iter()
            .all(|m| audio.events.maps[*m].emitters.is_empty())
    );
    assert!(
        audio.events.maps["34"].background[0]
            .note
            .contains("sequence 36")
    );
    assert!(
        audio.events.maps["3f"].background[0]
            .note
            .contains("sequence 84, channels 0-15, at volume 24000")
    );
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
    let mut names = PACK_CHARACTERS.to_vec();
    names.extend(["156", "157"]);
    let pack = conker_assets::Slice::load("2b", &names, PACK_WEAPONS, read).unwrap();
    let mut vault = PACK_CHARACTERS.to_vec();
    vault.push("59");
    let wise_guy = conker_assets::Slice::load("1a", &vault, PACK_WEAPONS, read).unwrap();
    names.push("59");
    let characters = pack.characters.iter().chain(&wise_guy.characters[3..]);
    for (name, character) in names.iter().zip(characters) {
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
    // The gas mask's put-on (0x63E at t10, 0x69B at t50) and take-off (0x69C at t18; its 0x63F
    // behind the t59 record, `held::due`), both characters (C, total-war-effects.md §4).
    for character in ["117", "128"] {
        let sounds = |clip, time| match play_at(character, clip, time) {
            Some(ClipAction::Play(p)) => p.sounds,
            _ => vec![],
        };
        assert_eq!(
            [(0x151, 10.0), (0x151, 50.0), (0x14E, 18.0), (0x14E, 59.0)].map(|(c, t)| sounds(c, t)),
            [vec![0x63E], vec![0x69B], vec![0x69C], vec![0x63F]]
        );
    }
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
    // The villagers' voice tables: the same pain voices for both, a cry of their own on clip 2, no
    // death voice (the Tedi's 0x58 entry is empty for them).
    assert_eq!(cue(&hit, Some("156")), Some((1715..=1726).collect()));
    assert_eq!(cue(&hit, Some("157")), cue(&hit, Some("156")));
    let Some(ClipAction::Play(cry)) = play_at("156", 2, 26.0) else {
        panic!("the male villager's cry")
    };
    assert_eq!(cry.sounds, [1709, 1710, 1711]);
    assert_eq!(play_at("157", 0x58, 21.0), None);
    let firing = On::Firing {
        weapon: 0x09,
        clips: vec![0x7C, 0x20D],
    };
    assert_eq!(cue(&firing, None), Some(vec![719]));
    // The Vault's (`heist.md`; C): the tommy gun's loop as the twin guns', the Wise Guy's battle cry
    // and pickup voices.
    let tommy = On::Firing {
        weapon: 0x38,
        clips: vec![0x1E0, 0x253, 0x254],
    };
    assert_eq!(cue(&tommy, None), Some(vec![719]));
    assert_eq!(cue(&tommy, Some("59")), Some((1347..=1350).collect()));
    let pickup = |weapon| On::Pickup {
        gives: Gives::Weapon(weapon),
    };
    assert_eq!(cue(&pickup(0x38), None), Some(vec![688]));
    assert_eq!(cue(&pickup(0x16), Some("59")), Some(vec![1342, 1343]));
    // The mounted guns (`mounted-gun.md` §4.4; V the oracle's sound calls).
    assert_eq!(cue(&On::Gun { from: 3, to: 4 }, None), Some(vec![0x616]));
    assert_eq!(cue(&On::GunStop, None), Some(vec![0x618, 0x619, 0x61A]));
    assert_eq!(cue(&On::GunZoom { step: 2 }, None), Some(vec![0x622]));
    // War: Beach (`beach-presentation.md` §6; V the oracle's sound calls): the escape's radio (MP3
    // 0x1A8), the radio voices and their clicks, the detonation's three records, the strike.
    let line = &audio.events.maps["2b"].lines["escape"];
    assert_eq!(
        (line.file.as_str(), line.volume),
        ("maps/2b/voice-1a8.wav", 32767)
    );
    let (rate, radio) = read_wav(files["audio/maps/2b/voice-1a8.wav"]).unwrap();
    let length = radio.len() as f32 / rate as f32;
    assert!(
        rate == 22050 && (4.0..8.0).contains(&length),
        "{rate} Hz {length} s"
    );
    // A bot says the next id at 32700 (V radio: 0x6A6), a human at 30000.
    for bot in [false, true] {
        let b = u16::from(bot);
        for (on, male, female) in [
            (On::PlungerReady { bot }, 0x6A9, 0x6A5),
            (On::Plunger { bot }, 0x6AB, 0x6A7),
        ] {
            assert_eq!(cue(&on, Some("156")), Some(vec![male + b]));
            assert_eq!(cue(&on, Some("157")), Some(vec![female + b]));
            let c = (audio.events.cues.iter()).find(|c| c.on == on).unwrap();
            assert_eq!(
                (c.play.volume, c.delay),
                ([30000, 32700][usize::from(bot)], 10)
            );
        }
    }
    // The escape tunnels' sound zones, triggers 0x23 and 0x24 (`beach-environment.md` §5; V the
    // record at read 600: vol 0x43B3, far 0x569 | 0x8000, near 0x1C3): 0x4A7, centred, horizontal.
    let zones: Vec<_> = (audio.events.maps["2b"].zones.iter())
        .map(|z| {
            let p = &z.play;
            (
                z.trigger,
                z.at,
                z.horizontal,
                p.sounds.clone(),
                p.volume,
                p.near,
                p.far,
            )
        })
        .collect();
    assert_eq!(
        zones,
        [
            (
                0x23,
                [-1536, -1434, -3769],
                true,
                vec![0x4A7],
                17331,
                451,
                1385
            ),
            (
                0x24,
                [987, -1434, -4077],
                true,
                vec![0x4A7],
                16350,
                601,
                1487
            )
        ]
    );
    assert_eq!(audio.sounds.sounds[&0x4A7].file, "maps/2b/1191.wav");
    // 0x1A's (V `lab/audio/map1a/`): its bases' hum, triggers 0x34-0x37; its 2 lamps and 5
    // switching lights; song 30 at 8000.
    let vault = &audio.events.maps["1a"];
    let zones: Vec<_> = (vault.zones.iter())
        .map(|z| {
            (
                z.trigger,
                z.play.sounds.clone(),
                z.play.volume,
                z.play.near,
                z.play.far,
            )
        })
        .collect();
    assert_eq!(
        zones,
        (0x34..=0x37)
            .map(|t| (t, vec![1116], 16350, 797, 903))
            .collect::<Vec<_>>()
    );
    assert_eq!(audio.sounds.sounds[&1116].file, "maps/1a/1116.wav");
    // Every kind-2 zone stops the frame after the last player leaves (C func_150A09D0; V 0x1A's).
    let zones = vault.zones.iter().chain(&audio.events.maps["2b"].zones);
    assert!(zones.map(|z| z.stops_on_leave).eq([true; 6]));
    // Heist (`heist-presentation.md` §5; V snd_bag): the bag's own clips' sounds (473 at the grab,
    // the release and the landing, 25 % at the start of the carried walk and run; 190 at the
    // landing), the carrier's footsteps on 0x116/0x117, the voices and the throw.
    let bag: Vec<_> = (audio.events.characters["87"].iter())
        .map(|e| match &e.action {
            ClipAction::Play(p) => (e.clip, e.time, p.sounds.clone(), p.chance),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        bag,
        [
            (5, 0.0, vec![473], 255),
            (5, 8.0, vec![190], 255),
            (5, 8.0, vec![473], 255),
            (7, 0.0, vec![473], 64),
            (8, 0.0, vec![473], 64),
            (9, 0.0, vec![473], 255),
            (9, 8.0, vec![190], 255),
            (9, 8.0, vec![473], 255)
        ]
    );
    assert_eq!(audio.sounds.sounds[&473].file, "characters/87/0473.wav");
    let carrier: Vec<_> = (audio.events.characters["59"].iter())
        .filter(|e| (0x114..=0x118).contains(&e.clip))
        .map(|e| (e.clip, e.time, e.action.clone()))
        .collect();
    assert_eq!(
        carrier,
        [
            (0x116, 10.0, ClipAction::Footstep(0)),
            (0x116, 30.0, ClipAction::Footstep(0)),
            (0x117, 7.0, ClipAction::Footstep(1)),
            (0x117, 17.0, ClipAction::Footstep(1))
        ]
    );
    let heist = |on: On| {
        let c = (audio.events.cues.iter()).find(|c| c.on == on).unwrap();
        let p = &c.play;
        (p.sounds.clone(), p.volume, p.own, p.near, p.far, p.channel)
    };
    assert_eq!(
        heist(On::CashPickup),
        (vec![1130, 1131], 32700, Some(32700), 500, 1000, Channel::B)
    );
    assert_eq!(
        heist(On::CashSteal { rare: true }),
        (vec![1351], 32700, Some(32700), 500, 1000, Channel::B)
    );
    assert_eq!(
        heist(On::CashSteal { rare: false }),
        (
            vec![1125, 1126, 1127],
            32700,
            None,
            500,
            1000,
            Channel::Free
        )
    );
    assert_eq!(heist(On::CashDelivery).0, [1128, 1129]);
    assert_eq!(
        heist(On::CashThrow),
        (vec![91], 28000, None, 500, 2500, Channel::B)
    );
    let volumes: Vec<u16> = vault.emitters.iter().map(|e| e.play.volume).collect();
    assert_eq!(volumes, [25000, 25000, 16000, 16000, 16000, 16000, 16000]);
    assert!(
        vault.background[0]
            .note
            .contains("sequence 30, channels 0-15, at volume 8000")
    );
    assert!(
        ["2d", "30", "34", "3f"]
            .iter()
            .all(|m| audio.events.maps[*m].zones.is_empty())
    );
    assert_eq!(cue(&On::RadioClick { end: false }, None), Some(vec![0x5B0]));
    assert_eq!(cue(&On::RadioClick { end: true }, None), Some(vec![0x5B1]));
    let blasts: Vec<_> = (audio.events.cues.iter())
        .filter(|c| c.on == On::Detonation)
        .map(|c| (c.play.sounds[0], c.at, c.delay, c.play.near, c.play.far))
        .collect();
    assert_eq!(
        blasts,
        [
            (0x2B7, Some([-319, -273, -3419]), 0, 7000, 9000),
            (0x2B8, Some([-2038, 1560, -2631]), 23, 7000, 9000),
            (0x2B6, Some([1519, 947, -2633]), 45, 7000, 9000)
        ]
    );
    assert_eq!(cue(&On::StrikeCall, None), Some(vec![0x69E]));
    assert_eq!(
        cue(&On::StrikeCall, Some("157")),
        Some((1715..=1726).collect())
    );
    assert_eq!(cue(&On::StrikeBlast, None), Some(vec![0x69D]));
    assert_eq!(cue(&On::Debris, None), Some((0x1B9..=0x1C1).collect()));
    // War: Colors (`war-leftovers.md` §7): the capture jingles (MP3 0x1A4, 0x1A7), the flag's radio
    // lines (2 squirrel, 7 Tedi variants; a bot's the next id; 10 frames after the click), the
    // canister's the same way, the losers' voices.
    for (team, id) in [("squirrels", "1a4"), ("tediz", "1a7")] {
        let line = &audio.events.maps["30"].lines[&format!("jingle-{team}")];
        assert_eq!(
            (line.file.as_str(), line.volume),
            (format!("maps/30/jingle-{id}.wav").as_str(), 32767)
        );
        let (rate, pcm) = read_wav(files[format!("audio/{}", line.file).as_str()]).unwrap();
        assert!(
            rate == 22050 && pcm.len() > rate as usize,
            "{team}: {rate} Hz"
        );
    }
    let radio = |on: On| {
        let c = (audio.events.cues.iter()).find(|c| c.on == on).unwrap();
        (c.play.sounds.clone(), c.play.volume, c.delay)
    };
    let flag = |flag, bot| radio(On::Flag { flag, bot });
    assert_eq!(flag(0, false), (vec![0x5A3, 0x5A5], 30000, 10));
    assert_eq!(flag(0, true), (vec![0x5A4, 0x5A6], 32700, 10));
    assert_eq!(
        flag(1, false).0,
        (0x642..=0x64E).step_by(2).collect::<Vec<_>>()
    );
    assert_eq!(
        flag(1, true).0,
        (0x643..=0x64F).step_by(2).collect::<Vec<_>>()
    );
    let carry = radio(On::Carry {
        canister: 1,
        bot: true,
    });
    assert_eq!(carry, (vec![0x641], 32700, 10));
    assert_eq!(
        cue(&On::Lose { team: 0 }, None),
        Some(vec![0x657, 0x658, 0x659])
    );
    assert_eq!(
        cue(&On::Lose { team: 1 }, None),
        Some(vec![0x662, 0x663, 0x664])
    );
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
        song: None,
        stem: None,
    }];
    let export = conker_extract::pack(&rom, &maps).unwrap();
    let files: BTreeMap<&str, &[u8]> = (export.files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let hud = Hud::load(|p| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    assert_eq!(hud.to_json(), files[HUD]);
    let end = End::load(|p| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    assert_eq!(end.to_json(), files[END]);
    // The STATS stick's 6 frames side by side (0x7E7..0x7EC).
    assert_eq!(end.textures["stick"].size, [192, 32]);
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
    // A mounted gun's gunner view (`mounted-gun.md` §3; its PNGs' sizes checked above).
    let size = |name: &str| hud.textures[name].size;
    assert_eq!(
        [
            "gun_handle",
            "gun_handle_lit",
            "gun_flash",
            "gun_corner",
            "gun_tick_side",
            "gun_tape_heading",
            "gun_tape_elevation"
        ]
        .map(size),
        [
            [64, 32],
            [64, 32],
            [32, 32],
            [16, 16],
            [16, 16],
            [16, 8],
            [8, 16]
        ]
    );
    assert_eq!(
        (hud.gun.sight.len(), &hud.gun.sight[0].image.texture[..]),
        (9, "reticle")
    );
    assert_eq!(hud.gun.handles.each_ref().map(|h| h.rect[0]), [9.0, 185.0]);
    assert_eq!(hud.gun.flash.centre, [205.0, 176.0]);
    // Heist (`heist-presentation.md` §1, §6; V hud1 DL 200, cs3b DL 970): the strip's "$" pip, the
    // skull and the carried bag; the STATS headers' team names, GREENS + WIN in 32-pixel tiles from
    // 65 (the FRENCHYS layout), the bubbles' sprite.
    let heist = &hud.heist;
    assert_eq!(
        (&heist.pip.texture[..], heist.pip.rect, size("pips")),
        ("pips", [14, 16, 7, 7], [32, 32])
    );
    assert_eq!(
        (heist.skull.rect, heist.bag.rect),
        ([48, 20, 16, 16], [142, 3, 12, 11])
    );
    assert_eq!(size("bubble_particle"), [64, 64]);
    assert_eq!(
        ["reds", "blues", "greens", "yellows"].map(|n| end.textures[n].size),
        [[64, 32], [64, 32], [96, 32], [96, 32]]
    );
    let tiles: Vec<i16> = (end.heist.won[2].iter())
        .flat_map(|p| (0..p.rect[2] / 32).map(|k| p.rect[0] + 32 * k))
        .collect();
    assert_eq!(tiles, [65, 97, 129, 151, 183]);

    // War: Beach (`beach-presentation.md` §1, §3): the strip's heads, the escape countdown and the
    // STATS header, against the oracle's draws (`beach-presentation-scratch/hw/<run>_dl_<n>/
    // draws.json`: det1 DL 318 one view, e1 DL 99 P1's view at the top of a split, cs4 DL 700
    // FRENCHYS WIN, cs3 DL 800 TEDIZ WIN).
    assert_eq!(
        (size("beach_teams"), end.textures["frenchys"].size),
        ([16, 32], [96, 32])
    );
    for name in ["explosion", "smoke", "fire"] {
        assert_eq!(size(name), [32, 32], "{name}");
    }
    assert_eq!(size("shell"), [16, 16]);
    assert_eq!(
        (size("strike_glow"), size("strike_streak")),
        ([64, 64], [128, 32])
    );
    let beach = &hud.beach;
    // A line's glyph rects as the client lays text out: centred on `at[0]` by its glyphs' widths
    // less 1 each, or from it; each glyph at the pen plus its offset.
    let layout = |t: &conker_assets::hud::Text, line: &str, down: i16| -> Vec<[i16; 4]> {
        use conker_assets::hud::Align;
        let glyphs: Vec<_> = line.chars().map(|c| hud.glyph(c).unwrap()).collect();
        let width: i16 = glyphs.iter().map(|g| i16::from(g.size[0]) - 1).sum();
        let mut pen = match t.align {
            Align::Left => t.at[0],
            Align::Centre => t.at[0] - width / 2,
        };
        (glyphs.iter())
            .map(|g| {
                let [w, h] = g.size.map(i16::from);
                let [dx, dy] = g.offset.map(i16::from);
                let rect = [pen + dx, t.at[1] + down + dy, w, h];
                pen += dx + w - 1;
                rect
            })
            .collect()
    };
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../lab/research/mp-original/beach-presentation-scratch/hw"
    );
    let draws = |run: &str| -> Option<Vec<Value>> {
        let text = std::fs::read_to_string(format!("{dir}/{run}/draws.json")).ok()?;
        serde_json::from_str(&text).ok()
    };
    let Some(det1) = draws("det1_dl_318") else {
        eprintln!("{dir}: no oracle draws, skipping them");
        return;
    };
    // (rect as [x, y, width, height], texture, s, t, prim) of each draw.
    let of = |d: &Value| {
        let r: Vec<i16> = (d["rect"].as_array().unwrap().iter())
            .map(|v| v.as_f64().unwrap() as i16)
            .collect();
        let st = |k: usize| d["st"][k].as_f64().unwrap() as u16;
        (
            [r[0], r[1], r[2] - r[0], r[3] - r[1]],
            d["texture"].as_str().unwrap().to_string(),
            [st(0), st(1)],
            d["prim"].as_str().unwrap().to_string(),
        )
    };
    let det1: Vec<_> = det1.iter().map(of).collect();
    // The heads: villagers at 198 from (0, 16), Tediz at 66 from (0, 0).
    let heads: Vec<_> = (det1.iter())
        .filter(|d| d.1.ends_with("RGBA32_16x32"))
        .map(|d| (d.0, d.2))
        .collect();
    let ours: Vec<_> = (0..2)
        .rev()
        .map(|k| {
            let i = &beach.strip.lives[k].rect;
            (beach.strip.rects[k], [i[0], i[1]])
        })
        .collect();
    assert_eq!(heads, ours);
    assert_eq!(beach.strip.kills, beach.strip.lives);
    type Draw = ([i16; 4], String, [u16; 2], String);
    let glyphs = |draws: &[Draw], prim: &str, y: std::ops::Range<i16>| {
        (draws.iter())
            .filter(|d| d.1.contains("_I8_") && d.3 == prim && y.contains(&d.0[1]))
            .map(|d| d.0)
            .collect::<Vec<[i16; 4]>>()
    };
    // The scores: the Tediz' 0 at 83, the villagers' 1 at 215 (grey over a black shadow).
    let numbers = [&beach.strip.numbers[1], &beach.strip.numbers[0]];
    let want: Vec<_> = [(numbers[0], "0"), (numbers[1], "1")]
        .iter()
        .flat_map(|(t, line)| layout(t, line, 0))
        .collect();
    assert_eq!(glyphs(&det1, "c0c0c0ff", 190..216), want);
    // The countdown: "81" in one view (top 6), "89" in P1's view of a split (its top 1, + 6).
    assert_eq!(
        glyphs(&det1, "ffffffff", 0..30),
        layout(&beach.escape, "81", 0)
    );
    let e1: Vec<_> = draws("e1_dl_99").unwrap().iter().map(of).collect();
    assert_eq!(
        glyphs(&e1, "ffffffff", 0..30),
        layout(&beach.escape, "89", 1)
    );
    // The STATS headers' 32-pixel tiles: FRENCHYS WIN (cs4), TEDIZ WIN (cs3).
    for (run, team) in [("cs4_dl_700", 0), ("cs3_dl_800", 1)] {
        let tiles: Vec<i16> = (draws(run).unwrap().iter().map(of))
            .filter(|d| d.3 == "ffff00ff" && d.0[1] == 34)
            .map(|d| d.0[0])
            .collect();
        let ours: Vec<i16> = (end.teams.beach_won[team].iter())
            .flat_map(|p| (0..p.rect[2] / 32).map(|k| p.rect[0] + 32 * k))
            .collect();
        assert_eq!(tiles, ours, "{run}");
        assert!(
            end.teams.beach_won[team]
                .iter()
                .all(|p| p.rect[1] == 34 && p.rect[3] == 32)
        );
    }
}

/// Tank's sound on 0x24 (`lab/research/mp-original/tank.md` §8; V
/// `lab/audio/captures/tank_s1_calls.txt`): song 32's recording, the bomb's countdown (song 50)
/// as a map line, the tank's fire on its clip 1, and its engine, gear, blasts, deaths and pickups
/// as cues for the tank, their sounds in its folder.
#[test]
fn tank_audio_reads_back() {
    use conker_assets::audio::{Audio, ClipAction, On};
    use conker_extract::audio::{pack_audio, wav};
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let (background, countdown) = (wav(&[1, 2], 22020), wav(&[3], 22020));
    let maps = [(
        "24".to_string(),
        Some(background.as_slice()),
        None,
        Some(countdown.as_slice()),
    )];
    let surfaces = std::collections::BTreeSet::new();
    let (files, _) = pack_audio(&rom, &surfaces, &maps, &[conker_extract::TANK], &[]).unwrap();
    let files: BTreeMap<&str, &[u8]> = (files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let audio = Audio::load(|p: &str| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    let map = &audio.events.maps["24"];
    assert_eq!(files["audio/maps/24/background.wav"], &background[..]);
    assert!(
        map.background[0]
            .note
            .contains("sequence 32, channels 0-15, at volume 18000")
    );
    assert_eq!(map.lines["countdown"].file, "maps/24/countdown.wav");
    assert_eq!(files["audio/maps/24/countdown.wav"], &countdown[..]);
    assert!(map.emitters.is_empty() && map.zones.is_empty());
    let clips: Vec<_> = (audio.events.characters["119"].iter())
        .map(|c| match &c.action {
            ClipAction::Play(p) => (c.clip, p.sounds.clone()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(clips, [(1, vec![699])]);
    let cue = |on: On| {
        (audio.events.cues.iter())
            .filter(|c| c.on == on && c.character.as_deref() == Some("119"))
            .map(|c| {
                (
                    c.play.sounds.clone(),
                    c.play.volume,
                    c.play.near,
                    c.play.far,
                )
            })
            .collect::<Vec<_>>()
    };
    let blast = (vec![689, 693, 694, 695, 701, 702], 32700, 1000, 3000);
    assert_eq!(
        cue(On::TankEngine { moving: false }),
        [(vec![1191], 0x2000, 200, 800)]
    );
    assert_eq!(
        cue(On::TankEngine { moving: true }),
        [(vec![1192], 0x2EE0, 200, 800)]
    );
    assert_eq!(
        cue(On::TankGear { coast: false }),
        [(vec![1187, 1188, 1189, 1190], 28000, 300, 1800)]
    );
    assert_eq!(cue(On::TankBlast { ground: false }), [blast.clone()]);
    assert_eq!(
        cue(On::TankBlast { ground: true }),
        [blast, (vec![750], 30000, 500, 1000)]
    );
    assert_eq!(
        cue(On::TankDeath { bomb: false }),
        [(vec![742], 12000, 500, 1000)]
    );
    assert_eq!(
        cue(On::TankDeath { bomb: true }),
        [(vec![735], 12000, 500, 1000)]
    );
    assert_eq!(cue(On::BombPickup), [(vec![11, 12], 26000, 100, 1000)]);
    let pickups: Vec<_> = [0x80, 1, 0x10, 2]
        .map(|bit| {
            cue(On::Pickup {
                gives: Gives::Weapon(bit),
            })[0]
                .0[0]
        })
        .into();
    assert_eq!(pickups, [1296, 1187, 385, 752]);
    for id in [1191, 1192, 742, 735, 750, 11, 12, 752] {
        assert_eq!(
            audio.sounds.sounds[&id].file,
            format!("characters/119/{id:04}.wav")
        );
    }
}
