//! The game's wall routine (`func_15044380` → hand-written MIPS from `0x150AB1F0`), ported in its own
//! arithmetic order: a box swept from the previous position to the new one is tested against each nearby
//! triangle and pushed out of it, one triangle after the other.
//!
//! - Layers (`func_15044380`): placement objects (layer 2), the secondary layer (1), then the terrain (0),
//!   each starting where the previous one left the body. Layer 3 (actors with collision meshes) is empty in
//!   multiplayer (V: RAM dump, only the players are objects) and pass 2 (`0x150AC3E4`, surfaces with all of
//!   0x0E000000) has no triangle in scene 0x2D (V: every surface word), so neither is ported.
//! - The box (`0x150AB4A8`, `func_150492CC`): `2·half_width` across, `height` tall with its centre
//!   `center_y` above the feet, `2·radius` plus the step's XZ length along the step, centred on the step's
//!   midpoint and turned to its direction.
//! - Candidates (`0x150AB644`): the layer's quadtree ([`Tree`]) is walked in order; a triangle is tested when
//!   its integer bounds overlap the step's (each side widened by `half_width`) and its height range
//!   `round(y) ± 200`; each static triangle once per layer. Objects are taken in placement order, all their
//!   triangles (the game first culls them by bounding square, which never drops a triangle of scene 0x2D's).
//! - Test (`func_150AAA20`): Voorhies' triangle-cube intersection (Graphics Gems III) on the box. Only
//!   triangles the previous position faces count, and only walls (`n.y/|n| < 0.5`, `0x150ABAA8`) unless the
//!   flag table (the terrain's surface words, also for the other layers' indices) has bit 24; scene 0x2D
//!   has none, so objects skip that lookup.
//! - Push (`0x150ABC64`..`0x150AC1B4`): when a point behind the step or the new position projects inside the
//!   triangle, out along its normal to `radius` from the plane; otherwise away from the nearest edge point to
//!   `radius`. Each push re-aims the box; the walk resumes with the next triangle.
//!
//! The player's caller (`func_1504CB98`) skips the routine while obj+0xF8 bit 0x4 is clear: inside the
//! level's type-1 trigger zones of kind 0, the camera volumes (`crate::movement::walls_on`; V on every
//! human row of the raw dumps).
//!
//! Verified bit for bit on the oracle's own calls: 4,707 calls (2,500 pushing) in 9 traces
//! (`lab/oracle/trajectories/wall_*_lldb.csv`, `tests/oracle.rs`).

// The MIPS compares (`c.ule.s`, `c.ult.s`, ...) treat NaN their own way; negated comparisons keep that.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::{CollisionWorld, Triangle};

/// What the routine moves (`func_1507C3E0` gives the three integer sizes, `func_15044660` the height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Half the box's width across the step (`sp+0x158`); also widens the candidate bounds.
    pub half_width: i32,
    /// Half the box's length before the step is added, and the push-out distance (`sp+0x15C`).
    pub radius: i32,
    /// Box height (`D_800CBDD8`) and its centre above the position (`D_800CBDDC`).
    pub height: f32,
    pub center_y: f32,
}

impl Body {
    /// Character 117 in multiplayer (scale 0.5: 60 × 60 × 180 halved), on the ground or in the air (V on 9
    /// traces): the height grows by the frame's vertical move, truncated, and the box is centred on it.
    pub fn player(old_y: f32, new_y: f32) -> Self {
        let dy = old_y - new_y;
        let height = (90.0 + if dy < 0.0 { -dy } else { dy }) as i32 as i16 as i32;
        Self {
            half_width: 30,
            radius: 30,
            height: height as f32,
            center_y: (height >> 1) as f32,
        }
    }

    /// The camera's eye (object type 0x2D, `func_1512BB10`): `size` (cam+0x95C) across and long, `height`
    /// (cam+0x960) tall, centred on the eye (C: func_1507C3E0, func_15044660; not traced).
    pub fn camera(size: f32, height: f32) -> Self {
        Self {
            half_width: size as i32 as i16 as i32,
            radius: size as i32 as i16 as i32,
            height: (height as i32 as i16) as f32,
            center_y: 0.0,
        }
    }
}

