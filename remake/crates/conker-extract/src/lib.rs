//! ROM to assets: [`Slice::from_rom`] gives the browser client everything the vertical slice
//! needs, in memory; the exports behind the conker-extract CLI write inspectable files.
//!
//! Every export returns the files to write (paths relative to an output directory) instead of
//! writing them, so the same code can run in the browser; only the CLI touches the file system.
//! Output layout: `scene-<id>/`, `level-<id>/`, `character-<entry>/`, `textures/` (ids in hex
//! for scenes, decimal for bank-01 entries). The formats are the decoder crates' (DevOldSchool
//! attributions live there).

pub mod audio;
pub mod hud;
pub mod outro;
pub mod traps;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Context, Result};
use conker_assets::{self as assets, ACOS_LEN, FORMAT, MANIFEST, Manifest};
pub use conker_assets::{ClipInfo, PACK_CHARACTERS, PACK_MAP, PACK_WEAPONS, RootClip};
use conker_rules::weapon::{self, Gives};
pub use extract_character::Held;
use extract_character::{
    Character, DEATHMATCH_PLAYER, FPS, ROUTE_BANK, SQUIRREL_SOLDIER, action_records, bind_pivots,
    held_models, matrix_slot, parse_attachment, parse_effect, parse_model, parse_routes,
};
use extract_core::{Rom, TEXTURE_COUNT, Textures, US_SHA1};
use extract_gbi::Material;
use extract_level::{
    Gltf, IDENTITY, Instance, Level, Mesh, Model, bind_material, bundle_segments, scene_level,
    to_gltf,
};
pub use extract_scene::{Layer, SpawnPoint, Triangle};
use extract_scene::{
    deathmatch_solid, deathmatch_water, floor_below, scene_collision, scene_path, scene_placements,
    scene_spawns,
};
use serde_json::{Value, json};

/// The vertical slice's map: scene 0x2D, the oracle's Deathmatch scene.
pub const SLICE_SCENE: usize = 0x2D;
/// The vertical slice's character: the bank-01 model every Deathmatch participant uses.
pub const SLICE_CHARACTER: usize = DEATHMATCH_PLAYER;
/// The clips that put Deathmatch's held models in the hand: the draws of weapons 0x09, 0x39, 0x23,
/// 0x24, 0x15, 0x21, 0x22, 0x14 and 0x16, and the grenade's pull (the Tediz' 0x176: bank-09 #75;
/// everyone else's 0x175: #76, `conker_rules::characters::clip`), then the crossbow's and the
/// shotgun's and the club's (0x40) draws (conker-anim's ids), and War: Total War's gas mask put on
/// and taken off (0x151, 0x14E: bank-09 #67 in the hand, #66 on the head), then the draws of
/// map 0x1A's bat (0x19: 0x10E) and tommy gun (0x38: 0x1DD) (C `func_15065A5C`), then War: Raptor's
/// bone crossbow (0x41: 0x2C8, action 46, bank-01 #0x2F as a child object) and bone knives (0x3F:
/// 0x2CA, action 47, bank-09 #0x7B) (V `../lab/research/mp-original/raptor.md` r9, r10), then Race's
/// kick (0x354: action 0x52, bank-09 #0x34 on the hand from tick 5 to 30; V k1 DL 250). The held
/// list sorts by the game's action number, not by this order (the club's model is scene 4).
pub const HELD_CLIPS: [u16; 21] = [
    0x7B, 0x1EE, 0x163, 0x152, 0x168, 0x120, 0x15D, 0x102, 0x76, 0x176, 0x175, 0xFB, 0x1F7, 0x2C2,
    0x151, 0x14E, 0x10E, 0x1DD, 0x2C8, 0x2CA, 0x354,
];
/// The caveman (class 0xB): scene 0x3F's Deathmatch bots (V oracle, `lab/oracle/caveman_README.md`).
pub const CAVEMAN: usize = 136;
/// The mounted gun (class 0x8A): two on 0x30, one on 0x34 and 0x2B (`../lab/research/mp-original/mounted-gun.md`).
pub const MOUNTED_GUN: usize = 138;
/// Scene 0x2B, the Beach (War: Beach, scenario 8).
pub const BEACH: usize = 0x2B;
/// War: Beach's detonation ring (model 0x54 → bank 09 #0x157, `beach-environment.md` §3.1).
pub const RING: usize = 0x157;
/// The Beach's villagers (its team 0): class 0x14 is model 156 (male), 0x1D and 0x1E 157 (female)
/// (V oracle, `../lab/research/mp-original/modes-and-maps.md`); the Tedi's rig and clips.
pub const VILLAGERS: [usize; 2] = [156, 157];
/// Scene 0x1A, the Vault: Heist's map (scenario 0) and Deathmatch scenario 6
/// (`../lab/research/mp-original/heist.md`).
pub const VAULT: usize = 0x1A;
/// The Wise Guy (class 1, obj+4 = 0x3B): every player on 0x1A (V t0, dm6); the Tedi's rig and clips.
pub const WISE_GUY: usize = 59;
/// Heist's cash, the walking money bag (scene uid 0x11, drawn at 0.4): its own 5-joint rig and
/// 12 clips (bank-02 #87).
pub const CASH: usize = 87;
/// Don Weaso (scene 0x1A's outro cs2, `../lab/research/mp-original/heist-presentation.md`).
pub const DON_WEASO: usize = 134;
/// Map 0x1A's AI script, `[0x14, 0xB7]` (G+0x16BC; heist.md §2.4): conker-bots' `script` does not
/// map 0x1A yet.
const VAULT_AI: usize = 0xB7;
/// Scene 0x24: Tank's map (scenario 1, `../lab/research/mp-original/tank.md`).
pub const TANK_MAP: usize = 0x24;
/// The tank (class 8): every player on 0x24 (V b1); its own 14-joint rig and 8 clips.
pub const TANK: usize = 119;
/// Map 0x24's AI script, `[0x14, 0xB8]` (tank.md §4): conker-bots' `script` does not map 0x24 yet.
const TANK_AI: usize = 0xB8;
/// Scene 0x3F, the Temple: Deathmatch scenario 7 and War: Raptor's map (scenario 2,
/// `../lab/research/mp-original/raptor.md`).
pub const TEMPLE: usize = 0x3F;
/// The raptor (class 9): every raptor player (V obj+4 = 0x53); its own 39-joint rig and 37 clips.
pub const RAPTOR: usize = 83;
/// Raptor's baby (scene uid 0x11), the eggs (uids 0x14-0x16) and the body a standing bite leaves
/// in the jaws (uid 0x13): the scene's character records (`[0x0E, 0x3F]`, C script 0xBE; V r2, r3).
pub const BABY: usize = 165;
pub const EGG: usize = 171;
pub const BODY: usize = 22;
const BABY_UID: u8 = 0x11;
const BODY_UID: u8 = 0x13;
const EGG_UIDS: [u8; 3] = [0x14, 0x15, 0x16];
/// Scene 0x33: Race's map (scenarios 11 and 12, `../lab/research/mp-original/race.md`).
pub const RACE_MAP: usize = 0x33;
/// Race's bots (classes 0xC-0xF, obj+4 = 0x90; V cls), on the Tedi's rig and clips; a human racer
/// (class 0xB) is model 136 (V obj+4 = 0x88), the [`CAVEMAN`]'s.
pub const RACER: usize = 144;
/// Map 0x33's AI script, `[0x14, 0xBC]` (race.md §4.3): conker-bots' `script` does not map 0x33 yet.
const RACE_AI: usize = 0xBC;
/// The pickable and cheat characters no mode's map brings (`../lab/research/mp-original/
/// characters.md` §3): Conker (class 0, model 0), Neo Conker (150), the weasel guard (152), the
/// zombies (159, 160), the Sergeant and the Tediz Leader (176, 177) and Gregg (178, 180). All on the
/// Tedi's rig, packed mesh-only ([`mesh_only`]): their `character.json`'s `clips_from` is the Tedi's.
pub const PICKS: [usize; 9] = [0, 150, 152, 159, 160, 176, 177, 178, 180];
/// The model a Conker player draws: rep 1 of its reps 0-4 (obj+4 = 1; every G_VTX of a WELLYTOP pick
/// loads bank 01 #1's vertex bytes, all 701; V `characters-scratch` k15 DLs 150 and 200).
pub const CONKER_DRAWN: usize = 1;
/// The picks' display lists the game leaves undrawn (`characters-scratch/drawn.py`, V k15, c16, c18,
/// c19 DL 200): 150's second list table (lists 5-9, the same vertex loads as 0-4, never run) and
/// the zombies' lists 4, 8, 9 and 10 (obj+0x94 = 0x710 at the spawn; its source not found).
/// Raptor's draw list 0 only (V `raptor-client-scratch` h1 DLs 100-102, h2 DLs 200-246): the
/// raptor's 1-3 (a box and straps on its head, then the second table), the baby's 1 (the second
/// table) and the body's 1.
const HIDDEN: [(usize, u16); 6] = [
    (150, 0x3E0),
    (159, 0x710),
    (160, 0x710),
    (RAPTOR, 0xE),
    (BABY, 0x2),
    (BODY, 0x2),
];
/// The other player models (characters.md §3's, less the unreachable bats and the [`PICKS`]): their
/// `character.json` has `hitbox` and `body` too. The tank's record is not its hit test (conker-rules'
/// `tank::BODY`).
const PLAYERS: [usize; 8] = [
    SQUIRREL_SOLDIER,
    SLICE_CHARACTER,
    CAVEMAN,
    VILLAGERS[0],
    VILLAGERS[1],
    WISE_GUY,
    RAPTOR,
    RACER,
];

/// Bank-01 model `entry` with every clip it plays: the baby's are the raptor's (its character byte
/// +6 is 83, V r3 obj+6; its own bank-02 entry is empty), the others' their animation entry's.
pub fn load_character(rom: &Rom, entry: usize) -> Result<Character> {
    match entry {
        BABY => on_tedi_rig(rom, BABY, &Character::load(rom, RAPTOR)?),
        _ => Ok(Character::load(rom, entry)?),
    }
}

/// War: Beach's villager class (0x11-0x20, re-rolled at every spawn, `func_151E2834`) → its model
/// and the display lists it hides (obj+0x94, bit k = list k; `func_15083384`, C; V b1, e1, use:
/// `../lab/research/mp-original/beach-presentation.md` §5). 0x11-0x1C, model 156: 0x3E less list
/// 2, 3 or 4 by (class − 0x11) >> 2, less lists 1, 5 or both by its low 2 bits (12 looks);
/// 0x1D-0x20, model 157: 0x0A or 0x14 (2 looks).
pub fn villager_look(class: u8) -> Option<(usize, u8)> {
    match class {
        0x11..=0x1C => {
            let v = class - 0x11;
            let mask = 0x3E & !(4 << (v >> 2)) & ![0, 2, 0x20, 0x22][usize::from(v & 3)];
            Some((156, mask))
        }
        0x1D..=0x20 => Some((157, if class < 0x1F { 0x0A } else { 0x14 })),
        _ => None,
    }
}
/// How far below a floor plane a spawn point may sit and still stand on it (spawns on slopes sit
/// up to ~1.3 units under the plane; the extract-scene example uses the same value).
const FLOOR_TOLERANCE: f32 = 2.0;
/// `.game` data's load address, and where the game's acos and sink tables sit in it.
pub(crate) const GAME_DATA: usize = 0x8008_2B20;
const ACOS_AT: usize = 0x8008_5EC0 - GAME_DATA;
const SINK_AT: usize = 0x8009_9140 - GAME_DATA;
/// The pack's `README.md`.
const PACK_README: &str = include_str!("pack-readme.md");

/// (path relative to the output directory, contents)
pub type Files = Vec<(String, Vec<u8>)>;

/// Files to write and a console summary.
#[derive(Debug, Default)]
pub struct Export {
    pub files: Files,
    pub summary: Vec<String>,
}

impl Export {
    fn add(&mut self, path: String, bytes: impl Into<Vec<u8>>) {
        self.files.push((path, bytes.into()));
    }

    fn append(&mut self, other: Export) {
        self.files.extend(other.files);
        self.summary.extend(other.summary);
    }
}

