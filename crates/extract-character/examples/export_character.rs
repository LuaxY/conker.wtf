//! Writes a character as glTF: `<stem>.gltf` (skinned mesh, skeleton, every playable clip),
//! `<stem>-bind.gltf` (bind pose only), each with its `.bin`, and `clips.json`.
//!
//! `CONKER_ROM=<rom> cargo run -p extract-character --example export_character -- <out dir> [entry]`
//!
//! `entry` is the bank-01 model (default 117, the Deathmatch player).

use extract_character::{Character, DEATHMATCH_PLAYER, FPS};
use extract_core::Rom;
use serde_json::json;
use std::path::PathBuf;

const USAGE: &str = "usage: export_character <out dir> [bank-01 entry]";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(args.first().ok_or(USAGE)?);
    let entry = match args.get(1) {
        Some(e) => e.parse()?,
        None => DEATHMATCH_PLAYER,
    };
    let rom = Rom::new(std::fs::read(std::env::var_os("CONKER_ROM").ok_or(USAGE)?)?)?;
    let character = Character::load(&rom, entry)?;
    let textures = rom.textures()?;
    std::fs::create_dir_all(&out)?;
    let stem = format!("bank01-{entry:04}");
    for (suffix, animations) in [("", true), ("-bind", false)] {
        let g = character.gltf(&textures, animations)?;
        let name = format!("{stem}{suffix}");
        let text = serde_json::to_string(&g.with_uri(&format!("{name}.bin")))?;
        std::fs::write(out.join(format!("{name}.gltf")), text)?;
        std::fs::write(out.join(format!("{name}.bin")), &g.bin)?;
        if animations {
            let j = &g.json;
            let prims = j["meshes"][0]["primitives"].as_array().unwrap();
            let count = |key: &str| -> u64 {
                prims
                    .iter()
                    .map(|p| {
                        j["accessors"][p["attributes"][key].as_u64().unwrap() as usize]["count"]
                            .as_u64()
                            .unwrap()
                    })
                    .sum()
            };
            let indices: u64 = prims
                .iter()
                .map(|p| {
                    j["accessors"][p["indices"].as_u64().unwrap() as usize]["count"]
                        .as_u64()
                        .unwrap()
                })
                .sum();
            let textured = prims
                .iter()
                .filter(|p| !p["attributes"]["TEXCOORD_0"].is_null())
                .count();
            let linked = j["materials"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| !m["pbrMetallicRoughness"]["baseColorTexture"].is_null())
                .count();
            println!(
                "{name}: {} joints, 1 mesh, {} primitives ({textured} with UVs, {linked} with a ROM texture), {} vertices, {} triangles, {} images, {} animations",
                character.model.joints.len(),
                prims.len(),
                count("POSITION"),
                indices / 3,
                j["images"].as_array().map_or(0, Vec::len),
                j["animations"].as_array().map_or(0, Vec::len),
            );
            let unresolved: Vec<_> = j["materials"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| {
                    m["pbrMetallicRoughness"]["baseColorTexture"].is_null()
                        && !m["extras"]["image"].is_null()
                })
                .map(|m| m["extras"]["image"].clone())
                .collect();
            println!("textured draws without a ROM texture: {unresolved:?}");
        }
    }

    let clips: Vec<_> = character
        .playable_clips()
        .enumerate()
        .map(|(i, c)| {
            json!({
                "animation": i, "pair": c.pair, "logical_ids": c.logical, "frames": c.frames.len(),
                "keyframe_step": c.step, "duration_ticks": c.duration,
                "duration_s": c.duration as f32 / FPS,
            })
        })
        .collect();
    let mut manifest = json!({
        "bank01_entry": entry,
        "bank02_entry": character.anim_entry,
        "route_bank_0f_entry": character.anim_entry,
        "joints": character.model.joints.len(),
        "fps": FPS,
        "pairs": character.clips.len(),
        "empty_pairs": character.clips.iter().filter(|c| c.frames.is_empty()).map(|c| c.pair).collect::<Vec<_>>(),
        "clips": clips,
    });
    if entry == DEATHMATCH_PLAYER {
        manifest["oracle_observed_logical_ids"] = json!({
            "source": "lab/work/mp/dm3*/dl_*.bin: character pool 0x800CC2D0 + 0x32C * i, u16 +0x84",
            "player_1_idle": [122, 89],
            "bots": [2, 124, 285, 338, 339, 343, 344],
        });
    }
    std::fs::write(
        out.join("clips.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    println!(
        "bank-02 entry {}: {} pairs, {} playable clips -> {}",
        character.anim_entry,
        character.clips.len(),
        clips.len(),
        out.display()
    );
    Ok(())
}
