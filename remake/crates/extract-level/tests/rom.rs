//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.
//! `matches_dos_reference` also needs env `CONKER_DOS_LEVEL` = the `level/` directory of
//! DevOldSchool/conkers-bfd-decomp's (MIT, e2400b27dda6) scene-45 outputs for this ROM
//! (`bank04/geometry/0045-NN.gltf`, `bank03/geometry/NNNN-00.gltf`, `assembly/scene-45.gltf`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use extract_core::Rom;
use extract_gbi::Image;
use extract_level::{Level, Mesh, Model, bundle_segments, primitive, scene_level, to_gltf};
use extract_scene::{Layer, scene_collision, scene_placements};
use serde_json::Value;

const SCENE: usize = 0x2D;

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

type Bounds = ([f32; 3], [f32; 3]);
const EMPTY: Bounds = ([f32::MAX; 3], [f32::MIN; 3]);

fn grow((lo, hi): Bounds, p: [f32; 3]) -> Bounds {
    (
        std::array::from_fn(|i| lo[i].min(p[i])),
        std::array::from_fn(|i| hi[i].max(p[i])),
    )
}

fn apply(m: &[f32; 16], v: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|r| (0..3).map(|c| m[c * 4 + r] * v[c]).sum::<f32>() + m[12 + r])
}

/// Per mesh: (vertices, faces, model-space bounds, texture id per face).
fn mesh_stats(mesh: &Mesh) -> (usize, usize, Bounds, Vec<Option<u32>>) {
    let (mut vertices, mut bounds, mut ids) = (0, EMPTY, Vec::new());
    for d in &mesh.ir.draws {
        let p = primitive(&mesh.ir, d);
        vertices += p.positions.len();
        bounds = p.positions.iter().copied().fold(bounds, grow);
        let id = match d.material.texture.and_then(|t| t.image) {
            Some(Image::Flat { index, .. }) => Some(index),
            _ => None,
        };
        ids.extend(std::iter::repeat_n(id, p.indices.len() / 3));
    }
    (vertices, ids.len(), bounds, ids)
}

/// World-space bounds of the instances `keep` selects.
fn world_bounds(level: &Level, keep: impl Fn(Option<usize>) -> bool) -> Bounds {
    let mut b = EMPTY;
    for inst in level.instances.iter().filter(|i| keep(i.placement)) {
        let mesh = &level.meshes[inst.mesh];
        for d in &mesh.ir.draws {
            for p in primitive(&mesh.ir, d).positions {
                b = grow(b, apply(&inst.matrix, p));
            }
        }
    }
    b
}

#[test]
fn scene_2d_counts() {
    let Some(rom) = rom() else { return };
    let bundle = rom.asset(&[0x04, SCENE]).unwrap();
    let segments = bundle_segments(&bundle).unwrap();
    assert_eq!(segments.len(), 16);
    let models: Vec<Model> = segments
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| Model::parse(s).unwrap())
        .collect();
    // DOS (summary.md): 15 non-empty segments, 3277 source vertices, 2009 G_TRI slots.
    assert_eq!(models.len(), 15);
    assert_eq!(models.iter().map(Model::vertex_count).sum::<usize>(), 3277);
    let slots: usize = models
        .iter()
        .map(|m| {
            let ir = m.walk().unwrap();
            assert!(
                ir.skipped.is_empty(),
                "uninterpreted opcodes {:x?}",
                ir.skipped
            );
            ir.draws.iter().map(|d| d.triangles.len()).sum::<usize>() + ir.degenerate as usize
        })
        .sum();
    assert_eq!(slots, 2009);

    let level = scene_level(rom, SCENE).unwrap();
    // Slots 0 and 1, 12 [0B] placements of segments 4-15, 34 [0C, 2] placements of 13 bank-03 models.
    assert_eq!(
        (
            level.instances.len(),
            level.meshes.len(),
            level.missing.len()
        ),
        (48, 27, 0)
    );
    let faces: BTreeMap<String, usize> = level
        .meshes
        .iter()
        .map(|m| (m.name(), mesh_stats(m).1))
        .collect();
    assert_eq!(faces["04-2d-00"], 1750); // 1751 records, one zero-area
    let total: usize = level
        .instances
        .iter()
        .map(|i| faces[&level.meshes[i.mesh].name()])
        .sum();
    // DOS's assembly (4178 faces) omits segment 13 (108 faces) and six bank-03 model-51 planes.
    assert_eq!(total, 4298);
    assert_eq!(total - 108 - 6 * faces["03-33"], 4178);
}