fn pretty(v: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(v).expect("JSON values serialise")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn word(w: u32) -> String {
    format!("{w:#010x}")
}

fn layer(l: Layer) -> String {
    match l {
        Layer::Terrain => "terrain".into(),
        Layer::Secondary => "secondary".into(),
        Layer::Placement(i) => format!("placement:{i}"),
    }
}

/// The scene's placements, spawn points and collision (`scene-<id>/`): `placements.json`,
/// `spawns.json` (with the floor under each), `collision.obj` (world space, faces in build order,
/// one object per layer run) and `collision.json` (layer runs, per-face surface words, histogram).
pub fn scene(rom: &Rom, scene: usize) -> Result<Export> {
    let dir = format!("scene-{scene:02x}");
    let placements = scene_placements(rom, scene)?;
    let spawns = scene_spawns(rom, scene)?;
    let collision = scene_collision(rom, scene, &placements)?;
    let tris = &collision.triangles;
    let mut out = Export::default();

    let placement_json: Vec<Value> = placements
        .iter()
        .enumerate()
        .map(|(i, p)| {
            // dispatch 0 = [0x0C, scene, 2] record placing a bank-03 model; 1/2 = [0x0B, scene]
            // record placing a bundle segment.
            let model_path = match p.dispatch {
                0 => json!([0x03, p.model]),
                _ => json!([0x04, scene, p.model]),
            };
            json!({
                "index": i, "position": p.position, "rotation_deg": p.rotation, "scale": p.scale,
                "dispatch": p.dispatch, "model_path": model_path,
                "flags": format!("{:#04x}", p.flags), "collision": p.has_collision(),
                "raw": hex(&p.raw),
            })
        })
        .collect();
    out.add(
        format!("{dir}/placements.json"),
        pretty(&json!(placement_json)),
    );

    let spawn_json: Vec<Value> = spawns
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let floor = floor_below(tris, s.position.map(f32::from), FLOOR_TOLERANCE).map(
                |(t, y)| json!({ "triangle": t, "height": y, "surface": tris[t].surface.map(word) }),
            );
            json!({
                "index": i, "position": s.position, "yaw": s.yaw,
                "facing_u16": u16::from(s.yaw) << 8, "camera": s.camera,
                "byte_06": s.byte_06, "byte_14": s.byte_14, "byte_15": s.byte_15, "floor": floor,
            })
        })
        .collect();
    out.add(format!("{dir}/spawns.json"), pretty(&json!(spawn_json)));

    let mut obj = format!("# scene {scene:#04x} collision, world space, faces in build order\n");
    let mut layers: Vec<Value> = Vec::new();
    let mut histogram = BTreeMap::<String, usize>::new();
    for (i, t) in tris.iter().enumerate() {
        if i == 0 || tris[i - 1].layer != t.layer {
            writeln!(obj, "o {}", layer(t.layer).replace(':', "_"))?;
            layers.push(json!({ "layer": layer(t.layer), "first_face": i, "faces": 0 }));
        }
        let run = layers.last_mut().expect("pushed above");
        run["faces"] = json!(run["faces"].as_u64().unwrap_or(0) + 1);
        for p in t.points {
            writeln!(obj, "v {} {} {}", p[0], p[1], p[2])?;
        }
        writeln!(obj, "f {} {} {}", 3 * i + 1, 3 * i + 2, 3 * i + 3)?;
        if let Some(s) = t.surface {
            *histogram.entry(word(s)).or_default() += 1;
        }
    }
    out.add(format!("{dir}/collision.obj"), obj);
    let surfaces: Vec<Option<String>> = tris.iter().map(|t| t.surface.map(word)).collect();
    out.add(
        format!("{dir}/collision.json"),
        pretty(&json!({
            "scene": scene, "faces": tris.len(), "layers": layers,
            "unresolved_placements": collision.unresolved,
            "surface_histogram": histogram, "surfaces": surfaces,
        })),
    );
    out.summary.push(format!(
        "scene {scene:#04x}: {} placements, {} spawns, {} collision faces ({} terrain), {} unresolved placements",
        placements.len(),
        spawns.len(),
        tris.len(),
        tris.iter().filter(|t| t.layer == Layer::Terrain).count(),
        collision.unresolved.len(),
    ));
    Ok(out)
}

/// The scene's level geometry (`level-<id>/`): `level-<id>.gltf` + `.bin`, textures as
/// `textures/*.png` next to it.
pub fn level(rom: &Rom, scene: usize) -> Result<Export> {
    let stem = format!("level-{scene:02x}");
    let level = scene_level(rom, scene)?;
    let gltf = to_gltf(&level, &rom.textures()?, &format!("{stem}.bin"))?;
    let mut out = Export::default();
    out.summary.push(format!(
        "level {scene:#04x}: {} instances of {} models, {} missing placements, {} PNGs, {} buffer bytes",
        level.instances.len(),
        level.meshes.len(),
        level.missing.len(),
        gltf.images.len(),
        gltf.bin.len(),
    ));
    out.summary
        .extend(gltf.warnings.iter().map(|w| format!("warning: {w}")));
    out.add(format!("{stem}/{stem}.gltf"), gltf.json);
    out.add(format!("{stem}/{stem}.bin"), gltf.bin);
    for (path, png) in gltf.images {
        out.add(format!("{stem}/{path}"), png);
    }
    Ok(out)
}

/// A bank-01 character (`character-<entry>/`): `bank01-<entry>.gltf` + `.bin` (skinned mesh,
/// skeleton, every playable clip; textures are PNG buffer views inside the `.bin`) and
/// `clips.json` (pair, logical ids and duration of each glTF animation, in order).
pub fn character(rom: &Rom, entry: usize) -> Result<Export> {
    let dir = format!("character-{entry:03}");
    let stem = format!("bank01-{entry:04}");
    let c = Character::load(rom, entry)?;
    let g = c.gltf(&rom.textures()?, true)?;
    let mut out = Export::default();
    out.add(
        format!("{dir}/{stem}.gltf"),
        serde_json::to_vec(&g.with_uri(&format!("{stem}.bin")))?,
    );
    out.add(format!("{dir}/{stem}.bin"), g.bin);

    let clips: Vec<Value> = c
        .playable_clips()
        .enumerate()
        .map(|(i, clip)| {
            json!({
                "animation": i, "pair": clip.pair, "logical_ids": clip.logical,
                "frames": clip.frames.len(), "keyframe_step": clip.step,
                "duration_ticks": clip.duration, "duration_s": f32::from(clip.duration) / FPS,
            })
        })
        .collect();
    let count = |key: &str| g.json[key].as_array().map_or(0, Vec::len);
    out.summary.push(format!(
        "character {entry}: {} joints, {} animations ({} pairs in bank-02 entry {}), {} images",
        c.model.joints.len(),
        count("animations"),
        c.clips.len(),
        c.anim_entry,
        count("images"),
    ));
    let manifest = json!({
        "bank01_entry": entry, "bank02_entry": c.anim_entry, "joints": c.model.joints.len(),
        "fps": FPS, "pairs": c.clips.len(), "clips": clips,
    });
    out.add(format!("{dir}/clips.json"), pretty(&manifest));
    Ok(out)
}

/// Every texture a ROM display list draws (bank-03 props, bank-04 scene segments, bank-01
/// character lists), decoded with the format, size and palette of its draws:
/// `textures/<id>-<w>x<h>-<format>[-p<palette id>+<offset>][-i].png`, one file per distinct
/// decode (`-i`: TMEM-interleaved rows). Images are upright (rows flipped from memory order).
pub fn textures(rom: &Rom) -> Result<Export> {
    // Decoding is `extract_gbi::Material::texture_png`; the file naming and dedup are private to
    // extract-level's glTF writer, so put every model in one pseudo-level and keep only its
    // images (instances, and so the glTF scene, stay empty).
    let mut level = Level::default();
    let mut rejected = Vec::new();
    let mut add = |path: Vec<usize>, model: &[u8], level: &mut Level| match Model::parse(model)
        .and_then(|m| m.walk())
    {
        Ok(ir) => level.meshes.push(Mesh { path, ir }),
        Err(e) => rejected.push(format!("{path:02x?}: {e}")),
    };
    for i in 0..rom.bank(0x03)?.entries.len() {
        let model = rom.asset(&[0x03, i])?;
        if !model.is_empty() {
            add(vec![0x03, i], &model, &mut level);
        }
    }
    for s in 0..rom.bank(0x04)?.entries.len() {
        let bundle = rom.asset(&[0x04, s])?;
        for (j, model) in bundle_segments(&bundle)?.into_iter().enumerate() {
            if !model.is_empty() {
                add(vec![0x04, s, j], model, &mut level);
            }
        }
    }
    let game_data = rom.game_data()?;
    for e in 0..rom.bank(0x01)?.entries.len() {
        let data = rom.asset(&[0x01, e])?;
        if data.is_empty() {
            continue;
        }
        match parse_model(&data, &game_data) {
            Ok(m) => level
                .meshes
                .extend(m.lists.into_iter().enumerate().map(|(k, ir)| Mesh {
                    path: vec![0x01, e, k],
                    ir,
                })),
            Err(err) => rejected.push(format!("[01, {e:02x}]: {err}")),
        }
    }

    let gltf = to_gltf(&level, &rom.textures()?, "unused.bin")?;
    let ids: std::collections::BTreeSet<&str> = gltf
        .images
        .iter()
        .map(|(path, _)| &path["textures/".len()..][..4])
        .collect();
    let mut out = Export::default();
    out.summary.push(format!(
        "textures: {} PNGs for {} of {TEXTURE_COUNT} texture ids, from {} walked display lists; {} models rejected, {} warnings",
        gltf.images.len(),
        ids.len(),
        level.meshes.len(),
        rejected.len(),
        gltf.warnings.len(),
    ));
    out.summary
        .extend(rejected.iter().map(|r| format!("rejected: {r}")));
    out.summary
        .extend(gltf.warnings.iter().map(|w| format!("warning: {w}")));
    out.files = gltf.images;
    Ok(out)
}

/// Everything the vertical slice needs: scene 0x2D placements, spawns and collision, its level
/// glTF with textures, and the Deathmatch character glTF with its clips.
pub fn slice(rom: &Rom) -> Result<Export> {
    let mut out = scene(rom, SLICE_SCENE)?;
    out.append(level(rom, SLICE_SCENE)?);
    out.append(character(rom, SLICE_CHARACTER)?);
    Ok(out)
}

/// The vertical slice in memory: scene 0x2D and character 117, as the browser client loads them.
/// Positions are game units, world space (the level GLB's nodes carry the placement transforms).
pub struct Slice {
    /// Scene 0x2D's level as one GLB, PNG textures embedded. Node extras: `model` (asset path)
    /// and `placement` (index into the scene's 46 placements; null for bundle slots 0-2).
    pub level_glb: Vec<u8>,
    /// Character 117 as one GLB: skinned mesh, 28 joints, 764 animations, textures embedded.
    pub character_glb: Vec<u8>,
    /// Scene 0x2D's collision in build order (terrain, secondary, placements); terrain faces
    /// carry their surface word.
    pub collision: Vec<Triangle>,
    /// The 20 multiplayer spawn records: position, yaw (facing u16 = `yaw << 8`), initial camera eye.
    pub spawns: Vec<SpawnPoint>,
    /// One entry per animation of `character_glb`, in the same order.
    pub clips: Vec<ClipInfo>,
    /// The character's root-motion clips.
    pub root_clips: Vec<RootClip>,
    /// The weapons and the grenade in a player's hand ([`HELD_CLIPS`]' models): which joint draws
    /// each and the animation events that show and hide it.
    pub held: Vec<Held>,
    /// Their models as one GLB, PNG textures embedded: glTF scene `i` is `held[i]`, a node and mesh
    /// per part (named `09-<entry>-<part>` in hex) in its joint's frame (character model units).
    pub held_glb: Vec<u8>,
}

impl Slice {
    /// Extracts the slice from ROM bytes (.z64, .v64 or .n64 order; the US ROM, SHA-1 checked).
    pub fn from_rom(rom: Vec<u8>) -> Result<Self> {
        let rom = Rom::new(rom)?;
        let textures = rom.textures()?;
        let level = to_gltf(&scene_level(&rom, SLICE_SCENE)?, &textures, "")?;
        let character = Character::load(&rom, SLICE_CHARACTER)?;
        let placements = scene_placements(&rom, SLICE_SCENE)?;
        let (held, models, scenes) = held_level(&rom)?;
        let (clips, root_clips) = clips(&character);
        Ok(Self {
            held_glb: held_gltf(&models, scenes, &textures, "")?.glb(),
            held,
            level_glb: level.glb(),
            character_glb: character.gltf(&textures, true)?.glb(),
            collision: scene_collision(&rom, SLICE_SCENE, &placements)?.triangles,
            spawns: scene_spawns(&rom, SLICE_SCENE)?,
            clips,
            root_clips,
        })
    }
}

/// The character's clips (one per glTF animation, in order) and its root-motion clips.
fn clips(character: &Character) -> (Vec<ClipInfo>, Vec<RootClip>) {
    let clips = (character.playable_clips())
        .map(|c| ClipInfo {
            pair: c.pair,
            logical: c.logical.clone(),
            frames: c.frames.len(),
            duration_ticks: c.duration,
        })
        .collect();
    let roots = (character.playable_clips())
        .filter_map(|c| {
            let rm = c.root_motion.as_ref()?;
            Some(RootClip {
                logical: c.logical.clone(),
                duration: c.duration,
                step: c.step,
                z: rm.z.clone(),
                end: rm.end,
            })
        })
        .collect();
    (clips, roots)
}

/// The held models ([`HELD_CLIPS`]'), their meshes as one pseudo-level (a mesh per part, at the
/// origin; a child object's per display list, in its bind pose), and one glTF scene per model
/// listing its parts' nodes.
fn held_level(rom: &Rom) -> Result<(Vec<Held>, Level, Value)> {
    let held = held_models(rom, SLICE_CHARACTER, &HELD_CLIPS)?;
    let (models, scenes) = attached_level(rom, &held)?;
    Ok((held, models, scenes))
}

/// The models of `held` as [`held_level`] lays them out: a glTF scene each.
fn attached_level(rom: &Rom, held: &[Held]) -> Result<(Level, Value)> {
    let game_data = rom.game_data()?;
    let (mut models, mut scenes) = (Level::default(), Vec::new());
    for h in held {
        let (bank, parts) = match h.object {
            true => {
                let model = parse_model(&rom.asset(&[0x01, h.entry])?, &game_data)?;
                let pivots = bind_pivots(&model.joints)?;
                let mut lists = model.lists;
                for v in lists.iter_mut().flat_map(|ir| &mut ir.vertices) {
                    let p = pivots
                        .get(matrix_slot(v.matrix))
                        .copied()
                        .unwrap_or_default();
                    v.position = std::array::from_fn(|a| v.position[a] + p[a] as i16);
                }
                (0x01, lists)
            }
            false => (
                0x09,
                parse_attachment(&rom.asset(&[0x09, h.entry])?, &game_data)?,
            ),
        };
        let first = models.meshes.len();
        scenes.push(json!({ "nodes": (first..first + parts.len()).collect::<Vec<_>>() }));
        for (part, ir) in parts.into_iter().enumerate() {
            models.instances.push(Instance {
                mesh: models.meshes.len(),
                placement: None,
                matrix: IDENTITY,
            });
            models.meshes.push(Mesh {
                path: vec![bank, h.entry, part],
                ir,
            });
        }
    }
    Ok((models, scenes.into()))
}

