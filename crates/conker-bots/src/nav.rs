//! The level's navigation graph and the game's path search, ported from the code (C) and checked on
//! the oracle's bot matches (V, `tests/oracle.rs`).
//!
//! The graph is the scene's asset `[0x19, scene]` (`func_15085B70`): a u16 node count, a u16, then
//! 16-byte nodes. Nodes with `barrier == 1` are not walked to: linked pairs of them are wall lines
//! that a straight move may not cross (`func_15086D94`).

use conker_camera::volume::CameraVolume;

/// No node (the game's 0xFF).
pub const NONE: u8 = 0xFF;
/// Extra cost of a node to avoid (`func_15086364`).
const AVOID_COST: i32 = 5000;
/// Distance² beyond any node (`D_8009D9CC`, `D_8009D9D0`).
const FAR2: f32 = 100_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// +0: position.
    pub pos: [i16; 3],
    /// +6: the height band ([`Graph::band`]) the node belongs to.
    pub band: u8,
    /// +7: the id the level script names it by (`func_15086D48`).
    pub id: u8,
    /// +8: half the width of the walkway: a walker aims up to twice this to one side.
    pub half: u8,
    /// +9: up to five neighbours ([`NONE`] = empty).
    pub next: [u8; 5],
    /// +0xE: 1 = a barrier point.
    pub barrier: u8,
}

/// The graph, with what the game derives from it at load.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: Vec<Node>,
    /// `D_800D2360` / `D_800D237C` (`func_15085BE8`): a height is in band `ids[k]` for the first k
    /// with `height < bounds[k]`.
    bounds: Vec<f32>,
    ids: Vec<u8>,
    /// Per node, the first type-3 zone of the level (`func_15086C70`) it lies in.
    zones: Vec<Option<usize>>,
    volumes: Vec<CameraVolume>,
}

fn f(v: i16) -> f32 {
    f32::from(v)
}

impl Graph {
    /// Parses `[0x19, scene]`; `triggers` is the scene's trigger table `[0x0C, scene, 5]` (0x34-byte
    /// records), whose type-3 action-0xB records are the zones the path search walks straight across.
    pub fn parse(asset: &[u8], triggers: &[u8]) -> Option<Self> {
        let count = usize::from(u16::from_be_bytes([*asset.first()?, *asset.get(1)?]));
        let body = asset.get(4..4 + 16 * count)?;
        let nodes: Vec<Node> = body
            .as_chunks::<16>()
            .0
            .iter()
            .map(|r| {
                let h = |o: usize| i16::from_be_bytes([r[o], r[o + 1]]);
                Node {
                    pos: [h(0), h(2), h(4)],
                    band: r[6],
                    id: r[7],
                    half: r[8],
                    next: [r[9], r[10], r[11], r[12], r[13]],
                    barrier: r[14],
                }
            })
            .collect();
        let volumes: Vec<CameraVolume> = triggers
            .as_chunks::<0x34>()
            .0
            .iter()
            .filter(|r| r[0x14] == 0 && r[0x15] >> 2 == 3 && r[0x17] == 0xB)
            .map(CameraVolume::parse_record)
            .collect();
        let zones = nodes
            .iter()
            .map(|n| volumes.iter().position(|v| v.contains(n.pos.map(f))))
            .collect();
        let (bounds, ids) = bands(&nodes);
        Some(Self {
            nodes,
            bounds,
            ids,
            zones,
            volumes,
        })
    }

    pub fn node(&self, i: u8) -> Option<&Node> {
        self.nodes.get(usize::from(i))
    }

    /// `func_15086D48`: the node with level-script id `id`.
    pub fn by_id(&self, id: i32) -> u8 {
        self.nodes
            .iter()
            .position(|n| i32::from(n.id) == id)
            .map_or(NONE, |i| i as u8)
    }

    /// `func_15085DA8`: the height band of `y`.
    pub fn band(&self, y: f32) -> u8 {
        let k = self.bounds.iter().take_while(|&&b| b <= y).count();
        self.ids.get(k).copied().unwrap_or(0)
    }

    fn pos(&self, i: u8) -> [f32; 3] {
        self.nodes[usize::from(i)].pos.map(f)
    }