/// The callers' switches around the routine (`D_800CBDD2..D4` and the layer bytes `D_80089120..22`).
/// The player sets none of them (V). The camera's sweep (`func_1512BB10`, C, untraced) sets `any_slope`;
/// `dd3` when its `+0x3D0` struct's byte `+0x102` is set; `long_step` on a cut (`+0x23C`) or in the aim
/// mode (`+0x2C` = 0x100); `no_secondary` when `+0x84` has bit 16, `no_objects` when it has bit 31.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sweep {
    /// `D_800CBDD2`: triangles of any slope push, not only walls.
    pub any_slope: bool,
    /// `D_800CBDD3`: a cross-product test the camera enables (`0x150ABCDC`, unverified).
    pub dd3: bool,
    /// `D_800CBDD4`: no 60-unit cap on the step.
    pub long_step: bool,
    /// Skip the secondary layer, the placement objects.
    pub no_secondary: bool,
    pub no_objects: bool,
}

/// The routine's result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swept {
    pub pos: [f32; 3],
    /// Layers that pushed (the routine's return value).
    pub hits: u32,
    /// The vertical velocity after the sweep: zeroed by a ceiling hit while moving up.
    pub y_vel: f32,
}

/// A static layer's quadtree, as the loader builds it (`func_15001DE0`): from a 65536-wide root, a node
/// with 10 or more triangles splits into four until half-size 256.
#[derive(Clone, Debug, Default)]
pub(super) struct Tree {
    /// In pre-order (the game's node order: a node's first child follows it).
    pub(super) nodes: Vec<Node>,
}

#[derive(Clone, Debug)]
pub(super) struct Node {
    pub(super) cx: i32,
    pub(super) cz: i32,
    pub(super) h: i32,
    /// Children (−x−z, −x+z, +x−z, +x+z) or the leaf's triangles (layer-local indices, ascending).
    pub(super) kids: Option<[u32; 4]>,
    pub(super) tris: Vec<u32>,
}

impl Tree {
    /// `tris`: the layer's triangles (layer-local index = position).
    pub(super) fn build(tris: &[&Triangle]) -> Self {
        let mut tree = Self::default();
        if !tris.is_empty() {
            let all: Vec<u32> = (0..tris.len() as u32).collect();
            tree.split(tris, &all, 0, 0, 0x8000);
        }
        tree
    }

    fn split(&mut self, tris: &[&Triangle], list: &[u32], cx: i32, cz: i32, h: i32) -> u32 {
        // func_15002248: the parent's triangles that touch the node's box (y ±16000).
        let shape = Shape::new((2 * h) as f32, 32000.0, (2 * h) as f32);
        let inside: Vec<u32> = list
            .iter()
            .copied()
            .filter(|&i| {
                let v = tris[i as usize]
                    .v
                    .map(|p| [(p[0] - cx) as f32, p[1] as f32, (p[2] - cz) as f32]);
                shape.touches(&v).is_some()
            })
            .collect();
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            cx,
            cz,
            h,
            kids: None,
            tris: Vec::new(),
        });
        if inside.len() < 10 || h < 0x200 {
            self.nodes[id as usize].tris = inside;
        } else {
            let q = h >> 1;
            let kids = [(-q, -q), (-q, q), (q, -q), (q, q)]
                .map(|(dx, dz)| self.split(tris, &inside, cx + dx, cz + dz, q));
            self.nodes[id as usize].kids = Some(kids);
        }
        id
    }

    /// Every node and leaf in walk order, one line each (checks against the game's tree).
    pub(super) fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_node(0, &mut out);
        out
    }

    fn dump_node(&self, id: u32, out: &mut String) {
        let Some(n) = self.nodes.get(id as usize) else {
            return;
        };
        match n.kids {
            Some(kids) => {
                out.push_str(&format!("node ({},{}) h {}\n", n.cx, n.cz, n.h));
                for k in kids {
                    self.dump_node(k, out);
                }
            }
            None => out.push_str(&format!(
                "node ({},{}) h {} leaf {}: {:?}\n",
                n.cx,
                n.cz,
                n.h,
                n.tris.len(),
                n.tris
            )),
        }
    }
}

