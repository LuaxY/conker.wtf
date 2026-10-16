//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.
//! Expected numbers are DevOldSchool/conkers-bfd-decomp's (MIT, e2400b27dda6) as run on this ROM
//! (`lab/reference/dos-out/summary.md`, `docs/evidence/assets/models/us_asset_inventory.md`).
//! `dos_*` tests also need env `CONKER_DOS_OUT` = `lab/reference/dos-out`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use extract_character::{Character, DEATHMATCH_PLAYER, held_models, load_clips};
use extract_core::Rom;

fn rom() -> Option<&'static Rom> {
    static ROM: OnceLock<Option<Rom>> = OnceLock::new();
    let rom = ROM.get_or_init(|| {
        let path = std::env::var_os("CONKER_ROM")?;
        Some(Rom::new(std::fs::read(path).expect("read CONKER_ROM")).expect("the US ROM"))
    });
    if rom.is_none() {
        eprintln!("CONKER_ROM unset: skipping");
    }
    rom.as_ref()
}

fn dos_out() -> Option<PathBuf> {
    let dir = std::env::var_os("CONKER_DOS_OUT").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("CONKER_DOS_OUT unset: skipping");
    }
    dir
}

fn player() -> Option<&'static Character> {
    static PLAYER: OnceLock<Option<Character>> = OnceLock::new();
    PLAYER
        .get_or_init(|| Some(Character::load(rom()?, DEATHMATCH_PLAYER).unwrap()))
        .as_ref()
}

#[test]
fn deathmatch_player_rig_and_clips() {
    let Some(c) = player() else { return };
    assert_eq!((c.entry, c.anim_entry), (117, 0));
    assert_eq!(c.model.joints.len(), 28);
    assert_eq!(c.model.anim_slots(), 28);
    // Conker's bank-02 entry 0: 766 pairs, two with empty streams.
    let frames: usize = c.clips.iter().map(|k| k.frames.len()).sum();
    let ticks: usize = c.playable_clips().map(|k| k.duration as usize).sum();
    assert_eq!((c.clips.len(), c.playable_clips().count()), (766, 764));
    assert_eq!((ticks, frames), (47_584, 15_228));
    // Logical id 0 is pair 25 (DOS's trace); 122, the oracle player's idle, is pair 40.
    let pair_of = |id| {
        c.clips
            .iter()
            .find(|k| k.logical.contains(&id))
            .unwrap()
            .pair
    };
    assert_eq!((pair_of(0), pair_of(122), pair_of(2)), (25, 40, 1));
}

/// The player clips that move the character by their root z (descriptor flag 0x40): the creep (0), weapon
/// 0x16's attack (0x77) and the shot death 0x17E among them (the movement agent's oracle traces).
#[test]
fn root_motion_clips() {
    let Some(c) = player() else { return };
    let moving: Vec<&extract_character::Clip> = c
        .playable_clips()
        .filter(|k| k.root_motion.is_some())
        .collect();
    let ids: BTreeSet<u16> = moving
        .iter()
        .flat_map(|k| k.logical.iter().copied())
        .collect();
    eprintln!("{} root-motion clips, ids {:x?}", moving.len(), ids);
    for id in [0, 0x77, 0x17E] {
        assert!(ids.contains(&id), "{id:#x}");
    }
    let creep = moving.iter().find(|k| k.logical.contains(&0)).unwrap();
    let rm = creep.root_motion.as_ref().unwrap();
    assert_eq!(rm.z.len(), creep.frames.len());
    // Clip 0's root travels 0 -> 143 model units (32 raw units each) over its 45 ticks.
    assert_eq!((rm.z[0], creep.duration), (0, 45));
    assert!((140..150).contains(&(rm.end as i32 / 32)), "{}", rm.end);
}

/// DevOldSchool's bank-wide totals over its 123 "indexed" bank-02 entries (tables of at least
/// four segments, its bank-04 bundle rule). The other 22 non-empty entries are one-pair tables it
/// keeps as raw "direct companion payloads"; they decode here too and are counted apart.
#[test]
fn bank02_totals() {
    let Some(rom) = rom() else { return };
    let game_data = rom.game_data().unwrap();
    let mut dos = [0; 4]; // entries, pairs, non-empty clips, frames
    let mut single = [0; 4];
    for e in 0..rom.bank(2).unwrap().entries.len() {
        if rom.asset(&[2, e]).unwrap().is_empty() {
            continue;
        }
        let c = load_clips(rom, &game_data, e).unwrap_or_else(|err| panic!("entry {e}: {err}"));
        let t = if c.len() >= 2 { &mut dos } else { &mut single };
        t[0] += 1;
        t[1] += c.len();
        t[2] += c.iter().filter(|k| !k.frames.is_empty()).count();
        t[3] += c.iter().map(|k| k.frames.len()).sum::<usize>();
    }
    eprintln!("indexed {dos:?}, one-pair {single:?}");
    assert_eq!(dos, [123, 2_660, 2_621, 57_732]);
    assert_eq!(single[..2], [22, 22]);
}