/// The models the original draws for the projectiles in flight, glTF scene `i` of `projectiles.gltf`
/// each (`../lab/research/mp-original/visuals-scratch`, oracle runs gren4, rocket, knife, bolt2): the
/// thrown grenade (bank 01 #0x7E, one joint), the rocket (bank 09 #0xE1), weapon 0x14's knife
/// (#0x23) and the crossbow's bolt (#0x35, also drawn in the crossbow), and Tank's shell (#0x33,
/// drawn at 1.5: V sh1 DL 42, `../lab/research/mp-original/tank-pack-scratch`; Race's missile, drawn
/// at 1.0: V m1 DL 224, `../lab/research/mp-original/race-pack-scratch`), and War: Raptor's
/// bone knife (0x3F: #0x7C, action 34, V k1 DLs 97 on, `../lab/research/mp-original/raptor-pack-scratch`;
/// the bone crossbow's bolt is #0x35, V c1), at the origin with +z along the flight. A bullet has no
/// model in flight (V twin DLs 104-110: only the muzzle flash).
pub const PROJECTILES: [[usize; 2]; 6] = [
    [0x01, 0x7E],
    [0x09, 0xE1],
    [0x09, 0x23],
    [0x09, 0x35],
    [0x09, 0x33],
    [0x09, 0x7C],
];

/// [`PROJECTILES`] as one pseudo-level (a mesh each, a bank-01 model in its bind pose) and their
/// glTF scenes.
pub fn projectiles_level(rom: &Rom) -> Result<(Level, Value)> {
    let game_data = rom.game_data()?;
    let mut level = Level::default();
    for path in PROJECTILES {
        let ir = match path[0] {
            0x01 => {
                let model = parse_model(&rom.asset(&path)?, &game_data)?;
                let pivots = bind_pivots(&model.joints)?;
                let mut ir = (model.lists.into_iter().next()).context("projectile: no list")?;
                for v in &mut ir.vertices {
                    let p = (pivots.get(matrix_slot(v.matrix)).copied()).unwrap_or_default();
                    v.position = std::array::from_fn(|a| v.position[a] + p[a] as i16);
                }
                ir
            }
            _ => parse_effect(&rom.asset(&path)?, &game_data)?,
        };
        level.instances.push(Instance {
            mesh: level.meshes.len(),
            placement: None,
            matrix: IDENTITY,
        });
        level.meshes.push(Mesh {
            path: path.to_vec(),
            ir,
        });
    }
    let scenes = (0..PROJECTILES.len()).map(|i| json!({ "nodes": [i] }));
    Ok((level, scenes.collect()))
}

/// The held models' glTF, one glTF scene per held model: Bevy's `#Scene<i>` is `held[i]`.
fn held_gltf(models: &Level, scenes: Value, textures: &Textures, bin_uri: &str) -> Result<Gltf> {
    let mut gltf = to_gltf(models, textures, bin_uri)?;
    let mut json: Value = serde_json::from_str(&gltf.json)?;
    json["scenes"] = scenes;
    gltf.json = json.to_string();
    Ok(gltf)
}

/// Bank-01 model `entry` (the squirrels' soldier [`SQUIRREL_SOLDIER`], the [`CAVEMAN`]) with the
/// clips it plays: the Tedi's (`tedi`; the same 28-joint rig), or another rig's (the [`BABY`] on the
/// [`RAPTOR`]'s).
pub fn on_tedi_rig(rom: &Rom, entry: usize, tedi: &Character) -> Result<Character> {
    Ok(Character {
        entry,
        anim_entry: tedi.anim_entry,
        model: parse_model(&rom.asset(&[0x01, entry])?, &rom.game_data()?)?,
        clips: tedi.clips.clone(),
    })
}

/// Pick `entry` ([`PICKS`]) on the Tedi's rig without clips: its glTF has none (the Tedi's drive
/// it, `clips_from`). Conker (0) is its drawn [`CONKER_DRAWN`]'s mesh under model 0's number (its
/// record, voices and class's model).
pub fn mesh_only(rom: &Rom, entry: usize) -> Result<Character> {
    let drawn = if entry == 0 { CONKER_DRAWN } else { entry };
    Ok(Character {
        entry,
        anim_entry: 0,
        model: parse_model(&rom.asset(&[0x01, drawn])?, &rom.game_data()?)?,
        clips: vec![],
    })
}

/// Heist's team looks (V `../lab/oracle/map1a/teams/`, run tm4b DL 96/100): the Wise Guy's one
/// material drawn from segment 0x0A (its image and palette) takes entry 1 + team of the model's
/// texture table (header word 6: 12-byte entries, the texture id first; C): REDS 7246, BLUES 7230,
/// GREENS 7231, YELLOWS 7232, CI8 32×32 with their palette in their last 0x200 bytes. Nothing else
/// differs between the teams (V: the model's commands, prim and env are the same).
const TEAM_SEGMENT: u32 = 0x0A;

/// The texture id of entry `k` of bank-01 model `entry`'s texture table (header word 6: 12-byte
/// entries, the texture id first; C).
fn texture_id(rom: &Rom, entry: usize, k: usize) -> Result<u32> {
    let model = rom.asset(&[0x01, entry])?;
    let word = |at: usize| {
        (model.get(at..at + 4))
            .map(|w| u32::from_be_bytes(w.try_into().unwrap()))
            .context("short bank-01 model")
    };
    word(word(0x18)? as usize + 12 * k)
}

/// The texture ids of a model's team looks ([`TEAM_SEGMENT`]): its texture table's entries 1-4,
/// the tank's flags 16-19 ([`TANK_FLAGS`]).
fn team_ids(rom: &Rom, entry: usize) -> Result<[u32; 4]> {
    let first = if entry == TANK { TANK_FLAGS } else { 1 };
    let ids: Vec<u32> = (first..first + 4)
        .map(|k| texture_id(rom, entry, k))
        .collect::<Result<_>>()?;
    Ok(ids.try_into().expect("four teams"))
}

/// `material` with the image (and palette) it draws from `segment` bound to texture `id`.
/// `character` with its materials drawing from `segment` bound to texture `id`.
fn bind_segment(mut character: Character, segment: u32, id: u32) -> Character {
    for d in character.model.lists.iter_mut().flat_map(|l| &mut l.draws) {
        d.material = bind_material(d.material, segment, id);
    }
    character
}

/// A character's team looks: their texture ids, PNG files (relative to its folder) and PNGs.
struct TeamLooks {
    ids: [u32; 4],
    files: Vec<String>,
    pngs: Vec<Vec<u8>>,
}

fn team_looks(rom: &Rom, character: &Character, textures: &Textures) -> Result<TeamLooks> {
    let ids = team_ids(rom, character.entry)?;
    let pngs = (ids.iter())
        .map(|&id| segment_png(character, TEAM_SEGMENT, id, textures))
        .collect::<Result<_>>()?;
    let files = (0..4).map(|t| format!("textures/team-{t}.png")).collect();
    Ok(TeamLooks { ids, files, pngs })
}

/// The PNG of `character`'s material drawing from `segment`, bound to texture `id`.
fn segment_png(
    character: &Character,
    segment: u32,
    id: u32,
    textures: &Textures,
) -> Result<Vec<u8>> {
    let drawn = |m: &Material| {
        let t = m.texture.and_then(|t| t.image);
        matches!(t, Some(extract_gbi::Image::Segment(a)) if a >> 24 == segment)
    };
    let material = (character.model.lists.iter().flat_map(|l| &l.draws))
        .map(|d| d.material)
        .find(drawn)
        .with_context(|| format!("{}: no segment-{segment:#x} material", character.entry))?;
    (bind_material(material, segment, id))
        .texture_png(|i| textures.get(i as usize).ok())
        .with_context(|| format!("texture {id}"))?
        .map_err(Into::into)
}

/// The glTF material of `character` drawing texture `id` (the glTF writer's materials, distinct, in
/// first-use order).
fn material_of(character: &Character, id: u32) -> Result<usize> {
    let mut materials = Vec::new();
    for d in character.model.lists.iter().flat_map(|l| &l.draws) {
        if !materials.contains(&d.material) {
            materials.push(d.material);
        }
    }
    let image = Some(extract_gbi::Image::Flat { index: id, mode: 0 });
    (materials.iter())
        .position(|m| m.texture.and_then(|t| t.image) == image)
        .with_context(|| format!("{}: no material draws texture {id}", character.entry))
}

/// `character.json`'s teams of a character bound to team 0's look: the glTF material drawing it.
fn team_material(character: &Character, ids: &[u32; 4]) -> Result<assets::Teams> {
    Ok(assets::Teams {
        material: material_of(character, ids[0])?,
        textures: (0..4).map(|t| format!("textures/team-{t}.png")).collect(),
    })
}

/// The tank's texture table entries (C `func_1502F01C`: a light code c ≥ 10 binds entry c − 10;
/// V b1, j1): segment 6 the brake lights (code 25 → entry 15 unlit, 22 → entry 12 while braking),
/// segment 7 the headlights (19 → entry 9, lit: the match's, and Colors cs5's op 9 on its tanks),
/// segment 0x0A the flag ([`TANK_FLAGS`]); entry 4 the treads (drawn directly).
const TANK_BRAKE: [usize; 2] = [15, 12];
const TANK_HEADLIGHTS: usize = 9;
/// The tank's flags, entries 16-19 (red, blue, green, yellow; a star on each): obj+0x68 = 16-19
/// by slot, the slots from 3 up yellow (V b1).
const TANK_FLAGS: usize = 16;
const TANK_TREADS: usize = 4;
/// The tank's joints: the turret (2) and the gun (3) (C `func_15033FE0` → `func_150344A0` on
/// models 0x28 and 0x77: joint 2 turned about y by −obj+0x2E4·65536/18000, joint 3 about x by
/// obj+0x2E6·65536/18000; V j1 DL 60: a turret yaw of 90° and a pitch of 36° on joints 2 and 3 only),
/// the muzzle (4, 110 along joint 3's z: I), the treads (13 then 12: the vertices of the game's
/// scroll lists 0 and 1, `0x800C4020[119]` = 187 and 215, 28 each; `func_150ED638` scrolls list 0
/// by its first speed argument, list 1 by its second; which side is the left: I).
const TANK_JOINTS: [usize; 5] = [2, 3, 4, 13, 12];
/// The lit brake lights' PNG in the tank's folder.
const BRAKE_LIT: &str = "textures/brake-lit.png";

/// The tank as the match draws it (brake lights unlit, headlights lit, flag 0) and its other
/// looks' PNGs: the flags (`textures/team-<k>.png`) and the lit brake lights ([`BRAKE_LIT`]).
fn tank(rom: &Rom, textures: &Textures) -> Result<(Character, Images)> {
    let tank = Character::load(rom, TANK)?;
    let id = |k| texture_id(rom, TANK, k);
    let flags = team_looks(rom, &tank, textures)?;
    let mut images: Images = flags.files.into_iter().zip(flags.pngs).collect();
    let lit = segment_png(&tank, 6, id(TANK_BRAKE[1])?, textures)?;
    images.push((BRAKE_LIT.into(), lit));
    let tank = bind_segment(tank, 6, id(TANK_BRAKE[0])?);
    let tank = bind_segment(tank, 7, id(TANK_HEADLIGHTS)?);
    Ok((bind_segment(tank, TEAM_SEGMENT, flags.ids[0]), images))
}

/// `character.json`'s tank parts of the tank as [`tank`] binds it.
fn tank_parts(rom: &Rom, tank: &Character) -> Result<assets::Tank> {
    let material = |k| material_of(tank, texture_id(rom, TANK, k)?);
    let [turret, gun, muzzle, list_0, list_1] = TANK_JOINTS;
    Ok(assets::Tank {
        turret,
        gun,
        muzzle,
        brake: material(TANK_BRAKE[0])?,
        brake_lit: BRAKE_LIT.into(),
        treads: material(TANK_TREADS)?,
        tread_joints: [list_0, list_1],
    })
}

/// The segments a character's face draws from (`func_1502F01C`, C; V
/// `../lab/research/mp-original/faces.md`): 6 and 7 the eyes, 0x0A and 0x0B Conker's mouth.
const FACE_SEGMENTS: [u32; 4] = [6, 7, 0x0A, 0x0B];

/// Model `entry`'s texture-table entries on face `segment`, from its bank-0x11 record: on 6 and 7
/// the eyes open, half shut and shut (eye codes 0-2: +8..+0xA, +0xB..+0xD), on 0x0A and 0x0B the
/// one a match keeps (+0x3B, +0x3C, `func_1507E5C8`).
pub fn face_entries(rom: &Rom, entry: usize, segment: u32) -> Result<Vec<usize>> {
    let asset = rom.asset(&[0x11, entry])?;
    let record = (asset.get(..4))
        .map(|w| u32::from_be_bytes(w.try_into().unwrap()) as usize)
        .and_then(|at| asset.get(at..at + 0x3D))
        .context("short bank-0x11 record")?;
    let at = match segment {
        6 => 8..0xB,
        7 => 0xB..0xE,
        s => 0x31 + s as usize..0x32 + s as usize,
    };
    Ok(record[at].iter().map(|&e| usize::from(e)).collect())
}

/// Whether a character blinks in a match (`func_1507E2B0`): every player model, bots too, and
/// Raptor's body (V faces runs n1, v1, k1, b1). Not the baby (its blink never runs, V b1) nor
/// those only outros draw (their flag 0x800C35EA stops it, V h3).
fn blinks(entry: usize) -> bool {
    PLAYERS.contains(&entry) || PICKS.contains(&entry) || entry == BODY
}