/// The box's half extents and their ratios to the width (`func_150492CC`: `D_800CC22C..D_800CC23C`).
#[derive(Clone, Copy, Debug)]
struct Shape {
    half: [f32; 3],
    ratio: [f32; 2],
}

/// `D_8009F6E0`: the point-in-triangle bounds' margin.
const EPS: f32 = f32::from_bits(0x3BA3_D70A);

impl Shape {
    fn new(width: f32, height: f32, depth: f32) -> Self {
        let w = if width == 0.0 {
            f32::from_bits(0x3A83_126F)
        } else {
            width
        };
        Self {
            half: [width / 2.0, height / 2.0, depth / 2.0],
            ratio: [height / w, depth / w],
        }
    }

    /// `func_150AAA20`: does the triangle `v` (box space) touch the box? Then its plane `(n, d)` as
    /// `func_150AA9A0` computes it (unnormalised, `n·p = d`).
    fn touches(&self, v: &[[f32; 3]; 3]) -> Option<[f32; 4]> {
        let h = self.half;
        let mut c = v.map(|p| face_code(p, h));
        if c.contains(&0) {
            return Some(plane(v));
        }
        if c[0] & c[1] & c[2] != 0 {
            return None;
        }
        for k in 0..3 {
            c[k] |= bevel2(v[k], h) << 8;
        }
        if c[0] & c[1] & c[2] != 0 {
            return None;
        }
        for k in 0..3 {
            c[k] |= bevel3(v[k], h) << 24;
        }
        if c[0] & c[1] & c[2] != 0 {
            return None;
        }
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            if c[a] & c[b] == 0 && edge_hits(v[a], v[b], c[a] | c[b], h) {
                return Some(plane(v));
            }
        }
        // The plane against the box's four diagonals (scaled to the box by the ratios).
        let p = plane(v);
        let [nx, ny, nz, d] = if ult(p[2], 0.0) { p.map(|c| -c) } else { p };
        let (sy, sz) = (ny * self.ratio[0], nz * self.ratio[1]);
        // (nx ± sy) ± sz: multiplying by ±1 is exact, so this is the game's add or subtract.
        for (fy, fz) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
            let mut den = (nx + fy * sy) + fz * sz;
            if den == 0.0 {
                den = f32::from_bits(0x38D1_B717);
            }
            let t = d / den;
            let q = [t, fy * (t * self.ratio[0]), fz * (t * self.ratio[1])];
            if ule(t.abs(), h[0]) && point_in(q, v) {
                return Some(p);
            }
        }
        None
    }
}

/// MIPS `c.ule.s` (unordered or less-equal) and `c.ult.s` (unordered or less).
fn ule(a: f32, b: f32) -> bool {
    !(a > b)
}
fn ult(a: f32, b: f32) -> bool {
    !(a >= b)
}

/// `func_150AA470`: which faces of the box `p` lies beyond.
fn face_code(p: [f32; 3], h: [f32; 3]) -> u32 {
    let mut c = 0;
    for k in 0..3 {
        if p[k] > h[k] {
            c |= 1 << (2 * k);
        }
        if p[k] < -h[k] {
            c |= 2 << (2 * k);
        }
    }
    c
}

/// `func_150AA4D0`: the twelve edge bevels.
fn bevel2(p: [f32; 3], h: [f32; 3]) -> u32 {
    let mut c = 0;
    for (i, (a, b)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
        let s = h[a] + h[b];
        let (sum, diff) = (p[a] + p[b], p[a] - p[b]);
        let bits = [(sum, 1), (diff, 2), (-sum, 8), (-diff, 4)];
        for (v, bit) in bits {
            if v > s {
                c |= bit << (4 * i);
            }
        }
    }
    c
}

/// `func_150AA5A8`: the eight corner bevels.
fn bevel3(p: [f32; 3], h: [f32; 3]) -> u32 {
    let s = (h[0] + h[1]) + h[2];
    let (xy, xmy) = (p[0] + p[1], p[0] - p[1]);
    let v = [xy + p[2], xy - p[2], xmy + p[2], xmy - p[2]];
    let mut c = 0;
    for (k, bits) in [
        (0, (1, 0x80)),
        (1, (2, 0x40)),
        (2, (4, 0x20)),
        (3, (8, 0x10)),
    ] {
        if v[k] > s {
            c |= bits.0;
        }
        if -v[k] > s {
            c |= bits.1;
        }
    }
    c
}

