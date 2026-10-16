//! The asset pack against the ROM path (path in env `CONKER_ROM`; the ROM test skips when it is
//! unset): `pack` of both maps (0x2D, 0x30) exported to a temporary folder and read back with
//! conker-assets gives, bit for bit, what the client (`Slice::from_rom`) and the server
//! (`conker-server`'s `scene`) build from the ROM, and its glTF files are valid, self-consistent
//! and draw the same as the in-memory GLBs.

use std::collections::{BTreeMap, BTreeSet};
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
use serde_json::{Value, json};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("conker-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Whether an RGBA PNG (the pack writes them so) has a texel that is neither clear nor black.
fn drawn(file: &[u8]) -> bool {
    let mut reader = png::Decoder::new(std::io::Cursor::new(file))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    (buf[..info.buffer_size()].as_chunks::<4>().0.iter()).any(|p| p[3] > 0 && p[..3] != [0, 0, 0])
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
    let maps = [0x2D, 0x30, 0x34, 0x3F, 0x2B, 0x1A, 0x24, 0x33].map(|scene| PackMap {
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
    // The fixed cameras' points ([0x0C, scene, 8]): the sea's hints 0x26 and 0x27 look from points
    // 0 and 1 (V cam+0x288 in `cameras-clips-g4.md` §1's swim1 and east runs); 0x1A's trip wires'
    // four; none on the other maps.
    let at: Vec<[i16; 3]> = beach.cameras.iter().map(|c| c.at).collect();
    assert_eq!(at.len(), 6);
    assert_eq!(at[..2], [[2024, 277, 9703], [-2290, 728, 10913]]);
    assert!(beach.cameras.iter().all(|c| c.rest.len() == 36));
    assert!(pack.cameras.is_empty());
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
    // The holster's op 41 (t14) removes the crossbow, a child object; item 0x3A's two grenades, on
    // the right hand, the head stumps (G5: 117's #0x59 on joint 0, 128's #0x58 on joint 1), then
    // the racer classes 0xD-0xF's helmets (#0x5A, #0x5C, #0x5B on joint 12, as 0x33's props) end
    // the list, with no events (`held-leftovers.md` §1, `LOB_ACTIONS`, `STUMP_ACTIONS`).
    let held = |entry| pack.held.iter().find(|h| h.entry == entry).unwrap();
    assert_eq!(held(0x6E).remove, [(0xC6, 14.0)]);
    let last: Vec<_> = (pack.held[pack.held.len() - 7..].iter())
        .map(|h| (h.entry, h.joint, h.create.len() + h.remove.len()))
        .collect();
    let lobs = [(137, 9, 0), (186, 9, 0), (0x59, 0, 0), (0x58, 1, 0)];
    assert_eq!(last[..4], lobs);
    assert_eq!(last[4..], [(0x5A, 12, 0), (0x5C, 12, 0), (0x5B, 12, 0)]);
    // Body parts (`held-leftovers.md` §2): the Tedi's head sphere first; none for the weasel guard
    // and the tank, which the game tests as a cylinder.
    let parts = tedi.parts.as_ref().unwrap();
    assert_eq!(
        (parts.broad, parts.spheres.len(), parts.spheres[0]),
        ([117, 95], 35, [12, 0xA, 38, 0, 33, 5])
    );
    for name in ["152", "119"] {
        let path = format!("characters/{name}/character.json");
        let c: Value = serde_json::from_slice(files[path.as_str()]).unwrap();
        assert!(c.get("parts").is_none(), "{name}");
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
    let squirrel = conker_extract::bind_faces(&rom, squirrel, &textures)
        .unwrap()
        .0;
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
    // The child objects' rigs, as `rig` draws them.
    for (name, entries) in conker_assets::RIGS {
        let rig = conker_extract::rig(&rom, entries).unwrap();
        let glb = rig.gltf(&textures, true).unwrap().glb();
        check_gltf(&dir, &conker_assets::rig_gltf(PACK_WEAPONS, name), &glb);
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
    assert_eq!(
        manifest.maps,
        ["2d", "30", "34", "3f", "2b", "1a", "24", "33"]
    );
    assert_eq!(
        manifest.characters[..9],
        ["128", "117", "136", "156", "157", "59", "119", "83", "144"]
    );

    // Each map's solid placements (the oracle's RAM) are those with collision flags less its
    // pickups (`+0x4F |= 0x20`) and its hidden ones: the game's layer-2 filter on ROM data. On 0x1A
    // the match sets 0x20 on every placement but 0, so its drawn 13 is not solid (V,
    // `lab/oracle/map1a/props_pickups_lights.md` §1).
    for scene in [0x2D, 0x30, 0x34, 0x3F, 0x2B, 0x1A, 0x24, 0x33] {
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
        ("33", 0x33, 17),
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
        // War: Beach's sea: placement 1 a water surface, with its current.
        let water = (map.collision.iter())
            .filter(|t| matches!(t.layer, PackLayer::Water(_)))
            .count();
        let current = map.current.iter().map(|p| p[3]).collect::<Vec<_>>();
        match scene {
            0x2B => assert_eq!((water, &current[..]), (10, &[64, 75, 40, 10][..])),
            _ => assert_eq!((water, &current[..]), (0, &[][..])),
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
    assert_eq!(vault.cameras[0].at, [4169, 39, -207]);
    assert_eq!(vault.cameras.len(), 4);
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
    // Its runtime segments (F4): the two screens and a lamp per switching light, each material in
    // `level.gltf` drawing its first frame (the lamps lit), every frame a PNG in the map's folder.
    let segments = conker_extract::map_segments(&rom, 0x1A, &textures)
        .unwrap()
        .0;
    assert_eq!(vault.segments, segments);
    assert_eq!(segments.lamps.len(), 5);
    let level: Value = serde_json::from_slice(files["maps/1a/level.gltf"]).unwrap();
    let screens = segments.screens.as_ref().unwrap().panels.iter();
    let lamps = segments.lamps.iter();
    for (materials, frames) in (screens.map(|p| (vec![p.material.clone()], p.frames.clone())))
        .chain(lamps.map(|l| (l.materials.clone(), vec![l.on.clone(), l.off.clone()])))
    {
        for m in &materials {
            assert_eq!(drawn_uri(&level, m), frames[0], "{m}");
        }
        for f in &frames {
            assert!(files[format!("maps/1a/{f}").as_str()].starts_with(b"\x89PNG"));
        }
    }
    // The vertices the lights pass by (`_UNLIT`, 1.0: their flag's bit 15, the oracle's
    // `lightVerticesCBFD`): V 2b, 97 terrain vertices keep their colour (83 black, 2 grey, the
    // bulbs' 12 white); 0x1A's 58.
    let unlit = |name: &str| -> usize {
        let level: Value =
            serde_json::from_slice(files[conker_assets::level_gltf(name).as_str()]).unwrap();
        let bin = files[format!("maps/{name}/level.bin").as_str()];
        let meshes = level["meshes"].as_array().unwrap().iter();
        let flags = meshes.flat_map(|m| m["primitives"].as_array().unwrap());
        (flags.filter_map(|p| p["attributes"]["_UNLIT"].as_u64()))
            .map(|a| {
                let a = &level["accessors"][a as usize];
                let v = &level["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
                let start = v["byteOffset"].as_u64().unwrap_or(0) as usize;
                let n = a["count"].as_u64().unwrap() as usize;
                let words = bin[start..start + 4 * n].as_chunks::<4>().0.iter();
                words.filter(|w| f32::from_le_bytes(**w) == 1.0).count()
            })
            .sum()
    };
    assert_eq!((unlit("2b"), unlit("1a")), (97, 58));
    // No packed level leaves a material on a runtime segment unbound (the `-seg` ones draw their
    // first frame); `segments.json` is empty off 0x1A.
    for name in ["2d", "30", "34", "3f", "2b", "1a", "24", "33"] {
        let level = files[conker_assets::level_gltf(name).as_str()];
        let level: Value = serde_json::from_slice(level).unwrap();
        for m in level["materials"].as_array().unwrap() {
            let texture = &m["pbrMetallicRoughness"]["baseColorTexture"];
            let segment = m["name"].as_str().unwrap().contains("-seg");
            assert!(!segment || texture.is_object(), "{name}: {}", m["name"]);
        }
        let empty = files[format!("maps/{name}/segments.json").as_str()] == b"{}\n";
        assert_eq!(empty, name != "1a", "{name}");
    }
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
    // The money bag (87) is drawn on its own rig: 5 joints, 12 clips, their table in its
    // character.json (pair 0 the logical 0, 9 and 10); Don Weaso (134) has its 5 outro clips.
    let cash: Value = serde_json::from_slice(files["characters/87/character.gltf"]).unwrap();
    assert_eq!(cash["skins"][0]["joints"].as_array().unwrap().len(), 5);
    assert_eq!(cash["animations"].as_array().unwrap().len(), 12);
    let table: Value = serde_json::from_slice(files["characters/87/character.json"]).unwrap();
    assert_eq!(table["clips"].as_array().unwrap().len(), 12);
    assert_eq!(table["clips"][0]["logical"], serde_json::json!([0, 9, 10]));
    let don: Value = serde_json::from_slice(files["characters/134/character.json"]).unwrap();
    assert_eq!(don["clips"].as_array().unwrap().len(), 5);

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
        treads: [13, 15],
        tread_joints: [13, 12],
    };
    assert_eq!(tank.tank.as_ref(), Some(&parts));
    let gltf: Value = serde_json::from_slice(files["characters/119/character.gltf"]).unwrap();
    assert_eq!(gltf["skins"][0]["joints"].as_array().unwrap().len(), 14);
    // The treads, a material per side (G5): the second a copy of the first but for its name, its
    // primitive the last, every vertex of each side on its joint (13 then 12), 16 triangles each.
    let mut copy = gltf["materials"][15].clone();
    copy["name"] = gltf["materials"][13]["name"].clone();
    assert_eq!(copy, gltf["materials"][13]);
    assert_eq!(gltf["materials"].as_array().unwrap().len(), 16);
    let prims = gltf["meshes"][0]["primitives"].as_array().unwrap();
    let accessor = |i: &Value| &gltf["accessors"][i.as_u64().unwrap() as usize];
    let bin = files["characters/119/character.bin"];
    for (k, joint) in parts.treads.into_iter().zip(parts.tread_joints) {
        let prim = (prims.iter())
            .find(|p| p["material"] == k)
            .unwrap_or_else(|| panic!("material {k}"));
        let joints = accessor(&prim["attributes"]["JOINTS_0"]);
        let view = &gltf["bufferViews"][joints["bufferView"].as_u64().unwrap() as usize];
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = joints["count"].as_u64().unwrap() as usize;
        assert!(
            (bin[start..start + 4 * count].as_chunks::<4>().0.iter())
                .all(|j| j[0] as usize == joint),
            "material {k}"
        );
        assert_eq!(accessor(&prim["indices"])["count"], 48, "material {k}");
    }
    assert_eq!(prims.last().unwrap()["material"], 15);
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
    // Scene 1: the carried gas bomb (action 0x18: bank 09 #0x3F on the turret's joint 2, V
    // `g5-pack-scratch/tank` k1 DL 45), one draw of 48 triangles at the origin.
    let props: Value = serde_json::from_slice(files["maps/24/props.gltf"]).unwrap();
    assert_eq!(props["nodes"].as_array().unwrap().len(), 23);
    assert_eq!(props["nodes"][1]["name"], "03-50@12");
    assert_eq!(props["scenes"][1]["nodes"], json!([22]));
    let bomb = &props["meshes"][props["nodes"][22]["mesh"].as_u64().unwrap() as usize];
    let prims = bomb["primitives"].as_array().unwrap();
    let indices = prims[0]["indices"].as_u64().unwrap() as usize;
    assert_eq!(
        (prims.len(), &props["accessors"][indices]["count"]),
        (1, &json!(144))
    );
    assert_eq!(parts.turret, 2);

    // Map 0x3F's War: Raptor (`lab/research/mp-original/raptor.md`): `raptor.json`; the raptor
    // (83) on its own rig, 39 joints (5 the skull, 6 the jaw and the carry point), 37 clips, model
    // lift 0; the baby (165) on the raptor's rig and clips; the egg (171) and the body (22) without
    // character.json; the props glTF of the cooked eggs, the never-shown 4-6 and the bone weapons.
    let raptor = conker_assets::raptor::Raptor::load("3f", read).unwrap();
    assert_eq!(raptor.to_json(), files["maps/3f/raptor.json"]);
    let rig = &raptor.raptor;
    assert_eq!(
        (
            rig.character.as_str(),
            rig.scale,
            rig.head,
            rig.jaw,
            rig.carry
        ),
        ("83", 0.7, 5, 6, 6)
    );
    assert_eq!(raptor.hidden, (0..15).collect::<Vec<u16>>());
    assert_eq!(raptor.cooked, [0, 1, 2, 3]);
    let gives: Vec<_> = (raptor.pickups.iter())
        .map(|p| (p.placement, p.gives, p.respawn))
        .collect();
    let (bow, knives) = (Gives::Weapon(0x41), Gives::Weapon(0x3F));
    let order = [bow, bow, knives, knives, bow, bow, knives, knives];
    let mut expected: Vec<_> = (15..).zip(order).map(|(k, g)| (k, g, Some(240))).collect();
    expected.extend([
        (23, Gives::Grenades, Some(240)),
        (24, Gives::Grenades, Some(240)),
    ]);
    assert_eq!(gives, expected);
    assert_eq!(
        raptor.spawns,
        [(8..16).collect(), (0..8).collect::<Vec<_>>()]
    );
    assert_eq!(
        (raptor.delivery, &raptor.feed[..], &raptor.hazard[..]),
        (0, &[1, 0x13][..], &[3, 0xD][..])
    );
    let records = |r: &conker_assets::raptor::Record| (r.character.clone(), r.uid, r.scale);
    assert_eq!(records(&raptor.baby), ("165".into(), 0x11, 0.45));
    assert_eq!(records(&raptor.body), ("22".into(), 0x13, 0.51));
    let eggs: Vec<_> = raptor.eggs.iter().map(records).collect();
    assert_eq!(
        eggs,
        [0x14, 0x15, 0x16].map(|u| ("171".to_string(), u, 0.2))
    );
    // The standing bite (attack 4: a kill) and the knock-back (6: 3 damage), C.
    let bite = |a: &conker_assets::raptor::Attack| (a.reaction, a.distance, a.radius, a.damage);
    assert_eq!(
        (bite(&rig.attacks[4]), bite(&rig.attacks[6])),
        ((0x46, 80, 30, 1), (0x1C, 50, 55, 3))
    );
    assert_eq!(rig.hits.len(), 9);
    let mut names = PACK_CHARACTERS.to_vec();
    names.push("83");
    let temple = conker_assets::Slice::load("3f", &names, PACK_WEAPONS, read).unwrap();
    let raptor_rig = &temple.characters[3];
    assert_eq!(
        (
            raptor_rig.model_lift,
            raptor_rig.clips.len(),
            raptor_rig.root_clips.len()
        ),
        (0, 37, 3)
    );
    // `hidden`: each draws its list 0 only (V); the egg has no `character.json`.
    for (name, joints, clips, hidden) in [
        ("83", 39, 37, Some(0xE)),
        ("165", 39, 37, Some(0x2)),
        ("171", 1, 0, None),
        ("22", 31, 32, Some(0x2)),
    ] {
        let gltf: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        let skin = gltf["skins"][0]["joints"].as_array().unwrap().len();
        let animations = gltf["animations"].as_array().map_or(0, Vec::len);
        assert_eq!((skin, animations), (joints, clips), "{name}");
        let json = format!("characters/{name}/character.json");
        let file = files
            .get(json.as_str())
            .map(|f| serde_json::from_slice(f).unwrap());
        let file: Option<conker_assets::Character> = file;
        assert_eq!(file.and_then(|c| c.hidden), hidden, "{name}");
    }
    let props: Value = serde_json::from_slice(files["maps/3f/props.gltf"]).unwrap();
    let nodes: Vec<_> = (props["nodes"].as_array().unwrap().iter())
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(nodes.len(), 15);
    assert_eq!(
        nodes[..5],
        [
            "04-3f-04@0",
            "04-3f-04@1",
            "04-3f-04@2",
            "04-3f-04@3",
            "03-55@4"
        ]
    );
    // Map 0x33's Race (`lab/research/mp-original/race.md`): `race.json` as `race_mode` builds it;
    // script 0xBC, no lights, lift nor kill gate; the power-ups back after 18 VIs; the bots' model
    // (144) on the Tedi's clips with its looks (class 0xF hides list 1); the props glTF of the board
    // and the helmets, a scene each.
    let race = conker_assets::race::Race::load("33", read).unwrap();
    assert_eq!(race, conker_extract::race_mode(&rom).unwrap());
    assert_eq!(race.to_json(), files["maps/33/race.json"]);
    // Its crowd (F3, F4): each panel's material in `level.gltf` draws its `frames[0]`, and every
    // frame is a PNG in the map's folder.
    let level: Value = serde_json::from_slice(files["maps/33/level.gltf"]).unwrap();
    let crowd = race.crowd.as_ref().unwrap();
    for panel in &crowd.panels {
        let uri = drawn_uri(&level, &panel.material);
        assert_eq!(uri, panel.frames[0], "{}", panel.material);
        for f in &panel.frames {
            assert!(files[format!("maps/33/{f}").as_str()].starts_with(b"\x89PNG"));
        }
    }
    let mut names = PACK_CHARACTERS.to_vec();
    names.push("144");
    let race_map = conker_assets::Slice::load("33", &names, PACK_WEAPONS, read).unwrap();
    assert!(race_map.lights.lights.is_empty());
    assert!(conker_rules::lift::parse(&race_map.triggers, 0x33).is_empty());
    assert!(conker_rules::lift::kill_gates(&race_map.triggers).is_empty());
    assert!(race_map.pickups.iter().all(|p| p.respawn == Some(18)));
    let barriers = (race_map.collision.iter())
        .filter(|t| matches!(t.layer, PackLayer::Placement(_)))
        .count();
    assert!(barriers > 0);
    let racer = &race_map.characters[3];
    assert_eq!(
        (&racer.clips, &racer.root_clips),
        (&pack.characters[1].clips, &pack.characters[1].root_clips)
    );
    assert_eq!(
        racer.looks,
        BTreeMap::from([(0xC, 0), (0xD, 0), (0xE, 0), (0xF, 2)])
    );
    eprintln!("model 144's lift: {}", racer.model_lift);
    let (props, scenes) = conker_extract::props_level(&rom, 0x33).unwrap();
    let mut gltf = extract_level::to_gltf(&props, &textures, "").unwrap();
    let mut json: Value = serde_json::from_str(&gltf.json).unwrap();
    let names: Vec<_> = (json["nodes"].as_array().unwrap().iter())
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, ["09-5d-00", "09-5a-00", "09-5c-00", "09-5b-00"]);
    json["scenes"] = scenes;
    gltf.json = json.to_string();
    check_gltf(&dir, &conker_assets::props_gltf("33"), &gltf.glb());

    // The bone crossbow in the hand is bank 01 #0x2F (a child object), the bone knives #0x7B.
    let held = |entry| pack.held.iter().find(|h| h.entry == entry).unwrap();
    assert_eq!((held(0x2F).joint, held(0x2F).create[0]), (9, (0x2C8, 21.0)));
    assert_eq!((held(0x7B).joint, held(0x7B).create[0]), (9, (0x2CA, 24.0)));

    // The picks (M14a, `lab/research/mp-original/characters.md` §3) after the racer: mesh-only glTFs
    // whose skeleton is the Tedi's node for node (names, children, animation slots), so its clips
    // drive them (`clips_from`); every player model's `hitbox` and `body` (the record's), no other's.
    let picks = conker_extract::PICKS.map(|p| p.to_string());
    assert_eq!(manifest.characters[9..18], picks);
    let mut names = PACK_CHARACTERS.to_vec();
    names.extend(picks.iter().map(String::as_str));
    let dm = conker_assets::Slice::load(PACK_MAP, &names, PACK_WEAPONS, read).unwrap();
    let skeleton = |name: &str| -> Vec<Value> {
        let gltf: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        assert_eq!(gltf["skins"][0]["joints"].as_array().unwrap().len(), 28);
        let nodes = gltf["nodes"].as_array().unwrap()[..28].iter();
        nodes
            .map(|n| serde_json::json!([n["name"], n["children"], n["extras"]["animSlot"]]))
            .collect()
    };
    for (name, c) in picks.iter().zip(&dm.characters[3..]) {
        let gltf: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        assert!(gltf.get("animations").is_none(), "{name}");
        assert_eq!(skeleton(name), skeleton("117"), "{name}");
        assert_eq!(c.clips_from.as_deref(), Some("117"));
        assert!(c.clips.is_empty() && c.root_clips.is_empty());
    }
    for name in [
        "128", "117", "136", "156", "157", "59", "83", "144", "119", "87", "134",
    ]
    .into_iter()
    .chain(picks.iter().map(String::as_str))
    {
        let json = files[format!("characters/{name}/character.json").as_str()];
        let c: conker_assets::Character = serde_json::from_slice(json).unwrap();
        let player = !["119", "87", "134"].contains(&name);
        let record = player.then(|| conker_extract::hitbox_body(&rom, name.parse().unwrap()));
        let record = record.map(Result::unwrap);
        assert_eq!(c.hitbox.zip(c.body), record, "{name}");
        assert_eq!(c.clips_from.is_some(), picks.iter().any(|p| p == name));
        let hidden = match name {
            "150" => Some(0x3E0),
            "159" | "160" => Some(0x710),
            "83" => Some(0xE),
            _ => None,
        };
        assert_eq!(c.hidden, hidden, "{name}");
    }
    assert_eq!(
        (dm.characters[1].hitbox, dm.characters[1].body),
        (Some([53, 87, 71]), Some([47, 91, 66]))
    );

    // The faces (`lab/research/mp-original/faces.md`): no glTF draws from a face segment (each is
    // bound to its open look); a blinker's `character.json` has both eyes, whose materials draw
    // its open PNG; no other character has `faces`.
    let blinkers = [
        "128", "136", "156", "157", "83", "144", "0", "150", "176", "22",
    ];
    for name in &manifest.characters {
        let gltf: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        let materials = gltf["materials"].as_array().unwrap();
        let image = |m: &Value| m["extras"]["image"].as_str().unwrap_or("").to_string();
        assert!(
            materials.iter().all(|m| !image(m).starts_with("segment")),
            "{name}"
        );
        let blinks = blinkers.contains(&name.as_str());
        let Some(json) = files.get(format!("characters/{name}/character.json").as_str()) else {
            assert!(!blinks, "{name}");
            continue;
        };
        let c: conker_assets::Character = serde_json::from_slice(json).unwrap();
        let segments: Vec<u8> = c.faces.iter().map(|f| f.segment).collect();
        assert_eq!(segments, if blinks { vec![6, 7] } else { vec![] }, "{name}");
        let file = |p: &str| files[format!("characters/{name}/{p}").as_str()];
        for f in &c.faces {
            assert!(f.textures.iter().all(|t| file(t).starts_with(b"\x89PNG")));
            for &m in &f.materials {
                let texture = &materials[m]["pbrMetallicRoughness"]["baseColorTexture"]["index"];
                let source = &gltf["textures"][texture.as_u64().unwrap() as usize]["source"];
                let uri = gltf["images"][source.as_u64().unwrap() as usize]["uri"].as_str();
                assert_eq!(
                    file(uri.unwrap()),
                    file(&f.textures[0]),
                    "{name} material {m}"
                );
            }
        }
    }

    // The materials' render modes (`lab/research/mp-original/alpha-scratch`): Conker's eye (0's
    // material 12) blends; Colors cs5's lit tank, `40-7-9`, draws its lights (4368) blended.
    let material = |name: &str, image: &str| -> Vec<(String, bool)> {
        let gltf: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        (gltf["materials"].as_array().unwrap().iter())
            .filter(|m| m["extras"]["image"] == image)
            .map(|m| {
                (
                    m["alphaMode"].as_str().unwrap().into(),
                    m["doubleSided"] == true,
                )
            })
            .collect()
    };
    assert_eq!(material("0", "flat 1937 mode 0"), [("BLEND".into(), false)]);
    assert_eq!(
        material("40-7-9", "flat 4368 mode 0"),
        [("BLEND".into(), false)]
    );
    assert!(material("40", "flat 4368 mode 0").is_empty());
    assert!(manifest.characters.contains(&"40-7-9".to_string()));
    std::fs::remove_dir_all(dir).unwrap();
}

/// `conker-extract pack` keeps an existing pack (edits included) unless `--force`, and never
/// writes into the source tree but in its ignored assets/ and lab/. Both refusals come before the
/// ROM is read.
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
    // The repo's ignored assets/ is allowed: it goes on, to the ROM read.
    let assets = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/no-pack-here");
    let (ok, err) = run(&["pack", "--out", assets]);
    assert!(!ok && err.contains("/nonexistent.z64"), "{err}");
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
            "characters/128",
            "characters/136",
            "characters/156",
            "characters/157",
            "characters/159",
            "characters/160",
            "characters/22",
            "characters/59",
            "characters/83",
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
    // 30, Beach's on 2b, Raptor's on 3f) and the maps' voice lines (Beach's escape).
    let lines = audio.events.maps.values().flat_map(|m| m.lines.values());
    listed.extend(lines.map(|l| format!("audio/{}", l.file)));
    for map in ["2d", "30", "2b", "1a", "3f"] {
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
    // The effects' models, shared by every map (`effects-g4.md` §4, §5, §7, §9): the blasts' ring
    // (bank 09 #0x157) one draw of 112 triangles, alpha-blended; the explosive death's chunk
    // (#0x132, 26 vertices: 12 triangles), the flak's puff (#0xE5, the icosahedron's 20) and the
    // splash's drop (#0xBE, 44), opaque, each with its CI texture. Then Race's death blast
    // (`g5-pack-scratch/race`, V: every G_VTX of d1's DL 380 found in these): the fire shell
    // (#0x158, as the ring), the board's pieces (#0x16F-#0x171: 10, 14 and 38) and the racer's
    // (#0x17D-#0x17F, #0x179: 32, 26, 16 and 58), opaque and CI8-textured.
    let fx: Value = serde_json::from_slice(files["weapons/deathmatch/effects.gltf"]).unwrap();
    let names: Vec<&str> = (fx["scenes"].as_array().unwrap().iter())
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "09-157", "09-132", "09-e5", "09-be", "09-158", "09-16f", "09-170", "09-171"
        ]
        .into_iter()
        .chain(["09-17d", "09-17e", "09-17f", "09-179"])
        .collect::<Vec<_>>()
    );
    let mut looks = vec![];
    for scene in fx["scenes"].as_array().unwrap() {
        let node = &fx["nodes"][scene["nodes"][0].as_u64().unwrap() as usize];
        let prims = fx["meshes"][node["mesh"].as_u64().unwrap() as usize]["primitives"]
            .as_array()
            .unwrap();
        let count = |p: &Value| {
            fx["accessors"][p["indices"].as_u64().unwrap() as usize]["count"]
                .as_u64()
                .unwrap()
        };
        let material = |p: &Value| &fx["materials"][p["material"].as_u64().unwrap() as usize];
        let textured = (prims.iter().map(material))
            .all(|m| m["pbrMetallicRoughness"]["baseColorTexture"].is_object());
        looks.push((
            prims.iter().map(count).sum::<u64>() / 3,
            material(&prims[0])["alphaMode"]
                .as_str()
                .unwrap()
                .to_string(),
            textured,
        ));
    }
    let look = |t, a: &str| (t, a.to_string(), true);
    let opaque = |t| look(t, "OPAQUE");
    assert_eq!(
        looks,
        [
            look(112, "BLEND"),
            opaque(12),
            opaque(20),
            opaque(44),
            look(112, "BLEND")
        ]
        .into_iter()
        .chain([10, 14, 38, 32, 26, 16].map(opaque))
        .chain([(58, "OPAQUE".to_string(), false)])
        .collect::<Vec<_>>()
    );
    assert!(
        !files
            .keys()
            .any(|p| p.starts_with("maps/") && p.ends_with("effects.gltf"))
    );
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
            // Race's kick (0x354) on the human racers' model 136 too.
            let race = *name == "136" && e.clip == 0x354;
            assert!(
                MATCH_CLIPS.contains(&e.clip) || race,
                "{name} {:#x}",
                e.clip
            );
            assert!(
                character.clips.iter().any(|c| c.logical.contains(&e.clip)),
                "{name} {:#x}",
                e.clip
            );
        }
    }
    // War: Beach's sea (V `beach-sea-scratch` swim1: the stroke's 0x8F/0x90/0x614/0x615, the wade's
    // water steps 0x2AB-0x2AE and 0x24C-0x24F, a landing in it 0x252, the swim's 0x6C-0x6E).
    let sea = |clip: u16| {
        let events = audio.events.characters["117"].iter();
        events.filter(move |e| e.clip == clip).map(|e| &e.action)
    };
    assert!(
        matches!(sea(0x22).next(), Some(ClipAction::Play(p)) if p.sounds == [0x8F, 0x90, 0x614, 0x615])
    );
    let wade: Vec<_> = sea(0x8B).collect();
    assert_eq!(wade, [&ClipAction::Footstep(4), &ClipAction::Footstep(1)]);
    assert!(sea(0xCC).any(|a| matches!(a, ClipAction::Play(p) if p.sounds == [0x6C, 0x6D, 0x6E])));
    let water = &audio.events.footsteps.surfaces[&conker_assets::audio::WATER_STEPS];
    assert_eq!(
        (water[1][0], water[3][0], water[4][0]),
        (0x24C, 0x252, 0x2AB)
    );
    // War: Raptor's (`raptor.md`): the raptor's, the baby's (the raptor's clips, so its events)
    // and the body's events, every one on a clip of its glTF.
    for name in ["83", "165", "22"] {
        let gltf: Value =
            serde_json::from_slice(files[conker_assets::character_gltf(name).as_str()]).unwrap();
        let clips: Vec<u64> = (gltf["animations"].as_array().unwrap().iter())
            .flat_map(|a| a["extras"]["logicalIds"].as_array().unwrap().clone())
            .map(|v| v.as_u64().unwrap())
            .collect();
        let events = &audio.events.characters[name];
        assert!(!events.is_empty(), "{name}");
        for e in events {
            assert!(clips.contains(&u64::from(e.clip)), "{name} {:#x}", e.clip);
        }
    }
    assert_eq!(
        audio.events.characters["165"],
        audio.events.characters["83"]
    );
    // What the oracle's sound calls showed (V, lab/audio/README.md).
    let play_at = |character: &str, clip: u16, time: f32| {
        audio.events.characters[character]
            .iter()
            .find(|e| e.clip == clip && e.time == time)
            .map(|e| e.action.clone())
    };
    // The raptor's call (C: its clip 0's op-9 event, `func_1506E998` at 300-3000 on B).
    let Some(ClipAction::Play(call)) = play_at("83", 0, 4.0) else {
        panic!("the raptor's call")
    };
    assert_eq!(
        (call.sounds, call.channel, call.near, call.far),
        (vec![948, 949, 950, 951], Channel::B, 300, 3000)
    );
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
    // The picks (M14a): the same clips' routes (bank-02 entry 0) and an equal voice table play the
    // same (177 the Tedi's, so its folder is gone: `common`); 176 has the squirrel's table on the
    // Tedi's routes (the squirrel's own routes, entry 128, add 1388); 178's and 180's tables are all
    // zero (no voice); the zombies' own sounds in their folders.
    assert_eq!(play_at("177", 0x58, 21.0), play_at("117", 0x58, 21.0));
    assert_eq!(play_at("176", 0x58, 21.0), None);
    assert_eq!(audio.sounds.sounds[&1254].file, "common/1254.wav");
    // The hit voices (the pickup voices are code's, by model: 117's bazooka line is not 177's).
    let hits = |n: &str| -> Vec<_> {
        (audio.events.cues.iter())
            .filter(|c| c.character.as_deref() == Some(n) && matches!(c.on, On::Hit { .. }))
            .map(|c| (c.on.clone(), c.play.sounds.clone()))
            .collect()
    };
    let events = &audio.events.characters;
    for (a, b) in [("177", "117"), ("152", "59"), ("178", "180")] {
        assert_eq!((&events[a], hits(a)), (&events[b], hits(b)), "{a} = {b}");
    }
    // Conker's two op-9 events for model 0 only (369 on clip 19, 373 on 0x58) are not 150's.
    assert_eq!(hits("150"), hits("0"));
    assert!(events["150"].iter().all(|e| events["0"].contains(e)));
    assert_eq!(events["0"].len(), events["150"].len() + 2);
    let voices = |name: &str| -> BTreeSet<u16> {
        (audio.events.characters[name].iter())
            .filter_map(|e| match &e.action {
                ClipAction::Play(p) if p.own == Some(24000) => Some(p.sounds.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    };
    assert!(voices("176").is_subset(&voices("128")));
    assert_eq!(hits("176"), hits("128"));
    assert!(!voices("0").is_empty() && !voices("159").is_empty());
    assert_eq!(audio.sounds.sounds[&1519].file, "characters/159/1519.wav");
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
    // Item 0x3A on 0x1A and 0x3F (`held-pack-scratch` s1a, s3f): its fuse from the pull, its own
    // bounces and 0x26's blast; no hiss in the hand and no last-60-ticks fuse.
    let lob = |on: On| {
        (audio.events.cues.iter())
            .filter(|c| c.on == on)
            .map(|c| {
                (
                    c.play.sounds.clone(),
                    c.play.volume,
                    c.play.near,
                    c.play.far,
                    c.play.channel,
                    c.play.pitch,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        lob(On::Lit { weapon: 0x3A }),
        [(vec![0x96], 14000, 400, 17000, Channel::A, [0, 0])]
    );
    assert_eq!(
        lob(On::Bounce { weapon: 0x3A }),
        [(
            vec![0x186, 0x187, 0x188],
            28000,
            300,
            1800,
            Channel::A,
            [0, 0]
        )]
    );
    assert_eq!(
        lob(On::Explosion { weapon: 0x3A }),
        lob(On::Explosion { weapon: 0x26 })
    );
    assert!(
        lob(On::Holding { weapon: 0x3A }).is_empty() && lob(On::Fuse { weapon: 0x3A }).is_empty()
    );
    assert!(
        [0x96, 0x186, 0x187, 0x188]
            .iter()
            .all(|id| audio.sounds.sounds.contains_key(id))
    );
    // The crossbows' aim laser (G5): 1232 at 2000, heard by the aimer (`match-flow-g3.md`).
    assert_eq!(
        lob(On::Laser),
        [(vec![1232], 2000, 0, 1, Channel::Free, [0, 0])]
    );
    assert_eq!(audio.sounds.sounds[&1232].file, "common/1232.wav");
    // War: Raptor's egg splat: both sounds at it, both in the pack (0x434 also Race's blow-up).
    let splat: Vec<_> = (audio.events.cues.iter())
        .filter(|c| c.on == On::RaptorSplat)
        .map(|c| {
            (
                c.play.sounds.clone(),
                c.play.volume,
                c.play.near,
                c.play.far,
            )
        })
        .collect();
    assert_eq!(
        splat,
        [
            (vec![0x1A6], 0x7D00, 500, 1000),
            (vec![0x434], 0x7D00, 500, 1000)
        ]
    );
    assert!(
        [0x1A6, 0x434]
            .iter()
            .all(|id| audio.sounds.sounds.contains_key(id))
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
    // Item 0x3A's icon: the grenade texture's bomb (V `held-pack-scratch` p1a, p3f DL 110).
    let lob = hud.grenades.lob_icon.as_ref().unwrap();
    assert_eq!(
        (lob.texture.as_str(), lob.rect),
        ("grenade", [16, 15, 11, 14])
    );
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
    // War: Raptor's bone knives and bone crossbow show the knives' and the crossbow's (C).
    assert_eq!(hud.weapon.icons[&0x3F], hud.weapon.icons[&0x14]);
    assert_eq!(hud.weapon.icons[&0x41], hud.weapon.icons[&0x18]);
    // Race's missiles and canister (V h1 DL 230's texrects), and its overview maps (V k1, b2).
    let icon = |id| {
        (
            hud.weapon.icons[&id].texture.as_str(),
            hud.weapon.icons[&id].rect,
        )
    };
    assert_eq!(
        (icon(0x33), icon(0x31)),
        (("atlas", [50, 39, 14, 12]), ("atlas2", [0, 28, 22, 14]))
    );
    assert_eq!(hud.textures["race_map_low"].size, [64, 64]);
    assert_ne!(
        files["hud/race_map_low.png"],
        files["hud/race_map_high.png"]
    );
    // Race's world effects (F3): the lamps' and the boost's flares, the exhaust's frames, the trail.
    let race_fx = [
        ("race_lamp", [64, 64]),
        ("race_boost", [64, 64]),
        ("race_exhaust_0", [32, 32]),
        ("race_exhaust_3", [32, 32]),
        ("race_trail", [128, 1]),
    ];
    for (name, size) in race_fx {
        assert_eq!(hud.textures[name].size, size, "{name}");
    }
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
    // cs3's bubbles: blob 0x4B3, sprite kind 0x2F's (G5; `effects-g4.md` §10).
    assert_eq!(size("bubble_particle"), [32, 32]);
    // The world effects' sprites (G5, `effects-g4.md`'s pack list): each the size its sprite kind's
    // record gives (`.game` data at 0x80090B60 + 12 · kind), none blank.
    let effects = [
        ("gun_flash_3d", [32, 32]),
        ("tracer", [64, 16]),
        ("puff", [16, 16]),
        ("flame_tongue", [32, 32]),
        ("smoke_grey", [32, 32]),
        ("ripple_00", [64, 64]),
        ("ripple_09", [64, 64]),
        ("flame_a0", [16, 32]),
        ("flame_b4", [16, 32]),
        ("dirt", [16, 16]),
        ("scorch", [32, 32]),
        ("fuse_spark", [32, 16]),
    ];
    for (name, want) in effects {
        assert_eq!(size(name), want, "{name}");
        assert!(drawn(files[format!("hud/{name}.png").as_str()]), "{name}");
    }
    let ripples: BTreeSet<&[u8]> = (0..10)
        .map(|k| files[format!("hud/ripple_{k:02}.png").as_str()])
        .collect();
    assert_eq!(ripples.len(), 10);
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
        "/../../lab/research/mp-original/beach-presentation-scratch/hw"
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
    // The rumble 0x4A9 (G5, `cameras-clips-g4.md` §11).
    assert_eq!(cue(On::TankRumble), [(vec![1193], 0x2EE0, 200, 800)]);
    let pickups: Vec<_> = [0x80, 1, 0x10, 2]
        .map(|bit| {
            cue(On::Pickup {
                gives: Gives::Weapon(bit),
            })[0]
                .0[0]
        })
        .into();
    assert_eq!(pickups, [1296, 1187, 385, 752]);
    for id in [1191, 1192, 1193, 742, 735, 750, 11, 12, 752] {
        assert_eq!(
            audio.sounds.sounds[&id].file,
            format!("characters/119/{id:04}.wav")
        );
    }
}

/// Race's sound on 0x33 (`lab/research/mp-original/race.md`; V `lab/audio/captures/race_*_calls.txt`):
/// no background, emitter nor zone; the kick's swing on clip 0x354 of both racers' models; the
/// boards' hums, the countdown, the power-ups, the missiles, the walls, the kicks' hits and the
/// blow-ups as cues for anyone.
#[test]
fn race_audio_reads_back() {
    use conker_assets::audio::{Audio, Channel, ClipAction, On};
    use conker_extract::audio::pack_audio;
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let maps = [("33".to_string(), None, None, None)];
    let surfaces = std::collections::BTreeSet::new();
    let characters = [conker_extract::CAVEMAN, conker_extract::RACER];
    let (files, _) = pack_audio(&rom, &surfaces, &maps, &characters, &[]).unwrap();
    let files: BTreeMap<&str, &[u8]> = (files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let audio = Audio::load(|p: &str| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    let map = &audio.events.maps["33"];
    assert!(map.background.is_empty() && map.emitters.is_empty() && map.zones.is_empty());
    for racer in ["136", "144"] {
        let kick: Vec<_> = (audio.events.characters[racer].iter())
            .filter(|c| c.clip == 0x354)
            .map(|c| match &c.action {
                ClipAction::Play(p) => (c.time, p.sounds.clone(), p.channel),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(kick, [(11.0, vec![90], Channel::A)], "{racer}");
    }
    let cue = |on: On| {
        (audio.events.cues.iter())
            .filter(|c| c.on == on)
            .map(|c| {
                assert!(c.character.is_none());
                let p = &c.play;
                (
                    p.sounds.clone(),
                    p.volume,
                    p.own,
                    p.near,
                    p.far,
                    p.channel,
                    p.if_idle,
                )
            })
            .collect::<Vec<_>>()
    };
    let free = Channel::Free;
    for (hum, id) in [(0, 1459), (1, 1460), (2, 1468)] {
        let expected = (vec![id], 1000, None, 200, 1000, free, false);
        assert_eq!(cue(On::RaceHum { hum }), [expected]);
    }
    let beep = |id| [(vec![id], 24000, Some(24000), 0, 0, free, false)];
    assert_eq!(cue(On::RaceCountdown { stage: 1 }), beep(1671));
    assert_eq!(cue(On::RaceCountdown { stage: 2 }), beep(1671));
    assert_eq!(cue(On::RaceCountdown { stage: 3 }), beep(1672));
    for item in [0x31, 0x33] {
        let gives = Gives::Weapon(item);
        let expected = (vec![1577], 32767, Some(32767), 100, 1000, free, false);
        assert_eq!(cue(On::Pickup { gives }), [expected]);
    }
    assert_eq!(
        cue(On::Shot { weapon: 0x31 }),
        [(vec![1576], 32767, Some(32767), 500, 1000, Channel::A, false)]
    );
    assert_eq!(
        cue(On::Shot { weapon: 0x33 }),
        [(vec![567], 30000, None, 500, 2000, Channel::A, false)]
    );
    assert_eq!(
        cue(On::RaceMissile),
        [(
            vec![750, 751, 752],
            30000,
            None,
            500,
            1000,
            Channel::A,
            true
        )]
    );
    let hit = On::Hit {
        weapon: 0x33,
        lethal: None,
    };
    assert_eq!(cue(hit), [(vec![263], 28000, None, 300, 1800, free, false)]);
    assert_eq!(
        cue(On::RaceKick),
        [(vec![1015], 28000, Some(24000), 300, 1800, Channel::A, false)]
    );
    let scrape: Vec<u16> = (778..786).collect();
    assert_eq!(
        cue(On::RaceWall),
        [(scrape, 28000, Some(24000), 300, 1800, Channel::A, true)]
    );
    let death = |kind_e| cue(On::RaceDeath { kind_e });
    assert_eq!(death(Some(false))[0].0, [1224]);
    assert_eq!(death(Some(true))[0].0, [1076]);
    let both: Vec<_> = death(None).into_iter().map(|c| c.0).collect();
    assert_eq!(both, [vec![694, 695], vec![689, 690, 691, 692]]);
}
