//! The game's line query (`func_150AC9C0`, hand-written MIPS, with `0x150AD364` the triangle test), ported in
//! its own arithmetic order. The camera's wall ray uses it (`func_15123A54`).
//!
//! - The direction is scaled to length 65536; the XZ line through the origin is kept as `dep = slope · indep
//!   + icept` along its major axis.
//! - Terrain (layer 0): from the quadtree leaf under the origin, each leaf's triangles whose XZ bounds the
//!   line crosses (their four corners not all on one side) are tested; the first leaf with a hit ends the
//!   walk, otherwise the walk goes on with the leaf 10 units past where the ray leaves this one, while that
//!   point is within `len` (XZ distance) of the origin. The secondary layer is not searched.
//! - Placements (layer 2, [`super::Layer::Placement`]): every triangle, after the terrain; a hit must be
//!   closer than the terrain's. The game first culls placements by a bounding sphere; scene 0x2D's never
//!   get culled by a ray that could hit them (I; V on the captured rays). The water's
//!   ([`super::Layer::Water`]) too: a ray stops at the surface (C, `func_150ACBD8`).
//! - Triangle (`0x150AD364`): the plane hit at `t ≥ 0`, inside the triangle's integer bounds widened by 1,
//!   closer than the best so far, and inside the triangle by the signs of the three edge cross products on
//!   the normal's largest axis.
//!
//! Verified bit for bit on the camera's captured rays (`tests/oracle.rs`, `camera_rays_match_the_game`).

// The MIPS compares (`c.ule.s`, `c.ult.s`, ...) treat NaN their own way; negated comparisons keep that.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use super::walls::Tree;
use super::{CollisionWorld, Triangle};

/// A hit of [`CollisionWorld::ray`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// Distance from the origin to `point`.
    pub dist: f32,
    pub point: [f32; 3],
    /// Index into [`CollisionWorld::tris`].
    pub tri: u32,
}

/// How far past a leaf's edge the walk looks for the next leaf (`10.0`).
const PAST: f32 = 10.0;

impl CollisionWorld {
    /// The game's line query (`func_150AC9C0`): the nearest triangle hit along `dir` (any length; the game
    /// normalizes it) from `origin`, closer than `len` (0: no limit).
    pub fn ray(&self, origin: [f32; 3], dir: [f32; 3], len: f32) -> Option<RayHit> {
        let [ox, _, oz] = origin;
        let [dx, dy, dz] = dir;
        if dx.to_bits() == 0 && dy == 0.0 && dz == 0.0 {
            return None;
        }
        let scale = 65536.0 / (dx * dx + dy * dy + dz * dz).sqrt();
        let d = [dx * scale, dy * scale, dz * scale];
        let line = Line::new(origin, d);
        let mut q = Query {
            o: origin,
            d,
            best: if len == 0.0 {
                65536.0 * 65536.0 * 65536.0
            } else {
                len
            },
            hit: None,
        };
        let tree = &self.walls[0];
        let mut leaf = find(tree, ox, oz, None);
        while let Some(id) = leaf {
            let n = &tree.nodes[id as usize];
            for &local in &n.tris {
                let t = &self.tris[self.layer_tris[0][local as usize] as usize];
                if line.crosses(t) {
                    q.test(t, self.layer_tris[0][local as usize]);
                }
            }
            if q.hit.is_some() {
                break;
            }
            let (px, pz) = line.exit(n);
            if len != 0.0 {
                let (ex, ez) = (ox - px, oz - pz);
                if !((ex * ex + ez * ez).sqrt() < len) {
                    break;
                }
            }
            leaf = find(tree, px, pz, Some(id));
        }
        // The water after the placements (the game keeps it among them, in placement order: the same
        // nearest hit but for an exact tie).
        for i in self
            .objects
            .iter()
            .flat_map(|r| r.clone())
            .chain(self.water.iter().copied())
        {
            let t = &self.tris[i as usize];
            if line.crosses(t) {
                q.test(t, i);
            }
        }
        q.hit
    }
}

/// The leaf containing `(x, z)` (bounds inclusive), by the game's threaded walk: into the first child when
/// inside, else on to the node after this one's subtree; `skip` (the leaf just searched) counts as outside.
fn find(tree: &Tree, x: f32, z: f32, skip: Option<u32>) -> Option<u32> {
    let mut id = 0u32;
    loop {
        let n = tree.nodes.get(id as usize)?;
        let (cx, cz, h) = (n.cx as f32, n.cz as f32, n.h as f32);
        let inside = Some(id) != skip && cx - h <= x && x <= cx + h && cz - h <= z && z <= cz + h;
        id = match (inside, n.kids) {
            (true, None) => return Some(id),
            (true, Some(_)) => id + 1,
            (false, _) => thread(tree, id),
        };
    }
}