/// A glTF document and its buffer, read back for comparison.
struct Doc {
    json: serde_json::Value,
    bin: Vec<u8>,
}

impl Doc {
    fn read(path: &std::path::Path) -> Self {
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let bin = std::fs::read(path.with_file_name(json["buffers"][0]["uri"].as_str().unwrap()))
            .unwrap();
        Self { json, bin }
    }

    fn ours(c: &Character, animations: bool) -> Self {
        let g = c
            .gltf(&rom().unwrap().textures().unwrap(), animations)
            .unwrap();
        Self {
            json: g.json,
            bin: g.bin,
        }
    }

    /// Accessor values as f64 (floats and unsigned integers; tightly packed views).
    fn values(&self, accessor: &serde_json::Value) -> Vec<f64> {
        let a = &self.json["accessors"][accessor.as_u64().unwrap() as usize];
        let v = &self.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let start = v["byteOffset"].as_u64().unwrap_or(0) as usize
            + a["byteOffset"].as_u64().unwrap_or(0) as usize;
        let width = match a["type"].as_str().unwrap() {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => 16,
        };
        let n = a["count"].as_u64().unwrap() as usize * width;
        let b = &self.bin[start..];
        (0..n)
            .map(|i| match a["componentType"].as_u64().unwrap() {
                5126 => f32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap()) as f64,
                5125 => u32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap()) as f64,
                5123 => u16::from_le_bytes(b[2 * i..2 * i + 2].try_into().unwrap()) as f64,
                5121 => b[i] as f64,
                t => panic!("component type {t}"),
            })
            .collect()
    }

    /// Every triangle as corners (model-space position bits, joint, normal in 1e-4 units),
    /// rotated so the smallest corner leads (winding kept), sorted; and every corner's UVs in
    /// 1e-4 units, by position and joint.
    fn triangles(&self) -> (Vec<[Corner; 3]>, Uvs) {
        let (mut tris, mut uvs) = (Vec::new(), BTreeMap::<Point, BTreeSet<_>>::new());
        let round = |v: f64| (v * 1e4).round() as i64;
        for p in self.json["meshes"][0]["primitives"].as_array().unwrap() {
            let attr =
                |k: &str| (!p["attributes"][k].is_null()).then(|| self.values(&p["attributes"][k]));
            let (pos, joints, normals) = (
                attr("POSITION").unwrap(),
                attr("JOINTS_0").unwrap(),
                attr("NORMAL").unwrap(),
            );
            let uv = attr("TEXCOORD_0");
            let corner = |i: usize| {
                let point = (
                    [0, 1, 2].map(|k| (pos[3 * i + k] as f32).to_bits()),
                    joints[4 * i] as u32,
                );
                (point, [0, 1, 2].map(|k| round(normals[3 * i + k])))
            };
            for t in self.values(&p["indices"]).chunks(3) {
                let mut c = t.iter().map(|&i| corner(i as usize)).collect::<Vec<_>>();
                if let Some(uv) = &uv {
                    for (&i, c) in t.iter().zip(&c) {
                        uvs.entry(c.0)
                            .or_default()
                            .insert([round(uv[2 * i as usize]), round(uv[2 * i as usize + 1])]);
                    }
                }
                let first = (0..3).min_by_key(|&k| c[k]).unwrap();
                c.rotate_left(first);
                tris.push([c[0], c[1], c[2]]);
            }
        }
        tris.sort();
        (tris, uvs)
    }
}

type Point = ([u32; 3], u32);
type Corner = (Point, [i64; 3]);
type Uvs = BTreeMap<Point, BTreeSet<[i64; 2]>>;

