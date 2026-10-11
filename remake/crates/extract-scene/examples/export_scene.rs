//! Writes one scene's placements, spawn points and collision for inspection, plus the SHA-1s of
//! every bundle's terrain records in DevOldSchool's `model-assets collision` encoding:
//!
//! `CONKER_ROM=<rom> cargo run -p extract-scene --example export_scene -- <out dir> [scene] [x,y,z ...]`
//!
//! `scene` is hex (default 0x2D). Each `x,y,z` point (e.g. a position seen in the oracle) gets its
//! nearest spawn points and placements and the collision floor under it.

use extract_core::Rom;
use extract_scene::{
    Layer, floor_below, parse_terrain, scene_collision, scene_placements, scene_spawns,
};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

/// How far below a floor plane a point may be and still stand on it (spawn points on slopes sit
/// up to ~1.3 units under the plane).
const FLOOR_TOLERANCE: f32 = 2.0;
const USAGE: &str = "usage: export_scene <out dir> [scene] [x,y,z ...]";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(args.first().ok_or(USAGE)?);
    let scene = match args.get(1) {
        Some(s) => usize::from_str_radix(s.trim_start_matches("0x"), 16)?,
        None => 0x2D,
    };
    let points = args
        .iter()
        .skip(2)
        .map(|a| {
            let v: Vec<f32> = a.split(',').map(str::parse).collect::<Result<_, _>>()?;
            <[f32; 3]>::try_from(v).map_err(|_| format!("bad point {a}").into())
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let rom = Rom::new(std::fs::read(std::env::var_os("CONKER_ROM").ok_or(USAGE)?)?)?;
    std::fs::create_dir_all(&out)?;
    let write = |name: &str, text: String| {
        std::fs::write(out.join(format!("scene-{scene:02x}-{name}")), text)
    };
    let json_text = |v: Value| serde_json::to_string_pretty(&v).unwrap();

    let placements = scene_placements(&rom, scene)?;
    let spawns = scene_spawns(&rom, scene)?;
    let collision = scene_collision(&rom, scene, &placements)?;
    let tris = &collision.triangles;
    let floor = |p: [f32; 3]| match floor_below(tris, p, FLOOR_TOLERANCE) {
        Some((i, y)) => json!({
            "triangle": i, "layer": layer(tris[i].layer), "surface": tris[i].surface.map(word),
            "height": y, "gap": p[1] - y,
        }),
        None => Value::Null,
    };

    // Placements: [0x0B, scene] records (dispatch 1/2) come first, then [0x0C, scene, 2] (dispatch 0).
    let dispatched = placements.iter().filter(|p| p.dispatch != 0).count();
    let placement_json: Vec<Value> = placements
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let (source, record) = if i < dispatched {
                (json!([0x0B, scene]), i)
            } else {
                (json!([0x0C, scene, 2]), i - dispatched)
            };
            let model_path = match p.dispatch {
                0 => json!([0x03, p.model]),
                _ => json!([0x04, scene, p.model]),
            };
            json!({
                "index": i, "source": source, "record": record,
                "position": p.position, "rotation_deg": p.rotation, "scale": p.scale,
                "dispatch": p.dispatch, "model": p.model, "model_path": model_path,
                "flags": format!("{:#04x}", p.flags), "collision": p.has_collision(),
                "raw": hex(&p.raw),
            })
        })
        .collect();
    write("placements.json", json_text(json!(placement_json)))?;

    let spawn_json: Vec<Value> = spawns
        .iter()
        .enumerate()
        .map(|(i, s)| {
            json!({
                "index": i, "position": s.position, "yaw": s.yaw, "facing_u16": u16::from(s.yaw) << 8,
                "camera": s.camera, "byte_06": s.byte_06, "byte_14": s.byte_14, "byte_15": s.byte_15,
                "floor": floor(s.position.map(f32::from)),
            })
        })
        .collect();
    write("spawns.json", json_text(json!(spawn_json)))?;

    // Collision: OBJ in build order, one object per layer run; the JSON holds the per-face surfaces.
    let mut obj = format!("# scene {scene:#04x} collision, world space, faces in build order\n");
    let mut layers: Vec<Value> = Vec::new();
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    for (i, t) in tris.iter().enumerate() {
        if i == 0 || tris[i - 1].layer != t.layer {
            writeln!(obj, "o {}", layer(t.layer).replace(':', "_"))?;
            layers.push(json!({"layer": layer(t.layer), "first_face": i, "faces": 0}));
        }
        let run = layers.last_mut().unwrap();
        run["faces"] = json!(run["faces"].as_u64().unwrap() + 1);
        for p in t.points {
            writeln!(obj, "v {} {} {}", p[0], p[1], p[2])?;
        }
        writeln!(obj, "f {} {} {}", 3 * i + 1, 3 * i + 2, 3 * i + 3)?;
        if let Some(s) = t.surface {
            *histogram.entry(word(s)).or_default() += 1;
        }
    }
    write("collision.obj", obj)?;
    let surfaces: Vec<Option<String>> = tris.iter().map(|t| t.surface.map(word)).collect();
    write(
        "collision.json",
        json_text(json!({
            "scene": scene, "faces": tris.len(), "layers": layers,
            "unresolved_placements": collision.unresolved,
            "surface_histogram": histogram, "surfaces": surfaces,
        })),
    )?;

    // Oracle points: nearest spawn records and placements, and the floor under each.
    let dist = |a: [f32; 3], b: [i16; 3]| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - f32::from(y)).powi(2))
            .sum::<f32>()
            .sqrt()
    };
    let nearest = |p: [f32; 3], items: &mut dyn Iterator<Item = (usize, [i16; 3])>, n: usize| {
        let mut v: Vec<(usize, f32)> = items.map(|(i, q)| (i, dist(p, q))).collect();
        v.sort_by(|a, b| a.1.total_cmp(&b.1));
        v.truncate(n);
        v
    };
    let point_json: Vec<Value> = points
        .iter()
        .map(|&p| {
            let near_spawns = nearest(p, &mut spawns.iter().map(|s| s.position).enumerate(), 2);
            let near_placements = nearest(p, &mut placements.iter().map(|q| q.position).enumerate(), 3);
            json!({
                "point": p,
                "nearest_spawns": near_spawns.iter().map(|&(i, d)| json!({
                    "index": i, "position": spawns[i].position, "yaw": spawns[i].yaw, "distance": d,
                })).collect::<Vec<_>>(),
                "nearest_placements": near_placements.iter().map(|&(i, d)| {
                    let q = &placements[i];
                    json!({
                        "index": i, "position": q.position, "dispatch": q.dispatch, "model": q.model,
                        "flags": format!("{:#04x}", q.flags), "distance": d,
                    })
                }).collect::<Vec<_>>(),
                "floor": floor(p),
            })
        })
        .collect();
    write("points.json", json_text(json!(point_json)))?;

    // Every bundle's terrain in DevOldSchool's record encoding: per face three big-endian
    // model-relative vertex offsets (0x28 + 16 * index); surface words verbatim.
    let mut terrain = Vec::new();
    for s in 0..rom.bank(0x04)?.entries.len() {
        let Some(t) = parse_terrain(&rom.asset(&[0x04, s])?)? else {
            continue;
        };
        let records = |faces: &[[u32; 3]]| {
            sha(&faces
                .iter()
                .flatten()
                .flat_map(|v| (0x28 + 16 * v).to_be_bytes())
                .collect::<Vec<_>>())
        };
        terrain.push(json!({
            "scene": s, "faces": t.mesh.faces.len(), "triangle_records_sha1": records(&t.mesh.faces),
            "surface_records_sha1": t.surfaces.as_ref().map(|w| sha(&w.iter().flat_map(|w| w.to_be_bytes()).collect::<Vec<_>>())),
            "secondary_faces": t.secondary.as_ref().map(|m| m.faces.len()),
            "secondary_triangle_records_sha1": t.secondary.as_ref().map(|m| records(&m.faces)),
        }));
    }
    std::fs::write(
        out.join("bank04-terrain-records.json"),
        json_text(json!(terrain)),
    )?;

    println!(
        "scene {scene:#04x}: {} placements ({dispatched} dispatched), {} spawns, {} collision faces, {} unresolved",
        placements.len(),
        spawns.len(),
        tris.len(),
        collision.unresolved.len()
    );
    println!("layers: {}", json!(layers));
    println!("terrain surface histogram: {}", json!(histogram));
    for s in &spawn_json {
        println!(
            "spawn {} {} floor {}",
            s["index"], s["position"], s["floor"]
        );
    }
    for p in &point_json {
        println!("point {}", p);
    }
    Ok(())
}

fn layer(l: Layer) -> String {
    match l {
        Layer::Terrain => "terrain".into(),
        Layer::Secondary => "secondary".into(),
        Layer::Placement(i) => format!("placement:{i}"),
    }
}

fn word(w: u32) -> String {
    format!("{w:#010x}")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha(bytes: &[u8]) -> String {
    hex(&Sha1::digest(bytes))
}
