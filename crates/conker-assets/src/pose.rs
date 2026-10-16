//! A character's pose from its glTF (`character.gltf` and its `.bin`): the joints' parents and rest
//! transforms and each animation's channels, sampled as the client's renderer samples them (Bevy:
//! linear translation and scale, shortest-arc slerp for rotation, held past the last key) into each
//! joint's model-space transform. The server places the body-part spheres ([`crate::Parts`]) on it
//! (`lab/research/mp-original/held-leftovers.md` §2, `pose.py` its prototype).

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::{Result, invalid, parse_json};

/// A joint's local transform: translation, rotation (x, y, z, w) and scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trs {
    pub t: [f32; 3],
    pub r: [f32; 4],
    pub s: [f32; 3],
}

/// A model-space transform: the linear part by column, then the translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub m: [[f32; 3]; 3],
    pub t: [f32; 3],
}

impl Affine {
    /// The point `p` of the joint's frame in model space.
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        core::array::from_fn(|i| m[0][i] * p[0] + m[1][i] * p[1] + m[2][i] * p[2] + self.t[i])
    }

    /// This transform followed by a child's local `l` (glTF's T·R·S).
    fn then(&self, l: &Trs) -> Self {
        let r = rotation(l.r);
        Self {
            m: core::array::from_fn(|c| self.linear(r[c].map(|v| v * l.s[c]))),
            t: self.apply(l.t),
        }
    }

    fn linear(&self, v: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        core::array::from_fn(|i| m[0][i] * v[0] + m[1][i] * v[1] + m[2][i] * v[2])
    }
}

const IDENTITY: Affine = Affine {
    m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    t: [0.0; 3],
};

/// The rotation matrix of the unit quaternion `q`, by column.
fn rotation([x, y, z, w]: [f32; 4]) -> [[f32; 3]; 3] {
    [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ]
}

/// The quaternion product `a·b` (`b` first, then `a`).
pub fn qmul([ax, ay, az, aw]: [f32; 4], [bx, by, bz, bw]: [f32; 4]) -> [f32; 4] {
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// The inverse of the unit quaternion `q`.
pub fn qinv([x, y, z, w]: [f32; 4]) -> [f32; 4] {
    [-x, -y, -z, w]
}

fn lerp3(a: [f32; 3], b: [f32; 3], u: f32) -> [f32; 3] {
    core::array::from_fn(|i| a[i] + (b[i] - a[i]) * u)
}

/// Shortest-arc spherical interpolation (glam's `Quat::slerp`, which Bevy's curves use).
fn slerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let mut d: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    let b = if d < 0.0 {
        d = -d;
        b.map(|v| -v)
    } else {
        b
    };
    let (wa, wb) = if d > 1.0 - 1e-6 {
        (1.0 - u, u)
    } else {
        let th = d.acos();
        let s = th.sin();
        (((1.0 - u) * th).sin() / s, (u * th).sin() / s)
    };
    let q: [f32; 4] = core::array::from_fn(|i| a[i] * wa + b[i] * wb);
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    q.map(|v| v / n)
}

/// `a` shown at `1 − w`, `b` at `w` (a cross-fade, as the client's two weighted animations).
pub fn blend(a: &[Trs], b: &[Trs], w: f32) -> Vec<Trs> {
    (a.iter().zip(b))
        .map(|(a, b)| Trs {
            t: lerp3(a.t, b.t, w),
            r: slerp(a.r, b.r, w),
            s: lerp3(a.s, b.s, w),
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Path {
    Translation,
    Rotation,
    Scale,
}

/// One animated property of one joint: key times (seconds) and values (a vec3's fourth is 0).
#[derive(Debug, PartialEq)]
struct Channel {
    joint: usize,
    path: Path,
    times: Vec<f32>,
    values: Vec<[f32; 4]>,
}

impl Channel {
    fn at(&self, s: f32) -> [f32; 4] {
        let k = self.times.partition_point(|&t| t <= s);
        if k == 0 {
            return self.values[0];
        }
        if k == self.times.len() {
            return self.values[k - 1];
        }
        let (t0, t1) = (self.times[k - 1], self.times[k]);
        let u = (s - t0) / (t1 - t0);
        let (a, b) = (self.values[k - 1], self.values[k]);
        match self.path {
            Path::Rotation => slerp(a, b, u),
            _ => core::array::from_fn(|i| a[i] + (b[i] - a[i]) * u),
        }
    }
}

/// A character's joints (`joint_NN`: the game's matrix slot NN) and clips.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub parents: Vec<Option<usize>>,
    pub rest: Vec<Trs>,
    /// Joints in an order where each comes after its parent.
    order: Vec<usize>,
    /// Added to every translation key, by joint: a `clips_from` model posed by another's clips
    /// ([`Self::posed_by`]) keys its joint at the other's key less the other's rest plus its own.
    shift: Vec<[f32; 3]>,
    /// The glTF animations by name (`pair_NNNN_anim_IIII`, [`crate::ClipInfo::animation`]).
    clips: Arc<HashMap<String, Vec<Channel>>>,
}

#[derive(Deserialize)]
struct Gltf {
    nodes: Vec<Node>,
    accessors: Vec<Accessor>,
    #[serde(rename = "bufferViews")]
    views: Vec<View>,
    #[serde(default)]
    animations: Vec<Animation>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(default)]
    name: String,
    #[serde(default)]
    children: Vec<usize>,
    translation: Option<[f32; 3]>,
    rotation: Option<[f32; 4]>,
    scale: Option<[f32; 3]>,
}

#[derive(Deserialize)]
struct Accessor {
    #[serde(rename = "bufferView")]
    view: usize,
    #[serde(default, rename = "byteOffset")]
    offset: usize,
    count: usize,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "componentType")]
    component: u32,
}