/// `func_150AA678`: does the segment `p1`–`p2` cross a face of the box it is outside of?
fn edge_hits(p1: [f32; 3], p2: [f32; 3], code: u32, h: [f32; 3]) -> bool {
    for k in 0..3 {
        if code & (3 << (2 * k)) == 0 {
            continue;
        }
        let inv = 1.0 / (p1[k] - p2[k]);
        for (bit, t, mask) in [
            (1, (h[k] - p2[k]) * inv, 0x3F ^ (1 << (2 * k))),
            (2, -(h[k] + p2[k]) * inv, 0x3F ^ (2 << (2 * k))),
        ] {
            if code & (bit << (2 * k)) != 0 {
                // func_150AA644: the crossing point, in the game's order.
                let q = [0, 1, 2].map(|i| (p1[i] - p2[i]) * t + p2[i]);
                if face_code(q, h) & mask == 0 {
                    return true;
                }
            }
        }
    }
    false
}

/// `func_150AA814`: is `p` inside the triangle (bounds widened by [`EPS`], then the edge cross products'
/// signs, ignoring components under 1)?
fn point_in(p: [f32; 3], v: &[[f32; 3]; 3]) -> bool {
    for k in 0..3 {
        let (mut lo, mut hi) = (v[0][k], v[0][k]);
        if v[1][k] < v[0][k] {
            lo = v[1][k];
        } else {
            hi = v[1][k];
        }
        if v[2][k] < lo {
            lo = v[2][k];
        } else if !ule(v[2][k], hi) {
            hi = v[2][k];
        }
        if !ule(p[k], hi + EPS) || p[k] < lo - EPS {
            return false;
        }
    }
    let mut keep = 7;
    let s = [(0, 1), (1, 2), (2, 0)].map(|(a, b)| side(v[a], v[b], p, &mut keep));
    s[0] & keep == s[1] & keep && s[1] & keep == s[2] & keep
}

/// `func_150AA778`: sign bits of `(a - b) × (a - p)`; components under 1 drop out of `keep`.
fn side(a: [f32; 3], b: [f32; 3], p: [f32; 3], keep: &mut u32) -> u32 {
    let e = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let w = [a[0] - p[0], a[1] - p[1], a[2] - p[2]];
    let c = [
        e[1] * w[2] - e[2] * w[1],
        e[2] * w[0] - e[0] * w[2],
        e[0] * w[1] - e[1] * w[0],
    ];
    let bits = ((c[0] < 0.0) as u32 * 4) | ((c[1] < 0.0) as u32 * 2) | (c[2] < 0.0) as u32;
    for (k, mask) in [(0, 3), (1, 5), (2, 6)] {
        if c[k].abs() < 1.0 {
            *keep &= mask;
        }
    }
    bits
}

/// `func_150AA9A0`: `(v0 - v1) × (v0 - v2)` and its offset `n·v0`.
fn plane(v: &[[f32; 3]; 3]) -> [f32; 4] {
    let a = [v[0][0] - v[1][0], v[0][1] - v[1][1], v[0][2] - v[1][2]];
    let b = [v[0][0] - v[2][0], v[0][1] - v[2][1], v[0][2] - v[2][2]];
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let d = (n[0] * v[0][0] + n[1] * v[0][1]) + n[2] * v[0][2];
    [n[0], n[1], n[2], d]
}

/// One layer's pass (`func_150AB1F0`): the routine's stack frame.
struct Pass<'a> {
    body: &'a Body,
    opts: &'a Sweep,
    old: [f32; 3],
    /// `sp+0x34`: the body, pushed as the pass goes.
    new: [f32; 3],
    /// `sp+0x30/0x2C`: the step's direction (cos, sin), the box's axis.
    dir: [f32; 2],
    /// `s2..s5`: the step's integer XZ bounds widened by the half-width; `fp`: `round(new.y)`.
    bounds: [i32; 4],
    fp: i32,
    shape: Shape,
    /// `sp+0x50`: the vertical velocity, then the last push's vertical step; `sp+0x4C`: its bits, zeroed by a
    /// ceiling.
    dy: f32,
    y_vel: u32,
    hit: bool,
    /// The static triangles already tested (`sp+0x6C`, at most 100).
    seen: Vec<u32>,
}

