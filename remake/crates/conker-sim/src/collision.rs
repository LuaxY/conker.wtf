//! Static collision in game units: the scene's triangles, a uniform XZ grid, and the floor query and wall
//! routine ([`CollisionWorld::sweep`], module `walls`) of the original game, bit for bit.
//!
//! The floor height is the game's integer scanline routine (hand-written MIPS at `0x150A3CBC` for the full
//! query, `0x150A411C` for the same-triangle fast path): vertices sorted by z, the triangle's two edges
//! interpolated at the query row with 8-bit fixed-point slopes, then across the row. Heights are in 1/256
//! units. The full query (`func_1510E950`) truncates the position; the fast path on last frame's floor
//! triangle (`func_150A3FC4`) rounds it (`cvt.w.s`, round half to even). Verified on 9,262 traced frames
//! (README of `lab/oracle`, movement section): floor_y equal to the bit except while pushing a wall.
//!
//! Surface words are carried but not interpreted: the traced floors are 0x5 (flat), 0x3 (the ramp) and 0x4
//! (the plateau), the traced walls 0x5, 0x4 (terrain) or none (placements). The wall routine tells walls from
//! floors by slope.

mod ray;
mod walls;
use crate::root::RootTrack;
use crate::trig::AcosTable;
use conker_camera::volume::CameraLevel;
pub use ray::RayHit;
use walls::Tree;
pub use walls::{Body, Sweep, Swept};

/// Floor height when nothing is under the point (the game's `-10000.0`, `0x800A2D60`).
pub const NO_FLOOR: f32 = -10000.0;
/// A floor is accepted up to this far above the reference height (`func_1510E950`'s `50.0`).
pub const STEP_UP: f32 = 50.0;

const CELL: i32 = 512;
/// Grid cells hold triangles within this of their XZ bounds: the floor routine's 2.
const MARGIN: i32 = 32;

/// Where a triangle comes from (extract-scene's `Layer`): the game builds its wall layers from these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// The scene bundle's segment 0 (wall layer 0, with the surface words).
    Terrain,
    /// Its segment 3 (wall layer 1).
    Secondary,
    /// The model of placement `n` (wall layer 2). Give only the placements the game collides with: the
    /// map's (the pack's collision holds no others; `extract_scene::deathmatch_solid` from the ROM).
    Placement(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    /// Vertices in game units, rounded to integers as the game stores them (s16).
    pub v: [[i32; 3]; 3],
    pub surface: u32,
    /// Unit normal (float, from the vertices); `normal[1] > 0` faces up.
    pub normal: [f32; 3],
}

/// The floor under a point: height, triangle index and its surface word.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(
    feature = "net",
    derive(serde::Serialize, serde::Deserialize, bevy::reflect::Reflect)
)]
pub struct Floor {
    pub y: f32,
    pub tri: u32,
    pub surface: u32,
}

#[derive(Clone, Debug, Default)]
pub struct CollisionWorld {
    /// Every triangle; the floor query searches them all.
    pub tris: Vec<Triangle>,
    origin: [i32; 2],
    dims: [i32; 2],
    cells: Vec<Vec<u32>>,
    /// Wall layers 0 (terrain) and 1 (secondary): their triangles (indices into `tris`) and quadtrees.
    layer_tris: [Vec<u32>; 2],
    walls: [Tree; 2],
    /// Wall layer 2: each placement's triangles, in the given (placement) order.
    objects: Vec<std::ops::Range<u32>>,
    /// Each of `objects`' placement number and how far up it is moved ([`CollisionWorld::shift`]).
    placed: Vec<(u16, i32)>,
    /// The terrain's surface words: the flag table the wall routine reads for every static layer.
    terrain_words: Vec<u32>,
    /// The game's acos table, from the ROM's `.game` data ([`AcosTable::from_game_data`]): the push angle
    /// of a wall (state 122's heading) and the stick's direction use it; without it, a close arctangent
    /// (a few units off).
    pub acos: Option<AcosTable>,
    /// The scene's camera data (`CameraLevel::new(scene, &rom.asset(&[0x0C, scene, 5])?)`): the volumes
    /// that turn the camera's wall checks off, and the zones where its sweep skips the placements.
    /// Without it the camera behaves as inside a volume everywhere (wave 2: no wall ray, no sweep).
    pub camera: Option<CameraLevel>,
    /// The character's root-motion clips ([`crate::root`]), from the ROM; empty: every clip moves by speed.
    pub roots: Vec<RootTrack>,
    /// The sink table per surface kind (the asset pack's `game/sink.json`); None: the game's own
    /// ([`crate::movement::SURFACES`]).
    pub sink: Option<[[i8; 3]; 32]>,
}