/// `character` as a match draws it outside a blink (`func_1502F01C`): each material still on a
/// face segment bound to the drawn model's ([`CONKER_DRAWN`] for Conker) entry for it, the eyes
/// open and Conker's mouth; and if it [`blinks`], its eyes' `faces` and their PNGs
/// (`textures/face-<entry>.png`).
pub fn bind_faces(
    rom: &Rom,
    mut character: Character,
    textures: &Textures,
) -> Result<(Character, Vec<assets::FaceSegment>, Images)> {
    use extract_gbi::Image;
    let drawn = if character.entry == 0 {
        CONKER_DRAWN
    } else {
        character.entry
    };
    let draws = |c: &Character| -> Vec<Material> {
        (c.model.lists.iter().flat_map(|l| &l.draws))
            .map(|d| d.material)
            .collect()
    };
    let face = |m: &Material| {
        let image = m.texture.and_then(|t| t.image);
        (FACE_SEGMENTS.into_iter())
            .find(|&s| matches!(image, Some(Image::Segment(a)) if a >> 24 == s))
    };
    let before = draws(&character);
    let segments: BTreeSet<u32> = before.iter().filter_map(face).collect();
    let (mut eyes, mut images): (Vec<_>, Images) = (vec![], vec![]);
    for s in segments {
        let entries = face_entries(rom, drawn, s)?;
        let ids: Vec<u32> = (entries.iter())
            .map(|&e| texture_id(rom, drawn, e))
            .collect::<Result<_>>()?;
        if s < 0x0A && blinks(character.entry) {
            for (e, &id) in entries.iter().zip(&ids) {
                let file = format!("textures/face-{e:02}.png");
                let png = segment_png(&character, s, id, textures)?;
                match images.iter().find(|i| i.0 == file) {
                    Some(i) => anyhow::ensure!(i.1 == png, "{}: two {file}", character.entry),
                    None => images.push((file, png)),
                }
            }
            eyes.push((s, entries));
        }
        character = bind_segment(character, s, ids[0]);
    }
    // Each draw's glTF material (the writer's: distinct, in first-use order).
    let mut distinct: Vec<Material> = vec![];
    let index: Vec<usize> = (draws(&character).into_iter())
        .map(|m| match distinct.iter().position(|d| *d == m) {
            Some(i) => i,
            None => {
                distinct.push(m);
                distinct.len() - 1
            }
        })
        .collect();
    let drawing = |s: Option<u32>| -> BTreeSet<usize> {
        (before.iter().zip(&index))
            .filter(|(m, _)| face(m) == s)
            .map(|(_, &i)| i)
            .collect()
    };
    let faces = (eyes.into_iter())
        .map(|(s, e)| {
            let materials = drawing(Some(s));
            anyhow::ensure!(
                materials.is_disjoint(&drawing(None)),
                "{}: segment {s}'s material draws more",
                character.entry
            );
            // A material ignoring its texture shows no face (176's 12, [`Material::samples_texel`]).
            Ok(assets::FaceSegment {
                segment: s as u8,
                materials: (materials.into_iter())
                    .filter(|&i| distinct[i].samples_texel())
                    .collect(),
                textures: std::array::from_fn(|k| format!("textures/face-{:02}.png", e[k])),
            })
        })
        .collect::<Result<_>>()?;
    Ok((character, faces, images))
}

/// A map of the pack: its scene and a recording of its background (`../lab/audio`'s), if any, of
/// the song its outro starts ([`outro::songs`]) or on 0x24 its bomb's countdown starts (song 50),
/// and of its song's positional channels (a raw recording, [`audio::stem`]).
pub struct PackMap<'a> {
    pub scene: usize,
    pub background: Option<&'a [u8]>,
    pub song: Option<&'a [u8]>,
    pub stem: Option<&'a [u8]>,
}

/// A map's folder under `maps/`: its scene in lowercase hex (`2d`, `30`).
pub fn map_name(scene: usize) -> String {
    format!("{scene:x}")
}

/// The asset pack (`conker-extract pack`): the data files ([`conker_assets::Slice`]) of each map
/// (the first one also writes the shared files), their levels, the characters
/// ([`PACK_CHARACTERS`]: the squirrel soldier, the Tedi and the caveman) and held models as `.gltf` + `.bin` +
/// PNG files, the sound ([`audio::pack_audio`]), `pack.json` and `README.md`; with 0x2D, 0x30 or
/// 0x2B, War's outros ([`outro::outros`]): their actors' characters (with the clips they play) and
/// voice lines; with 0x30, War: Colors' two flags as characters (every clip); with 0x30, 0x34 or 0x2B,
/// the mounted gun ([`MOUNTED_GUN`]) as a character (its clip); with 0x2B, the [`VILLAGERS`] after
/// [`PACK_CHARACTERS`] (with their `character.json` and its looks, [`villager_look`]); with 0x1A,
/// the [`WISE_GUY`] after them (its team looks) and Heist's [`CASH`]; with 0x24, the [`TANK`] after
/// them (its flags, lights and parts, [`tank`]) and the map's props; with 0x3F, the [`RAPTOR`] last,
/// War: Raptor's [`BABY`], [`EGG`] and [`BODY`] (every clip), its props and `raptor.json`
/// ([`raptor_mode`]); with 0x33, Race's bots' [`RACER`] after them (its looks), the board and
/// helmets as the map's props and `race.json` ([`race_mode`]); last, the [`PICKS`], mesh-only.
pub fn pack(rom: &Rom, maps: &[PackMap]) -> Result<Export> {
    let textures = rom.textures()?;
    let tedi = Character::load(rom, SLICE_CHARACTER)?;
    let mut characters = vec![on_tedi_rig(rom, SQUIRREL_SOLDIER, &tedi)?];
    let caveman = on_tedi_rig(rom, CAVEMAN, &tedi)?;
    let villagers = maps.iter().any(|m| m.scene == BEACH).then_some(VILLAGERS);
    let villagers: Vec<_> = (villagers.into_iter().flatten())
        .map(|v| on_tedi_rig(rom, v, &tedi))
        .collect::<Result<_>>()?;
    let vault = maps.iter().any(|m| m.scene == VAULT);
    let wise_guy = (vault.then(|| on_tedi_rig(rom, WISE_GUY, &tedi))).transpose()?;
    let teams = (wise_guy.as_ref())
        .map(|c| team_looks(rom, c, &textures))
        .transpose()?;
    // The Wise Guy's glTF draws team 0's look.
    let wise_guy = wise_guy.map(|c| bind_segment(c, TEAM_SEGMENT, teams.as_ref().unwrap().ids[0]));
    let tank = (maps.iter().any(|m| m.scene == TANK_MAP))
        .then(|| tank(rom, &textures))
        .transpose()?;
    let (tank, tank_looks) = tank.unzip();
    let temple = maps.iter().any(|m| m.scene == TEMPLE);
    let raptor = (temple.then(|| Character::load(rom, RAPTOR))).transpose()?;
    let race = maps.iter().any(|m| m.scene == RACE_MAP);
    let racer = (race.then(|| on_tedi_rig(rom, RACER, &tedi))).transpose()?;
    let picks: Vec<_> = (PICKS.iter())
        .map(|&p| mesh_only(rom, p))
        .collect::<Result<_>>()?;
    characters.extend(
        ([tedi, caveman].into_iter())
            .chain(villagers)
            .chain(wise_guy)
            .chain(tank)
            .chain(raptor)
            .chain(racer)
            .chain(picks),
    );
    // Their faces bound to the match's look (after the Wise Guy's team and the tank's lights).
    let (characters, faces): (Vec<_>, Vec<_>) = (characters.into_iter())
        .map(|c| bind_faces(rom, c, &textures).map(|(c, f, i)| (c, (f, i))))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .unzip();
    // `PACK_CHARACTERS`, then the villagers, the Wise Guy, the tank, the raptor, the racer and the
    // picks: each folder is its bank-01 entry (Conker's, 0, draws #1).
    let entries: Vec<usize> = characters.iter().map(|c| c.entry).collect();
    let names: Vec<String> = entries.iter().map(usize::to_string).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    debug_assert_eq!(names[..PACK_CHARACTERS.len()], PACK_CHARACTERS);
    let (held, models, scenes) = held_level(rom)?;
    let mut outros: Vec<Vec<_>> = (maps.iter())
        .map(|m| outro::outros(rom, m.scene))
        .collect::<Result<_>>()?;
    let songs: Vec<_> = (maps.iter().zip(&mut outros))
        .flat_map(|(m, o)| outro::songs(o, m.song))
        .collect();
    let baked: Vec<_> = outros.concat();
    // Beach's outros' villagers (156, 157) and Raptor's raptor and baby are packed with every clip
    // already.
    let mut outro_characters = outro::characters(rom, &baked)?;
    outro_characters.retain(|(_, c)| !entries.contains(&c.entry) && c.entry != BABY);
    let named = |c: Character| (c.entry.to_string(), c);
    // War: Colors' flags on 0x30 (bank 01 #169 red, #168 green: 9 joints, the cloth's 4 clips).
    if maps.iter().any(|m| m.scene == 0x30) {
        for f in conker_rules::colors::FLAGS {
            outro_characters.push(named(Character::load(rom, usize::from(f.model))?));
        }
    }
    // The mounted guns on 0x30, 0x34 and 0x2B (bank 01 #138: 12 joints, clip 1 the fire loop).
    if maps.iter().any(|m| matches!(m.scene, 0x30 | 0x34 | BEACH)) {
        outro_characters.push(named(Character::load(rom, MOUNTED_GUN)?));
    }
    // Heist's cash on 0x1A (bank 01 #87: 5 joints, its own 12 clips).
    if vault {
        outro_characters.push(named(Character::load(rom, CASH)?));
    }
    // War: Raptor's baby (the raptor's rig and clips), egg (1 joint, no clip) and body (31 joints,
    // its own 32 clips) on 0x3F.
    if temple {
        for entry in [BABY, EGG, BODY] {
            outro_characters.push(named(load_character(rom, entry)?));
        }
    }
    let outro_characters: Vec<_> = (outro_characters.into_iter())
        .map(|(name, c)| Ok((name, bind_faces(rom, c, &textures)?)))
        .collect::<Result<_>>()?;
    let parse = |text: &str| serde_json::from_str::<Value>(text);
    let mut out = Export::default();
    let (mut gltfs, mut surfaces, mut summary) = (Vec::new(), BTreeSet::new(), Vec::new());
    let mut data = None;
    for (i, map) in maps.iter().enumerate() {
        let name = map_name(map.scene);
        let (segments, frames) = map_segments(rom, map.scene, &textures)?;
        let d = pack_data(
            rom,
            map.scene,
            &characters,
            &faces,
            &held,
            &outros[i],
            segments,
        )?;
        for (path, bytes) in d.write(&name, &names, PACK_WEAPONS)? {
            if i == 0 || path.starts_with("maps/") {
                out.add(path, bytes);
            }
        }
        let mut level = to_gltf(&map_level(rom, map.scene)?, &textures, "level.bin")?;
        level.images.extend(frames);
        if map.scene == RACE_MAP {
            level.images.extend(race_crowd(rom, &textures)?.1);
        }
        gltfs.push((
            assets::level_gltf(&name),
            parse(&level.json)?,
            level.bin,
            level.images,
        ));
        if let Some((models, scenes)) = outro::models(rom, map.scene, &outros[i])? {
            let g = held_gltf(&models, scenes, &textures, "outro.bin")?;
            let json = parse(&g.json)?;
            gltfs.push((assets::outro_gltf(&name), json, g.bin, g.images));
        }
        if map.scene == BEACH {
            let ring = parse_effect(&rom.asset(&[0x09, RING])?, &rom.game_data()?)?;
            let level = Level {
                meshes: vec![Mesh {
                    path: vec![0x09, RING],
                    ir: ring,
                }],
                instances: vec![Instance {
                    mesh: 0,
                    placement: None,
                    matrix: IDENTITY,
                }],
                ..Level::default()
            };
            let scenes = json!([{ "name": format!("09-{RING:02x}"), "nodes": [0] }]);
            let g = held_gltf(&level, scenes, &textures, "effects.bin")?;
            let json = parse(&g.json)?;
            gltfs.push((assets::effects_gltf(&name), json, g.bin, g.images));
        }
        if map.scene == TEMPLE {
            out.add(
                assets::raptor::raptor_json(&name),
                raptor_mode(rom)?.to_json(),
            );
        }
        if map.scene == RACE_MAP {
            out.add(assets::race::race_json(&name), race_mode(rom)?.to_json());
        }
        if matches!(map.scene, SLICE_SCENE | TANK_MAP | TEMPLE | RACE_MAP) {
            let (props, scenes) = props_level(rom, map.scene)?;
            let props = held_gltf(&props, scenes, &textures, "props.bin")?;
            let json = parse(&props.json)?;
            gltfs.push((assets::props_gltf(&name), json, props.bin, props.images));
        }
        // Footsteps for the surface kinds of the maps' terrain (bits 0-4 of the surface words).
        surfaces.extend((d.collision.iter().filter_map(|t| t.surface)).map(|w| (w & 0x1F) as u8));
        summary.push(format!(
            "{name} ({} collision faces, {} spawns, {} pickups, {} triggers)",
            d.collision.len(),
            d.spawns.len(),
            d.pickups.len(),
            d.triggers.len() / 0x34,
        ));
        data.get_or_insert(d);
    }
    let data = data.context("pack: no map")?;
    let held_models = held_gltf(&models, scenes, &textures, "held.bin")?;
    for ((name, c), (_, face_images)) in names.iter().zip(&characters).zip(&faces) {
        let g = c.gltf(&textures, true)?;
        let (json, bin, mut images) =
            external_images(g.with_uri("character.bin"), &g.bin, "character");
        images.extend(face_images.iter().cloned());
        if let Some(t) = teams.as_ref().filter(|_| c.entry == WISE_GUY) {
            images.extend(t.files.iter().cloned().zip(t.pngs.iter().cloned()));
        }
        if c.entry == TANK {
            images.extend(tank_looks.iter().flatten().cloned());
        }
        gltfs.push((assets::character_gltf(name), json, bin, images));
    }
    for (name, (c, faces, face_images)) in &outro_characters {
        let g = c.gltf(&textures, true)?;
        let (json, bin, mut images) =
            external_images(g.with_uri("character.bin"), &g.bin, "character");
        images.extend(face_images.iter().cloned());
        // Heist's bag and cs2's Don Weaso are 0x1A's characters too (the client's `characters`);
        // Raptor's baby and body carry their `hidden` (the body its `faces`).
        if [CASH, DON_WEASO, BABY, BODY].contains(&c.entry) {
            let path = format!("characters/{name}/character.json");
            out.add(path, assets::to_json(&character_file(rom, c, faces)?, 2));
        }
        gltfs.push((assets::character_gltf(name), json, bin, images));
    }
    gltfs.push((
        assets::held_gltf(PACK_WEAPONS),
        parse(&held_models.json)?,
        held_models.bin,
        held_models.images,
    ));
    let (projectiles, scenes) = projectiles_level(rom)?;
    let g = held_gltf(&projectiles, scenes, &textures, "projectiles.bin")?;
    let path = assets::projectiles_gltf(PACK_WEAPONS);
    gltfs.push((path, parse(&g.json)?, g.bin, g.images));
    // The flamethrower's stream draws the fire jets' flame (V flame DL 130: blob 0x916).
    let (_, id, format, w, h) = traps::TEXTURES[0];
    let flame = extract_gbi::decode(format, w, h, &textures.get(id)?, &[], true)?;
    let flame = extract_gbi::png(w as u32, h as u32, &flame)?;
    out.add(assets::flame_png(PACK_WEAPONS), flame);
    let mut pngs = 0;
    // One accessor, node, material or animation per line.
    for (path, json, bin, images) in gltfs {
        let dir = path
            .rsplit_once('/')
            .expect("pack paths have a folder")
            .0
            .to_string();
        out.add(path.replace(".gltf", ".bin"), bin);
        out.add(path, assets::to_json(&json, 2));
        for (image, png) in images {
            // 0x2D's props share the level's folder, and some of its textures.
            let path = format!("{dir}/{image}");
            if !out.files.iter().any(|f| f.0 == path) {
                pngs += 1;
                out.add(path, png);
            }
        }
    }
    let backgrounds: Vec<_> = (maps.iter())
        .map(|m| (map_name(m.scene), m.background, m.stem, m.song))
        .collect();
    let outro_sounds: Vec<(String, BTreeSet<u16>)> = (maps.iter().zip(&outros))
        .map(|(m, o)| {
            let ids = o.iter().flat_map(|o| o.sounds.iter().map(|s| s.id));
            (map_name(m.scene), ids.collect())
        })
        .collect();
    let (sound_files, audio) =
        audio::pack_audio(rom, &surfaces, &backgrounds, &entries, &outro_sounds)?;
    let wavs = sound_files.iter().filter(|f| f.0.ends_with(".wav")).count();
    out.files.extend(sound_files);
    let (hud_files, hud) = hud::pack_hud(rom)?;
    out.files.extend(hud_files);
    out.files.extend(hud::pack_end(rom)?.0);
    out.files.extend(outro::voices(rom, &baked)?);
    out.files.extend(songs);
    if maps.iter().any(|m| m.scene == traps::SCENE) {
        out.files.extend(traps::textures(rom)?);
    }
    let outro_characters = outro_characters.into_iter().map(|c| c.0);
    let manifest = Manifest {
        format: FORMAT,
        source_rom_sha1: US_SHA1.into(),
        maps: maps.iter().map(|m| map_name(m.scene)).collect(),
        characters: (names.iter().map(|n| n.to_string()))
            .chain(outro_characters)
            .collect(),
        weapons: vec![PACK_WEAPONS.into()],
        audio,
        hud: true,
    };
    out.add(MANIFEST.into(), manifest.to_json());
    out.add("README.md".into(), PACK_README);
    let characters: Vec<String> = (names.iter().zip(&data.characters))
        .map(|(name, c)| {
            format!(
                "{name} ({} clips, {} with root motion, model lift {})",
                c.clips.len(),
                c.root_clips.len(),
                c.model_lift
            )
        })
        .collect();
    out.summary.push(format!(
        "pack: maps {}, characters {}, weapons {PACK_WEAPONS} ({} held models), {pngs} PNGs, \
         {wavs} WAVs, a HUD of {} textures and {} glyphs",
        summary.join(" and "),
        characters.join(" and "),
        data.held.len(),
        hud.textures.len(),
        hud.font.len(),
    ));
    Ok(out)
}