impl<'a> Pass<'a> {
    fn new(body: &'a Body, opts: &'a Sweep, old: [f32; 3], new: [f32; 3], y_vel: f32) -> Self {
        let (dx, dz) = (old[0] - new[0], old[2] - new[2]);
        let depth = body.radius as f32 * 2.0 + (dx * dx + dz * dz).sqrt();
        let mut pass = Self {
            body,
            opts,
            old,
            new,
            dir: [0.0, -1.0],
            bounds: [0; 4],
            fp: 0,
            shape: Shape::new(body.half_width as f32 * 2.0, body.height, depth),
            dy: 0.0,
            y_vel: y_vel.to_bits(),
            hit: false,
            seen: Vec::new(),
        };
        pass.aim();
        pass.dy = y_vel;
        pass
    }

    /// `func_150AC1C4`: the direction and bounds of the step from `old` to `new`.
    fn aim(&mut self) {
        let (o, n) = (self.old, self.new);
        self.fp = n[1].round_ties_even() as i32;
        let d = [n[0] - o[0], n[1] - o[1], n[2] - o[2]];
        self.dy = d[1];
        let q = d[0] * d[0] + d[2] * d[2];
        if q != 0.0 {
            let len = q.sqrt();
            self.dir = [d[0] / len, -d[2] / len];
        }
        let (x0, x1) = if ult(o[0], n[0]) {
            (o[0], n[0])
        } else {
            (n[0], o[0])
        };
        let (z0, z1) = if ult(o[2], n[2]) {
            (o[2], n[2])
        } else {
            (n[2], o[2])
        };
        let w = self.body.half_width;
        self.bounds = [x0 as i32 - w, x1 as i32 + w, z0 as i32 - w, z1 as i32 + w];
    }

    /// The candidate filter (`0x150AB8DC`): integer bounds, then `round(y) ± 200`.
    fn near(&self, t: &Triangle) -> bool {
        let [s2, s3, s4, s5] = self.bounds;
        let (lo, hi) = (
            [0, 1, 2].map(|k| t.v.iter().map(|p| p[k]).min().unwrap()),
            [0, 1, 2].map(|k| t.v.iter().map(|p| p[k]).max().unwrap()),
        );
        lo[0] < s3
            && s2 < hi[0]
            && lo[2] < s5
            && s4 < hi[2]
            && lo[1] < self.fp + 200
            && self.fp - 200 < hi[1]
    }