    /// `func_15086BD0`: 3D distance between two nodes, 0 if either is [`NONE`].
    pub fn dist(&self, a: u8, b: u8) -> f32 {
        if a == NONE || b == NONE {
            return 0.0;
        }
        let (p, q) = (
            self.nodes[usize::from(a)].pos,
            self.nodes[usize::from(b)].pos,
        );
        let d: [f32; 3] = std::array::from_fn(|k| (i32::from(p[k]) - i32::from(q[k])) as f32);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    /// `func_15086D94`: a straight move from `p` by (`dx`, `dz`) crosses a barrier line of `p`'s band:
    /// the move's length times the crossing's fraction, else −1. The fraction is the last one computed
    /// for a crossed line, not the nearest (the game's own quirk).
    pub fn crossing(&self, p: [f32; 3], dx: f32, dz: f32) -> f32 {
        let band = self.band(p[1]);
        let (x, z) = (p[0], p[2]);
        let (mut best, mut last) = (100.0f32, 0.0f32);
        for (i, a) in self.nodes.iter().enumerate() {
            if a.barrier != 1 || a.band != band {
                continue;
            }
            for &nb in &a.next {
                if nb == NONE || usize::from(nb) <= i || self.nodes[usize::from(nb)].barrier != 1 {
                    continue;
                }
                let b = &self.nodes[usize::from(nb)];
                let (ax, az, bx, bz) = (
                    i32::from(a.pos[0]),
                    i32::from(a.pos[2]),
                    i32::from(b.pos[0]),
                    i32::from(b.pos[2]),
                );
                let n0 = (bz - az) as f32;
                let n1 = -((bx - ax) as f32);
                let (fax, faz) = (ax as f32, az as f32);
                let c = -(fax * n0 + n1 * faz);
                let s0 = (x * n0 + z * n1) + c;
                let s1 = ((x + dx) * n0 + (z + dz) * n1) + c;
                if !((s1 < 0.0 && 0.0 <= s0) || (s0 < 0.0 && 0.0 <= s1)) {
                    continue;
                }
                let (e0, e1) = (s0.abs(), s1.abs());
                let t = e0 / (e0 + e1);
                last = t;
                let (m0, m1) = (-n1, n0);
                let c2 = -(fax * m0 + m1 * faz);
                let along = ((t * dx + x) * m0 + (t * dz + z) * m1) + c2;
                let end = ((bx as f32) * m0 + m1 * (bz as f32)) + c2;
                let on = (0.0 < end && 0.0 < along && along <= end)
                    || (end < 0.0 && along < 0.0 && end <= along);
                if on && t < best {
                    best = t;
                }
            }
        }
        if best <= 1.0 {
            (dx * dx + dz * dz).sqrt() * last
        } else {
            -1.0
        }
    }

    /// `func_15085DF8`: the nearest node of band `band` (`None`: any) whose barrier flag is `barrier`
    /// (`None`: any) and, for walkable nodes, reachable from `p` without crossing a barrier line;
    /// with its distance (`D_800D2354`).
    pub fn nearest(&self, p: [f32; 3], barrier: Option<u8>, band: Option<u8>) -> (u8, i32) {
        let check = barrier == Some(0);
        let (mut best, mut found) = (FAR2, NONE);
        for (i, n) in self.nodes.iter().enumerate() {
            if band.is_some_and(|b| b != n.band) || barrier.is_some_and(|b| b != n.barrier) {
                continue;
            }
            let d = [f(n.pos[0]) - p[0], f(n.pos[1]) - p[1], f(n.pos[2]) - p[2]];
            let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            if d2 < best && (!check || self.crossing(p, d[0], d[2]) < 0.0) {
                (best, found) = (d2, i as u8);
            }
        }
        (found, best.sqrt() as i32)
    }

    /// `func_15086364`: from node `start`, Dijkstra over the 3D hop lengths (whole units, +5000 into a
    /// node to avoid) until a goal comes off the queue. Returns the node to walk to next (runs of
    /// nodes inside one zone are crossed in one go), the path's length (`D_800D2354`) and the goal
    /// reached (`D_80087298`); [`NONE`] when there is no path, `start` itself when it is a goal.
    pub fn next_toward(&self, start: u8, goals: &[u8], avoid: &[u8]) -> (u8, i32, u8) {
        let count = self.nodes.len();
        if goals.is_empty() || usize::from(start) >= count {
            return (NONE, 0, NONE);
        }
        if goals.contains(&start) {
            return (start, 0, start);
        }
        let mut dist = vec![1_000_000i32; count];
        let mut prev = vec![NONE; count];
        let mut open: Vec<u8> = Vec::with_capacity(count);
        let cost =
            |a: u8, b: u8| self.dist(a, b) as i32 + if avoid.contains(&b) { AVOID_COST } else { 0 };
        for &nb in &self.nodes[usize::from(start)].next {
            if nb != NONE {
                open.push(nb);
                prev[usize::from(nb)] = start;
                dist[usize::from(nb)] = i32::from(self.dist(start, nb) as i32 as i16)
                    + if avoid.contains(&nb) { AVOID_COST } else { 0 };
            }
        }
        let mut path = Vec::new();
        loop {
            let mut pick = None;
            let mut least = 1_000_000;
            for (k, &n) in open.iter().enumerate() {
                if n != NONE && dist[usize::from(n)] < least {
                    (least, pick) = (dist[usize::from(n)], Some(k));
                }
            }
            let Some(k) = pick else { break };
            let s = open[k];
            open[k] = NONE;
            if k + 1 == open.len() {
                open.pop();
            }
            if goals.contains(&s) {
                let mut n = s;
                while n != start {
                    path.push(n);
                    n = prev[usize::from(n)];
                }
                path.push(start);
                break;
            }
            for &nb in &self.nodes[usize::from(s)].next {
                if nb == NONE || nb == start {
                    continue;
                }
                let nd = cost(s, nb) + dist[usize::from(s)];
                if nd >= dist[usize::from(nb)] {
                    continue;
                }
                dist[usize::from(nb)] = nd;
                prev[usize::from(nb)] = s;
                if !open.contains(&nb) {
                    match open.iter().position(|&o| o == NONE) {
                        Some(free) => open[free] = nb,
                        None => open.push(nb),
                    }
                }
            }
        }
        if path.len() < 2 {
            return (NONE, 0, NONE);
        }
        let reached = path[0];
        self.smooth(&path, reached)
    }

    /// The tail of `func_15086364` over the path `[goal, …, start]`: the length with zone runs taken
    /// straight, and the next node (past a zone run unless the walker is entering or leaving it).
    fn smooth(&self, path: &[u8], reached: u8) -> (u8, i32, u8) {
        let z: Vec<bool> = path
            .iter()
            .map(|&n| self.zones[usize::from(n)].is_some())
            .collect();
        let mut k = path.len() - 2;
        let mut next = path[k];
        let mut settled = z[k] && (!z[k + 1] || (k > 0 && !z[k - 1]));
        let mut length = 0.0f32;
        while k > 0 {
            let mut s = k - 1;
            if z[k - 1] {
                while s > 0 && z[s - 1] {
                    s -= 1;
                }
            }
            length += self.dist(path[k], path[s]);
            if !settled {
                settled = true;
                if z[k] {
                    next = path[s];
                }
            }
            k = s;
        }
        (next, length as i32, reached)
    }

    /// `func_15086098`: the first node to walk to from `p` toward the goals: the nearest node of
    /// `p`'s band, skipped for the next one on the path when `p` is already past it (inside its zone,
    /// within twice its width, or alongside the edge to that next node).
    pub fn approach(&self, p: [f32; 3], goals: &[u8], avoid: &[u8]) -> u8 {
        let (start, _) = self.nearest(p, Some(0), Some(self.band(p[1])));
        if goals.is_empty() || start == NONE {
            return start;
        }
        let next = |s| self.next_toward(s, goals, avoid).0;
        let mut best = NONE;
        let ip = p.map(|v| v as i32 as f32);
        if let Some(v) = self.zones[usize::from(start)]
            && self.volumes[v].contains(ip)
        {
            best = next(start);
        } else {
            let n = &self.nodes[usize::from(start)];
            let w = 2.0 * f32::from(n.half);
            if FAR2 < w * w {
                best = next(start);
            } else {
                let (sx, sz) = (f(n.pos[0]), f(n.pos[2]));
                let (mut least, mut side) = (FAR2, NONE);
                for &nb in &n.next {
                    if nb == NONE {
                        continue;
                    }
                    let b = &self.nodes[usize::from(nb)];
                    let dx = (i32::from(b.pos[0]) - i32::from(n.pos[0])) as f32;
                    let dz = (i32::from(b.pos[2]) - i32::from(n.pos[2])) as f32;
                    let inv = 1.0 / (dx * dx + dz * dz).sqrt();
                    let (ux, uz) = (dx * inv, dz * inv);
                    let t = (p[0] * ux + p[2] * uz) + -(sx * ux + sz * uz);
                    if 0.0 < t {
                        let (ex, ez) = (p[0] - (sx + ux * t), p[2] - (sz + uz * t));
                        let d2 = ex * ex + ez * ez;
                        if d2 < least {
                            (least, side) = (d2, nb);
                        }
                    }
                }
                if side != NONE && next(start) == side {
                    best = side;
                }
            }
        }
        if best != NONE { best } else { start }
    }

    /// The node's position.
    pub fn at(&self, i: u8) -> [f32; 3] {
        self.pos(i)
    }
}

/// `func_15085BE8`: per band present, the boundary halfway between its highest node and the next
/// band's lowest; the last bound is 100000.
fn bands(nodes: &[Node]) -> (Vec<f32>, Vec<u8>) {
    let mut lo = [10_000.0f32; 6];
    let mut hi = [-10_000.0f32; 6];
    for n in nodes {
        if let Some(b) = lo.get_mut(usize::from(n.band)) {
            let y = f(n.pos[1]);
            *b = b.min(y);
            hi[usize::from(n.band)] = hi[usize::from(n.band)].max(y);
        }
    }
    let ids: Vec<u8> = (0..6u8)
        .filter(|&b| lo[usize::from(b)] <= hi[usize::from(b)])
        .collect();
    let mut bounds: Vec<f32> = ids
        .windows(2)
        .map(|w| (lo[usize::from(w[1])] + hi[usize::from(w[0])]) * 0.5)
        .collect();
    bounds.push(100_000.0);
    (bounds, ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(pos: [i16; 3], band: u8, next: [u8; 5], barrier: u8) -> Node {
        Node {
            pos,
            band,
            id: 0,
            half: 20,
            next,
            barrier,
        }
    }

    /// A line 0-1-2-3 along x, with a shortcut 0-3 that costs more, and a barrier pair 4-5 across z.
    fn graph() -> Graph {
        let n = NONE;
        let nodes = vec![
            node([0, 0, 0], 0, [1, 3, n, n, n], 0),
            node([100, 0, 0], 0, [0, 2, n, n, n], 0),
            node([200, 0, 0], 0, [1, 3, n, n, n], 0),
            node([300, 0, 0], 0, [2, 0, n, n, n], 0),
            node([50, 0, -500], 0, [5, n, n, n, n], 1),
            node([50, 0, 500], 0, [4, n, n, n, n], 1),
            node([0, 300, 0], 1, [n; 5], 0),
        ];
        let (bounds, ids) = bands(&nodes);
        Graph {
            zones: vec![None; nodes.len()],
            nodes,
            bounds,
            ids,
            volumes: Vec::new(),
        }
    }

    #[test]
    fn bands_split_halfway() {
        let g = graph();
        assert_eq!(g.bounds, [150.0, 100_000.0]);
        assert_eq!((g.band(149.0), g.band(150.0)), (0, 1));
    }

    #[test]
    fn dijkstra_takes_the_cheaper_way_and_avoids() {
        let g = graph();
        // 0 → 3 directly is 300, the same as through 1 and 2: the first queued wins.
        assert_eq!(g.next_toward(0, &[3], &[]).0, 3);
        assert_eq!(g.next_toward(0, &[2], &[]), (1, 100, 2));
        // Avoiding 1 makes 0 → 3 → 2 cheaper.
        assert_eq!(g.next_toward(0, &[2], &[1]).0, 3);
        assert_eq!(g.next_toward(2, &[2], &[]), (2, 0, 2));
        assert_eq!(g.next_toward(0, &[], &[]).0, NONE);
    }

    #[test]
    fn barrier_lines_block_and_measure() {
        let g = graph();
        // From (0, 0, 0) to (100, 0, 0) crosses the 4-5 line at x = 50: halfway.
        assert_eq!(g.crossing([0.0, 0.0, 0.0], 100.0, 0.0), 50.0);
        assert_eq!(g.crossing([0.0, 0.0, 0.0], 40.0, 0.0), -1.0);
        // The nearest walkable node of band 0 to (60, 0, 10) is 1, behind the line from there? No:
        // 1 is at x = 100, on the same side. From (40, 0, 0), node 1 is across the line, so 0 wins.
        assert_eq!(g.nearest([60.0, 0.0, 10.0], Some(0), Some(0)).0, 1);
        assert_eq!(g.nearest([45.0, 0.0, 0.0], Some(0), Some(0)).0, 0);
    }

    #[test]
    fn approach_skips_a_passed_node() {
        let g = graph();
        // Halfway between 1 and 2, heading for 3: the nearest is 1 or 2; alongside 1 → 2, go to 2.
        assert_eq!(g.approach([140.0, 0.0, 5.0], &[3], &[]), 2);
    }
}