impl CollisionWorld {
    /// The root track of animation `id`, if it moves by its root.
    pub fn root_track(&self, id: u16) -> Option<&RootTrack> {
        self.roots.iter().find(|t| t.ids.contains(&id))
    }

    /// Builds the world from world-space triangles and their surface words (0 where a mesh has none). Every
    /// triangle is also a terrain wall; use [`CollisionWorld::layered`] for the game's wall layers.
    pub fn new(triangles: impl IntoIterator<Item = ([[f32; 3]; 3], u32)>) -> Self {
        Self::layered(triangles.into_iter().map(|(p, s)| (p, s, Layer::Terrain)))
    }

    /// The world with the game's wall layers: extract-scene's triangles in its order (terrain, secondary,
    /// placements), each with its surface word and [`Layer`].
    pub fn layered(triangles: impl IntoIterator<Item = ([[f32; 3]; 3], u32, Layer)>) -> Self {
        let mut layers = Vec::new();
        let tris: Vec<Triangle> = triangles
            .into_iter()
            .map(|(p, surface, layer)| {
                layers.push(layer);
                Triangle {
                    v: p.map(|q| q.map(|c| c.round() as i32)),
                    surface,
                    normal: normal(&p),
                }
            })
            .collect();
        let mut world = Self::grid(tris);
        for (i, layer) in layers.iter().enumerate() {
            match *layer {
                Layer::Terrain => {
                    world.layer_tris[0].push(i as u32);
                    world.terrain_words.push(world.tris[i].surface);
                }
                Layer::Secondary => world.layer_tris[1].push(i as u32),
                Layer::Placement(n) => match world.objects.last_mut() {
                    Some(r) if layers[i - 1] == *layer => r.end += 1,
                    _ => {
                        world.objects.push(i as u32..i as u32 + 1);
                        world.placed.push((n, 0));
                    }
                },
            }
        }
        for k in 0..2 {
            let refs: Vec<&Triangle> = world.layer_tris[k]
                .iter()
                .map(|&i| &world.tris[i as usize])
                .collect();
            world.walls[k] = Tree::build(&refs);
        }
        world
    }

    /// Moves placement `n`'s triangles to `dy` above where they were built (a prop that moves: War:
    /// Beach's plunger). The floor query and the object walls read the triangles live; the grid is
    /// XZ only.
    pub fn shift(&mut self, n: u16, dy: i32) {
        for (range, (id, at)) in self.objects.iter().zip(&mut self.placed) {
            if *id == n && *at != dy {
                for t in &mut self.tris[range.start as usize..range.end as usize] {
                    t.v.iter_mut().for_each(|v| v[1] += dy - *at);
                }
                *at = dy;
            }
        }
    }

    /// The wall layer `k`'s quadtree (0 terrain, 1 secondary), one node per line in walk order: the game
    /// builds the same tree at load (checked against its RAM in `tests/oracle.rs`).
    pub fn wall_tree(&self, k: usize) -> String {
        self.walls[k].dump()
    }

    fn grid(tris: Vec<Triangle>) -> Self {
        let bbox = |t: &Triangle, axis: usize| {
            let c = t.v.map(|v| v[axis]);
            (
                c.iter().min().unwrap() - MARGIN,
                c.iter().max().unwrap() + MARGIN,
            )
        };
        let (mut lo, mut hi) = ([i32::MAX; 2], [i32::MIN; 2]);
        for t in &tris {
            for (k, axis) in [0, 2].into_iter().enumerate() {
                let (a, b) = bbox(t, axis);
                lo[k] = lo[k].min(a);
                hi[k] = hi[k].max(b);
            }
        }
        if tris.is_empty() {
            return Self::default();
        }
        let dims = [0, 1].map(|k| (hi[k] - lo[k]) / CELL + 1);
        let mut cells = vec![Vec::new(); (dims[0] * dims[1]) as usize];
        for (i, t) in tris.iter().enumerate() {
            let ((x0, x1), (z0, z1)) = (bbox(t, 0), bbox(t, 2));
            for cz in (z0 - lo[1]) / CELL..=(z1 - lo[1]) / CELL {
                for cx in (x0 - lo[0]) / CELL..=(x1 - lo[0]) / CELL {
                    cells[(cz * dims[0] + cx) as usize].push(i as u32);
                }
            }
        }
        Self {
            tris,
            origin: lo,
            dims,
            cells,
            ..Self::default()
        }
    }

    /// Triangle indices whose XZ bounds (plus 32) may contain the integer point.
    pub fn near(&self, x: i32, z: i32) -> &[u32] {
        let (cx, cz) = ((x - self.origin[0]) / CELL, (z - self.origin[1]) / CELL);
        if x < self.origin[0] || z < self.origin[1] || cx >= self.dims[0] || cz >= self.dims[1] {
            return &[];
        }
        &self.cells[(cz * self.dims[0] + cx) as usize]
    }