/// The placements a Deathmatch on `scene` hides from its start to its end (prop +0x6E set, V in the
/// oracle's RAM and under lldb, `lab/oracle/walls_README.md`): 0x2D's Total War props (21-28, 37)
/// and four scene objects (0, 1, 8, 9); 0x30's Colors flags (18, 19) and its placement 0; 0x3F's
/// props of its Raptor mode, the bone weapons on the weapon spots (15-22), three at the west nest
/// (4-6) and four scene objects at (6026, 156, 2188) (0-3); none on 0x34; 0x2B's segments 6 and 7
/// (placements 2, 3; War: Beach, scenario 8); 0x1A's 4-7 (V dm3, dm15, hs7: `../lab/oracle/map1a/
/// props_pickups_lights.md` §1; its 13 is drawn); 0x24's ([`TANK_PROPS`]). The pack leaves them out
/// of the level; the collision keeps only [`solid_placements`].
pub fn hidden_placements(scene: usize) -> &'static [usize] {
    match scene {
        0x2D => &[0, 1, 8, 9, 21, 22, 23, 24, 25, 26, 27, 28, 37],
        0x30 => &[0, 18, 19],
        0x3F => &[0, 1, 2, 3, 4, 5, 6, 15, 16, 17, 18, 19, 20, 21, 22],
        BEACH => &[2, 3],
        VAULT => &[4, 5, 6, 7],
        TANK_MAP => &TANK_PROPS,
        _ => &[],
    }
}

/// Map 0x24's hidden placements (V b1: +0x6E set at the start; `../lab/research/mp-original/
/// tank.md` §2): `[3, 72]` (9, 13), the gas bomb `[3, 80]` (12, shown on the centre post by the
/// match; 14 a second one, never shown), a second canisters `[3, 71]` (15), and the wrecks
/// `[3, 95]` (17-33: 18 + slot shown at a death for 330 frames, 17 at scale 1 never). They are
/// `maps/24/props.gltf`'s ([`props_level`]).
pub const TANK_PROPS: [usize; 22] = [
    9, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33,
];

/// 0x2D's War: Total War props among its [`hidden_placements`]: the shelter's doors (0, 1), the gas
/// masks (21-26) and the canisters (27, 28), which the client shows by the match.
pub const TOTAL_WAR_PROPS: [usize; 10] = [0, 1, 21, 22, 23, 24, 25, 26, 27, 28];

/// The placements the pack's collision keeps: [`deathmatch_solid`]'s, and on 0x2D the shelter's
/// doors (0, 1), solid in War: Total War while shown (V d2, `walls_README`: 23-73) and moved out of
/// reach in a Deathmatch (conker-rules' `gas::seat`).
pub fn solid_placements(scene: usize) -> Vec<usize> {
    let doors: &[usize] = if scene == SLICE_SCENE { &[0, 1] } else { &[] };
    [doors, deathmatch_solid(scene)].concat()
}

/// A map's props (`maps/<map>/props.gltf`): on 0x2D War: Total War's, glTF scene 0
/// [`TOTAL_WAR_PROPS`] in place (named `model@placement` as the level's nodes), scene 1 the
/// canister's model (3/85) at the origin, for a carrier's hands (a worn mask is a held model,
/// bank-09 #66); on 0x24 Tank's, scene 0 [`TANK_PROPS`] in place; on 0x3F War: Raptor's, scene 0
/// its [`hidden_placements`] in place (the cooked eggs 0-3, the never-shown 4-6, the bone weapons
/// 15-22); on 0x33 Race's board and helmets ([`race_attached`]), a scene each at the origin.
pub fn props_level(rom: &Rom, scene_id: usize) -> Result<(Level, Value)> {
    if scene_id == RACE_MAP {
        return attached_level(rom, &race_attached(rom)?);
    }
    let props: &[usize] = match scene_id {
        SLICE_SCENE => &TOTAL_WAR_PROPS,
        _ => hidden_placements(scene_id),
    };
    let scene = scene_level(rom, scene_id)?;
    let mut level = Level::default();
    let mut add = |mesh: &Mesh, placement, matrix| {
        let mesh = match level.meshes.iter().position(|m| m.path == mesh.path) {
            Some(i) => i,
            None => {
                level.meshes.push(mesh.clone());
                level.meshes.len() - 1
            }
        };
        level.instances.push(Instance {
            mesh,
            placement,
            matrix,
        });
    };
    let placed =
        (scene.instances.iter()).filter(|i| i.placement.is_some_and(|p| props.contains(&p)));
    for i in placed {
        add(&scene.meshes[i.mesh], i.placement, i.matrix);
    }
    if scene_id != SLICE_SCENE {
        let n = level.instances.len();
        return Ok((level, json!([{ "nodes": (0..n).collect::<Vec<_>>() }])));
    }
    let canister = (scene.meshes.iter())
        .find(|m| m.path == [0x03, 0x55])
        .context("0x2D: no canister model")?;
    add(canister, None, IDENTITY);
    let n = level.instances.len() - 1;
    let scenes = json!([{ "nodes": (0..n).collect::<Vec<_>>() }, { "nodes": [n] }]);
    Ok((level, scenes))
}

/// A map's lights (`maps/<map>/lights.json`): on 0x2D War: Total War's, on 0x2B War: Beach's, on
/// 0x1A the level's own, else none. The level's own
/// light table is not located, so these are constants (V: `../lab/research/mp-original/
/// total-war-effects.md` §1, §3, the oracle's light objects and RSP lights): the viewports' ambient
/// light; the shelter's two white lamps (kind 0x23: on but during the countdown, 0xBA's `obj` and
/// `obj2` 0xE016; size 55, behaviour 4: they flicker, `flicker`), its red alarm light (kind 0x24, behaviour 1:
/// the countdown only, 85 + 55 sin φ, φ += 0.1021 a frame); a green light on each canister at home
/// (`obj(0xE017)`, behaviour 17: 60 + 30 sin φ, φ += 0.2021, at the prop) and one on its carrier
/// (kind 0x10, size 30). On 0x2B War: Beach's night (V the oracle's light objects and RSP lights,
/// `../lab/research/mp-original/beach-presentation.md` §7; `calm`: all match): the bluish ambient;
/// two white lamps (behaviour 4, kind 0xE1A, size 50: they flicker; the map's two emitters are
/// their buzz); the exits' two red lights (behaviour 1, kind 0x503: 87 + 43 sin φ, φ += 0.1021).
/// On 0x1A (V dm3, dm15, hs7, lt3: `../lab/oracle/map1a/props_pickups_lights.md` §3) its 34 type-9
/// triggers in order, the level drawn lit (every vertex) with ambient (94, 94, 96): at the record's
/// position, of its +0x1B, +0x1A, +0x19 (B, G, R stored), by its command (+0x20): 0 static of size
/// +0x1E; 1 the bases' pulse (behaviour 1: 50 + 40 sin φ, φ += 0.1021, from a random phase; the
/// size byte unused); 0x14 a flickering lamp of size 70 (behaviour 4, the byte unused); 8-0xC
/// switching on and off at random (behaviour 255, of size +0x1E, `switching`). On 0x24 (V j1 DL 60,
/// `../lab/research/mp-original/tank-pack-scratch/lit24.py`) as on 0x1A: its 16 type-9 triggers
/// (0x01-0x10, all static), the level drawn lit with ambient (19, 68, 219); the tanks' own two
/// lights (white ahead, red behind while braking) are the client's (M10).
fn map_lights(rom: &Rom, scene: usize) -> Result<assets::Lights> {
    use assets::{Light, When};
    let light = |at, rgb, (size, swing, step), when| Light {
        at,
        rgb,
        size,
        swing,
        step,
        when,
        flicker: false,
    };
    let lamp = |at, size| Light {
        flicker: true,
        ..light(at, [255; 3], (size, 0.0, 0.0), When::Calm)
    };
    if matches!(scene, VAULT | TANK_MAP) {
        let triggers = rom.asset(&[0x0C, scene, 5])?;
        let mut out = assets::Lights {
            ambient: if scene == VAULT {
                [94, 94, 96]
            } else {
                [19, 68, 219]
            },
            ..Default::default()
        };
        let lights = (triggers.as_chunks::<0x34>().0.iter()).filter(|r| r[0x15] >> 2 == 9);
        for r in lights {
            let at = [0, 2, 4].map(|o| f32::from(i16::from_be_bytes([r[o], r[o + 1]])));
            let (rgb, size) = ([r[0x1B], r[0x1A], r[0x19]], f32::from(r[0x1E]));
            let fixed = light(at, rgb, (size, 0.0, 0.0), When::Calm);
            out.lights.push(
                match u32::from_be_bytes(r[0x20..0x24].try_into().unwrap()) {
                    0 => fixed,
                    1 => light(at, rgb, (50.0, 40.0, 0.1021), When::Calm),
                    0x14 => Light {
                        flicker: true,
                        ..light(at, rgb, (70.0, 0.0, 0.0), When::Calm)
                    },
                    8..=0xC => {
                        out.switching.push(out.lights.len());
                        fixed
                    }
                    c => anyhow::bail!("{scene:#x}: light command {c:#x}"),
                },
            );
        }
        return Ok(out);
    }
    if scene == BEACH {
        let exit = |at| light(at, [255, 0, 0], (87.0, 43.0, 0.1021), When::Calm);
        return Ok(assets::Lights {
            ambient: [0x70, 0x8D, 0xD5],
            lights: vec![
                lamp([2616.0, -236.0, 8004.0], 50.0),
                lamp([-3032.0, -327.0, 8306.0], 50.0),
                exit([727.0, -952.0, -3710.0]),
                exit([-1431.0, -952.0, -3643.0]),
            ],
            ..Default::default()
        });
    }
    if scene != SLICE_SCENE {
        return Ok(assets::Lights::default());
    }
    let (red, green) = ([255, 0, 0], [0, 255, 0]);
    let canister = (60.0, 30.0, 0.2021);
    let mut lights = vec![
        lamp([650.0, -1191.0, 486.0], 55.0),
        lamp([-1101.0, -1201.0, -229.0], 55.0),
        light(
            [-149.0, -1421.0, -43.0],
            red,
            (85.0, 55.0, 0.1021),
            When::Countdown,
        ),
    ];
    for (placement, at) in conker_rules::gas::CANISTERS {
        lights.push(light(at, green, canister, When::Shown(placement)));
    }
    lights.push(light([0.0; 3], green, (30.0, 0.0, 0.0), When::Carried));
    Ok(assets::Lights {
        ambient: [50, 50, 52],
        lights,
        ..Default::default()
    })
}