    /// `0x150AB988`..`0x150AC1B4`: test one triangle and push the body out of it.
    fn test(&mut self, t: &Triangle, any_slope: bool) {
        let (old, [c, s]) = (self.old, self.dir);
        let mid = [0, 1, 2].map(|k| (old[k] + self.new[k]) * 0.5);
        let centre = [mid[0], mid[1] + self.body.center_y, mid[2]];
        // func_150AC344: the vertices in box space (across, up, back along the step).
        let v = t.v.map(|p| {
            let d = [0, 1, 2].map(|k| p[k] as f32 - centre[k]);
            [d[0] * s + d[2] * c, d[1], d[2] * s - d[0] * c]
        });
        let Some([nx, ny, nz, d]) = self.shape.touches(&v) else {
            return;
        };
        let mut q = (nx * nx + ny * ny) + nz * nz;
        if q == 0.0 {
            q = 1.0;
        }
        let len = q.sqrt();
        // Only triangles the previous position faces, and (unless allowed) only walls.
        let e = [mid[0] - old[0], mid[1] - old[1], mid[2] - old[2]];
        let v0 = [
            (e[0] * s + v[0][0]) + e[2] * c,
            v[0][1] + e[1],
            (v[0][2] - e[0] * c) + e[2] * s,
        ];
        if !ult((nx * v0[0] + ny * v0[1]) + nz * v0[2], 0.0) {
            return;
        }
        if !any_slope && !self.opts.any_slope && !ult(ny / len, 0.5) {
            return;
        }
        // The step is capped at 60 (the midpoint follows, the tests below still use the uncapped one).
        let new = self.new;
        let mut mid = mid;
        if !self.opts.long_step {
            let e = [new[0] - old[0], new[1] - old[1], new[2] - old[2]];
            let l = ((e[0] * e[0] + e[1] * e[1]) + e[2] * e[2]).sqrt();
            if l != 0.0 && !(l <= 60.0) {
                let k = 60.0 / l;
                for i in 0..3 {
                    self.new[i] = e[i] * k + old[i];
                    mid[i] = (self.new[i] + old[i]) * 0.5;
                }
            }
        }
        let boxed = |p: [f32; 3]| [p[0] * s + p[2] * c, p[1], p[2] * s - p[0] * c];
        // A point half a step behind the previous position (the game's old − new; with `long_step` the
        // register meant for new.z still holds the midpoint's), then the new position.
        let new_z = if self.opts.long_step { mid[2] } else { new[2] };
        let behind = boxed([old[0] - new[0], old[1] - new[1], old[2] - new_z]);
        let n = self.new;
        let mut at = boxed([n[0] - mid[0], n[1] - mid[1], n[2] - mid[2]]);
        let plane = [nx, ny, nz, d];
        let (inside, c1) = project_inside(plane, behind, &v);
        let flat = if self.opts.dd3 && c1 <= 0.8 {
            at[2] = 0.8;
            true
        } else {
            inside || project_inside(plane, at, &v).0
        };
        let push = if flat {
            // 0x150AC0F8: out along the normal, to `radius` from the plane.
            let reach = d + self.body.radius as f32 * len;
            let pen = ((reach - nx * at[0]) - ny * at[1]) - nz * at[2];
            if pen <= 0.0 {
                return;
            }
            if ult(ny / len, f32::from_bits(0xBE4C_CCCD)) && !(self.dy <= 0.0) {
                self.y_vel = 0;
            }
            let k = pen / q;
            [k * (nx * s - nz * c), k * ny, k * (nx * c + nz * s)]
        } else {
            // 0x150ABEEC: away from the nearest point of the triangle's edges.
            let mut best = (65536.0f32, [0.0f32; 3]);
            for (a, b) in [(1, 0), (2, 1), (0, 2)] {
                let (a, b) = (v[a], v[b]);
                let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let mut t = ((((e[0] * at[0] + e[1] * at[1]) + e[2] * at[2]) - e[0] * a[0])
                    - e[1] * a[1])
                    - e[2] * a[2];
                t /= (e[0] * e[0] + e[1] * e[1]) + e[2] * e[2];
                if t < 0.0 {
                    t = 0.0;
                } else if !ule(t, 1.0) {
                    t = 1.0;
                }
                let p = [0, 1, 2].map(|k| a[k] + t * e[k]);
                let dist = (((p[0] - at[0]) * (p[0] - at[0]) + (p[1] - at[1]) * (p[1] - at[1]))
                    + (p[2] - at[2]) * (p[2] - at[2]))
                    .sqrt();
                if ult(dist, best.0) {
                    best = (dist, p);
                }
            }
            let r = self.body.radius as f32;
            if !ult(best.0, r) {
                return;
            }
            let p = best.1;
            let w = [at[0] - p[0], at[1] - p[1], at[2] - p[2]];
            let l = ((w[0] * w[0] + w[1] * w[1]) + w[2] * w[2]).sqrt();
            let k = (r - best.0) / l;
            let w = w.map(|c| c * k);
            [w[0] * s - w[2] * c, w[1], w[2] * s + w[0] * c]
        };
        for (n, d) in self.new.iter_mut().zip(push) {
            *n += d;
        }
        self.hit = true;
        self.aim();
    }
}