/// The node after `id`'s subtree in pre-order (the game's `+4` thread).
fn thread(tree: &Tree, id: u32) -> u32 {
    match tree.nodes[id as usize].kids {
        Some(kids) => thread(tree, kids[3]),
        None => id + 1,
    }
}

/// The ray's XZ line along its major axis: `dep = slope · indep + icept`.
struct Line {
    o: [f32; 3],
    d: [f32; 3],
    x_major: bool,
    slope: f32,
    icept: f32,
}

impl Line {
    fn new(o: [f32; 3], d: [f32; 3]) -> Self {
        const TINY: f32 = f32::from_bits(0x38D1_B717); // 1e-4
        let [ox, _, oz] = o;
        let (ex, ez) = (ox + d[0], oz + d[2]);
        let x_major = !(d[0].abs() <= d[2].abs());
        let (run_x, run_z) = (ex - ox, ez - oz);
        let (slope, icept) = if x_major {
            let run_x = if run_x == 0.0 { TINY } else { run_x };
            let slope = run_z / run_x;
            (slope, oz - slope * ox)
        } else {
            let run_z = if run_z == 0.0 { TINY } else { run_z };
            let slope = run_x / run_z;
            (slope, ox - slope * oz)
        };
        Self {
            o,
            d,
            x_major,
            slope,
            icept,
        }
    }

    /// The triangle's integer XZ bounds have corners on both sides of the line (`0x150ACBF8`).
    fn crosses(&self, t: &Triangle) -> bool {
        let (lo, hi) = bounds(t);
        let (i, j) = if self.x_major { (0, 2) } else { (2, 0) };
        let (i0, i1, j0, j1) = (lo[i] as f32, hi[i] as f32, lo[j] as f32, hi[j] as f32);
        let (m, c) = (self.slope, self.icept);
        let (a, b) = (m * i0, m * i1);
        let corners = [(c - j0) + a, (a + c) - j1, b + (c - j1), (b + c) - j0];
        let above = corners.iter().filter(|&&v| !(v < 0.0)).count();
        above != 0 && above != 4
    }

    /// Where the walk looks for the next leaf: 10 past the point where the ray leaves `n`'s square.
    fn exit(&self, n: &super::walls::Node) -> (f32, f32) {
        let ([ox, _, oz], [dx, _, dz]) = (self.o, self.d);
        let (east, south) = (!(dx < 0.0), dz < 0.0);
        let (cx, cz, h) = (n.cx as f32, n.cz as f32, n.h as f32);
        if dx == 0.0 {
            return (
                ox,
                if south {
                    (cz - h) - PAST
                } else {
                    (cz + h) + PAST
                },
            );
        }
        if dz == 0.0 {
            return (
                if east {
                    (cx + h) + PAST
                } else {
                    (cx - h) - PAST
                },
                oz,
            );
        }
        let mut x = if east { cx + h } else { cx - h };
        let mut z = ((x - ox) / dx) * dz + oz;
        if z < cz - h || !(z <= cz + h) {
            z = if south { cz - h } else { cz + h };
            x = ((z - oz) / dz) * dx + ox;
        }
        (
            if east { x + PAST } else { x - PAST },
            if south { z - PAST } else { z + PAST },
        )
    }
}

/// A triangle's integer bounds (the game's per-layer bounds tables): min and max of x, y, z.
fn bounds(t: &Triangle) -> ([i32; 3], [i32; 3]) {
    let v = t.v;
    (
        [0, 1, 2].map(|k| v[0][k].min(v[1][k]).min(v[2][k])),
        [0, 1, 2].map(|k| v[0][k].max(v[1][k]).max(v[2][k])),
    )
}

/// The search's state: origin, scaled direction, the best distance so far and its hit.
struct Query {
    o: [f32; 3],
    d: [f32; 3],
    best: f32,
    hit: Option<RayHit>,
}

