//! Level geometry: a scene's bundle segments and placed props, walked with [`extract_gbi`] and
//! written as glTF 2.0 (JSON + one binary buffer + PNG textures).
//!
//! # Scene composition
//! - Bundle `[0x04, scene]` is a table of models (segments). Slots 0-2 are drawn in place
//!   (DOS's scene-consumer graph: renderer edges with unresolved conditions, identity
//!   transform); slot 3 only feeds collision (`func_15003668`) and is not drawn.
//! - Placements (`extract_scene::scene_placements`): `[0x0B, scene]` records place bundle
//!   segments, `[0x0C, scene, 2]` records place bank-03 models, both through the placement
//!   transform. All of them are exported, including colocated alternatives whose visibility
//!   condition is unknown (scene 0x2D: segments 12 and 13 at one transform); node extras keep
//!   the model path and placement index so a consumer can choose.
//!
//! # Sources
//! Ported from DevOldSchool/conkers-bfd-decomp (MIT, commit e2400b27dda6): the bundle and 40-byte
//! model contract (`docs/rzip-assets.md` "Direct models, bundles and collision",
//! `docs/evidence/assets/models/us_asset_inventory.md` "Indexed bank-04 model geometry"), the
//! slot roles (`us_static_scene_assemblies.md`), and from `scripts/model_assets.py` the glTF
//! alpha-mode rule (`decode_other_mode`) and the zero-area face rule
//! (`omit_zero_area_preview_faces`).

use std::collections::{BTreeMap, HashMap};

use extract_core::{Rom, Table, Textures};
use extract_gbi::{Draw, Image, Ir, Material, Palette, Vertex, walk};
use extract_scene::Placement;
use serde_json::{Value, json};

/// Size of a direct model's header; 16-byte vertices follow it, and segment 1 points there.
pub const HEADER: usize = 0x28;
/// Bundle slots drawn in place; slot 3 and up are collision-only or placed.
const INITIAL_SLOTS: usize = 3;
/// The identity transform, column-major.
pub const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] extract_core::Error),
    #[error(transparent)]
    Gbi(#[from] extract_gbi::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

fn malformed(what: &'static str) -> Error {
    extract_core::Error::Malformed(what).into()
}

/// A direct model (bank 03, bank-04 bundle segments): ten big-endian header words (0/1 = primary
/// display list offset and size, 2/3 and 4/5 = secondary and tertiary regions, 8/9 = vertex-colour
/// descriptors), then 16-byte vertices from [`HEADER`] up to the display list.
#[derive(Debug, Clone, Copy)]
pub struct Model<'a> {
    pub data: &'a [u8],
    pub header: [u32; 10],
}

impl<'a> Model<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let head = data
            .get(..HEADER)
            .ok_or_else(|| malformed("model shorter than its header"))?;
        let header =
            std::array::from_fn(|i| u32::from_be_bytes(head[i * 4..i * 4 + 4].try_into().unwrap()));
        let (dl, size) = (header[0] as usize, header[1] as usize);
        if dl < HEADER
            || !(dl - HEADER).is_multiple_of(16)
            || dl.checked_add(size).is_none_or(|end| end > data.len())
        {
            return Err(malformed("model display list is not after its vertices"));
        }
        Ok(Self { data, header })
    }

    /// Source vertices between the header and the display list.
    pub fn vertex_count(&self) -> usize {
        (self.header[0] as usize - HEADER) / 16
    }

    /// Walks the primary display list: segment 0 addresses the model, segment 1 its vertices.
    /// Segment-8 calls (runtime render-state lists) stay unresolved.
    pub fn walk(&self) -> Result<Ir> {
        let data = self.data;
        Ok(walk(self.header[0], |a| {
            let offset = (a & 0xFF_FFFF) as usize;
            match a >> 24 {
                0 => data.get(offset..),
                1 => data.get(HEADER + offset..),
                _ => None,
            }
        })?)
    }
}

/// The segments of a scene bundle `[0x04, scene]` (a table of models; empty segments stay empty).
pub fn bundle_segments(bundle: &[u8]) -> Result<Vec<&[u8]>> {
    if bundle.is_empty() {
        return Ok(Vec::new());
    }
    let table = Table::parse(bundle)?;
    Ok((0..table.entries.len())
        .map(|i| table.raw(i))
        .collect::<Result<_, _>>()?)
}

/// A walked model, named by its asset path: `[0x04, scene, segment]` or `[0x03, model]`.
#[derive(Debug, Clone)]
pub struct Mesh {
    pub path: Vec<usize>,
    pub ir: Ir,
}

impl Mesh {
    /// The path in hex, e.g. `04-2d-0c`.
    pub fn name(&self) -> String {
        let parts: Vec<String> = self.path.iter().map(|p| format!("{p:02x}")).collect();
        parts.join("-")
    }
}