#[derive(Deserialize)]
struct View {
    #[serde(default, rename = "byteOffset")]
    offset: usize,
}

#[derive(Deserialize)]
struct Animation {
    name: String,
    channels: Vec<GltfChannel>,
    samplers: Vec<Sampler>,
}

#[derive(Deserialize)]
struct GltfChannel {
    sampler: usize,
    target: Target,
}

#[derive(Deserialize)]
struct Target {
    node: usize,
    path: String,
}

#[derive(Deserialize)]
struct Sampler {
    input: usize,
    output: usize,
}

const FILE: &str = "character.gltf";

impl Skeleton {
    /// Reads a character's glTF JSON and its buffer (`character.bin`).
    pub fn parse(gltf: &[u8], bin: &[u8]) -> Result<Self> {
        let g: Gltf = parse_json(FILE, gltf)?;
        let joint = |n: &Node| n.name.strip_prefix("joint_")?.parse::<usize>().ok();
        let count = g.nodes.iter().filter_map(joint).max().map_or(0, |j| j + 1);
        let mut parents = vec![None; count];
        let mut rest = vec![
            Trs {
                t: [0.0; 3],
                r: [0.0, 0.0, 0.0, 1.0],
                s: [1.0; 3],
            };
            count
        ];
        let of = |i: usize| g.nodes.get(i).and_then(joint);
        for n in &g.nodes {
            let Some(j) = joint(n) else { continue };
            rest[j] = Trs {
                t: n.translation.unwrap_or([0.0; 3]),
                r: n.rotation.unwrap_or([0.0, 0.0, 0.0, 1.0]),
                s: n.scale.unwrap_or([1.0; 3]),
            };
            for c in n.children.iter().filter_map(|&c| of(c)) {
                parents[c] = Some(j);
            }
        }
        let mut order: Vec<usize> = (0..count).filter(|&j| parents[j].is_none()).collect();
        let mut k = 0;
        while k < order.len() {
            let p = order[k];
            order.extend((0..count).filter(|&j| parents[j] == Some(p)));
            k += 1;
        }
        let floats = |a: usize, width: usize| -> Result<Vec<[f32; 4]>> {
            let acc = (g.accessors.get(a)).ok_or_else(|| invalid(FILE, "accessors", a))?;
            let n = match acc.kind.as_str() {
                "SCALAR" => 1,
                "VEC3" => 3,
                "VEC4" => 4,
                k => return Err(invalid(FILE, format!("accessors[{a}]"), k)),
            };
            if acc.component != 5126 || n != width {
                return Err(invalid(FILE, format!("accessors[{a}]"), "not f32 keys"));
            }
            let view = (g.views.get(acc.view)).ok_or_else(|| invalid(FILE, "bufferViews", a))?;
            let at = view.offset + acc.offset;
            let bytes = (bin.get(at..at + 4 * n * acc.count))
                .ok_or_else(|| invalid(FILE, format!("accessors[{a}]"), "past the buffer"))?;
            Ok(bytes
                .chunks_exact(4 * n)
                .map(|c| {
                    core::array::from_fn(|i| match i < n {
                        true => f32::from_le_bytes(c[4 * i..4 * i + 4].try_into().unwrap()),
                        false => 0.0,
                    })
                })
                .collect())
        };
        let mut clips = HashMap::new();
        for a in &g.animations {
            let mut channels = Vec::new();
            for c in &a.channels {
                let (Some(joint), Some(s)) = (of(c.target.node), a.samplers.get(c.sampler)) else {
                    continue;
                };
                let (path, width) = match c.target.path.as_str() {
                    "translation" => (Path::Translation, 3),
                    "rotation" => (Path::Rotation, 4),
                    "scale" => (Path::Scale, 3),
                    _ => continue,
                };
                let times = floats(s.input, 1)?.into_iter().map(|t| t[0]).collect();
                let values = floats(s.output, width)?;
                channels.push(Channel {
                    joint,
                    path,
                    times,
                    values,
                });
            }
            clips.insert(a.name.clone(), channels);
        }
        Ok(Self {
            parents,
            rest,
            order,
            shift: vec![[0.0; 3]; count],
            clips: Arc::new(clips),
        })
    }