/// `func_150ABD20`: projects `p` onto the plane and tells whether it falls inside the triangle (the cross
/// products' signs on their largest axis). Also the second cross product on that axis (for `dd3`).
fn project_inside([nx, ny, nz, d]: [f32; 4], p: [f32; 3], v: &[[f32; 3]; 3]) -> (bool, f32) {
    let t = (((d - nx * p[0]) - ny * p[1]) - nz * p[2]) / ((nx * nx + ny * ny) + nz * nz);
    let q = [t * nx + p[0], t * ny + p[1], t * nz + p[2]];
    let cross = [(1, 0), (2, 1), (0, 2)].map(|(a, b)| {
        let (a, b) = (v[a], v[b]);
        let e = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let w = [q[0] - b[0], q[1] - b[1], q[2] - b[2]];
        [
            e[1] * w[2] - e[2] * w[1],
            e[2] * w[0] - e[0] * w[2],
            e[0] * w[1] - e[1] * w[0],
        ]
    });
    let a = cross[0].map(f32::abs);
    let axis = if a[0] <= a[1] {
        if ule(a[2], a[1]) { 1 } else { 2 }
    } else if ule(a[2], a[0]) {
        0
    } else {
        2
    };
    let signs = cross.map(|c| c[axis] < 0.0);
    (signs[0] == signs[1] && signs[1] == signs[2], cross[1][axis])
}

/// Triangle flags the routine reads (the terrain's surface word): bit 24 lets any slope push.
const ANY_SLOPE: u32 = 0x0100_0000;
/// The static triangles a pass remembers (`sp+0x6C`).
const SEEN: usize = 100;

impl CollisionWorld {
    /// The game's wall routine (`func_15044380`): moves `body` from `old` to `new` and out of the walls it
    /// sweeps through, layer after layer. `y_vel` is the vertical velocity (a ceiling hit while it is
    /// positive zeroes it).
    pub fn sweep(
        &self,
        old: [f32; 3],
        new: [f32; 3],
        body: &Body,
        opts: &Sweep,
        y_vel: f32,
    ) -> Swept {
        let mut out = Swept {
            pos: new,
            hits: 0,
            y_vel,
        };
        let layer = |out: &mut Swept, f: &dyn Fn(&mut Pass)| {
            let mut pass = Pass::new(body, opts, old, out.pos, out.y_vel);
            f(&mut pass);
            out.pos = pass.new;
            out.hits += pass.hit as u32;
            if pass.y_vel == 0 {
                out.y_vel = 0.0;
            }
        };
        if !opts.no_objects && !self.objects.is_empty() {
            layer(&mut out, &|pass| {
                for range in &self.objects {
                    for i in range.clone() {
                        let t = &self.tris[i as usize];
                        if pass.near(t) {
                            pass.test(t, false);
                        }
                    }
                }
            });
        }
        for (k, tree) in self.walls.iter().enumerate().rev() {
            if tree.nodes.is_empty() || (k == 1 && opts.no_secondary) {
                continue;
            }
            layer(&mut out, &|pass| self.walk(k, tree, 0, pass));
        }
        out
    }

    /// `0x150AB644`: the quadtree in order, skipping nodes outside the (moving) bounds.
    fn walk(&self, layer: usize, tree: &Tree, id: u32, pass: &mut Pass) {
        let n = &tree.nodes[id as usize];
        let [s2, s3, s4, s5] = pass.bounds;
        if s3 < n.cx - n.h || n.cx + n.h < s2 || s5 < n.cz - n.h || n.cz + n.h < s4 {
            return;
        }
        if let Some(kids) = n.kids {
            for k in kids {
                self.walk(layer, tree, k, pass);
            }
            return;
        }
        for &local in &n.tris {
            let t = &self.tris[self.layer_tris[layer][local as usize] as usize];
            if !pass.near(t) || pass.seen.contains(&local) {
                continue;
            }
            if pass.seen.len() < SEEN {
                pass.seen.push(local);
            }
            // The flag table is the terrain's for every static layer (the game indexes it by the layer's own
            // triangle number).
            let any = self
                .terrain_words
                .get(local as usize)
                .is_some_and(|w| w & ANY_SLOPE != 0);
            pass.test(t, any);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(tris: &[[[i32; 3]; 3]]) -> CollisionWorld {
        CollisionWorld::new(tris.iter().map(|t| (t.map(|p| p.map(|c| c as f32)), 0)))
    }

    #[test]
    fn box_test_inside_edges_and_plane() {
        let shape = Shape::new(60.0, 94.0, 77.5);
        // A vertex inside the box.
        assert!(
            shape
                .touches(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 100.0, 0.0]])
                .is_some()
        );
        // A big wall through the box with every vertex outside: found by the diagonals.
        let wall = [
            [-500.0, -500.0, 10.0],
            [500.0, -500.0, 10.0],
            [0.0, 500.0, 10.0],
        ];
        let p = shape.touches(&wall).unwrap();
        assert_eq!((p[0], p[1]), (0.0, 0.0));
        // The same wall beyond the box's depth (38.75).
        let far = wall.map(|v| [v[0], v[1], 40.0]);
        assert!(shape.touches(&far).is_none());
        // An edge crossing the box between two outside vertices.
        let thin = [[-100.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 0.0, 1000.0]];
        assert!(shape.touches(&thin).is_some());
    }