    /// The game's floor query at `(x, z)`. `prev` is last frame's floor triangle (fast path, rounded
    /// position); otherwise every triangle under the truncated position is a candidate. Among those below
    /// `max(y, ref_y) + 50`, the lowest at or above `ref_y` wins, else the highest below it; when `y` is
    /// below `ref_y`, simply the highest (`ref_y` is the height at the start of the frame, `y` the current
    /// one).
    pub fn floor(&self, x: f32, z: f32, y: f32, ref_y: f32, prev: Option<u32>) -> Option<Floor> {
        if let Some(p) = prev
            && let Some(h) = height(&self.tris[p as usize].v, round_even(x), round_even(z))
        {
            return Some(self.hit(p, h));
        }
        let (xi, zi) = (x as i32, z as i32);
        let limit = if y < ref_y { ref_y } else { y } + STEP_UP;
        let mut best: Option<(f32, u32)> = None;
        for &i in self.near(xi, zi) {
            let Some(h) = height(&self.tris[i as usize].v, xi, zi) else {
                continue;
            };
            let h = h as f32 / 256.0;
            if h >= limit {
                continue;
            }
            let take = match best {
                None => true,
                Some((b, _)) if y < ref_y || h < ref_y => b < h,
                Some((b, _)) => b < ref_y || h < b,
            };
            if take {
                best = Some((h, i));
            }
        }
        best.map(|(h, i)| Floor {
            y: h,
            tri: i,
            surface: self.tris[i as usize].surface,
        })
    }

    fn hit(&self, tri: u32, h: i32) -> Floor {
        Floor {
            y: h as f32 / 256.0,
            tri,
            surface: self.tris[tri as usize].surface,
        }
    }
}

fn normal(p: &[[f32; 3]; 3]) -> [f32; 3] {
    let u = [0, 1, 2].map(|i| p[1][i] - p[0][i]);
    let v = [0, 1, 2].map(|i| p[2][i] - p[0][i]);
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        [0.0; 3]
    } else {
        n.map(|c| c / len)
    }
}

/// `cvt.w.s` in the default rounding mode: nearest, ties to even.
fn round_even(v: f32) -> i32 {
    v.round_ties_even() as i32
}

/// MIPS `div` as the recompiled game runs it: truncating, x/0 = -1 (x >= 0) or 1.
fn div(a: i32, b: i32) -> i32 {
    match b {
        0 if a >= 0 => -1,
        0 => 1,
        _ => (a as i64 / b as i64) as i32,
    }
}

