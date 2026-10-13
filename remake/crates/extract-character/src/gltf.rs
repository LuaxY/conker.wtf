//! glTF 2.0 export: one rigidly skinned mesh in bind pose, its skeleton and the character's clips.
//!
//! The conventions are DevOldSchool/conkers-bfd-decomp's (MIT, commit e2400b27dda6,
//! `scripts/model_assets.py` `encode_gltf`, `character_local_animation_translation`):
//! - joint nodes in matrix-slot order carry the stored parent-relative translations; the bind pose
//!   has no rotation, so each inverse bind matrix is a translation by minus the joint's pivot;
//! - vertices are joint-local: each is moved by the pivot of the matrix active when it was loaded
//!   (`DA380003` before `G_VTX`) and bound to that joint with weight 1;
//! - a clip keys every stored frame at `index * step` ticks (the last clamped to `duration - 1`)
//!   of the 30 Hz clock: rotations always, scales where the clip has scale channels, translations
//!   (bind + masked delta / 16, plus root motion / 1024 on the root) where present.

use std::collections::HashMap;

use extract_core::Textures;
use extract_gbi::{Image, Material};
use serde_json::{Value, json};

use crate::anim::{JOINT_SCALE, ROOT_SCALE, SCALE_ONE, quaternion};
use crate::model::{bind_pivots, matrix_slot};
use crate::{Character, Error, Result};

const FLOAT: u32 = 5126;
const UNSIGNED_BYTE: u32 = 5121;
const UNSIGNED_INT: u32 = 5125;
const G_CULL_BACK: u32 = 0x400;

/// A glTF document and its single binary buffer (textures included as PNG buffer views).
#[derive(Debug)]
pub struct Gltf {
    /// `buffers[0]` has no `uri` (see [`Gltf::with_uri`] and [`Gltf::glb`]).
    pub json: Value,
    pub bin: Vec<u8>,
}

impl Gltf {
    /// The document with `buffers[0].uri` set, for a `.gltf` next to its `.bin`.
    pub fn with_uri(&self, uri: &str) -> Value {
        let mut json = self.json.clone();
        json["buffers"][0]["uri"] = uri.into();
        json
    }

    /// Binary glTF (one self-contained file: the PNGs are already buffer views).
    pub fn glb(&self) -> Vec<u8> {
        extract_gbi::glb(&serde_json::to_vec(&self.json).unwrap(), &self.bin)
    }
}

