//! Bank-01 character models: the 56-byte header, the joint table and the display lists.
//!
//! Ported from DevOldSchool/conkers-bfd-decomp (MIT, commit e2400b27dda6): `docs/rzip-assets.md`
//! §"Character containers and animation", `docs/evidence/assets/models/us_asset_inventory.md`
//! §"Rigged character-model geometry" and `scripts/model_assets.py`
//! (`parse_character_model_geometry`, `character_global_bind_pivots`).

use extract_gbi::{Ir, walk_from};

use crate::held::RENDER_STATES;
use crate::{Error, Result, be32, get};

/// The geometry mode a model's lists start from: the renderer's culls back faces (V
/// `../lab/research/mp-original/alpha-scratch`: 0x620405 at every draw of 0, 150 and 90, whose
/// lists never set it; only G_CULL_BACK reaches the glTF).
const INHERITED: u32 = 0x400;

/// One 16-byte joint record (`func_150A81D0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    /// Parent joint's matrix slot; None for a root (stored as -1).
    pub parent: Option<u8>,
    /// Runtime matrix slot (0x40 bytes each in segment 3), selected by `DA380003` in the lists.
    pub matrix: u8,
    /// Channel slot in the animation frames.
    pub anim: u8,
    pub flags: u8,
    /// Translation relative to the parent joint.
    pub translation: [f32; 3],
}

impl Joint {
    pub fn parse(raw: &[u8; 16]) -> Self {
        let f = |o: usize| f32::from_be_bytes(raw[o..o + 4].try_into().unwrap());
        Self {
            parent: u8::try_from(raw[0] as i8).ok(),
            matrix: raw[1],
            anim: raw[2],
            flags: raw[3],
            translation: [f(4), f(8), f(12)],
        }
    }
}

/// A character model: joints indexed by matrix slot and every callable display list.
#[derive(Debug)]
pub struct Model {
    /// `joints[i].matrix == i`.
    pub joints: Vec<Joint>,
    /// Lists of the primary pointer table (pair 1), then the alternate table (pair 5, chosen at
    /// run time by `func_1502CCFC`). Each list is callable on its own, so each is walked with
    /// fresh state.
    pub lists: Vec<Ir>,
}

impl Model {
    /// The number of animation channel slots the rig reads (`max(anim) + 1`).
    pub fn anim_slots(&self) -> usize {
        self.joints
            .iter()
            .map(|j| j.anim as usize + 1)
            .max()
            .unwrap_or(0)
    }
}

/// Parses the joint table and orders it by matrix slot. Matrix and animation slots must each
/// cover `0..n` exactly and every parent must exist.
pub fn parse_joints(table: &[u8]) -> Result<Vec<Joint>> {
    if !table.len().is_multiple_of(16) {
        return Err(Error::Malformed("joint table is not 16-byte records"));
    }
    let records: Vec<Joint> = table.as_chunks::<16>().0.iter().map(Joint::parse).collect();
    let n = records.len();
    let mut joints: Vec<Option<Joint>> = vec![None; n];
    let mut anims = vec![false; n];
    for j in &records {
        let slot = joints
            .get_mut(j.matrix as usize)
            .ok_or(Error::Malformed("joint matrix slot out of range"))?;
        let anim = anims
            .get_mut(j.anim as usize)
            .ok_or(Error::Malformed("joint animation slot out of range"))?;
        if slot.is_some() || *anim {
            return Err(Error::Malformed("joint slot used twice"));
        }
        (*slot, *anim) = (Some(*j), true);
        if j.parent.is_some_and(|p| p as usize >= n) || j.translation.iter().any(|v| !v.is_finite())
        {
            return Err(Error::Malformed("bad joint record"));
        }
    }
    Ok(joints.into_iter().map(Option::unwrap).collect())
}

/// Model-space bind pivot of every joint: its translation plus all its ancestors'. Bind pose has no
/// rotation, so vertices (stored joint-local) move by their joint's pivot only. Summed in f64 from
/// the root down, as DevOldSchool's exporter does.
pub fn bind_pivots(joints: &[Joint]) -> Result<Vec<[f64; 3]>> {
    fn pivot(joints: &[Joint], i: usize, depth: usize) -> Result<[f64; 3]> {
        let j = joints[i];
        let local = j.translation.map(f64::from);
        match j.parent {
            None => Ok(local),
            Some(_) if depth >= joints.len() => Err(Error::Malformed("joint parent cycle")),
            Some(p) => {
                let up = pivot(joints, p as usize, depth + 1)?;
                Ok(std::array::from_fn(|k| local[k] + up[k]))
            }
        }
    }
    (0..joints.len()).map(|i| pivot(joints, i, 0)).collect()
}

/// Parses a bank-01 model: a 0x38-byte header of seven `(offset, size)` pairs (`func_1503CF20`;
/// vertices follow it; pair 2 is the joint table). Display lists use segment 0 for model offsets
/// (normal streams, list calls) and segment 1 for vertices, both relative to the model start.
/// Segment 8 is the renderer's render-state lists in `game_data` (the `.game` data), as for the
/// held models ([`RENDER_STATES`]; V the same address at every character draw); the lists start
/// from [`INHERITED`].
pub fn parse_model(model: &[u8], game_data: &[u8]) -> Result<Model> {
    let states = game_data.get(RENDER_STATES - crate::GAME_DATA_VRAM..);
    let w: Vec<usize> = (0..14)
        .map(|k| be32(model, k * 4).map(|v| v as usize))
        .collect::<Result<_>>()?;
    let joints = parse_joints(get(model, w[4], w[5])?)?;
    let mut lists = Vec::new();
    for (table, size) in [(w[2], w[3]), (w[10], w[11])] {
        if table == 0 {
            continue;
        }
        for k in (0..size).step_by(4) {
            let start = be32(model, table + k)?;
            lists.push(walk_from(start, INHERITED, |a| {
                let off = (a & 0xFF_FFFF) as usize;
                match a >> 24 {
                    0 | 1 => model.get(off..),
                    8 => states?.get(off..),
                    _ => None,
                }
            })?);
        }
    }
    if lists.is_empty() {
        return Err(Error::Malformed("model has no display lists"));
    }
    Ok(Model { joints, lists })
}