#[derive(Debug, Clone)]
pub struct Instance {
    /// Index into [`Level::meshes`].
    pub mesh: usize,
    /// Index into `extract_scene::scene_placements`; None for the bundle's initial slots.
    pub placement: Option<usize>,
    /// Model to world, column-major.
    pub matrix: [f32; 16],
}

#[derive(Debug, Clone, Default)]
pub struct Level {
    pub meshes: Vec<Mesh>,
    pub instances: Vec<Instance>,
    /// Placements whose model is missing (scenes 17 and 62).
    pub missing: Vec<usize>,
    /// Segment → texture id: what [`to_gltf`] draws the materials reading that segment with (the
    /// game binds it at run time). They keep their own names (`m{n}-seg{a}`) and stay apart from
    /// materials drawing the same texture.
    pub segments: BTreeMap<u32, u32>,
}

impl Level {
    fn mesh(&mut self, path: Vec<usize>, model: &[u8]) -> Result<usize> {
        if let Some(i) = self.meshes.iter().position(|m| m.path == path) {
            return Ok(i);
        }
        let ir = Model::parse(model)?.walk()?;
        self.meshes.push(Mesh { path, ir });
        Ok(self.meshes.len() - 1)
    }
}

/// The placement transform as a column-major matrix (scale, rotate X, Z, Y, translate).
pub fn placement_matrix(p: &Placement) -> [f32; 16] {
    // Columns from the transform at the origin, so they keep full precision far from it.
    let mut linear = p.clone();
    linear.position = [0; 3];
    let mut m = IDENTITY;
    for (i, axis) in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .into_iter()
        .enumerate()
    {
        m[i * 4..i * 4 + 3].copy_from_slice(&linear.transform(axis));
    }
    m[12..15].copy_from_slice(&p.position.map(f32::from));
    m
}

/// Everything the game draws for `scene` from ROM data: bundle slots 0-2 in place, then every
/// placement's model at its transform.
pub fn scene_level(rom: &Rom, scene: usize) -> Result<Level> {
    let bundle = rom.asset(&[0x04, scene])?;
    let segments = bundle_segments(&bundle)?;
    let mut level = Level::default();
    for (slot, model) in segments.iter().enumerate().take(INITIAL_SLOTS) {
        if !model.is_empty() {
            let mesh = level.mesh(vec![0x04, scene, slot], model)?;
            level.instances.push(Instance {
                mesh,
                placement: None,
                matrix: IDENTITY,
            });
        }
    }
    for (i, p) in extract_scene::scene_placements(rom, scene)?
        .iter()
        .enumerate()
    {
        let model = p.model as usize;
        let mesh = if p.dispatch == 0 {
            let data = rom.asset(&[0x03, model])?;
            (!data.is_empty())
                .then(|| level.mesh(vec![0x03, model], &data))
                .transpose()?
        } else {
            match segments.get(model) {
                Some(data) if !data.is_empty() => Some(level.mesh(vec![0x04, scene, model], data)?),
                _ => None,
            }
        };
        match mesh {
            Some(mesh) => level.instances.push(Instance {
                mesh,
                placement: Some(i),
                matrix: placement_matrix(p),
            }),
            None => level.missing.push(i),
        }
    }
    Ok(level)
}

/// DOS omits zero-area faces from previews, unless their vertices use different matrices
/// (equal local positions can separate once posed).
fn zero_area(v: [&Vertex; 3]) -> bool {
    let p = v.map(|v| v.position.map(i64::from));
    let a: [i64; 3] = std::array::from_fn(|i| p[1][i] - p[0][i]);
    let b: [i64; 3] = std::array::from_fn(|i| p[2][i] - p[0][i]);
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    cross == [0; 3] && v[0].matrix == v[1].matrix && v[1].matrix == v[2].matrix
}

/// One draw's vertex arrays: source vertices deduplicated within the draw, zero-area faces dropped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[u8; 4]>,
    /// Normalised texture coordinates; empty when the draw is untextured.
    pub uvs: Vec<[f32; 2]>,
    /// Unit CBFD normals; empty unless every vertex has a non-zero one.
    // ponytail: no per-face fallback for zero normals (DOS splits vertices for one); the draw
    // just goes unlit. None in scene 0x2D.
    pub normals: Vec<[f32; 3]>,
    /// The vertices' lighting flag (bit 15 of their flag word): set, the lights pass them by and
    /// they keep their colour (the oracle's `lightVerticesCBFD`, C).
    pub unlit: Vec<bool>,
    pub indices: Vec<u32>,
}