/// Width, height and RGBA8 of a PNG file.
fn rgba(file: &[u8]) -> (usize, usize, Vec<u8>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(file));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let px = |c: &[u8]| match c.len() {
        4 => [c[0], c[1], c[2], c[3]],
        3 => [c[0], c[1], c[2], 255],
        2 => [c[0], c[0], c[0], c[1]],
        _ => [c[0], c[0], c[0], 255],
    };
    let rgba = buf[..info.buffer_size()]
        .chunks(info.color_type.samples())
        .flat_map(px)
        .collect();
    (info.width as usize, info.height as usize, rgba)
}

/// (vertices, faces, bounds, texture id per face, flat ids with images) of a DOS glTF.
fn dos_stats(path: &Path) -> (usize, usize, Bounds, Vec<Option<u32>>, BTreeSet<u32>) {
    let g: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (mut vertices, mut bounds, mut ids, mut images) = (0, EMPTY, Vec::new(), BTreeSet::new());
    let f3 = |v: &Value| -> [f32; 3] { std::array::from_fn(|i| v[i].as_f64().unwrap() as f32) };
    for p in g["meshes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["primitives"].as_array().unwrap())
    {
        let pos = &g["accessors"][p["attributes"]["POSITION"].as_u64().unwrap() as usize];
        vertices += pos["count"].as_u64().unwrap() as usize;
        bounds = grow(grow(bounds, f3(&pos["min"])), f3(&pos["max"]));
        let material = &g["materials"][p["material"].as_u64().unwrap() as usize];
        // Names are `flat_NNNN_mode_M_...` or `untextured_...`.
        let id = material["name"]
            .as_str()
            .unwrap()
            .strip_prefix("flat_")
            .map(|s| s[..4].parse::<u32>().unwrap());
        if let (Some(id), Some(_)) = (id, material["pbrMetallicRoughness"].get("baseColorTexture"))
        {
            images.insert(id);
        }
        let faces = g["accessors"][p["indices"].as_u64().unwrap() as usize]["count"]
            .as_u64()
            .unwrap()
            / 3;
        ids.extend(std::iter::repeat_n(id, faces as usize));
    }
    (vertices, ids.len(), bounds, ids, images)
}

#[test]
fn matches_dos_reference() {
    let Some(rom) = rom() else { return };
    let Some(root) = std::env::var_os("CONKER_DOS_LEVEL") else {
        eprintln!("CONKER_DOS_LEVEL unset: skipping");
        return;
    };
    let root = Path::new(&root);
    let level = scene_level(rom, SCENE).unwrap();
    let gltf = to_gltf(&level, &rom.textures().unwrap(), "level.bin").unwrap();
    let ours_images: BTreeSet<u32> = gltf
        .images
        .iter()
        .map(|(path, _)| path["textures/".len()..][..4].parse().unwrap())
        .collect();

    let (mut dos_images, mut vertex_diffs) = (BTreeSet::new(), Vec::new());
    for mesh in &level.meshes {
        let file = match mesh.path[..] {
            [4, scene, s] => root.join(format!("bank04/geometry/{scene:04}-{s:02}.gltf")),
            [3, m] => root.join(format!("bank03/geometry/{m:04}-00.gltf")),
            _ => unreachable!(),
        };
        let (vertices, faces, bounds, ids) = mesh_stats(mesh);
        let (dos_vertices, dos_faces, dos_bounds, dos_ids, images) = dos_stats(&file);
        dos_images.extend(images);
        let name = mesh.name();
        eprintln!(
            "{name}: {vertices} vertices (DOS {dos_vertices}), {faces} faces (DOS {dos_faces}), \
             bounds {bounds:?}, {} textured faces",
            ids.iter().flatten().count()
        );
        assert_eq!(faces, dos_faces, "{name} faces");
        assert_eq!(bounds, dos_bounds, "{name} bounds");
        assert_eq!(ids, dos_ids, "{name} texture id per face");
        if vertices != dos_vertices {
            vertex_diffs.push((name, vertices, dos_vertices));
        }
    }
    // Bank-03 model 84: DOS repeats vertices within a primitive (its first two have 62 vertices
    // but 54 distinct positions each; the cause is in its vertex keying, not traced). Faces,
    // bounds and texture ids agree.
    assert_eq!(vertex_diffs, [("03-54".to_string(), 141, 203)]);
    eprintln!(
        "texture ids with images: ours {}, DOS {}",
        ours_images.len(),
        dos_images.len()
    );
    assert_eq!(ours_images, dos_images);

    // Pixels, from each family's textures.tsv (flat id -> PNG). Both write the last memory row
    // first (upright art); widths can differ (DOS uses the tile bounds, we the row stride), so
    // compare the common area.
    let mut dos_files = BTreeMap::new();
    for bank in ["bank03", "bank04"] {
        let dir = root.join(bank);
        let tsv = std::fs::read_to_string(dir.join("textures.tsv")).unwrap();
        for line in tsv.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            dos_files.insert(
                f[0].parse::<u32>().unwrap(),
                dir.join("textures").join(f[2]),
            );
        }
    }
    let (mut same_size, mut other_size) = (0, Vec::new());
    for (path, file) in &gltf.images {
        let id: u32 = path["textures/".len()..][..4].parse().unwrap();
        let (w, h, ours) = rgba(file);
        let (dw, dh, theirs) = rgba(&std::fs::read(&dos_files[&id]).unwrap());
        let mut off = 0;
        for y in 0..h.min(dh) {
            for x in 0..w.min(dw) * 4 {
                off = off.max(ours[y * w * 4 + x].abs_diff(theirs[y * dw * 4 + x]));
            }
        }
        // 5-bit channels: we replicate bits, DOS rounds v * 255 / 31 down.
        assert!(off <= 1, "{path}: channels differ by {off}");
        if (w, h) == (dw, dh) {
            same_size += 1;
        } else {
            other_size.push((id, [w, h], [dw, dh]));
        }
    }
    eprintln!("pixels match DOS: {same_size} same size, others {other_size:?}");
    assert_eq!(
        other_size,
        [(3419, [32, 64], [31, 64]), (2343, [64, 63], [63, 63])]
    );

    // Instance transforms against DOS's static assembly (it omits 4/45/13 and the 3/51 planes).
    let assembly: Value =
        serde_json::from_slice(&std::fs::read(root.join("assembly/scene-45.gltf")).unwrap())
            .unwrap();
    let dos_nodes: HashMap<String, Vec<f32>> = assembly["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let e = &n["extras"];
            let key = format!(
                "{}/{}/{}",
                e["model"], e["placement_bank"], e["record"]["index"]
            );
            let m = n["matrix"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32);
            (key, m.collect())
        })
        .collect();
    let dispatched = scene_placements(rom, SCENE)
        .unwrap()
        .iter()
        .filter(|p| p.dispatch != 0)
        .count();
    let (mut matched, mut unmatched) = (0, Vec::new());
    for inst in &level.instances {
        let path = &level.meshes[inst.mesh].path;
        let model = if path[0] == 3 {
            vec![3, path[1], 0]
        } else {
            path.clone()
        };
        let (bank, record) = match inst.placement {
            None => ("null".to_string(), "null".to_string()),
            Some(p) if p < dispatched => ("11".into(), p.to_string()),
            Some(p) => ("12".into(), (p - dispatched).to_string()),
        };
        let key = format!("{}/{bank}/{record}", serde_json::to_string(&model).unwrap());
        match dos_nodes.get(&key) {
            Some(theirs) => {
                let off = theirs
                    .iter()
                    .zip(inst.matrix)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0, f32::max);
                assert!(off < 1e-3, "{key}: matrices differ by {off}");
                matched += 1;
            }
            None => unmatched.push(key),
        }
    }
    eprintln!("instances matching DOS's assembly: {matched}; not in it: {unmatched:?}");
    assert_eq!((matched, dos_nodes.len(), unmatched.len()), (41, 41, 7));
}