/// Bank-03 model 0x33 (class 51): 0x2D's quads at the centre (placements 40-45) and 0x34's 29 and
/// 30, solid in the match (`lab/oracle/walls_README.md`) but not drawn (V quad3 DL 130: slot 0's
/// camera facing placement 40 from 700 units, no batch within 300 of it; I on 0x34). Untextured,
/// they drew as white quads.
pub const UNDRAWN: [usize; 2] = [0x03, 0x33];

/// A map's level: the scene's ([`scene_level`]) without its [`hidden_placements`] and the
/// [`UNDRAWN`] model, its pickups' models moved to their spots where the game moves them off their
/// placements ([`weapon::pickups`]).
pub fn map_level(rom: &Rom, scene: usize) -> Result<Level> {
    let mut level = scene_level(rom, scene)?;
    let hidden = hidden_placements(scene);
    let meshes = &level.meshes;
    (level.instances).retain(|inst| {
        inst.placement.is_none_or(|i| !hidden.contains(&i))
            && !meshes[inst.mesh].path.starts_with(&UNDRAWN)
    });
    let (pickups, spots) = pickup_table(rom, scene)?;
    for (p, spot) in pickups.iter().zip(spots) {
        for inst in &mut level.instances {
            if inst.placement == Some(usize::from(p.placement)) {
                inst.matrix[12..15].copy_from_slice(&spot.map(f32::from));
            }
        }
    }
    // The runtime segments' first frames: Race's crowd, 0x1A's screens and lamps (lit).
    if scene == RACE_MAP {
        level.segments = crowd_frames(rom)?
            .into_iter()
            .map(|(s, ids)| (s, ids[0]))
            .collect();
    }
    if scene == VAULT {
        let ids = vault_ids(rom)?;
        level.segments = (VAULT_SCREENS.map(|(s, k)| (s, ids[k])).into_iter())
            .chain(VAULT_LAMPS.map(|s| (s, ids[VAULT_LIT])))
            .collect();
    }
    Ok(level)
}

/// Map 0x1A's 8 pickups in the manager's order (script 0xB7's items `G[0x1174..]` and handles
/// 0x3001-4, 0x3009-C, C; V dm6): k0-k3 the tommy gun, the bazooka, the knives and the crossbow,
/// which roam (spots 0-7); k4-k7 two sniper rifles and two grenade packs on fixed spots.
/// Not in conker-rules' `weapon::pickups` yet.
const VAULT_PICKUPS: [weapon::Pickup; 8] = [
    pick(10, Gives::Weapon(0x38)),
    pick(1, Gives::Weapon(0x16)),
    pick(2, Gives::Weapon(0x14)),
    pick(3, Gives::Weapon(0x18)),
    pick(8, Gives::Weapon(0x24)),
    pick(9, Gives::Weapon(0x24)),
    pick(11, Gives::Grenades),
    pick(12, Gives::Grenades),
];

const fn pick(placement: u16, gives: Gives) -> weapon::Pickup {
    weapon::Pickup { placement, gives }
}
/// Map 0x1A's pickup spots, the ids (byte 7) of nodes of `[0x19, 0x1A]` (`G[0x14F4]`, V), the
/// pickup exactly 60 above the node (V dm3, dm15, hs7: `../lab/oracle/map1a/props_pickups_lights.md`
/// §2): 0-7 where k0-k3 roam, 8-11 the fixed ones'.
const VAULT_SPOTS: [u8; 12] = [
    0x2E, 0x33, 0x23, 0x38, 5, 0xF, 0x16, 0x1B, 0x79, 0x7A, 0x7B, 0x7C,
];
/// The spots k0-k7 start on: k0-k3 the spot nearest their placement (the game picks a random free
/// one of 0-7 at the start, V: dm3 [3, 5, 6, 0], dm15 [6, 7, 4, 2], hs7 [0, 2, 7, 3]), k4-k7 spots
/// 8-11 (V; 11 and 12 each take the spot near the other's placement).
const VAULT_START: [usize; 8] = [0, 2, 3, 1, 8, 9, 10, 11];

/// Map 0x24's 4 power-ups in the manager's order (script 0xB8's handles 0x3001-0x3004, their bits
/// `G[0x1174..]`; tank.md §3, C): the shield `[3, 88]` (0x80, placement 16), the boost's
/// canisters `[3, 71]` (0x01, 8), the one-shot shells' container `[3, 81]` (0x10, 10) and the
/// scope's launcher `[3, 70]` (0x02, 11). `Gives::Weapon` holds the power-up's bit until
/// conker-rules has power-ups (M9).
const TANK_PICKUPS: [weapon::Pickup; 4] = [
    pick(16, Gives::Weapon(0x80)),
    pick(8, Gives::Weapon(0x01)),
    pick(10, Gives::Weapon(0x10)),
    pick(11, Gives::Weapon(0x02)),
];
/// Map 0x24's 6 power-up spots (`G[0xEA8]` = 6), ids of nodes of `[0x19, 0x24]`, the props 60
/// above (V p1, c1).
const TANK_SPOTS: [u8; 6] = [0x7B, 0x17, 0x40, 0x7C, 0x7D, 0x7E];
/// The spots the power-ups start on: random free ones (V with 2 humans: [1, 3, 2, 4]).
const TANK_START: [usize; 4] = [1, 3, 2, 4];

/// A map whose pickups the manager moves among spots (nodes of its navigation graph): its
/// pickups, the node ids of its spots, the spot each pickup starts on, how many (the first ones)
/// roam and among which spots, and the respawn in VIs.
struct Roaming {
    pickups: &'static [weapon::Pickup],
    spots: &'static [u8],
    start: &'static [usize],
    roamers: usize,
    roam: std::ops::Range<usize>,
    respawn: u16,
}

/// 0x1A's (respawn 300 VIs, MP+0xEAC, V dm6, not the other maps' 240) and 0x24's (300 VIs, 150
/// frames: `G[0xEAC]`, C, V p1).
fn roaming(scene: usize) -> Option<Roaming> {
    match scene {
        VAULT => Some(Roaming {
            pickups: &VAULT_PICKUPS,
            spots: &VAULT_SPOTS,
            start: &VAULT_START,
            roamers: 4,
            roam: 0..8,
            respawn: 300,
        }),
        TANK_MAP => Some(Roaming {
            pickups: &TANK_PICKUPS,
            spots: &TANK_SPOTS,
            start: &TANK_START,
            roamers: 4,
            roam: 0..6,
            respawn: 300,
        }),
        _ => None,
    }
}

/// The pickup spot on the node of navigation graph `nav` with id `id` (byte 7): 60 above it.
fn node_spot(nav: &[u8], id: u8) -> Result<[i16; 3]> {
    let count = usize::from(u16::from_be_bytes([nav[0], nav[1]]));
    let nodes = (nav
        .get(4..4 + 16 * count)
        .context("short navigation graph")?)
    .as_chunks::<16>()
    .0;
    let node = (nodes.iter().find(|r| r[7] == id)).with_context(|| format!("no node {id:#x}"))?;
    let [x, y, z] = [0, 2, 4].map(|o| i16::from_be_bytes([node[o], node[o + 1]]));
    Ok([x, y + 60, z])
}

/// A map's pickups and, when the game moves them off their placements, their spots (conker-rules'
/// `weapon::pickups`, [`roaming`]'s from the navigation graph, and 0x33's [`RACE_PICKUPS`]).
pub fn pickup_table(rom: &Rom, scene: usize) -> Result<(Vec<weapon::Pickup>, Vec<[i16; 3]>)> {
    if scene == RACE_MAP {
        return Ok((RACE_PICKUPS.to_vec(), vec![]));
    }
    if let Some(r) = roaming(scene) {
        let nav = rom.asset(&[0x19, scene])?;
        let spots = (r.start.iter())
            .map(|&k| node_spot(&nav, r.spots[k]))
            .collect::<Result<_>>()?;
        return Ok((r.pickups.to_vec(), spots));
    }
    let (pickups, spots) = weapon::pickups(scene as u16)
        .with_context(|| format!("no pickup table for scene {scene:#x}"))?;
    Ok((pickups.to_vec(), spots.to_vec()))
}

/// War: Raptor's pickups on 0x3F in the manager's order (script 0xBE 0x3B58: `G[0xEA4]` = 10, flags
/// 6, respawn `G[0xEAC]` = 240 VIs; C, V r9-r11): k0-k7 the bone crossbows (0x41) and knives (0x3F)
/// on placements 15-22, k8 and k9 grenades on 23 and 24 (Deathmatch's too).
const RAPTOR_PICKUPS: [(u16, assets::Gives); 10] = [
    (15, assets::Gives::Weapon(0x41)),
    (16, assets::Gives::Weapon(0x41)),
    (17, assets::Gives::Weapon(0x3F)),
    (18, assets::Gives::Weapon(0x3F)),
    (19, assets::Gives::Weapon(0x41)),
    (20, assets::Gives::Weapon(0x41)),
    (21, assets::Gives::Weapon(0x3F)),
    (22, assets::Gives::Weapon(0x3F)),
    (23, assets::Gives::Grenades),
    (24, assets::Gives::Grenades),
];
/// The placements a Raptor match hides (V r4: +0x6E set from read 24): 0-3 the cooked eggs till
/// one is used, 4-6 never shown, 7-14 Deathmatch's weapons.
const RAPTOR_HIDDEN: std::ops::Range<u16> = 0..15;
/// Script 0xBE's trigger records (C, `raptor.md` §3.1): the delivery (the cavemen's fire), the
/// feed (the nest's ledge, then its pit), the hazard (by the camp) and the raptor bots' jumps.
const RAPTOR_DELIVERY: usize = 0;
const RAPTOR_FEED: [usize; 2] = [1, 0x13];
const RAPTOR_HAZARD: [usize; 2] = [3, 0xD];
const RAPTOR_JUMPS: [usize; 4] = [4, 5, 6, 7];
/// The raptor's draw scale (V obj+0x14C, `../lab/research/mp-original/raptor-pack-scratch` s1).
const RAPTOR_SCALE: f32 = 0.7;
/// The raptor's skull joint (I: 316 of its vertices hang on it, its lower jaw's parent).
const RAPTOR_HEAD: usize = 5;
/// The clip-event op that arms an attack (`func_1506BEDC`: obj+0xD0 = the low byte, +0x114 = the
/// rest), and an attack entry's size.
const OP_ATTACK: u32 = 7;
const ATTACK: usize = 0x28;

/// `maps/3f/raptor.json` from the ROM: the scene's placements and character records, the raptor's
/// model header and class data (`[0x0F, 83]`: header word 1 its attack table; its route events).
fn raptor_mode(rom: &Rom) -> Result<assets::raptor::Raptor> {
    use assets::raptor::{Attack, Hit, Raptor, Record, Rig};
    let placements = scene_placements(rom, TEMPLE)?;
    let records = rom.asset(&[0x0E, TEMPLE])?;
    let record = |uid: u8| -> Result<Record> {
        let r = (records.as_chunks::<0x30>().0.iter())
            .find(|r| r[0x28] == uid)
            .with_context(|| format!("0x3F: no character record {uid:#x}"))?;
        let h = |o: usize| i16::from_be_bytes([r[o], r[o + 1]]);
        Ok(Record {
            character: r[4].to_string(),
            uid,
            at: [h(6), h(8), h(0xA)],
            yaw: r[0xC],
            start: r[0xD],
            scale: f32::from_be_bytes(r[0x20..0x24].try_into()?),
        })
    };
    // A cooked egg's prop id 0xFF − k is its placement's uid byte (C).
    let cooked = (0..4)
        .map(|k| {
            (placements.iter())
                .position(|p| p.raw[outro::PLACEMENT_UID] == 0xFF - k)
                .map(|i| i as u16)
                .with_context(|| format!("0x3F: no cooked egg {k}"))
        })
        .collect::<Result<_>>()?;
    let model = rom.asset(&[0x01, RAPTOR])?;
    let word = |data: &[u8], at: usize| -> Result<usize> {
        let w = data.get(at..at + 4).context("short raptor data")?;
        Ok(u32::from_be_bytes(w.try_into()?) as usize)
    };
    // Its special joint 0 (model header word 12: the list's offset), the jaw (C `func_1503DA3C`).
    let jaw = usize::from(*model.get(word(&model, 4 * 12)?).context("raptor: no jaw")?);
    let class = rom.asset(&[ROUTE_BANK, RAPTOR])?;
    let mut hits = Vec::new();
    for (clip, route) in parse_routes(&class)?.iter().enumerate() {
        if route.aux_offset == 0 {
            continue;
        }
        let (mut at, mut last) = (route.aux_offset as usize + 0x10, 0.0);
        loop {
            let t = f32::from_bits(word(&class, at)? as u32);
            if t >= 900.0 {
                break;
            }
            let t = extract_character::due(&mut last, t);
            let (op, arg) = (word(&class, at + 4)? as u32 & 0xFF, word(&class, at + 8)?);
            if op == OP_ATTACK {
                hits.push(Hit {
                    clip: clip as u16,
                    t,
                    attack: (arg & 0xFF).checked_sub(1),
                    window: (arg >> 8) as u8,
                });
            }
            at += 12;
        }
    }
    let table = word(&class, 4)?;
    let entries = (hits.iter().filter_map(|h| h.attack).max()).map_or(0, |a| a + 1);
    let attacks = (0..entries)
        .map(|k| {
            let e = (class.get(table + ATTACK * k..table + ATTACK * (k + 1)))
                .context("short attack table")?;
            Ok(Attack {
                reaction: e[0],
                sound: word(e, 4)? as u32,
                angle: e[9],
                distance: e[0xA],
                height: e[0xC] as i8,
                radius: e[0xE],
                damage: e[0xF],
                flags: word(e, 0x24)? as u32,
                raw: hex(e),
            })
        })
        .collect::<Result<_>>()?;
    let to = |r: &[usize]| r.to_vec();
    Ok(Raptor {
        hidden: RAPTOR_HIDDEN.collect(),
        cooked,
        pickups: (RAPTOR_PICKUPS.iter())
            .map(|&(placement, gives)| assets::Pickup {
                placement,
                gives,
                position: placements[usize::from(placement)].position.map(f32::from),
                respawn: Some(240),
                roams: vec![],
            })
            .collect(),
        spawns: conker_rules::spawn::RAPTOR_START_3F
            .map(|t| t.iter().map(|&r| usize::from(r)).collect()),
        delivery: RAPTOR_DELIVERY,
        feed: to(&RAPTOR_FEED),
        hazard: to(&RAPTOR_HAZARD),
        jumps: to(&RAPTOR_JUMPS),
        baby: record(BABY_UID)?,
        body: record(BODY_UID)?,
        eggs: EGG_UIDS.map(record).into_iter().collect::<Result<_>>()?,
        raptor: Rig {
            character: RAPTOR.to_string(),
            scale: RAPTOR_SCALE,
            head: RAPTOR_HEAD,
            jaw,
            carry: jaw,
            attacks,
            hits,
        },
    })
}

