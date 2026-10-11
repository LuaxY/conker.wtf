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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Context, Result};
use conker_assets::{self as assets, ACOS_LEN, FORMAT, MANIFEST, Manifest};
pub use conker_assets::{ClipInfo, PACK_CHARACTERS, PACK_MAP, PACK_WEAPONS, RootClip};
use conker_rules::weapon::{self, Gives};
pub use extract_character::Held;
use extract_character::{
    Character, DEATHMATCH_PLAYER, FPS, SQUIRREL_SOLDIER, held_models, parse_attachment, parse_model,
};
use extract_core::{Rom, TEXTURE_COUNT, Textures, US_SHA1};
use extract_level::{
    Gltf, IDENTITY, Instance, Level, Mesh, Model, bundle_segments, scene_level, to_gltf,
};
pub use extract_scene::{Layer, SpawnPoint, Triangle};
use extract_scene::{floor_below, scene_collision, scene_placements, scene_spawns};
use serde_json::{Value, json};

/// The vertical slice's map: scene 0x2D, the oracle's Deathmatch scene.
pub const SLICE_SCENE: usize = 0x2D;
/// The vertical slice's character: the bank-01 model every Deathmatch participant uses.
pub const SLICE_CHARACTER: usize = DEATHMATCH_PLAYER;
/// The clips that put Deathmatch's held models in the hand: the draws of weapons 0x09, 0x39, 0x23,
/// 0x24, 0x15, 0x21, 0x22, 0x14 and 0x16, and the grenade's pull (conker-anim's ids).
pub const HELD_CLIPS: [u16; 10] = [
    0x7B, 0x1EE, 0x163, 0x152, 0x168, 0x120, 0x15D, 0x102, 0x76, 0x176,
];
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
    for e in 0..rom.bank(0x01)?.entries.len() {
        let data = rom.asset(&[0x01, e])?;
        if data.is_empty() {
            continue;
        }
        match parse_model(&data) {
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
/// origin), and one glTF scene per model listing its parts' nodes.
fn held_level(rom: &Rom) -> Result<(Vec<Held>, Level, Value)> {
    let held = held_models(rom, SLICE_CHARACTER, &HELD_CLIPS)?;
    let game_data = rom.game_data()?;
    let (mut models, mut scenes) = (Level::default(), Vec::new());
    for h in &held {
        let parts = parse_attachment(&rom.asset(&[0x09, h.entry])?, &game_data)?;
        let first = models.meshes.len();
        scenes.push(json!({ "nodes": (first..first + parts.len()).collect::<Vec<_>>() }));
        for (part, ir) in parts.into_iter().enumerate() {
            models.instances.push(Instance {
                mesh: models.meshes.len(),
                placement: None,
                matrix: IDENTITY,
            });
            models.meshes.push(Mesh {
                path: vec![0x09, h.entry, part],
                ir,
            });
        }
    }
    Ok((held, models, scenes.into()))
}

/// The held models' glTF, one glTF scene per held model: Bevy's `#Scene<i>` is `held[i]`.
fn held_gltf(models: &Level, scenes: Value, textures: &Textures, bin_uri: &str) -> Result<Gltf> {
    let mut gltf = to_gltf(models, textures, bin_uri)?;
    let mut json: Value = serde_json::from_str(&gltf.json)?;
    json["scenes"] = scenes;
    gltf.json = json.to_string();
    Ok(gltf)
}

/// The squirrels' soldier ([`SQUIRREL_SOLDIER`]) with the clips it plays: the Tedi's (`tedi`).
pub fn squirrel(rom: &Rom, tedi: &Character) -> Result<Character> {
    Ok(Character {
        entry: SQUIRREL_SOLDIER,
        anim_entry: tedi.anim_entry,
        model: parse_model(&rom.asset(&[0x01, SQUIRREL_SOLDIER])?)?,
        clips: tedi.clips.clone(),
    })
}

/// A map of the pack: its scene and a recording of its background (`../lab/audio`'s), if any.
pub struct PackMap<'a> {
    pub scene: usize,
    pub background: Option<&'a [u8]>,
}

/// A map's folder under `maps/`: its scene in lowercase hex (`2d`, `30`).
pub fn map_name(scene: usize) -> String {
    format!("{scene:x}")
}

/// The asset pack (`conker-extract pack`): the data files ([`conker_assets::Slice`]) of each map
/// (the first one also writes the shared files), their levels, the characters
/// ([`PACK_CHARACTERS`]: the squirrel soldier and the Tedi) and held models as `.gltf` + `.bin` +
/// PNG files, the sound ([`audio::pack_audio`]), `pack.json` and `README.md`.
pub fn pack(rom: &Rom, maps: &[PackMap]) -> Result<Export> {
    let textures = rom.textures()?;
    let tedi = Character::load(rom, SLICE_CHARACTER)?;
    let characters = [squirrel(rom, &tedi)?, tedi];
    let (held, models, scenes) = held_level(rom)?;
    let parse = |text: &str| serde_json::from_str::<Value>(text);
    let mut out = Export::default();
    let (mut gltfs, mut surfaces, mut summary) = (Vec::new(), BTreeSet::new(), Vec::new());
    let mut data = None;
    for (i, map) in maps.iter().enumerate() {
        let name = map_name(map.scene);
        let d = pack_data(rom, map.scene, &characters, &held)?;
        for (path, bytes) in d.write(&name, &PACK_CHARACTERS, PACK_WEAPONS)? {
            if i == 0 || path.starts_with("maps/") {
                out.add(path, bytes);
            }
        }
        let level = to_gltf(&map_level(rom, map.scene)?, &textures, "level.bin")?;
        gltfs.push((
            assets::level_gltf(&name),
            parse(&level.json)?,
            level.bin,
            level.images,
        ));
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
    for (name, c) in PACK_CHARACTERS.iter().zip(&characters) {
        let g = c.gltf(&textures, true)?;
        let (json, bin, images) = external_images(g.with_uri("character.bin"), &g.bin, "character");
        gltfs.push((assets::character_gltf(name), json, bin, images));
    }
    gltfs.push((
        assets::held_gltf(PACK_WEAPONS),
        parse(&held_models.json)?,
        held_models.bin,
        held_models.images,
    ));
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
        pngs += images.len();
        for (image, png) in images {
            out.add(format!("{dir}/{image}"), png);
        }
    }
    let backgrounds: Vec<_> = (maps.iter())
        .map(|m| (map_name(m.scene), m.background))
        .collect();
    let (sound_files, audio) = audio::pack_audio(rom, &surfaces, &backgrounds)?;
    let wavs = sound_files.iter().filter(|f| f.0.ends_with(".wav")).count();
    out.files.extend(sound_files);
    let (hud_files, hud) = hud::pack_hud(rom)?;
    out.files.extend(hud_files);
    out.files.extend(hud::pack_end(rom)?.0);
    let manifest = Manifest {
        format: FORMAT,
        source_rom_sha1: US_SHA1.into(),
        maps: maps.iter().map(|m| map_name(m.scene)).collect(),
        characters: PACK_CHARACTERS.map(String::from).into(),
        weapons: vec![PACK_WEAPONS.into()],
        audio,
        hud: true,
    };
    out.add(MANIFEST.into(), manifest.to_json());
    out.add("README.md".into(), PACK_README);
    let characters: Vec<String> = (PACK_CHARACTERS.iter().zip(&data.characters))
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

/// A map's level: the scene's ([`scene_level`]), its pickups' models moved to their spots where the
/// game moves them off their placements ([`weapon::pickups`]).
pub fn map_level(rom: &Rom, scene: usize) -> Result<Level> {
    let mut level = scene_level(rom, scene)?;
    let (pickups, spots) = pickup_table(scene)?;
    for (p, spot) in pickups.iter().zip(spots) {
        for inst in &mut level.instances {
            if inst.placement == Some(usize::from(p.placement)) {
                inst.matrix[12..15].copy_from_slice(&spot.map(f32::from));
            }
        }
    }
    Ok(level)
}

fn pickup_table(scene: usize) -> Result<(&'static [weapon::Pickup], &'static [[i16; 3]])> {
    weapon::pickups(scene as u16).with_context(|| format!("no pickup table for scene {scene:#x}"))
}

/// A map's data from the ROM, as the pack keeps it: what the server and the client read from the
/// ROM (the collision, spawn records, pickups on their placements or spots, trigger table,
/// navigation graph, clips, held models, acos table and the bots' scripts), and the sink table and
/// model lifts they compile in.
fn pack_data(
    rom: &Rom,
    scene: usize,
    characters: &[Character],
    held: &[Held],
) -> Result<assets::Slice> {
    let placements = scene_placements(rom, scene)?;
    let (pickups, spots) = pickup_table(scene)?;
    let game_data = rom.game_data()?;
    let table = |at: usize, n: usize| game_data.get(at..at + n).context(".game data too short");
    let sink = table(SINK_AT, 96)?;
    Ok(assets::Slice {
        collision: (scene_collision(rom, scene, &placements)?.triangles.iter())
            .map(|t| assets::Triangle {
                points: t.points,
                layer: match t.layer {
                    Layer::Terrain => assets::Layer::Terrain,
                    Layer::Secondary => assets::Layer::Secondary,
                    Layer::Placement(i) => assets::Layer::Placement(i),
                },
                surface: t.surface,
            })
            .collect(),
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
        // On 0x2D as the server's `Arena::deathmatch` places them.
        pickups: (pickups.iter().enumerate())
            .map(|(k, p)| assets::Pickup {
                placement: p.placement,
                gives: match p.gives {
                    Gives::Weapon(id) => assets::Gives::Weapon(id),
                    Gives::Grenades => assets::Gives::Grenades,
                },
                position: (spots.get(k).copied())
                    .unwrap_or(placements[usize::from(p.placement)].position)
                    .map(f32::from),
            })
            .collect(),
        // conker-bots' `Assets::paths`.
        triggers: rom.asset(&[0x0C, scene, 5])?.into_owned(),
        nav: rom.asset(&[0x19, scene])?.into_owned(),
        ai: rom.asset(&[0x14, 0xBA])?.into_owned(),
        class: rom.asset(&[0x0F, 0])?.into_owned(),
        characters: (characters.iter())
            .map(|c| {
                let (clips, root_clips) = clips(c);
                Ok(assets::Character {
                    model_lift: model_lift(rom, c.entry)?,
                    clips,
                    root_clips,
                })
            })
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