pub fn primitive(ir: &Ir, draw: &Draw) -> Primitive {
    let texture = draw.material.texture;
    let mut out = Primitive::default();
    let mut slots = HashMap::new();
    let mut normals = Vec::new();
    for tri in &draw.triangles {
        let v = tri.map(|i| &ir.vertices[i as usize]);
        if zero_area(v) {
            continue;
        }
        for v in v {
            let next = out.positions.len() as u32;
            let index = *slots
                .entry((v.address, v.matrix, v.normal))
                .or_insert_with(|| {
                    out.positions.push(v.position.map(f32::from));
                    out.colors.push(v.color);
                    out.unlit.push(v.flag & 0x8000 != 0);
                    if let Some(t) = texture {
                        out.uvs.push(t.uv(v.uv));
                    }
                    normals.push(v.normal.filter(|n| *n != [0; 3]).map(|n| {
                        let n = n.map(f32::from);
                        let len = n.iter().map(|c| c * c).sum::<f32>().sqrt();
                        n.map(|c| c / len)
                    }));
                    next
                });
            out.indices.push(index);
        }
    }
    out.normals = normals
        .into_iter()
        .collect::<Option<_>>()
        .unwrap_or_default();
    out
}

/// glTF wrap mode of a tile axis's clamp/mirror bits.
fn wrap(cm: u8) -> u32 {
    match cm {
        _ if cm & 2 != 0 => 33071, // CLAMP_TO_EDGE
        _ if cm & 1 != 0 => 33648, // MIRRORED_REPEAT
        _ => 10497,                // REPEAT
    }
}

/// `material` with its images on `segment` bound to texture `id`.
pub fn bind_material(mut material: Material, segment: u32, id: u32) -> Material {
    let bound = |image: Image, mode| match image {
        Image::Segment(a) if a >> 24 == segment => Image::Flat { index: id, mode },
        other => other,
    };
    if let Some(t) = &mut material.texture {
        t.image = t.image.map(|i| bound(i, 0));
        if let Some(p) = &mut t.palette {
            p.image = bound(p.image, 1);
        }
    }
    material
}

/// The flat images (texture ids) a material reads: pixels, then palette.
fn flat_ids(m: &Material) -> impl Iterator<Item = u32> {
    let t = m.texture;
    let pixels = t.and_then(|t| t.image);
    let palette = t.and_then(|t| t.palette).map(|p| p.image);
    [pixels, palette].into_iter().filter_map(|i| match i {
        Some(Image::Flat { index, .. }) => Some(index),
        _ => None,
    })
}

/// File stem and PNG of a material's texture ([`Material::texture_png`]), as [`to_gltf`] names its
/// `textures/<stem>.png`; `blobs` holds its texture ids' bytes. None for untextured materials and
/// runtime-bound (segment) images.
pub fn texture_png(
    m: &Material,
    blobs: &HashMap<u32, Vec<u8>>,
) -> Option<Result<(String, Vec<u8>)>> {
    let file = m.texture_png(|id| blobs.get(&id).cloned())?;
    let t = m.texture?;
    let Some(Image::Flat { index, .. }) = t.image else {
        return None;
    };
    let [w, h] = t.size();
    let format = format!("{:?}", m.texture_format()?);
    let mut stem = format!(
        "{index:04}-{w}x{h}-{}",
        format.to_lowercase().replace(['(', ')'], "")
    );
    if let Some(Palette {
        image: Image::Flat { index: p, .. },
        offset,
    }) = t.palette
        && (p != index || offset != 0)
    {
        stem += &format!("-p{p:04}+{offset}");
    }
    if t.interleaved {
        stem += "-i";
    }
    Some(file.map(|file| (stem, file)).map_err(Error::from))
}

/// A glTF 2.0 asset: `json` references `bin_uri` and `images` by their relative paths.
#[derive(Debug, Clone)]
pub struct Gltf {
    pub json: String,
    pub bin: Vec<u8>,
    /// (relative path, PNG file), in the order of the JSON's `images`.
    pub images: Vec<(String, Vec<u8>)>,
    /// Materials exported without their texture, and why.
    pub warnings: Vec<String>,
}

impl Gltf {
    /// Binary glTF, one self-contained file: the PNGs move into the buffer as buffer views.
    pub fn glb(&self) -> Vec<u8> {
        let mut json: Value = serde_json::from_str(&self.json).expect("to_gltf writes JSON");
        let mut bin = self.bin.clone();
        let mut views = json["bufferViews"].take();
        let views_list = views.as_array_mut().expect("to_gltf writes bufferViews");
        if let Some(images) = json.get_mut("images").and_then(Value::as_array_mut) {
            for (image, (_, png)) in images.iter_mut().zip(&self.images) {
                bin.resize(bin.len().next_multiple_of(4), 0);
                *image = json!({ "bufferView": views_list.len(), "mimeType": "image/png" });
                views_list
                    .push(json!({ "buffer": 0, "byteOffset": bin.len(), "byteLength": png.len() }));
                bin.extend_from_slice(png);
            }
        }
        bin.resize(bin.len().next_multiple_of(4), 0);
        json["bufferViews"] = views;
        json["buffers"] = json!([{ "byteLength": bin.len() }]);
        extract_gbi::glb(
            &serde_json::to_vec(&json).expect("JSON values serialise"),
            &bin,
        )
    }
}