/// Race's power-ups on 0x33 in the script's order (script 0xBC's handles 0x3001-0x3007 at G+0x1D4,
/// the placements' uids 1-7; their types at G+0x20C; C, V L1): canisters `[3, 100]` (4, 5, 9) and
/// missiles `[3, 101]` (6, 7, 8, 10), as the item a racer holds (s126+0x75 less 0x80: 0x31, 0x33; V w4).
const RACE_PICKUPS: [weapon::Pickup; 7] = [
    pick(4, Gives::Weapon(0x31)),
    pick(5, Gives::Weapon(0x31)),
    pick(6, Gives::Weapon(0x33)),
    pick(7, Gives::Weapon(0x33)),
    pick(8, Gives::Weapon(0x33)),
    pick(9, Gives::Weapon(0x31)),
    pick(10, Gives::Weapon(0x33)),
];
/// VIs until a taken power-up is back: 9 frames (script 0xBC 0x16A8; V L1 541 → 551).
const RACE_RESPAWN: u16 = 18;
/// The bots' looks on [`RACER`], class → the display lists it hides (obj+0x94; C `func_15083384`,
/// V cls: 2 on the class-0xF bots only).
const RACER_LOOKS: [(u8, u8); 4] = [(0xC, 0), (0xD, 0), (0xE, 0), (0xF, 2)];
/// The routes: the scenario, the barriers it shows by uid (script 0xBC shows 0x30FE/0x30FD when the
/// route flag `0x8008FDBC & 0x40` is set, scenario 12, else 0x30FF/0x30FC; C, V L1, L2) and its
/// overview map (`hud.json`'s; C `func_150918EC`, V k1, b2: blobs 0xCF1, 0xCF2).
const RACE_ROUTES: [(u8, [u8; 2], &str); 2] = [
    (11, [0xFF, 0xFC], "race_map_low"),
    (12, [0xFE, 0xFD], "race_map_high"),
];
/// The start line's node id (script 0xBC's `command(0xF000, 0x11, 0x13, ..)`, C) and the one
/// behind it (the lap's last, V L1, L2).
const RACE_START: [u8; 2] = [0x13, 0x12];
/// The board's action (script 0xBC's player command 0x78 → `func_15097A8C` → `func_15083568(obj,
/// 0x79, ..)`; V k1, cls) and the helmets' by bot class (C `func_15083384`, V cls).
const RACE_BOARD: u32 = 0x79;
const RACE_HEADS: [(u8, u32); 3] = [(0xD, 0x76), (0xE, 0x78), (0xF, 0x77)];
/// The racers' classes: the humans' (model 136) and the bots' ([`RACER`]) (V cls).
const RACE_HUMAN: u8 = 0xB;
const RACE_BOTS: std::ops::RangeInclusive<u8> = 0xC..=0xF;
/// Race's crowd, 0x33's level materials on segments 4-7 (panels in the stands, drawn white when
/// unbound). Each frame, `func_150D765C` (scene 0x33's case in `func_1510B9D0`'s switch) binds
/// segment s (2-7) to texture `table[(k + 13 (s - 2)) % 5]`: the first of the two tables whose
/// addresses are at 0x80088900 for segments 2-5, the second for 6-7; k is the u16 at 0x800D9910
/// (`func_150D7630`: + 64 a VI) >> 8 (C; V cd 142-235, w1, w3b: segments 4-7 draw those ids, and no
/// 0x33 level material draws segments 2 and 3).
const CROWD_TABLES: usize = 0x8008_8900 - GAME_DATA;
const CROWD_SEGMENTS: std::ops::RangeInclusive<u32> = 4..=7;
/// VIs a crowd frame shows (256 / 64, C).
const CROWD_STEP: u8 = 4;
/// 0x1A's runtime segments: `func_15100464` (scene 0x1A's case in `func_1510B9D0`'s switch) binds
/// each frame, of the 5 texture ids at 0x80090324 ([`vault_ids`]), segments 2 and 3 (the computer
/// screens) to `ids[(t / 4 + j) % 3]`, j 0 and 1, t the u8 at 0x800DD405 (+ 1 a frame,
/// `func_15100340`), and segment 4 + k to `ids[3]` while switching light k (light command 8 + k) is
/// on, `ids[4]` while it is off (the byte at 0x800BE500 + k, toggled with the light) (C; V hud1-3,
/// bag_dl, bag_ap, bag_dl3, dm3, dm15, cs2, cs3: the dumps' segments 2-8 draw those ids).
const VAULT_IDS: usize = 0x8009_0324 - GAME_DATA;
const VAULT_SCREENS: [(u32, usize); 2] = [(2, 0), (3, 1)];
const VAULT_LAMPS: std::ops::RangeInclusive<u32> = 4..=8;
const VAULT_LIT: usize = 3;
/// VIs a screen frame shows: 4 frames of t, 2 VIs each (I: the multiplayer's 30 frames a second).
const VAULT_STEP: u8 = 8;

/// Race's board, then the helmets of [`RACE_HEADS`], as their actions attach them (one bank-09
/// model on a matrix slot each): `props.gltf`'s scenes on 0x33.
pub fn race_attached(rom: &Rom) -> Result<Vec<Held>> {
    let game_data = rom.game_data()?;
    let actions = std::iter::once(RACE_BOARD).chain(RACE_HEADS.map(|h| h.1));
    actions
        .map(|a| {
            let records = action_records(&game_data, a)?;
            let &[r] = records.as_slice() else {
                anyhow::bail!("action {a:#x}: {} records", records.len())
            };
            Ok(Held {
                entry: usize::from(r[0]),
                object: r[3] == 0,
                joint: r[1],
                create: vec![],
                remove: vec![],
            })
        })
        .collect()
}

/// One lap of a route (`high`: scenario 12's) on navigation graph `nav`, node indices from the
/// start's: each next node is `func_150888A8`'s pick among the links (+9..+0xD) but the node it
/// came from, of those with +0xE 0: the first whose +0xF is not the other route's (1 on 12, 2 on
/// 11), or a later one marked for a route (+0xF not 0); else one of the other route's (C; V L1, L2:
/// the oracle's race-table nodes).
fn race_lap(nav: &[u8], high: bool) -> Result<Vec<u8>> {
    let count = usize::from(u16::from_be_bytes([nav[0], nav[1]]));
    let nodes = (nav.get(4..4 + 16 * count))
        .context("short navigation graph")?
        .as_chunks::<16>()
        .0;
    let node = |i: u8| {
        nodes
            .get(usize::from(i))
            .with_context(|| format!("no node {i}"))
    };
    let index = |id: u8| {
        (nodes.iter().position(|r| r[7] == id))
            .map(|i| i as u8)
            .with_context(|| format!("no node id {id:#x}"))
    };
    let other = if high { 1 } else { 2 };
    let (start, mut prev) = (index(RACE_START[0])?, index(RACE_START[1])?);
    let mut lap = vec![start];
    loop {
        let at = lap[lap.len() - 1];
        let (mut pick, mut fallback) = (None, None);
        for (k, &l) in node(at)?[9..14].iter().enumerate() {
            if l == 0xFF || l == prev || node(l)?[0xE] != 0 {
                continue;
            }
            if node(l)?[0xF] == other {
                fallback = Some(l);
            } else if k == 0 || pick.is_none() || node(l)?[0xF] != 0 {
                pick = Some(l);
            }
        }
        let next = (pick.or(fallback)).with_context(|| format!("node {at}: no way on"))?;
        if next == start {
            return Ok(lap);
        }
        anyhow::ensure!(lap.len() < count, "the lap does not come back to its start");
        prev = at;
        lap.push(next);
    }
}

/// Each crowd segment ([`CROWD_SEGMENTS`]) and its 5 texture ids in the order shown: the original
/// draws `ids[k % 5]`.
pub fn crowd_frames(rom: &Rom) -> Result<Vec<(u32, [u32; 5])>> {
    let data = rom.game_data()?;
    let word = |at: usize| -> Result<usize> {
        let w = data.get(at..at + 4).context("short crowd tables")?;
        Ok(u32::from_be_bytes(w.try_into()?) as usize)
    };
    (CROWD_SEGMENTS.map(|s| {
        let at = word(CROWD_TABLES + 4 * usize::from(s >= 6))?;
        let table = at.checked_sub(GAME_DATA).context("crowd table")?;
        let id = |j: usize| -> Result<u32> {
            Ok(word(table + 4 * ((j + 13 * (s as usize - 2)) % 5))? as u32)
        };
        Ok((s, [id(0)?, id(1)?, id(2)?, id(3)?, id(4)?]))
    }))
    .collect()
}

/// Race's crowd for `race.json`, each panel's `level.gltf` material and frames, and the frames'
/// PNGs (paths relative to the map's folder, named as the level's).
pub fn race_crowd(rom: &Rom, textures: &Textures) -> Result<(assets::race::Crowd, Images)> {
    let (level, names) = level_names(rom, RACE_MAP, textures)?;
    let (mut panels, mut images) = (Vec::new(), Images::new());
    for (s, ids) in crowd_frames(rom)? {
        let (materials, frames) = segment_frames(&level, &names, s, &ids, textures, &mut images)?;
        let [material] = <[String; 1]>::try_from(materials)
            .map_err(|m| anyhow::anyhow!("0x33: segment {s}: {} materials", m.len()))?;
        panels.push(assets::race::Panel { material, frames });
    }
    let step = CROWD_STEP;
    Ok((assets::race::Crowd { step, panels }, images))
}

/// A map's level ([`map_level`]) and its `level.gltf` material names.
fn level_names(rom: &Rom, scene: usize, textures: &Textures) -> Result<(Level, Vec<String>)> {
    let level = map_level(rom, scene)?;
    let gltf: Value = serde_json::from_str(&to_gltf(&level, textures, "level.bin")?.json)?;
    let names = (gltf["materials"].as_array().into_iter().flatten())
        .filter_map(|m| Some(m["name"].as_str()?.to_string()))
        .collect();
    Ok((level, names))
}

/// The `level.gltf` materials (`names`) of `level`'s draws on runtime segment `s`, and each of
/// `ids` as they draw it: its PNG's path relative to the map's folder (the file added to `images`
/// once).
fn segment_frames(
    level: &Level,
    names: &[String],
    s: u32,
    ids: &[u32],
    textures: &Textures,
    images: &mut Images,
) -> Result<(Vec<String>, Vec<String>)> {
    use extract_gbi::Image;
    let on_s = |m: &Material| match m.texture.and_then(|t| t.image) {
        Some(Image::Segment(a)) if a >> 24 == s => Some(a),
        _ => None,
    };
    let draws = || level.meshes.iter().flat_map(|m| &m.ir.draws);
    let drawn = (draws().map(|d| d.material).find(|m| on_s(m).is_some()))
        .with_context(|| format!("no segment-{s} material"))?;
    let suffixes: BTreeSet<_> = (draws().filter_map(|d| on_s(&d.material)))
        .map(|a| format!("-seg{a:08x}"))
        .collect();
    let materials = (names.iter())
        .filter(|n| suffixes.iter().any(|x| n.ends_with(x)))
        .cloned()
        .collect();
    let mut frames = Vec::new();
    for &id in ids {
        let blobs = [(id, textures.get(id as usize)?)].into();
        let (stem, png) = extract_level::texture_png(&bind_material(drawn, s, id), &blobs)
            .with_context(|| format!("segment {s}: texture {id}"))??;
        let file = format!("textures/{stem}.png");
        if !images.iter().any(|(f, _)| *f == file) {
            images.push((file.clone(), png));
        }
        frames.push(file);
    }
    Ok((materials, frames))
}