impl Query {
    /// `0x150AD364`: the ray against one triangle.
    fn test(&mut self, t: &Triangle, index: u32) {
        let [[x0, y0, z0], [x1, y1, z1], [x2, y2, z2]] = t.v.map(|p| p.map(|c| c as f32));
        let ([ox, oy, oz], [dx, dy, dz]) = (self.o, self.d);
        let (e1x, e1y, e1z) = (x1 - x0, y1 - y0, z1 - z0);
        let (e2x, e2y, e2z) = (x2 - x0, y2 - y0, z2 - z0);
        let nx = e1y * e2z - e1z * e2y;
        let ny = e1z * e2x - e1x * e2z;
        let nz = e1x * e2y - e1y * e2x;
        let w = -(nx * x0) - ny * y0 - nz * z0;
        let denom = nx * dx + ny * dy + nz * dz;
        if denom == 0.0 {
            return;
        }
        let t_hit = (-(nx * ox) - ny * oy - nz * oz - w) / denom;
        if t_hit < 0.0 {
            return;
        }
        let (hx, hy, hz) = (ox + t_hit * dx, oy + t_hit * dy, oz + t_hit * dz);
        let (lo, hi) = bounds(t);
        let within = |v: f32, k: usize| (lo[k] - 1) as f32 <= v && !(((hi[k] + 1) as f32) < v);
        if !within(hx, 0) || !within(hz, 2) || !within(hy, 1) {
            return;
        }
        let (px, py, pz) = (hx - ox, hy - oy, hz - oz);
        let dist = (px * px + py * py + pz * pz).sqrt();
        if !(dist < self.best) {
            return;
        }
        // The edge cross products' components on the normal's largest axis (`0x150AD5A4`).
        let (qx, qy, qz) = (hx - x0, hy - y0, hz - z0);
        let cx = e1y * qz - e1z * qy;
        let cy = e1z * qx - e1x * qz;
        let cz = e1x * qy - e1y * qx;
        let [a, b, c] = if cx.abs() <= cy.abs() {
            if cz.abs() <= cy.abs() {
                // y: (v2 - v1) × (h - v1) and (v0 - v2) × (h - v2), their y.
                let b = (z2 - z1) * (hx - x1) - (x2 - x1) * (hz - z1);
                let c = (z0 - z2) * (hx - x2) - (x0 - x2) * (hz - z2);
                [cy, b, c]
            } else {
                Self::z_axis(t, cz, hx, hy)
            }
        } else if cz.abs() <= cx.abs() {
            let b = (y2 - y1) * (hz - z1) - (z2 - z1) * (hy - y1);
            let c = (y0 - y2) * (hz - z2) - (z0 - z2) * (hy - y2);
            [cx, b, c]
        } else {
            Self::z_axis(t, cz, hx, hy)
        };
        let negative = [a, b, c].iter().filter(|&&v| v < 0.0).count();
        if negative == 0 || negative == 3 {
            self.best = dist;
            self.hit = Some(RayHit {
                dist,
                point: [hx, hy, hz],
                tri: index,
            });
        }
    }

    /// The z components (`0x150AD6A8`).
    fn z_axis(t: &Triangle, cz: f32, hx: f32, hy: f32) -> [f32; 3] {
        let [[x0, y0, _], [x1, y1, _], [x2, y2, _]] = t.v.map(|p| p.map(|c| c as f32));
        let b = (x2 - x1) * (hy - y1) - (y2 - y1) * (hx - x1);
        let c = (x0 - x2) * (hy - y2) - (y0 - y2) * (hx - x2);
        [cz, b, c]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_the_nearest_wall_within_the_length() {
        // Two walls x = 100 and x = 300 (y -100..400, z ±1000), and a floor.
        let wall = |x: f32| {
            [
                (
                    [[x, -100., -1000.], [x, -100., 1000.], [x, 400., -1000.]],
                    5,
                ),
                ([[x, 400., -1000.], [x, -100., 1000.], [x, 400., 1000.]], 5),
            ]
        };
        let floor = (
            [[-900., 0., -900.], [900., 0., -900.], [-900., 0., 900.]],
            5,
        );
        let w = CollisionWorld::new(wall(300.0).into_iter().chain(wall(100.0)).chain([floor]));
        let hit = w.ray([0.0, 10.0, 0.0], [2.0, 0.0, 0.0], 0.0).unwrap();
        assert_eq!((hit.dist, hit.point), (100.0, [100.0, 10.0, 0.0]));
        assert_eq!(w.tris[hit.tri as usize].v[0][0], 100);
        assert!(w.ray([0.0, 10.0, 0.0], [1.0, 0.0, 0.0], 99.0).is_none());
        assert!(w.ray([0.0, 10.0, 0.0], [-1.0, 0.0, 0.0], 0.0).is_none());
        // Straight down onto the floor; a zero direction is no ray.
        assert_eq!(
            w.ray([-5.0, 10.0, -5.0], [0.0, -1.0, 0.0], 0.0)
                .unwrap()
                .dist,
            10.0
        );
        assert!(w.ray([5.0, 10.0, 5.0], [0.0; 3], 0.0).is_none());
    }
}