/// Matrix slot of a vertex: `DA380003` loads segment-3 matrices at a 0x40-byte stride. A vertex
/// loaded before any matrix command uses slot 0, as DevOldSchool's exporter does.
pub fn matrix_slot(matrix: Option<u32>) -> usize {
    matrix.map_or(0, |m| (m & 0xFF_FFFF) as usize / 0x40)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joint(parent: i8, matrix: u8, anim: u8, t: [f32; 3]) -> Vec<u8> {
        let mut r = vec![parent as u8, matrix, anim, 0];
        t.iter().for_each(|v| r.extend(v.to_be_bytes()));
        r
    }

    #[test]
    fn joints_by_matrix_slot_and_pivots() {
        // Records out of slot order; slot 2's parent is slot 1, whose parent is root slot 0.
        let table = [
            joint(1, 2, 0, [0.0, 1.0, 0.5]),
            joint(-1, 0, 2, [1.0, 2.0, 3.0]),
            joint(0, 1, 1, [0.0, 10.0, 0.0]),
        ]
        .concat();
        let joints = parse_joints(&table).unwrap();
        assert_eq!(
            joints.iter().map(|j| j.matrix).collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert_eq!(joints[0].parent, None);
        assert_eq!(
            joints[2],
            Joint {
                parent: Some(1),
                matrix: 2,
                anim: 0,
                flags: 0,
                translation: [0.0, 1.0, 0.5]
            }
        );
        assert_eq!(
            bind_pivots(&joints).unwrap(),
            [[1.0, 2.0, 3.0], [1.0, 12.0, 3.0], [1.0, 13.0, 3.5]]
        );
    }

    #[test]
    fn rejects_bad_joint_tables() {
        let bad = |t: Vec<u8>| parse_joints(&t).is_err();
        assert!(bad(vec![0; 15]));
        assert!(bad(joint(-1, 1, 0, [0.0; 3]))); // slot 1 of 1
        assert!(bad(
            [joint(-1, 0, 0, [0.0; 3]), joint(0, 0, 1, [0.0; 3])].concat()
        )); // slot twice
        assert!(bad(joint(3, 0, 0, [0.0; 3]))); // absent parent
        assert!(bad(joint(-1, 0, 0, [f32::NAN, 0.0, 0.0])));
        let cycle =
            parse_joints(&[joint(1, 0, 0, [0.0; 3]), joint(0, 1, 1, [0.0; 3])].concat()).unwrap();
        assert!(bind_pivots(&cycle).is_err());
    }

    #[test]
    fn matrix_slots() {
        assert_eq!(matrix_slot(None), 0);
        assert_eq!(matrix_slot(Some(0x0300_0000)), 0);
        assert_eq!(matrix_slot(Some(0x0300_06C0)), 27);
    }

    #[test]
    fn walks_both_pointer_tables() {
        // Header: vertices 0x38..0x48 (one vertex), primary table at 0x48 (one list), joints at
        // 0x50, alternate table at 0x4C (one list). Lists at 0x60 and 0x70.
        let mut m = vec![0u8; 0x80];
        let words = [0x48, 0, 0x48, 4, 0x50, 16, 0, 0, 0, 0, 0x4C, 4, 0, 0];
        for (k, w) in words.iter().enumerate() {
            m[k * 4..k * 4 + 4].copy_from_slice(&(*w as u32).to_be_bytes());
        }
        m[0x38..0x3A].copy_from_slice(&7i16.to_be_bytes());
        m[0x48..0x4C].copy_from_slice(&0x60u32.to_be_bytes());
        m[0x4C..0x50].copy_from_slice(&0x70u32.to_be_bytes());
        m[0x50..0x60].copy_from_slice(&joint(-1, 0, 0, [0.0; 3]));
        let cmds: [(u32, u32); 4] = [
            (0xDA38_0003, 0x0300_0000),
            (0x0100_1002, 0x0100_0038),
            (0xDF00_0000, 0),
            (0xDF00_0000, 0),
        ];
        for (i, (w0, w1)) in cmds.iter().enumerate() {
            let at = if i < 3 { 0x60 + i * 8 } else { 0x70 };
            m[at..at + 4].copy_from_slice(&w0.to_be_bytes());
            m[at + 4..at + 8].copy_from_slice(&w1.to_be_bytes());
        }
        let model = parse_model(&m, &[]).unwrap();
        assert_eq!(model.lists.len(), 2);
        assert_eq!(model.lists[0].vertices[0].position[0], 7);
        assert_eq!(model.lists[0].vertices[0].matrix, Some(0x0300_0000));
        assert!(model.lists[1].vertices.is_empty());
        assert_eq!(model.anim_slots(), 1);
    }
}