/// The 5 texture ids 0x1A's runtime segments draw ([`VAULT_IDS`]).
pub fn vault_ids(rom: &Rom) -> Result<[u32; 5]> {
    let data = rom.game_data()?;
    let w = data
        .get(VAULT_IDS..VAULT_IDS + 20)
        .context("short .game data")?;
    Ok(std::array::from_fn(|k| {
        u32::from_be_bytes(w[4 * k..4 * k + 4].try_into().unwrap())
    }))
}

/// A map's `segments.json` and the frames' PNGs (named as the level's): 0x1A's screens and its
/// switching lights' lamps (a lamp's segment by its light's command, [`VAULT_IDS`]); empty elsewhere.
pub fn map_segments(
    rom: &Rom,
    scene: usize,
    textures: &Textures,
) -> Result<(assets::Segments, Images)> {
    let mut images = Images::new();
    if scene != VAULT {
        return Ok((assets::Segments::default(), images));
    }
    let ids = vault_ids(rom)?;
    let (level, names) = level_names(rom, scene, textures)?;
    let mut frames_of =
        |s, ids: &[u32]| segment_frames(&level, &names, s, ids, textures, &mut images);
    let mut panels = Vec::new();
    for (s, k) in VAULT_SCREENS {
        let (materials, frames) = frames_of(s, &[ids[k], ids[(k + 1) % 3], ids[(k + 2) % 3]])?;
        let [material] = <[String; 1]>::try_from(materials)
            .map_err(|m| anyhow::anyhow!("0x1A: segment {s}: {} materials", m.len()))?;
        panels.push(assets::race::Panel { material, frames });
    }
    let triggers = rom.asset(&[0x0C, scene, 5])?;
    let commands = (triggers.as_chunks::<0x34>().0.iter())
        .filter(|r| r[0x15] >> 2 == 9)
        .map(|r| u32::from_be_bytes(r[0x20..0x24].try_into().unwrap()))
        .filter(|c| (8..=0xC).contains(c));
    let mut lamps = Vec::new();
    for c in commands {
        let (materials, frames) = frames_of(4 + c - 8, &ids[VAULT_LIT..])?;
        let [on, off] = <[String; 2]>::try_from(frames).expect("a frame per id");
        lamps.push(assets::Lamp { materials, on, off });
    }
    let screens = Some(assets::race::Crowd {
        step: VAULT_STEP,
        panels,
    });
    Ok((assets::Segments { screens, lamps }, images))
}

/// `maps/33/race.json` from the ROM: the scene's placements (the barriers by uid), its navigation
/// graph (the laps), the board's and helmets' actions (their joints) and the crowd.
pub fn race_mode(rom: &Rom) -> Result<assets::race::Race> {
    use assets::race::{Attached, Race, Route};
    let placements = scene_placements(rom, RACE_MAP)?;
    let barrier = |uid: u8| {
        (placements.iter())
            .position(|p| p.raw[outro::PLACEMENT_UID] == uid)
            .map(|i| i as u16)
            .with_context(|| format!("0x33: no barrier {uid:#x}"))
    };
    let nav = rom.asset(&[0x19, RACE_MAP])?;
    let routes = (RACE_ROUTES.iter())
        .map(|&(scenario, uids, map)| {
            Ok(Route {
                scenario,
                shown: uids.into_iter().map(barrier).collect::<Result<_>>()?,
                lap: race_lap(&nav, scenario == 12)?,
                map: map.into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut barriers: Vec<u16> = routes.iter().flat_map(|r| r.shown.clone()).collect();
    barriers.sort_unstable();
    let attached = race_attached(rom)?;
    let at = |k: usize| Attached {
        scene: k,
        joint: attached[k].joint,
    };
    let bots = RACE_BOTS.map(|c| (c, RACER.to_string()));
    Ok(Race {
        barriers,
        routes,
        racers: std::iter::once((RACE_HUMAN, CAVEMAN.to_string()))
            .chain(bots)
            .collect(),
        board: at(0),
        heads: (RACE_HEADS.iter().enumerate())
            .map(|(k, h)| (h.0, at(k + 1)))
            .collect(),
        crowd: Some(race_crowd(rom, &rom.textures()?)?.0),
    })
}

/// A map's AI script (`[0x14, id]`): conker-bots' `script`, 0x1A's [`VAULT_AI`], 0x24's
/// [`TANK_AI`] and 0x33's [`RACE_AI`].
pub fn ai_script(scene: usize) -> usize {
    match scene {
        VAULT => VAULT_AI,
        TANK_MAP => TANK_AI,
        RACE_MAP => RACE_AI,
        _ => conker_bots::script(scene),
    }
}

/// A map's data from the ROM, as the pack keeps it: what the server and the client read from the
/// ROM (the collision, spawn records, pickups on their placements or spots, trigger table,
/// navigation graph, clips, held models, acos table and the bots' scripts), the sink table and
/// model lifts they compile in, and its War `outros` (0x2D's, 0x30's).
fn pack_data(
    rom: &Rom,
    scene: usize,
    characters: &[Character],
    faces: &[(Vec<assets::FaceSegment>, Images)],
    held: &[Held],
    outros: &[assets::outro::Outro],
    segments: assets::Segments,
) -> Result<assets::Slice> {
    let placements = scene_placements(rom, scene)?;
    let (pickups, spots) = pickup_table(rom, scene)?;
    let nav = rom.asset(&[0x19, scene])?.into_owned();
    // The roaming pickups' spots (0x1A's k0-k3, 0x24's four) and their respawn.
    let roaming = roaming(scene);
    let roams: Vec<[f32; 3]> = match &roaming {
        Some(r) => (r.roam.clone())
            .map(|k| Ok(node_spot(&nav, r.spots[k])?.map(f32::from)))
            .collect::<Result<_>>()?,
        None => vec![],
    };
    let game_data = rom.game_data()?;
    let table = |at: usize, n: usize| game_data.get(at..at + n).context(".game data too short");
    let sink = table(SINK_AT, 96)?;
    let solid = solid_placements(scene);
    let water = deathmatch_water(scene);
    Ok(assets::Slice {
        collision: (scene_collision(rom, scene, &placements)?.triangles.iter())
            .filter(|t| !matches!(t.layer, Layer::Placement(i) if !solid.contains(&i)))
            .map(|t| assets::Triangle {
                points: t.points,
                layer: match t.layer {
                    Layer::Terrain => assets::Layer::Terrain,
                    Layer::Secondary => assets::Layer::Secondary,
                    Layer::Placement(i) if water.contains(&i) => assets::Layer::Water(i),
                    Layer::Placement(i) => assets::Layer::Placement(i),
                },
                surface: t.surface,
            })
            .collect(),
        // The sea's current (War: Beach's, `func_15059444`'s path).
        current: match water.is_empty() {
            true => vec![],
            false => scene_path(rom, scene, 1)?,
        },
        spawns: (scene_spawns(rom, scene)?.iter())
            .map(|s| assets::SpawnPoint {
                position: s.position,
                byte_06: s.byte_06,
                yaw: s.yaw,
                camera: s.camera,
                byte_14: s.byte_14,
                byte_15: s.byte_15,
            })
            .collect(),
        // On 0x2D as the server's `Arena::deathmatch` places them (and Total War's gas masks).
        pickups: (pickups.iter().enumerate())
            .map(|(k, p)| assets::Pickup {
                placement: p.placement,
                gives: match p.gives {
                    Gives::Weapon(id) => assets::Gives::Weapon(id),
                    Gives::Grenades => assets::Gives::Grenades,
                    Gives::Mask => assets::Gives::Mask,
                    // Tank's power-ups: the pack keeps their bits as weapons.
                    Gives::Power(bit) => assets::Gives::Weapon(bit),
                },
                position: (spots.get(k).copied())
                    .unwrap_or(placements[usize::from(p.placement)].position)
                    .map(f32::from),
                respawn: (roaming.as_ref().map(|r| r.respawn))
                    .or((scene == RACE_MAP).then_some(RACE_RESPAWN)),
                roams: match roaming.as_ref().is_some_and(|r| k < r.roamers) {
                    true => roams.clone(),
                    false => vec![],
                },
            })
            .collect(),
        // conker-bots' `Assets::paths`.
        triggers: rom.asset(&[0x0C, scene, 5])?.into_owned(),
        nav,
        lights: map_lights(rom, scene)?,
        outros: outros.to_vec(),
        segments,
        ai: rom.asset(&[0x14, ai_script(scene)])?.into_owned(),
        class: rom.asset(&[0x0F, conker_bots::class(scene)])?.into_owned(),
        characters: (characters.iter().zip(faces))
            .map(|(c, f)| character_file(rom, c, &f.0))
            .collect::<Result<_>>()?,
        held: (held.iter())
            .map(|h| assets::Held {
                entry: h.entry,
                joint: h.joint,
                create: h.create.clone(),
                remove: h.remove.clone(),
            })
            .collect(),
        acos: (table(ACOS_AT, 2 * ACOS_LEN)?.as_chunks::<2>().0.iter())
            .map(|&b| u16::from_be_bytes(b))
            .collect(),
        sink: std::array::from_fn(|k| std::array::from_fn(|j| sink[3 * k + j] as i8)),
    })
}

/// `characters/<entry>/character.json` of a packed character.
fn character_file(
    rom: &Rom,
    c: &Character,
    faces: &[assets::FaceSegment],
) -> Result<assets::Character> {
    let (clips, root_clips) = clips(c);
    let pick = PICKS.contains(&c.entry);
    let player = (pick || PLAYERS.contains(&c.entry))
        .then(|| hitbox_body(rom, c.entry))
        .transpose()?;
    Ok(assets::Character {
        model_lift: model_lift(rom, c.entry)?,
        clips,
        root_clips,
        hitbox: player.map(|p| p.0),
        body: player.map(|p| p.1),
        clips_from: pick.then(|| SLICE_CHARACTER.to_string()),
        hidden: HIDDEN.iter().find(|h| h.0 == c.entry).map(|h| h.1),
        looks: (0x11..=0x20)
            .filter_map(|k| {
                villager_look(k)
                    .filter(|l| l.0 == c.entry)
                    .map(|l| (k, l.1))
            })
            .chain(RACER_LOOKS.into_iter().filter(|_| c.entry == RACER))
            .collect(),
        teams: (matches!(c.entry, WISE_GUY | TANK))
            .then(|| team_material(c, &team_ids(rom, c.entry)?))
            .transpose()?,
        tank: (c.entry == TANK).then(|| tank_parts(rom, c)).transpose()?,
        faces: faces.to_vec(),
    })
}

/// Model units bank-01 model `entry` is drawn above its player: +6 of its bank-0x11 record, copied
/// to obj+0xC8 at spawn (`func_150837D4`; V: 43 for the Tedi, 42 for the squirrel). The asset
/// starts with the record's offset.
fn model_lift(rom: &Rom, entry: usize) -> Result<i16> {
    let model = rom.asset(&[0x11, entry])?;
    let lift = (model.get(..4))
        .map(|w| u32::from_be_bytes(w.try_into().unwrap()) as usize + 6)
        .and_then(|at| model.get(at..at + 2))
        .context("short bank-0x11 record")?;
    Ok(i16::from_be_bytes([lift[0], lift[1]]))
}

/// Model `entry`'s hitbox (radius, half height, centre) and push body (radius, height, centre) at
/// full size: +0x20/+0x22/+0x24 and +0x1A/+0x1C/+0x1E of its bank-0x11 record (C
/// `characters-scratch/rec11.py`; × the class's scale, truncated, they are obj+0xD2-D6 and
/// +0xE4-E8 for every pick, V `bodies.py`).
pub fn hitbox_body(rom: &Rom, entry: usize) -> Result<([i16; 3], [i16; 3])> {
    let model = rom.asset(&[0x11, entry])?;
    let r = (model.get(..4))
        .map(|w| u32::from_be_bytes(w.try_into().unwrap()) as usize)
        .and_then(|at| model.get(at + 0x1A..at + 0x26))
        .context("short bank-0x11 record")?;
    let h = |k: usize| i16::from_be_bytes([r[2 * k], r[2 * k + 1]]);
    Ok(([h(3), h(4), h(5)], [h(0), h(1), h(2)]))
}

/// (relative path, PNG file) of a glTF's images.
type Images = Vec<(String, Vec<u8>)>;

/// A glTF whose images are buffer views, with each image as the PNG file
/// `textures/<stem>-<i>.png` instead and the buffer without them (views renumbered, 4-aligned).
fn external_images(mut json: Value, bin: &[u8], stem: &str) -> (Value, Vec<u8>, Images) {
    let views = json["bufferViews"].as_array().cloned().unwrap_or_default();
    let bytes = |v: &Value| {
        let start = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        &bin[start..start + v["byteLength"].as_u64().expect("views have a length") as usize]
    };
    let mut pngs = Vec::new();
    let mut image_views = Vec::new();
    for (i, image) in json["images"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let view = image["bufferView"].as_u64().expect("embedded images") as usize;
        let path = format!("textures/{stem}-{i:02}.png");
        pngs.push((path.clone(), bytes(&views[view]).to_vec()));
        image_views.push(view);
        *image = json!({ "uri": path });
    }
    let (mut out, mut kept, mut index) = (Vec::new(), Vec::new(), vec![None; views.len()]);
    for (i, view) in views.iter().enumerate() {
        if image_views.contains(&i) {
            continue;
        }
        out.resize(out.len().next_multiple_of(4), 0);
        let mut view = view.clone();
        view["byteOffset"] = out.len().into();
        out.extend_from_slice(bytes(&views[i]));
        index[i] = Some(kept.len());
        kept.push(view);
    }
    out.resize(out.len().next_multiple_of(4), 0);
    for accessor in json["accessors"].as_array_mut().into_iter().flatten() {
        let view = accessor["bufferView"]
            .as_u64()
            .expect("accessors have a view") as usize;
        accessor["bufferView"] = index[view].expect("accessors read no image").into();
    }
    json["bufferViews"] = kept.into();
    json["buffers"][0]["byteLength"] = out.len().into();
    (json, out, pngs)
}