/// Same triangles (positions bit-exact, joints, normals), UVs, skeleton and inverse binds as
/// DOS's bind glTF. Its vertex count differs (it merges reloads of one source vertex); like ours,
/// its PNGs are upright (top row first) and its v flipped to match.
fn assert_bind_matches(c: &Character, dos: &Doc) {
    let ours = Doc::ours(c, false);
    let ((a, our_uvs), (b, dos_uvs)) = (ours.triangles(), dos.triangles());
    assert_eq!(a.len(), b.len(), "triangle count");
    assert!(a == b, "triangles differ");
    let (mut same, mut only_ours) = (0, 0);
    for (point, set) in &our_uvs {
        match dos_uvs.get(point) {
            Some(theirs) => {
                // Within 2e-4 (rounding of 1 - v), for every UV of the smaller set.
                let near = |a: &BTreeSet<[i64; 2]>, b: &BTreeSet<[i64; 2]>| {
                    a.iter().all(|u| {
                        b.iter()
                            .any(|w| (u[0] - w[0]).abs() <= 2 && (u[1] - w[1]).abs() <= 2)
                    })
                };
                assert!(
                    near(set, theirs) || near(theirs, set),
                    "UVs at {point:?}: {set:?} vs {theirs:?}"
                );
                same += 1;
            }
            None => only_ours += 1,
        }
    }
    eprintln!(
        "entry {}: {} triangles, UVs agree at {same} corner points ({only_ours} only ours)",
        c.entry,
        a.len()
    );
    assert!(same > 0);
    let ibm = |d: &Doc| d.values(&d.json["skins"][0]["inverseBindMatrices"]);
    assert_eq!(ibm(&ours), ibm(dos));
    for (i, j) in c.model.joints.iter().enumerate() {
        let n = &dos.json["nodes"][i];
        assert_eq!(n["extras"]["matrixIndex"], i);
        assert_eq!(n["extras"]["animationIndex"], j.anim);
        assert_eq!(
            n["extras"]["parentMatrixIndex"].as_i64().unwrap(),
            j.parent.map_or(-1, i64::from)
        );
        let t: Vec<f32> = n["translation"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(t, j.translation, "joint {i}");
    }
}

#[test]
fn dos_bind_meshes() {
    let (Some(rom), Some(dir)) = (rom(), dos_out()) else {
        return;
    };
    for entry in [117, 0] {
        let c = Character::load(rom, entry).unwrap();
        assert_bind_matches(
            &c,
            &Doc::read(&dir.join(format!("characters/geometry/{entry:04}-00-bind.gltf"))),
        );
    }
}
/// Every clip of Conker (entry 0, the clips the Deathmatch model plays) against DOS's glTF:
/// key times, and every rotation, translation and scale channel value.
#[test]
fn dos_clips() {
    let (Some(rom), Some(dir)) = (rom(), dos_out()) else {
        return;
    };
    let c = Character::load(rom, 0).unwrap();
    let ours = Doc::ours(&c, true);
    let dos = Doc::read(&dir.join("characters/geometry/0000-00.gltf"));
    let anims = |d: &Doc| d.json["animations"].as_array().unwrap().clone();
    let (our_anims, dos_anims) = (anims(&ours), anims(&dos));
    assert_eq!(our_anims.len(), 764);
    assert_eq!(dos_anims.len(), 764);
    let (mut channels, mut values, mut worst) = (0, 0, 0f64);
    for o in &our_anims {
        let d = dos_anims
            .iter()
            .find(|d| d["extras"]["sourcePair"] == o["extras"]["pair"])
            .unwrap();
        assert_eq!(
            o["extras"]["logicalIds"],
            d["extras"]["logicalAnimationIndices"]
        );
        assert_eq!(
            o["extras"]["durationTicks"],
            d["extras"]["sourceDurationTicks"]
        );
        let sampled = |doc: &Doc, a: &serde_json::Value| {
            let mut m = std::collections::BTreeMap::new();
            for ch in a["channels"].as_array().unwrap() {
                let s = &a["samplers"][ch["sampler"].as_u64().unwrap() as usize];
                let key = (
                    ch["target"]["node"].as_u64().unwrap(),
                    ch["target"]["path"].as_str().unwrap().to_string(),
                );
                m.insert(key, (doc.values(&s["input"]), doc.values(&s["output"])));
            }
            m
        };
        let (so, sd) = (sampled(&ours, o), sampled(&dos, d));
        assert_eq!(
            so.keys().collect::<Vec<_>>(),
            sd.keys().collect::<Vec<_>>(),
            "{}",
            o["name"]
        );
        for (key, (ti, out)) in &so {
            let (dti, dout) = &sd[key];
            assert_eq!(ti, dti, "{} {key:?} times", o["name"]);
            assert_eq!(out.len(), dout.len());
            for (x, y) in out.iter().zip(dout) {
                worst = worst.max((x - y).abs());
            }
            channels += 1;
            values += out.len();
        }
    }
    eprintln!("{channels} channels, {values} values, max |ours - DOS| = {worst:e}");
    assert!(worst < 1e-5);
}

/// RGBA8 rows of a PNG (any colour type the `png` crate expands to 8-bit RGB or RGBA).
fn png_rows(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let w = info.width as usize;
    let px = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => panic!("{other:?}"),
    };
    buf[..info.height as usize * w * px]
        .chunks(w * px)
        .map(|row| {
            row.chunks(px)
                .flat_map(|p| [p[0], p[1], p[2], *p.get(3).unwrap_or(&255)])
                .collect()
        })
        .collect()
}