    #[test]
    fn pushes_out_of_a_wall_to_the_radius() {
        // A wall x = 100 facing -x (walls push only from the side they face), y -100 to 400, z ±1000.
        let w = world(&[
            [[100, -100, -1000], [100, -100, 1000], [100, 400, -1000]],
            [[100, 400, -1000], [100, -100, 1000], [100, 400, 1000]],
        ]);
        let (body, opts) = (Body::player(0.0, 0.0), Sweep::default());
        // Running +x into it: stopped 30 short of the plane.
        let s = w.sweep([60.0, 0.0, 0.0], [77.5, 0.0, 0.0], &body, &opts, -9.0);
        assert_eq!((s.pos, s.hits, s.y_vel), ([70.0, 0.0, 0.0], 1, -9.0));
        // Away from it: untouched.
        let s = w.sweep([60.0, 0.0, 0.0], [42.5, 0.0, 0.0], &body, &opts, -9.0);
        assert_eq!((s.pos, s.hits), ([42.5, 0.0, 0.0], 0));
        // Sliding along it at an angle keeps the tangential part.
        let s = w.sweep([65.0, 0.0, 0.0], [80.0, 0.0, 10.0], &body, &opts, -9.0);
        assert_eq!(s.hits, 1);
        assert!(
            (s.pos[0] - 70.0).abs() < 1e-3 && (s.pos[2] - 10.0).abs() < 1e-3,
            "{:?}",
            s.pos
        );
    }

    #[test]
    fn floors_do_not_push_and_ceilings_stop_the_rise() {
        let floor = [[-1000, 0, -1000], [-1000, 0, 1000], [1000, 0, 1000]];
        let w = world(&[floor]);
        let opts = Sweep::default();
        let s = w.sweep(
            [0.0, 0.0, 0.0],
            [10.0, -4.5, 0.0],
            &Body::player(0.0, -4.5),
            &opts,
            -9.0,
        );
        assert_eq!((s.pos, s.hits), ([10.0, -4.5, 0.0], 0));
        // A ceiling at 110 facing down: rising into it pushes the body 30 below it (from the box's
        // centre, 60 up) and zeroes the vertical velocity.
        let w = world(&[[[-2000, 110, -2000], [2000, 110, -2000], [0, 110, 2000]]]);
        let s = w.sweep(
            [0.0, 0.0, 0.0],
            [0.0, 30.0, 0.0],
            &Body::player(0.0, 30.0),
            &opts,
            50.0,
        );
        assert_eq!((s.pos, s.hits, s.y_vel), ([0.0, 20.0, 0.0], 1, 0.0));
    }

    #[test]
    fn tree_splits_crowded_nodes() {
        // 12 small triangles near the origin: the root splits down to half-size 256.
        let tris: Vec<Triangle> = (0..12)
            .map(|i| Triangle {
                v: [[i * 10, 0, 5], [i * 10 + 5, 0, 5], [i * 10, 10, 5]],
                surface: 0,
                normal: [0.0; 3],
            })
            .collect();
        let refs: Vec<&Triangle> = tris.iter().collect();
        let t = Tree::build(&refs);
        let leaves: Vec<&Node> = t.nodes.iter().filter(|n| n.kids.is_none()).collect();
        assert!(leaves.iter().all(|n| n.h >= 256));
        let full = leaves.iter().find(|n| n.tris.len() == 12).unwrap();
        assert_eq!((full.cx, full.cz, full.h), (256, 256, 256));
    }
}