#[derive(Default)]
struct Buffer {
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Buffer {
    fn view(&mut self, bytes: &[u8]) -> usize {
        self.bin.resize(self.bin.len().next_multiple_of(4), 0);
        self.views
            .push(json!({"buffer": 0, "byteOffset": self.bin.len(), "byteLength": bytes.len()}));
        self.bin.extend(bytes);
        self.views.len() - 1
    }

    fn accessor(
        &mut self,
        bytes: &[u8],
        component: u32,
        kind: &str,
        count: usize,
        extra: Value,
    ) -> usize {
        let view = self.view(bytes);
        let mut a =
            json!({"bufferView": view, "componentType": component, "count": count, "type": kind});
        if let Value::Object(m) = extra {
            a.as_object_mut().unwrap().extend(m);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }

    /// Float vectors; `bounds` adds the min/max that positions and animation inputs require.
    fn floats<const N: usize>(&mut self, values: &[[f32; N]], bounds: bool) -> usize {
        let kind = match N {
            1 => "SCALAR",
            2 => "VEC2",
            3 => "VEC3",
            4 => "VEC4",
            _ => "MAT4",
        };
        let bytes: Vec<u8> = values
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let extra = if bounds {
            let fold = |f: fn(f32, f32) -> f32| -> Vec<f32> {
                (0..N)
                    .map(|k| values.iter().map(|v| v[k]).reduce(f).unwrap_or(0.0))
                    .collect()
            };
            json!({"min": fold(f32::min), "max": fold(f32::max)})
        } else {
            Value::Null
        };
        self.accessor(&bytes, FLOAT, kind, values.len(), extra)
    }
}

#[derive(Default)]
struct Primitive {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    colors: Vec<[u8; 4]>,
    joints: Vec<u8>,
    indices: Vec<u32>,
    /// (list, IR vertex) to local index.
    remap: HashMap<(usize, u32), u32>,
}

fn normalize(v: [f64; 3]) -> Option<[f32; 3]> {
    let len = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    (len > 0.0).then(|| v.map(|x| (x / len) as f32))
}

fn wrap(cm: u8) -> u32 {
    match cm & 3 {
        1 => 33648,     // MIRRORED_REPEAT
        2 | 3 => 33071, // CLAMP_TO_EDGE
        _ => 10497,     // REPEAT
    }
}

impl Character {
    /// The skinned model; with `animations`, every [`Character::playable_clips`] clip.
    /// `textures` is the ROM's texture blob ([`extract_core::Rom::textures`]).
    pub fn gltf(&self, textures: &Textures, animations: bool) -> Result<Gltf> {
        let joints = &self.model.joints;
        let pivots = bind_pivots(joints)?;
        let mut buf = Buffer::default();

        // Geometry: one primitive per display list and distinct draw state, in first-use order
        // (the game hides lists by a mask, obj+0x94: the villagers' looks); materials deduplicated.
        let mut materials: Vec<Material> = Vec::new();
        let mut prims: Vec<(usize, usize, Primitive)> = Vec::new();
        for (li, ir) in self.model.lists.iter().enumerate() {
            for draw in &ir.draws {
                let m = match materials.iter().position(|m| *m == draw.material) {
                    Some(m) => m,
                    None => {
                        materials.push(draw.material);
                        materials.len() - 1
                    }
                };
                let p = match prims.iter().position(|p| (p.0, p.1) == (li, m)) {
                    Some(p) => p,
                    None => {
                        prims.push((li, m, Primitive::default()));
                        prims.len() - 1
                    }
                };
                let prim = &mut prims[p].2;
                let texture = draw.material.texture;
                for tri in &draw.triangles {
                    let mut pos = [[0f64; 3]; 3];
                    for (k, &v) in tri.iter().enumerate() {
                        let vert = &ir.vertices[v as usize];
                        let pivot = pivots
                            .get(matrix_slot(vert.matrix))
                            .ok_or(Error::Malformed("vertex loaded under an absent joint"))?;
                        pos[k] = std::array::from_fn(|a| vert.position[a] as f64 + pivot[a]);
                    }
                    let (e1, e2) = (sub(pos[1], pos[0]), sub(pos[2], pos[0]));
                    let face = normalize([
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ]);
                    for (k, &v) in tri.iter().enumerate() {
                        let next = prim.positions.len() as u32;
                        let index = *prim.remap.entry((li, v)).or_insert(next);
                        if index == next {
                            let vert = &ir.vertices[v as usize];
                            prim.positions.push(pos[k].map(|x| x as f32));
                            let n = vert.normal.and_then(|n| normalize(n.map(f64::from)));
                            prim.normals.push(n.or(face).unwrap_or([0.0, 1.0, 0.0]));
                            if let Some(t) = texture {
                                prim.uvs.push(t.uv(vert.uv));
                            }
                            prim.colors.push(vert.color);
                            prim.joints.push(matrix_slot(vert.matrix) as u8);
                        }
                        prim.indices.push(index);
                    }
                }
            }
        }

        // Materials (alpha mode and culling as the level writer reads them), textures (deduplicated
        // by PNG bytes) and samplers (by wrap modes).
        let (mut images, mut gl_textures, mut samplers, mut gl_materials) =
            (vec![], vec![], vec![], vec![]);
        let mut image_of: HashMap<Vec<u8>, usize> = HashMap::new();
        let mut sampler_of: HashMap<[u32; 2], usize> = HashMap::new();
        let mut texture_of: HashMap<(usize, usize), usize> = HashMap::new();
        for m in &materials {
            let mut pbr = json!({"metallicFactor": 0.0, "roughnessFactor": 1.0});
            let png = m.texture_png(|id| textures.get(id as usize).ok());
            if let Some(bytes) = png.and_then(Result::ok) {
                let image = *image_of.entry(bytes).or_insert_with_key(|bytes| {
                    images.push(json!({"bufferView": buf.view(bytes), "mimeType": "image/png"}));
                    images.len() - 1
                });
                let cm = m.texture.unwrap().tile.cm;
                let sampler = *sampler_of.entry(cm.map(wrap)).or_insert_with_key(|w| {
                    samplers.push(json!({"wrapS": w[0], "wrapT": w[1]}));
                    samplers.len() - 1
                });
                let texture = *texture_of.entry((image, sampler)).or_insert_with(|| {
                    gl_textures.push(json!({"source": image, "sampler": sampler}));
                    gl_textures.len() - 1
                });
                pbr["baseColorTexture"] = json!({"index": texture});
            }
            let image = m.texture.and_then(|t| t.image).map(|i| match i {
                Image::Flat { index, mode } => format!("flat {index} mode {mode}"),
                Image::Segment(a) => format!("segment {a:#010x}"),
            });
            let mut material = json!({
                "name": format!("material_{:02}", gl_materials.len()),
                "pbrMetallicRoughness": pbr,
                "alphaMode": m.alpha_mode(),
                "doubleSided": m.geometry_mode & G_CULL_BACK == 0,
                "extras": {
                    "image": image,
                    "combine": format!("{:06x}{:08x}", m.combine[0], m.combine[1]),
                    "otherMode": format!("{:06x}{:08x}", m.other_mode[0], m.other_mode[1]),
                    "geometryMode": format!("{:06x}", m.geometry_mode),
                    "primColor": format!("{:08x}", m.prim_color),
                    "envColor": format!("{:08x}", m.env_color),
                },
            });
            if m.alpha_mode() == "MASK" {
                material["alphaCutoff"] = json!(0.5);
            }
            gl_materials.push(material);
        }

        let mut primitives = Vec::new();
        for (list, material, p) in &prims {
            let mut attributes = json!({
                "POSITION": buf.floats(&p.positions, true),
                "NORMAL": buf.floats(&p.normals, false),
                "COLOR_0": buf.accessor(&p.colors.concat(), UNSIGNED_BYTE, "VEC4", p.colors.len(), json!({"normalized": true})),
                "JOINTS_0": buf.accessor(&p.joints.iter().flat_map(|&j| [j, 0, 0, 0]).collect::<Vec<_>>(), UNSIGNED_BYTE, "VEC4", p.joints.len(), Value::Null),
                "WEIGHTS_0": buf.floats(&vec![[1.0, 0.0, 0.0, 0.0]; p.joints.len()], false),
            });
            if !p.uvs.is_empty() {
                attributes["TEXCOORD_0"] = buf.floats(&p.uvs, false).into();
            }
            let indices: Vec<u8> = p.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
            primitives.push(json!({
                "attributes": attributes,
                "indices": buf.accessor(&indices, UNSIGNED_INT, "SCALAR", p.indices.len(), Value::Null),
                "material": material,
                "extras": {"list": list},
            }));
        }

        // Skeleton: node i is the joint in matrix slot i; the mesh node comes last.
        let mut nodes: Vec<Value> = joints
            .iter()
            .enumerate()
            .map(|(i, j)| {
                json!({
                    "name": format!("joint_{i:02}"),
                    "translation": j.translation,
                    "extras": {"matrixSlot": i, "animSlot": j.anim, "flags": j.flags},
                })
            })
            .collect();
        let mut roots = Vec::new();
        for (i, j) in joints.iter().enumerate() {
            match j.parent {
                Some(p) => {
                    let children = &mut nodes[p as usize]["children"];
                    if children.is_null() {
                        *children = json!([]);
                    }
                    children.as_array_mut().unwrap().push(i.into());
                }
                None => roots.push(i),
            }
        }
        let name = format!("bank01_{:04}", self.entry);
        nodes.push(json!({"name": name, "mesh": 0, "skin": 0}));
        roots.push(nodes.len() - 1);
        let inverse_bind: Vec<[f32; 16]> = pivots
            .iter()
            .map(|p| {
                let mut m = [0.0; 16];
                (m[0], m[5], m[10], m[15]) = (1.0, 1.0, 1.0, 1.0);
                (m[12], m[13], m[14]) = (-p[0] as f32, -p[1] as f32, -p[2] as f32);
                m
            })
            .collect();
        let skin = json!({
            "name": format!("{name}_rig"),
            "joints": (0..joints.len()).collect::<Vec<_>>(),
            "inverseBindMatrices": buf.floats(&inverse_bind, false),
        });

        let mut gl_animations = Vec::new();
        for clip in self.playable_clips().filter(|_| animations) {
            let times: Vec<[f32; 1]> = clip.times().iter().map(|&t| [t]).collect();
            let input = buf.floats(&times, true);
            let (mut clip_samplers, mut channels) = (Vec::new(), Vec::new());
            let mut channel = |output: usize, node: usize, path: &str| {
                channels.push(
                    json!({"sampler": clip_samplers.len(), "target": {"node": node, "path": path}}),
                );
                clip_samplers
                    .push(json!({"input": input, "output": output, "interpolation": "LINEAR"}));
            };
            for (node, j) in joints.iter().enumerate() {
                let a = j.anim as usize;
                let mut prev: Option<[f32; 4]> = None;
                let rotations: Vec<[f32; 4]> = clip
                    .frames
                    .iter()
                    .map(|f| {
                        let mut q = quaternion(f.rotations[a]);
                        if prev
                            .is_some_and(|p| p.iter().zip(q).map(|(x, y)| x * y).sum::<f32>() < 0.0)
                        {
                            q = q.map(|v| -v);
                        }
                        prev = Some(q);
                        q
                    })
                    .collect();
                channel(buf.floats(&rotations, false), node, "rotation");

                if clip.frames[0].scales[3 * a..3 * a + 3]
                    .iter()
                    .any(Option::is_some)
                {
                    let scales: Vec<[f32; 3]> = clip
                        .frames
                        .iter()
                        .map(|f| {
                            std::array::from_fn(|k| match f.scales[3 * a + k] {
                                None | Some(0) => 1.0,
                                Some(v) => v as f32 / SCALE_ONE,
                            })
                        })
                        .collect();
                    channel(buf.floats(&scales, false), node, "scale");
                }

                let root = j.parent.is_none() && clip.frames.iter().any(|f| f.root != [0; 3]);
                if root || clip.frames[0].translations[a].is_some() {
                    let translations: Vec<[f32; 3]> = clip
                        .frames
                        .iter()
                        .map(|f| {
                            std::array::from_fn(|k| {
                                let delta = f.translations[a]
                                    .map_or(0.0, |d| d[k] as f64 / JOINT_SCALE as f64);
                                let motion = if j.parent.is_none() {
                                    f.root[k] as f64 / ROOT_SCALE as f64
                                } else {
                                    0.0
                                };
                                (j.translation[k] as f64 + delta + motion) as f32
                            })
                        })
                        .collect();
                    channel(buf.floats(&translations, false), node, "translation");
                }
            }
            let mut name = format!("pair_{:04}", clip.pair);
            if let Some(id) = clip.logical.first() {
                name += &format!("_anim_{id:04}");
            }
            gl_animations.push(json!({
                "name": name,
                "samplers": clip_samplers,
                "channels": channels,
                "extras": {
                    "bank02Entry": self.anim_entry, "pair": clip.pair, "logicalIds": clip.logical,
                    "durationTicks": clip.duration, "keyframeStep": clip.step, "frames": clip.frames.len(),
                },
            }));
        }

        let mut json = json!({
            "asset": {"version": "2.0", "generator": "remake extract-character"},
            "scene": 0,
            "scenes": [{"nodes": roots}],
            "nodes": nodes,
            "meshes": [{"name": name, "primitives": primitives}],
            "skins": [skin],
            "materials": gl_materials,
            "accessors": buf.accessors,
            "bufferViews": buf.views,
            "buffers": [{"byteLength": buf.bin.len()}],
            "extras": {"bank01Entry": self.entry, "bank02Entry": self.anim_entry, "fps": crate::FPS},
        });
        for (key, list) in [
            ("images", images),
            ("textures", gl_textures),
            ("samplers", samplers),
            ("animations", gl_animations),
        ] {
            if !list.is_empty() {
                json[key] = list.into();
            }
        }
        Ok(Gltf { json, bin: buf.bin })
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|k| a[k] - b[k])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glb_and_uri() {
        let g = Gltf {
            json: json!({"asset": {"version": "2.0"}, "buffers": [{"byteLength": 3}]}),
            bin: vec![1, 2, 3],
        };
        let glb = g.glb();
        let text_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        let text: Value = serde_json::from_slice(&glb[20..20 + text_len]).unwrap();
        assert_eq!(text, g.json);
        assert_eq!(&glb[28 + text_len..], [1, 2, 3, 0]);
        assert_eq!(g.with_uri("a.bin")["buffers"][0]["uri"], "a.bin");
    }

    #[test]
    fn buffer_alignment_and_bounds() {
        let mut b = Buffer::default();
        b.view(&[1]);
        let a = b.floats(&[[1.0, -2.0], [3.0, 4.0]], true);
        assert_eq!(b.views[1]["byteOffset"], 4);
        assert_eq!(b.accessors[a]["min"], json!([1.0, -2.0]));
        assert_eq!(b.accessors[a]["max"], json!([3.0, 4.0]));
        assert_eq!(b.accessors[a]["type"], "VEC2");
        let m = b.floats(&[[0.0; 16]], false);
        assert_eq!(b.accessors[m]["type"], "MAT4");
        assert_eq!(wrap(1), 33648);
        assert_eq!(wrap(2), 33071);
        assert_eq!(wrap(0), 10497);
    }
}