/// The Deathmatch model's textures equal DOS's PNGs (both upright: top row first, i.e. flipped
/// from memory order) within the 5-to-8-bit rounding difference.
#[test]
fn dos_textures() {
    let (Some(c), Some(dir)) = (player(), dos_out()) else {
        return;
    };
    let ours = Doc::ours(c, false);
    let mut compared = 0;
    for m in ours.json["materials"].as_array().unwrap() {
        let image = m["extras"]["image"].as_str().unwrap();
        let id: usize = image.split(' ').nth(1).unwrap().parse().unwrap();
        let tex = &m["pbrMetallicRoughness"]["baseColorTexture"]["index"];
        let view = &ours.json["bufferViews"][ours.json["images"][ours.json["textures"]
            [tex.as_u64().unwrap() as usize]["source"]
            .as_u64()
            .unwrap() as usize]["bufferView"]
            .as_u64()
            .unwrap() as usize];
        let start = view["byteOffset"].as_u64().unwrap() as usize;
        let rows =
            png_rows(&ours.bin[start..start + view["byteLength"].as_u64().unwrap() as usize]);
        let file = std::fs::read_dir(dir.join("characters/textures"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .contains(&format!("-{id:04}-"))
            })
            .unwrap();
        let theirs = png_rows(&std::fs::read(&file).unwrap());
        assert_eq!(
            (rows.len(), rows[0].len()),
            (theirs.len(), theirs[0].len()),
            "{file:?}"
        );
        let off = rows
            .iter()
            .flatten()
            .zip(theirs.iter().flatten())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(off <= 1, "{file:?}: channels differ by {off}");
        compared += 1;
    }
    assert_eq!(compared, 4);
}

/// Deathmatch's weapons and the grenade in the hand: the bank-09 models the draw clips (and the
/// grenade's pull) create, the joint each is drawn with, and their events (V on the oracle for
/// 26/27, 48 and 75: `lab/oracle/trajectories/weapon_hold*_lldb.txt`, `raw/weapon_dumps/`).
#[test]
fn held_weapons() {
    let Some(rom) = rom() else { return };
    let draws = [
        0x7B, 0x1EE, 0x163, 0x152, 0x168, 0x120, 0x15D, 0x102, 0x76, 0x176,
    ];
    let held = held_models(rom, DEATHMATCH_PLAYER, &draws).unwrap();
    let models: Vec<(usize, u8)> = held.iter().map(|h| (h.entry, h.joint)).collect();
    let expected = [(65, 9), (26, 4), (27, 9), (32, 9), (33, 9), (48, 9)];
    assert_eq!(models[..6], expected);
    assert_eq!(models[6..], [(55, 9), (58, 9), (77, 9), (75, 4), (101, 9)]);
    let twin = &held[1];
    assert_eq!(
        (twin.create.as_slice(), twin.remove.as_slice()),
        (&[(0x7B, 15.0)][..], &[(0x7D, 14.0)][..])
    );
    // Weapon 0x14's knife leaves the hand 8 ticks into a throw.
    assert!(held[4].remove.contains(&(0xF7, 8.0)));
}

/// Total War's gas mask: the put-on (0x151) and take-off (0x14E) clips' bank-09 models, the same
/// for the Tedi and the squirrel: #67 in the hand (joint 9), #66 on the head (joint 12). Each list
/// stops at its first record not yet due (`held::due`): 0x14E's t59 holds back its later t24 and
/// t58 records, so the hand's create there never fires (V e2: event index stuck at 2).
#[test]
fn gas_mask_models() {
    let Some(rom) = rom() else { return };
    for character in [117, 128] {
        let mut held = held_models(rom, character, &[0x151, 0x14E]).unwrap();
        held.sort_by_key(|h| h.entry);
        let models: Vec<_> = (held.iter())
            .map(|h| (h.entry, h.joint, h.create.clone(), h.remove.clone()))
            .collect();
        assert_eq!(
            models,
            [
                (66, 12, vec![(0x151, 59.0)], vec![(0x14E, 24.0)]),
                (
                    67,
                    9,
                    vec![(0x14E, 59.0), (0x151, 26.0)],
                    vec![(0x14E, 59.0), (0x151, 59.0)]
                ),
            ],
            "{character}"
        );
    }
}