#[test]
fn visual_bounds_enclose_collision() {
    let Some(rom) = rom() else { return };
    let level = scene_level(rom, SCENE).unwrap();
    let placements = scene_placements(rom, SCENE).unwrap();
    let collision = scene_collision(rom, SCENE, &placements).unwrap();
    let visual = world_bounds(&level, |_| true);

    let mut layers: Vec<(Layer, Bounds)> = Vec::new();
    for t in &collision.triangles {
        let i = match layers.iter().position(|l| l.0 == t.layer) {
            Some(i) => i,
            None => {
                layers.push((t.layer, EMPTY));
                layers.len() - 1
            }
        };
        layers[i].1 = t.points.iter().copied().fold(layers[i].1, grow);
    }
    let inside = |(lo, hi): Bounds, (vlo, vhi): Bounds, tol: f32| {
        (0..3).all(|i| lo[i] >= vlo[i] - tol && hi[i] <= vhi[i] + tol)
    };
    let mut outside = Vec::new();
    for &(layer, b) in &layers {
        let own = match layer {
            Layer::Terrain => Some(world_bounds(&level, |p| p.is_none())),
            Layer::Placement(i) => Some(world_bounds(&level, |p| p == Some(i))),
            Layer::Secondary => None, // segment 3: collision only, never drawn
        };
        if let Some(own) = own {
            assert!(
                inside(b, own, 0.01),
                "{layer:?}: collision {b:?} outside its visual {own:?}"
            );
        }
        if !inside(b, visual, 0.01) {
            outside.push((layer, b));
        }
    }
    let points: Vec<[f32; 3]> = collision.triangles.iter().flat_map(|t| t.points).collect();
    let out = points
        .iter()
        .filter(|p| !inside((**p, **p), visual, 0.01))
        .count();
    let all = points.iter().copied().fold(EMPTY, grow);
    eprintln!(
        "{} collision triangles, bounds {all:?}; visual bounds {visual:?}; \
         {out}/{} collision vertices outside the visual bounds; layers outside: {outside:?}",
        collision.triangles.len(),
        points.len(),
    );
    // Only the secondary layer (an invisible wall/ceiling up to y = 2055) leaves the visual
    // bounds; the visual top is the terrain at y = 1623.
    assert_eq!(collision.triangles.len(), 4299);
    assert!(outside.iter().all(|(l, _)| *l == Layer::Secondary));
}

#[test]
fn exports_scene_2d_gltf() {
    let Some(rom) = rom() else { return };
    let level = scene_level(rom, SCENE).unwrap();
    let gltf = to_gltf(&level, &rom.textures().unwrap(), "level.bin").unwrap();
    assert!(gltf.warnings.is_empty(), "{:?}", gltf.warnings);
    let g: Value = serde_json::from_str(&gltf.json).unwrap();
    assert_eq!(g["nodes"].as_array().unwrap().len(), 48);
    assert_eq!(g["meshes"].as_array().unwrap().len(), 27);
    assert_eq!(gltf.images.len(), 144);
    for view in g["bufferViews"].as_array().unwrap() {
        let end = view["byteOffset"].as_u64().unwrap() + view["byteLength"].as_u64().unwrap();
        assert!(end as usize <= gltf.bin.len());
    }
    for (_, file) in &gltf.images {
        assert!(file.starts_with(b"\x89PNG"));
    }
}