    /// This model posed by `other`'s clips (a pick whose `character.json` names `clips_from`; the
    /// same joints): a keyed translation is the other's key less the other's rest plus this one's,
    /// an unkeyed joint stays at this one's rest (as the client's retarget draws it).
    pub fn posed_by(self, other: &Skeleton) -> Self {
        let shift = (self.rest.iter().enumerate())
            .map(|(j, r)| {
                let o = other.rest.get(j).map_or(r.t, |o| o.t);
                core::array::from_fn(|i| r.t[i] - o[i])
            })
            .collect();
        Self {
            shift,
            clips: other.clips.clone(),
            ..self
        }
    }

    /// Whether it has glTF animation `name`.
    pub fn has(&self, name: &str) -> bool {
        self.clips.contains_key(name)
    }

    /// Each joint's local transform in animation `name` at clip time `tick` (30 a second, held
    /// past the last key); the rest for joints it does not key, and for an animation it lacks.
    pub fn sample(&self, name: &str, tick: f32) -> Vec<Trs> {
        let mut out = self.rest.clone();
        let s = tick / 30.0;
        for c in self.clips.get(name).into_iter().flatten() {
            let v = c.at(s);
            let j = &mut out[c.joint];
            match c.path {
                Path::Translation => j.t = core::array::from_fn(|i| v[i] + self.shift[c.joint][i]),
                Path::Rotation => j.r = v,
                Path::Scale => j.s = [v[0], v[1], v[2]],
            }
        }
        out
    }

    /// Each joint's model-space transform for the local transforms `local`.
    pub fn model(&self, local: &[Trs]) -> Vec<Affine> {
        let mut out = vec![IDENTITY; local.len()];
        for &j in &self.order {
            let parent = self.parents[j].map_or(IDENTITY, |p| out[p]);
            out[j] = parent.then(&local[j]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pack (`CONKER_ASSETS`, else `<repo>/assets`); None (the test skips) without it.
    fn pack(path: &str) -> Option<Vec<u8>> {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into());
        std::fs::read(std::path::Path::new(&dir).join(path)).ok()
    }

    /// The recipe's check (`gltfparts.py`, V pc3's oracle matrices 43.6 and 70.5): the Tedi's
    /// head sphere (joint 12, (0, 33, 5)) at scale 0.5 and lift 43 sits 43.7 above the feet in the
    /// crouch (0x16, pair 9) at tick 16, 70.0 in the idle (0xF, pair 7) at tick 22.
    #[test]
    fn the_tedis_head_crouched_and_standing() {
        let (Some(gltf), Some(bin)) = (
            pack("characters/117/character.gltf"),
            pack("characters/117/character.bin"),
        ) else {
            eprintln!("no pack: skipping");
            return;
        };
        let sk = Skeleton::parse(&gltf, &bin).unwrap();
        assert_eq!(
            (sk.rest.len(), sk.parents[12], sk.parents[0]),
            (28, Some(1), None)
        );
        let head = |clip: &str, tick: f32| {
            let m = sk.model(&sk.sample(clip, tick));
            0.5 * (m[12].apply([0.0, 33.0, 5.0])[1] + 43.0)
        };
        let (crouched, standing) = (
            head("pair_0009_anim_0022", 16.0),
            head("pair_0007_anim_0015", 22.0),
        );
        assert!((crouched - 43.7).abs() < 0.1, "{crouched}");
        assert!((standing - 70.0).abs() < 0.1, "{standing}");
        // A mesh-only pick posed by the Tedi's clips: its own rest where the clip keys nothing.
        let (Some(g0), Some(b0)) = (
            pack("characters/0/character.gltf"),
            pack("characters/0/character.bin"),
        ) else {
            return;
        };
        let conker = Skeleton::parse(&g0, &b0).unwrap().posed_by(&sk);
        let still = conker.sample("pair_0009_anim_0022", 16.0);
        let keyed = sk.sample("pair_0009_anim_0022", 16.0);
        for j in 0..28 {
            let want: [f32; 3] =
                core::array::from_fn(|i| keyed[j].t[i] - sk.rest[j].t[i] + conker.rest[j].t[i]);
            assert!(
                (0..3).all(|i| (still[j].t[i] - want[i]).abs() < 1e-4),
                "joint {j}"
            );
        }
    }

    #[test]
    fn slerp_takes_the_short_way_and_blend_mixes() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let b = [0.0, -1.0, 0.0, 0.0]; // 180° about y, the far sign
        let q = slerp(a, b, 0.5);
        assert!((q[1].abs() - 0.5f32.sqrt()).abs() < 1e-6 && (q[3] - 0.5f32.sqrt()).abs() < 1e-6);
        let t = |x: f32| Trs {
            t: [x, 0.0, 0.0],
            r: a,
            s: [1.0; 3],
        };
        assert_eq!(blend(&[t(0.0)], &[t(10.0)], 0.25)[0].t, [2.5, 0.0, 0.0]);
        let c = rotation(qmul([0.0, 0.5f32.sqrt(), 0.0, 0.5f32.sqrt()], qinv(a)));
        assert!((c[0][2] + 1.0).abs() < 1e-6); // 90° about y takes +x to −z
    }
}