/// The game's integer height of triangle `v` at integer `(x, z)`, in 1/256 units; None outside.
pub fn height(v: &[[i32; 3]; 3], x: i32, z: i32) -> Option<i32> {
    let [mut a, mut b, mut c] = *v;
    // a = lowest z, c = highest, b between (the game's exact swap order, ties included).
    if a[2] < b[2] {
        if c[2] < a[2] {
            (a, c) = (c, a);
        }
    } else if b[2] < c[2] {
        (a, b) = (b, a);
    } else {
        (a, c) = (c, a);
    }
    if z < a[2] {
        return None;
    }
    if c[2] < b[2] {
        (b, c) = (c, b);
    }
    if c[2] < z {
        return None;
    }
    let long = c[2] - a[2] + 1;
    let xa = (z - a[2]).wrapping_mul(div((c[0] - a[0]) << 8, long)) >> 8;
    let xa = xa + a[0];
    let (s0, s1, t) = if z < b[2] {
        (a, b, z - a[2])
    } else {
        (b, c, z - b[2])
    };
    let short = s1[2] - s0[2] + 1;
    let xb = (t.wrapping_mul(div((s1[0] - s0[0]) << 8, short)) >> 8) + s0[0];
    let s = if xb - x < xa - x { 1 } else { -1 };
    if ((xa - x + s) ^ (xb - x - s)) >= 0 {
        return None;
    }
    let ymax = a[1].max(b[1]).max(c[1]) << 8;
    let ymin = a[1].min(b[1]).min(c[1]) << 8;
    let ya = (a[1] << 8).wrapping_add((z - a[2]).wrapping_mul(div((c[1] - a[1]) << 8, long)));
    let yb = (s0[1] << 8).wrapping_add(t.wrapping_mul(div((s1[1] - s0[1]) << 8, short)));
    let slope = div(ya.wrapping_sub(yb) << 8, xa - xb);
    let y = yb.wrapping_add((x - xb).wrapping_mul(slope) >> 8);
    Some(y.min(ymax).max(ymin))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Scene 0x2D's ramp north of the spawn (triangles 716/717) and the flat floor before it (620).
    const RAMP: [[i32; 3]; 3] = [
        [-2624, -344, 4345],
        [-2624, -521, 5033],
        [-1935, -521, 5033],
    ];

    fn world() -> CollisionWorld {
        let f = |v: [[i32; 3]; 3]| v.map(|p| p.map(|c| c as f32));
        CollisionWorld::new([
            (
                f([
                    [-1935, -521, 5033],
                    [-2624, -521, 5033],
                    [-1867, -521, 6409],
                ]),
                5,
            ),
            (f(RAMP), 3),
            (
                f([
                    [-1935, -521, 5033],
                    [-1935, -344, 4345],
                    [-2624, -344, 4345],
                ]),
                3,
            ),
        ])
    }

    #[test]
    fn integer_height_matches_traced_floors() {
        // run_a row 127: the first frame on the ramp, full query at the truncated position; the trace's
        // floor_y -514.878906 is -131809 / 256.
        assert_eq!(height(&RAMP, -2291, 5018), Some(-131_809));
        // 8-bit slope: -177/689 * 256 truncates to -65, so 673 rows down is -344 - 673 * 65 / 256.
        assert_eq!(height(&RAMP, -2291, 5018).unwrap(), -344 * 256 - 673 * 65);
        assert_eq!(height(&RAMP, -1900, 5018), None);
        assert_eq!(height(&RAMP, -2291, 5040), None);
    }

    /// A placement moved down 60 is floor 60 lower (from its last triangle too), back at 0 where it
    /// was built; others stay.
    #[test]
    fn a_placement_shifts() {
        let quad = |y: f32, layer| {
            let [a, b, c, d] = [
                [-50.0, y, -50.0],
                [-50.0, y, 50.0],
                [50.0, y, 50.0],
                [50.0, y, -50.0],
            ];
            [([a, b, c], 5, layer), ([a, c, d], 5, layer)]
        };
        let mut w = CollisionWorld::layered(
            [
                quad(0.0, Layer::Terrain),
                quad(100.0, Layer::Placement(5)),
                quad(200.0, Layer::Placement(3)),
            ]
            .concat(),
        );
        let at = |w: &CollisionWorld, y| w.floor(0.0, 0.0, y, y, None).map(|f| f.y);
        w.shift(5, -60);
        w.shift(5, -60);
        assert_eq!(
            (at(&w, 60.0), at(&w, 110.0), at(&w, 300.0)),
            (Some(40.0), Some(40.0), Some(200.0))
        );
        assert_eq!(w.floor(0.0, 0.0, 40.0, 40.0, Some(2)).unwrap().y, 40.0);
        w.shift(5, 0);
        assert_eq!(at(&w, 120.0), Some(100.0));
    }

    #[test]
    fn fast_path_rounds_full_query_truncates() {
        let w = world();
        // run_a row 128 at z = 5001.86: the fast path on triangle 1 rounds z to 5002.
        let fast = w
            .floor(-2291.895, 5001.861, -514.9, -514.9, Some(1))
            .unwrap();
        assert_eq!((fast.y, fast.tri, fast.surface), (-510.816_4, 1, 3));
        // Without a previous triangle the full query truncates z to 5001.
        let full = w.floor(-2291.895, 5001.861, -514.9, -514.9, None).unwrap();
        assert_eq!(full.y, -510.5625);
    }

    #[test]
    fn full_query_prefers_lowest_floor_at_or_above_reference() {
        let tri = |y: i32| {
            (
                [
                    [0., y as f32, 0.],
                    [100., y as f32, 0.],
                    [0., y as f32, 100.],
                ],
                y as u32,
            )
        };
        let w = CollisionWorld::new([tri(-40), tri(10), tri(30), tri(70)]);
        let pick = |y: f32| w.floor(10.0, 10.0, y, y, None).map(|f| f.y);
        assert_eq!(pick(0.0), Some(10.0)); // 10 and 30 are in [0, 50): the lowest wins
        assert_eq!(pick(25.0), Some(30.0));
        assert_eq!(pick(-20.0), Some(10.0)); // 10 < 30 (limit -20 + 50)
        assert_eq!(pick(80.0), Some(70.0)); // nothing at or above 80: the highest below
        assert_eq!(pick(-100.0), None); // everything is 50 or more above
        // Falling (y below the reference): the highest under ref + 50.
        assert_eq!(
            w.floor(10.0, 10.0, -20.0, 0.0, None).map(|f| f.y),
            Some(30.0)
        );
    }

    #[test]
    fn grid_finds_triangles_across_cells() {
        let w = world();
        for (x, z) in [(-2291, 5018), (-1950, 4400), (-1874, 6400)] {
            assert!(
                w.near(x, z)
                    .iter()
                    .any(|&i| height(&w.tris[i as usize].v, x, z).is_some())
            );
        }
        assert!(w.near(100_000, 0).is_empty());
    }
}