#[derive(Default)]
struct Buffer {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Buffer {
    /// Appends `bytes` as a 4-byte-aligned buffer view and returns its accessor index.
    fn accessor(&mut self, bytes: &[u8], target: u32, mut accessor: Value) -> usize {
        self.bin.resize(self.bin.len().next_multiple_of(4), 0);
        accessor["bufferView"] = json!(self.views.len());
        self.views.push(json!({
            "buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len(), "target": target,
        }));
        self.bin.extend_from_slice(bytes);
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }
}

fn f32_bytes<const N: usize>(v: &[[f32; N]]) -> Vec<u8> {
    v.iter().flatten().flat_map(|x| x.to_le_bytes()).collect()
}

fn hex(v: u32) -> String {
    format!("{v:#010x}")
}

/// Writes `level` as glTF: one mesh per model, one primitive per draw, one node per instance.
/// Materials keep the raw RDP state in `extras`; base colour = texture x vertex colour. Draws
/// whose vertices carry CBFD normals are lit and get NORMAL; the rest are unlit
/// (`KHR_materials_unlit`): their vertex colours hold the shading. The list's G_LIGHTING bit
/// cannot decide this, since unset geometry-mode bits are inherited at runtime (scene 0x2D's
/// terrain sets it on draws without normals; props with normals never set it). Textures that
/// fail to decode are left off their material and listed in [`Gltf::warnings`].
pub fn to_gltf(level: &Level, textures: &Textures, bin_uri: &str) -> Result<Gltf> {
    let (mut pngs, mut warnings, mut unlit) = (Vec::new(), Vec::new(), false);
    let bound =
        |m: Material| (level.segments.iter()).fold(m, |m, (&s, &id)| bind_material(m, s, id));
    let mut blobs = HashMap::new();
    for m in level.meshes.iter().flat_map(|m| &m.ir.draws) {
        for id in flat_ids(&bound(m.material)) {
            blobs.entry(id).or_insert_with(|| {
                textures.get(id as usize).unwrap_or_else(|e| {
                    warnings.push(format!("texture {id}: {e}"));
                    Vec::new()
                })
            });
        }
    }

    let mut buf = Buffer::default();
    let (mut materials, mut images, mut samplers, mut gl_textures) = (
        Vec::<Value>::new(),
        Vec::<Value>::new(),
        Vec::<Value>::new(),
        Vec::<Value>::new(),
    );
    let mut material_ids = HashMap::<(Material, bool), usize>::new();
    let mut image_ids = HashMap::<String, usize>::new();
    let mut sampler_ids = HashMap::<[u32; 3], usize>::new();
    let mut texture_ids = HashMap::<[usize; 2], usize>::new();

    let mut meshes = Vec::new();
    let mut mesh_ids = Vec::new();
    for mesh in &level.meshes {
        let mut primitives = Vec::new();
        for draw in &mesh.ir.draws {
            let p = primitive(&mesh.ir, draw);
            if p.indices.is_empty() {
                continue;
            }
            let m = draw.material;
            let lit = !p.normals.is_empty();
            let material = match material_ids.get(&(m, lit)) {
                Some(&i) => i,
                None => {
                    let mut pbr = json!({ "metallicFactor": 0.0, "roughnessFactor": 1.0 });
                    let mut extras = json!({
                        "combine": m.combine.map(hex),
                        "otherMode": m.other_mode.map(hex),
                        "geometryMode": hex(m.geometry_mode),
                        "primColor": hex(m.prim_color),
                        "envColor": hex(m.env_color),
                    });
                    if let Some(call) = m.state_call {
                        extras["stateCall"] = json!(hex(call));
                    }
                    if let Some(t) = m.texture {
                        extras["texture"] = json!({
                            "image": format!("{:?}", t.image),
                            "palette": format!("{:?}", t.palette),
                            "format": format!("{:?}", m.texture_format()),
                            "size": t.size(),
                            "level": t.level,
                        });
                    }
                    match texture_png(&bound(m), &blobs) {
                        Some(Ok((stem, file))) => {
                            let image = *image_ids.entry(stem).or_insert_with_key(|stem| {
                                let uri = format!("textures/{stem}.png");
                                images.push(json!({ "uri": uri }));
                                pngs.push((uri, file));
                                images.len() - 1
                            });
                            let tile = m.texture.unwrap().tile;
                            let filter = if m.other_mode[0] >> 12 & 3 == 0 {
                                9728 // NEAREST (point sampling)
                            } else {
                                9729 // LINEAR
                            };
                            let key = [wrap(tile.cm[0]), wrap(tile.cm[1]), filter];
                            let sampler = *sampler_ids.entry(key).or_insert_with(|| {
                                samplers.push(json!({
                                    "wrapS": key[0], "wrapT": key[1],
                                    "magFilter": filter, "minFilter": filter,
                                }));
                                samplers.len() - 1
                            });
                            let texture =
                                *texture_ids.entry([image, sampler]).or_insert_with(|| {
                                    gl_textures
                                        .push(json!({ "source": image, "sampler": sampler }));
                                    gl_textures.len() - 1
                                });
                            pbr["baseColorTexture"] = json!({ "index": texture });
                        }
                        Some(Err(e)) => warnings.push(format!(
                            "{} material {}: {e}",
                            mesh.name(),
                            materials.len()
                        )),
                        None => {}
                    }
                    let n = materials.len();
                    let name = match m.texture.and_then(|t| t.image) {
                        Some(Image::Flat { index, .. }) => format!("m{n}-tex{index:04}"),
                        Some(Image::Segment(a)) => format!("m{n}-seg{a:08x}"),
                        None => format!("m{n}-untextured"),
                    };
                    let alpha = m.alpha_mode();
                    let mut material = json!({
                        "name": name,
                        "pbrMetallicRoughness": pbr,
                        "alphaMode": alpha,
                        // F3DEX2 G_CULL_BACK; glTF cannot cull front faces only.
                        "doubleSided": m.geometry_mode & 0x400 == 0,
                        "extras": extras,
                    });
                    if alpha == "MASK" {
                        material["alphaCutoff"] = json!(0.5);
                    }
                    if !lit {
                        material["extensions"] = json!({ "KHR_materials_unlit": {} });
                        unlit = true;
                    }
                    materials.push(material);
                    material_ids.insert((m, lit), materials.len() - 1);
                    materials.len() - 1
                }
            };

            let (min, max) =
                p.positions
                    .iter()
                    .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), v| {
                        (
                            std::array::from_fn(|i| lo[i].min(v[i])),
                            std::array::from_fn(|i| hi[i].max(v[i])),
                        )
                    });
            let n = p.positions.len();
            let mut attributes = json!({
                "POSITION": buf.accessor(&f32_bytes(&p.positions), 34962, json!({
                    "componentType": 5126, "count": n, "type": "VEC3", "min": min, "max": max,
                })),
                "COLOR_0": buf.accessor(p.colors.as_flattened(), 34962, json!({
                    "componentType": 5121, "normalized": true, "count": n, "type": "VEC4",
                })),
            });
            if !p.uvs.is_empty() {
                attributes["TEXCOORD_0"] = json!(buf.accessor(
                    &f32_bytes(&p.uvs),
                    34962,
                    json!({ "componentType": 5126, "count": n, "type": "VEC2" }),
                ));
            }
            if p.unlit.contains(&true) {
                let unlit: Vec<[f32; 1]> =
                    p.unlit.iter().map(|&u| [f32::from(u8::from(u))]).collect();
                attributes["_UNLIT"] = json!(buf.accessor(
                    &f32_bytes(&unlit),
                    34962,
                    json!({ "componentType": 5126, "count": n, "type": "SCALAR" }),
                ));
            }
            if lit {
                attributes["NORMAL"] = json!(buf.accessor(
                    &f32_bytes(&p.normals),
                    34962,
                    json!({ "componentType": 5126, "count": n, "type": "VEC3" }),
                ));
            }
            let indices: Vec<u8> = p.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
            let indices = buf.accessor(
                &indices,
                34963,
                json!({ "componentType": 5125, "count": p.indices.len(), "type": "SCALAR" }),
            );
            primitives.push(json!({
                "attributes": attributes, "indices": indices, "material": material, "mode": 4,
            }));
        }
        mesh_ids.push((!primitives.is_empty()).then_some(meshes.len()));
        if !primitives.is_empty() {
            meshes.push(json!({ "name": mesh.name(), "primitives": primitives }));
        }
    }

    let nodes: Vec<Value> = level
        .instances
        .iter()
        .filter_map(|inst| {
            let source = &level.meshes[inst.mesh];
            let name = match inst.placement {
                Some(p) => format!("{}@{p}", source.name()),
                None => source.name(),
            };
            let mut node = json!({
                "name": name,
                "mesh": mesh_ids[inst.mesh]?,
                "extras": { "model": source.path, "placement": inst.placement },
            });
            if inst.matrix != IDENTITY {
                node["matrix"] = json!(inst.matrix);
            }
            Some(node)
        })
        .collect();
    buf.bin.resize(buf.bin.len().next_multiple_of(4), 0);
    let mut root = json!({
        "asset": { "version": "2.0", "generator": "conker remake extract-level" },
        "scene": 0,
        "scenes": [{ "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "buffers": [{ "uri": bin_uri, "byteLength": buf.bin.len() }],
        "bufferViews": buf.views,
        "accessors": buf.accessors,
    });
    if !images.is_empty() {
        root["images"] = json!(images);
        root["samplers"] = json!(samplers);
        root["textures"] = json!(gl_textures);
    }
    if unlit {
        root["extensionsUsed"] = json!(["KHR_materials_unlit"]);
    }
    Ok(Gltf {
        json: serde_json::to_string_pretty(&root).expect("JSON values serialise"),
        bin: buf.bin,
        images: pngs,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table of `items` as the asset tree stores it (bit 31 on the last size).
    fn table(items: &[Vec<u8>]) -> Vec<u8> {
        let mut head = Vec::new();
        let mut body = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let last = if i + 1 == items.len() { 1 << 31 } else { 0 };
            head.extend(((items.len() * 8 + body.len()) as u32).to_be_bytes());
            head.extend((item.len() as u32 | last).to_be_bytes());
            body.extend(item);
        }
        [head, body].concat()
    }

    /// A model with vertices (x, y, z) = `verts`, uv (32 * i, 0), colour [i; 4] and display list
    /// `dl` right after them.
    fn model(verts: &[[i16; 3]], dl: &[(u32, u32)]) -> Vec<u8> {
        let dl_offset = HEADER + 16 * verts.len();
        let mut m = vec![0; HEADER];
        m[..4].copy_from_slice(&(dl_offset as u32).to_be_bytes());
        m[4..8].copy_from_slice(&((dl.len() * 8) as u32).to_be_bytes());
        for (i, v) in verts.iter().enumerate() {
            let mut b = [0; 16];
            for (k, c) in v.iter().enumerate() {
                b[2 * k..2 * k + 2].copy_from_slice(&c.to_be_bytes());
            }
            b[8..10].copy_from_slice(&(32 * i as i16).to_be_bytes());
            b[12..].copy_from_slice(&[i as u8; 4]);
            m.extend(b);
        }
        m.extend(
            dl.iter()
                .flat_map(|(a, b)| [a.to_be_bytes(), b.to_be_bytes()].concat()),
        );
        m
    }

    fn vtx(n: u32) -> (u32, u32) {
        (0x0100_0000 | n << 12 | n << 1, 0x0100_0000)
    }

    fn tri(a: u32, b: u32, c: u32) -> (u32, u32) {
        (0x0500_0000 | (a * 2) << 16 | (b * 2) << 8 | (c * 2), 0)
    }

    const END: (u32, u32) = (0xDF00_0000, 0);
    const QUAD: [[i16; 3]; 4] = [[0, 0, 0], [10, 0, 0], [10, 0, 10], [0, 0, 10]];

    #[test]
    fn splits_bundles() {
        let a = model(&QUAD, &[END]);
        let b = vec![0xAB; 24];
        let bundle = table(&[a.clone(), vec![], b.clone()]);
        assert_eq!(bundle_segments(&bundle).unwrap(), [&a[..], &[], &b[..]]);
        assert!(bundle_segments(&[]).unwrap().is_empty());
        // A size running past the bundle, and a first offset that is not a table size.
        let mut bad = bundle.clone();
        bad[0x14] = 0x7F;
        assert!(bundle_segments(&bad).is_err());
        assert!(bundle_segments(&[0, 0, 0, 3, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn parses_model_headers() {
        let m = model(&QUAD, &[vtx(4), tri(0, 1, 2), END]);
        let parsed = Model::parse(&m).unwrap();
        assert_eq!(parsed.vertex_count(), 4);
        assert_eq!(parsed.header[1], 24);

        assert!(Model::parse(&m[..HEADER - 1]).is_err());
        let with_dl = |dl: u32, size: u32| {
            let mut b = m.clone();
            b[..4].copy_from_slice(&dl.to_be_bytes());
            b[4..8].copy_from_slice(&size.to_be_bytes());
            Model::parse(&b).map(|_| ())
        };
        assert!(with_dl(0x20, 8).is_err()); // inside the header
        assert!(with_dl(0x30, 8).is_err()); // not on a vertex boundary
        assert!(with_dl(0x68, 32).is_err()); // past the end
        assert!(with_dl(0x68, u32::MAX).is_err()); // overflowing
        assert!(with_dl(0x28, 8).is_ok()); // no vertices
    }

    #[test]
    fn walks_vertices_from_the_header() {
        let m = model(&QUAD, &[vtx(4), tri(0, 1, 2), tri(0, 2, 3), END]);
        let ir = Model::parse(&m).unwrap().walk().unwrap();
        assert_eq!(ir.vertices.len(), 4);
        assert_eq!(ir.vertices[2].position, [10, 0, 10]);
        assert_eq!(ir.draws[0].triangles, [[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn primitives_dedupe_and_drop_zero_area() {
        let verts = [QUAD[0], QUAD[1], QUAD[2], [5, 0, 5], QUAD[0]];
        // (0, 1, 2) kept; (0, 2, 3) collinear; (0, 1, 4) repeats a position: both dropped.
        // (2, 1, 0) reuses the first triangle's vertices.
        let m = model(
            &verts,
            &[
                vtx(5),
                tri(0, 1, 2),
                tri(0, 2, 3),
                tri(0, 1, 4),
                tri(2, 1, 0),
                END,
            ],
        );
        let mut ir = Model::parse(&m).unwrap().walk().unwrap();
        ir.vertices[1].flag |= 0x8000; // the lighting flag: the lights pass it by
        let p = primitive(&ir, &ir.draws[0]);
        assert_eq!(p.indices, [0, 1, 2, 2, 1, 0]);
        assert_eq!(p.unlit, [false, true, false]);
        assert_eq!(
            p.positions,
            [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [10.0, 0.0, 10.0]]
        );
        assert_eq!(p.colors, [[0; 4], [1; 4], [2; 4]]);
        assert!(p.uvs.is_empty());

        // The same positions under different matrices are not zero-area.
        let mut v = ir.vertices[0];
        let w = Vertex {
            matrix: Some(0x0300_0040),
            ..v
        };
        assert!(zero_area([&v, &v, &v]) && !zero_area([&v, &w, &v]));
        v.position = [i16::MAX, i16::MIN, i16::MAX]; // i64 cross product: no overflow
        assert!(!zero_area([&v, &ir.vertices[1], &ir.vertices[2]]));
    }

    #[test]
    fn primitives_carry_normals() {
        // CBFD normals right after the 5-command list: s8 x, y per cache slot, z from the flag.
        let normals = HEADER as u32 + 16 * 4 + 5 * 8;
        let dl = [
            (0xDC38_000E, normals),
            vtx(4),
            tri(0, 1, 2),
            tri(0, 2, 3),
            END,
        ];
        let mut m = model(&QUAD, &dl);
        m.extend([0, 100, 0, 100, 0, 100, 0, 100]);
        let ir = Model::parse(&m).unwrap().walk().unwrap();
        assert_eq!(primitive(&ir, &ir.draws[0]).normals, [[0.0, 1.0, 0.0]; 4]);
        // One zero normal: the whole draw has none (and is exported unlit).
        let len = m.len();
        m[len - 2..].fill(0);
        let ir = Model::parse(&m).unwrap().walk().unwrap();
        assert!(primitive(&ir, &ir.draws[0]).normals.is_empty());
    }

    #[test]
    fn alpha_modes() {
        let alpha_mode = |low| {
            Material {
                other_mode: [0, low],
                ..Material::default()
            }
            .alpha_mode()
        };
        assert_eq!(alpha_mode(0x0055_2078), "OPAQUE");
        assert_eq!(alpha_mode(0x0055_2079), "MASK"); // alpha compare threshold
        assert_eq!(alpha_mode(0x0055_3078), "MASK"); // coverage x alpha
        assert_eq!(alpha_mode(0x0055_4078), "BLEND"); // force blend
        assert_eq!(alpha_mode(0x0055_0878), "BLEND"); // z-mode translucent
        assert_eq!(alpha_mode(0x0055_0C78), "OPAQUE"); // z-mode decal
        assert_eq!(
            [wrap(0), wrap(1), wrap(2), wrap(3)],
            [10497, 33648, 33071, 33071]
        );
    }

    #[test]
    fn glb_embeds_images() {
        let gltf = Gltf {
            json: json!({
                "images": [{ "uri": "textures/a.png" }, { "uri": "textures/b.png" }],
                "bufferViews": [{ "buffer": 0, "byteOffset": 0, "byteLength": 4, "target": 34962 }],
                "buffers": [{ "uri": "level.bin", "byteLength": 4 }],
            })
            .to_string(),
            bin: vec![9; 4],
            images: vec![
                ("textures/a.png".into(), vec![1, 2, 3]),
                ("textures/b.png".into(), vec![4]),
            ],
            warnings: Vec::new(),
        };
        // JSON and BIN chunks of a GLB.
        let chunks = |glb: &[u8]| {
            let text = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
            let json: Value = serde_json::from_slice(&glb[20..20 + text]).unwrap();
            (json, glb[28 + text..].to_vec())
        };
        let (g, bin) = chunks(&gltf.glb());
        assert_eq!(bin, [9, 9, 9, 9, 1, 2, 3, 0, 4, 0, 0, 0]);
        assert_eq!(g["buffers"], json!([{ "byteLength": 12 }]));
        assert_eq!(
            g["images"],
            json!([
                { "bufferView": 1, "mimeType": "image/png" },
                { "bufferView": 2, "mimeType": "image/png" },
            ])
        );
        assert_eq!(
            g["bufferViews"][2],
            json!({ "buffer": 0, "byteOffset": 8, "byteLength": 1 })
        );
        // No images: none added.
        let plain = Gltf {
            json: json!({ "bufferViews": [], "buffers": [{ "uri": "x.bin", "byteLength": 0 }] })
                .to_string(),
            images: Vec::new(),
            ..gltf
        };
        let (g, bin) = chunks(&plain.glb());
        assert!(g.get("images").is_none() && g["buffers"][0].get("uri").is_none());
        assert_eq!(bin, [9; 4]);
    }

    #[test]
    fn placement_matrix_matches_transform() {
        let mut raw = [0u8; extract_scene::PLACEMENT_SIZE];
        for (i, v) in [-3000i16, 1200, 7000, 269, 323, -40].iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
        }
        for (i, s) in [2.3f32, 1.0, 0.5].iter().enumerate() {
            raw[0x20 + i * 4..0x24 + i * 4].copy_from_slice(&s.to_be_bytes());
        }
        let p = Placement::parse(&raw);
        let m = placement_matrix(&p);
        assert_eq!(
            m[3..16].iter().step_by(4).copied().collect::<Vec<_>>(),
            [0.0, 0.0, 0.0, 1.0]
        );
        for v in [[1.0, 2.0, 3.0], [-50.0, 0.0, 400.0]] {
            let ours: [f32; 3] = std::array::from_fn(|r| {
                (0..3).map(|c| m[c * 4 + r] * v[c]).sum::<f32>() + m[12 + r]
            });
            let theirs = p.transform(v);
            assert!(
                ours.iter().zip(theirs).all(|(a, b)| (a - b).abs() < 1e-2),
                "{ours:?} {theirs:?}"
            );
        }
    }

    #[test]
    fn writes_gltf() {
        let dl = [
            vtx(4),
            (0xD9FF_FFFF, 0x0000_0400), // G_CULL_BACK, lighting off
            tri(0, 1, 2),
            (0xEF00_0000, 0x0000_4000), // force blend
            tri(0, 2, 3),
            END,
        ];
        let segment = model(&QUAD, &dl);
        let mut level = Level::default();
        let mesh = level.mesh(vec![4, 0x2D, 0], &segment).unwrap();
        assert_eq!(level.mesh(vec![4, 0x2D, 0], &[]).unwrap(), mesh); // reused, not re-parsed
        for placement in [None, Some(3)] {
            level.instances.push(Instance {
                mesh,
                placement,
                matrix: IDENTITY,
            });
        }
        let textures = Textures::new(&[], &[]).unwrap();
        let out = to_gltf(&level, &textures, "level.bin").unwrap();
        assert!(out.images.is_empty() && out.warnings.is_empty());
        let g: Value = serde_json::from_str(&out.json).unwrap();
        assert_eq!(g["buffers"][0]["byteLength"], out.bin.len());
        assert_eq!(out.bin.len() % 4, 0);
        let names: Vec<_> = g["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].clone())
            .collect();
        assert_eq!(names, [json!("04-2d-00"), json!("04-2d-00@3")]);
        let prims = g["meshes"][0]["primitives"].as_array().unwrap();
        assert_eq!(prims.len(), 2);
        let m: Vec<_> = g["materials"].as_array().unwrap().iter().collect();
        assert_eq!(
            (m[0]["alphaMode"].as_str(), m[1]["alphaMode"].as_str()),
            (Some("OPAQUE"), Some("BLEND"))
        );
        assert_eq!(m[0]["doubleSided"], false);
        assert_eq!(g["extensionsUsed"], json!(["KHR_materials_unlit"]));
        for view in g["bufferViews"].as_array().unwrap() {
            assert_eq!(view["byteOffset"].as_u64().unwrap() % 4, 0);
        }
        let position =
            &g["accessors"][prims[0]["attributes"]["POSITION"].as_u64().unwrap() as usize];
        assert_eq!(
            (position["count"].clone(), position["max"].clone()),
            (json!(3), json!([10.0, 0.0, 10.0]))
        );
    }
}
