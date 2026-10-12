//! What a bot's character does each frame: the character class's behaviour scripts (5-byte commands,
//! `func_1507BC14`; the scripts are the class data `[0x0F, 0]` from the ROM) and the steering toward
//! the walk target (`func_15056B08`), named after the game object's fields (C).
//!
//! The game steers a bot directly (move angle and speed). Here a bot plays through the same input as
//! a human, so the steering ends in a desired direction and speed that [`crate::Bots`] turns into a
//! stick (I: the human acceleration, turning and braking apply instead of the class's own).

use crate::nav::{Graph, NONE};
use conker_rules::Rng;

/// The script every id a class lacks runs (`0x80086A88` → `0x8009A120` in the game's data, C/V the
/// oracle's RAM): 0x67, 0x31, 0x33 (turn and speed setup), wait 20 frames, go to the same script.
const DEFAULT_SCRIPT: [u8; 25] = [
    0x67, 0x14, 0x0F, 0x19, 0x00, 0x31, 0x80, 0x00, 0x00, 0x00, 0x33, 0xFF, 0x00, 0x00, 0x00, 0x01,
    0x14, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00,
];
/// An id no class script has (the default script's place in [`Scripts`]' table).
const DEFAULT_ID: u8 = 0;
/// `wait` that never runs out (`0x639C`, 255 · 100).
const FOREVER: u16 = 25_500;
/// Script commands a frame may run before the script is called stuck.
const STEPS: usize = 64;

/// The class's behaviour scripts: `[0x0F, 0]` is a 0x10-byte header, then the class data. Header
/// word 0 is the script table's offset from the asset start: (u32 offset into the class data, u8 id,
/// 3 bytes) records, 0-ended (V: the oracle's RAM, `0x800D1588[0x75]` → 0x80207C30, table at header
/// + 0x7AAC, script 1 at data + 0x6AC0).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scripts {
    data: Vec<u8>,
    table: Vec<(u8, usize)>,
}

impl Scripts {
    pub fn parse(asset: &[u8]) -> Option<Self> {
        let word =
            |o: usize| Some(u32::from_be_bytes(asset.get(o..o + 4)?.try_into().ok()?) as usize);
        let mut data = asset.get(0x10..)?.to_vec();
        let mut table = Vec::new();
        let mut at = word(0)?;
        while let (Some(off), Some(&id)) = (word(at), asset.get(at + 4)) {
            if off == 0 {
                break;
            }
            table.push((id, off));
            at += 8;
        }
        // The game's fallback, appended to the data where `start` finds it.
        table.push((DEFAULT_ID, data.len()));
        data.extend_from_slice(&DEFAULT_SCRIPT);
        Some(Self { data, table })
    }

    /// `func_1507BB28`: where script `id` starts; ids the class lacks get the game's default script.
    fn start(&self, id: u8) -> usize {
        self.table
            .iter()
            .find(|e| e.0 == id)
            .or(self.table.last())
            .map_or(0, |e| e.1)
    }

    fn byte(&self, at: usize) -> u8 {
        self.data.get(at).copied().unwrap_or(0)
    }
}

/// The walk and fight state of one participant (the game object's AI fields).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Npc {
    /// +0x20F current node, +0x210 previous, +0x211 goal, +0x212 node to avoid, +0x213 last nearest.
    pub cur: u8,
    pub prev: u8,
    pub goal: u8,
    pub avoid: u8,
    pub near: u8,
    /// +0x216: path length left beyond the current node.
    pub length: i32,
    /// +0x232 script, +0x218 position in the class data, +0x21C wait (hundredths of frames).
    pub script: u8,
    pub at: Option<usize>,
    pub wait: u16,
    /// +0x223: 0 walk to the waypoint, 1 chase the target, 0x11 wander to the waypoint.
    pub mode: u8,
    /// +0x222: the player chased.
    pub target: u8,
    /// +0x44: the speed asked for.
    pub speed: f32,
    /// +0x23E: arrival radius / 8.
    pub radius: u8,
    /// +0x25C and +0xF4 flag words.
    pub flags: u32,
    pub f4: u32,
    /// The waypoint the path search set (the bot's path record, waypoint 0).
    pub waypoint: [i16; 3],
    /// The virtual pad (struct126 +0x8A held, +0x8C pressed, +0x8E/+0x8F frames they stay).
    pub held: u16,
    pub pressed: u16,
    pub hold: i8,
    pub press: i8,
    /// struct126 +0x1AF: the pickup radius · 64/100 the AI script asks for.
    pub pickup: u8,
    /// struct126 +0x16C: the aim yaw in degrees while R is held, and the point aimed at this frame.
    pub aim_yaw: Option<f32>,
    pub aim: Option<[f32; 3]>,
    /// s126+0x85, +0x86: the virtual stick a mounted gun's scope turns with ([`crate::Bots`]).
    pub stick: Option<[i8; 2]>,
    /// +0x22C bit 0x200: placed since the AI's last frame (the match start, a respawn, a lift). The
    /// placement (`func_15085430`) sets it; the AI's frame takes it: state 0 and a replan (V, the
    /// oracle on 0x30: p6 lifted at 323, cleared with the state at 324).
    pub placed: bool,
}

impl Default for Npc {
    fn default() -> Self {
        Self {
            cur: NONE,
            prev: NONE,
            goal: NONE,
            avoid: NONE,
            near: NONE,
            length: 0,
            script: 1,
            at: None,
            wait: 0,
            mode: 0,
            target: 0,
            speed: 0.0,
            radius: 10,
            flags: 0,
            f4: 0,
            waypoint: [0; 3],
            held: 0,
            pressed: 0,
            hold: 0,
            press: 0,
            pickup: 64,
            aim_yaw: None,
            aim: None,
            stick: None,
            placed: false,
        }
    }
}

/// `+0x25C` bit: the goal is reached (or there is none).
pub const AT_GOAL: u32 = 0x800;

/// What the steering asks of the body this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Steer {
    /// The point walked to (the waypoint, or the chased player).
    pub toward: [f32; 3],
    pub speed: f32,
}

/// The world a bot's character sees: its position, facing, and the other participants.
pub struct View<'a> {
    pub pos: [f32; 3],
    pub facing: u16,
    pub players: &'a [[f32; 3]],
}

impl Npc {
    /// Runs the behaviour script as `func_1507BC14` does: count the wait down, then run commands
    /// until one waits again.
    pub fn run(&mut self, s: &Scripts, g: &Graph, v: &View, rng: &mut Rng) {
        if self.wait != 0 && self.at.is_some() {
            if self.wait != FOREVER {
                self.wait = self.wait.wrapping_sub(100);
            }
            if self.wait < 50_000 && self.wait != 0 {
                return;
            }
            self.wait = 0;
            if self.f4 & 0x400 != 0 {
                self.f4 = (self.f4 & !0x400) | 4;
                self.wait = 20_000;
                return;
            }
        }
        let mut at = *self.at.get_or_insert_with(|| s.start(self.script));
        for _ in 0..STEPS {
            if self.wait != 0 {
                break;
            }
            let op = s.byte(at);
            if op >= 0xF7 {
                at += 1;
                continue;
            }
            let a = [
                s.byte(at + 1),
                s.byte(at + 2),
                s.byte(at + 3),
                s.byte(at + 4),
            ];
            at = self.command(op, a, at, s, g, v, rng) + 5;
        }
        self.at = Some(at);
    }

    /// One command at `at`; returns where the next one is, minus 5.
    #[allow(clippy::too_many_arguments)]
    fn command(
        &mut self,
        op: u8,
        a: [u8; 4],
        at: usize,
        s: &Scripts,
        g: &Graph,
        v: &View,
        rng: &mut Rng,
    ) -> usize {
        match op {
            // Walk (0x01), chase (0x07), wander (0x57): wait a0 frames at speed a2 (`func_15075548`).
            0x01 | 0x07 | 0x57 => {
                self.mode = match op {
                    0x07 => 1,
                    0x57 => 0x11,
                    _ => 0,
                };
                self.wait = u16::from(a[0]).wrapping_mul(100);
                if a[2] != 0xFF {
                    self.speed = if a[2] == 1 { 0.5 } else { f32::from(a[2]) };
                }
                self.f4 &= !0x143E;
                match a[0] {
                    0xFA => self.f4 |= 0x22,
                    0xFB => self.f4 |= 4,
                    _ => {}
                }
                if a[3] & 0x80 != 0 {
                    self.f4 |= 0x10;
                }
                at
            }
            // Go to script a1 (`func_15075DE8`).
            0x08 => {
                let id = if a[1] == 0 { self.script } else { a[1] };
                if a[3] != 0 {
                    self.script = a[3];
                }
                s.start(id).wrapping_sub(5)
            }
            // Facing within [a2, a3) 256ths of the chased player: jump (`func_1507A620`).
            0x1E => {
                let rel = if a[1] != 0 {
                    0 // the walk angle's top byte; only the chase form is used
                } else {
                    let t = v
                        .players
                        .get(usize::from(self.target))
                        .copied()
                        .unwrap_or(v.pos);
                    let to = conker_sim::movement::atan2_u16(-(t[2] - v.pos[2]), t[0] - v.pos[0]);
                    (v.facing.wrapping_sub(to) >> 8) as u8
                };
                if rel.wrapping_sub(a[2]) < a[3].wrapping_sub(a[2]) {
                    return jump(s, at, a[0]);
                }
                at
            }
            // Path index test (`func_15075938`): with the one-waypoint path, "index != 1" holds.
            0x90 => {
                let (index, wanted) = (0i32, if a[1] == 0xFF { 0 } else { i32::from(a[1]) });
                let hit = match a[2] {
                    0 => wanted == index,
                    1 => wanted != index,
                    2 => wanted < index,
                    _ => index < wanted,
                };
                if hit { jump(s, at, a[0]) } else { at }
            }
            0xA2 => {
                self.radius = a[0];
                at
            }
            0xD2 => {
                self.next_node(a[0] == 1, g, v.pos, rng);
                at
            }
            0xD3 => {
                if self.goal == self.cur || self.goal == NONE {
                    self.flags |= AT_GOAL;
                    return jump(s, at, a[0]);
                }
                at
            }
            // Turn rates, speed changes, chase-start scripts: the body's own turning and
            // acceleration are the human's here (I), so these only take their random draws.
            0x31 => {
                if a[2] != 0 {
                    rng.next_u32();
                }
                at
            }
            0x67 => {
                if a[3] != 0 {
                    rng.next_u32();
                }
                at
            }
            0x0C => at,
            _ => at,
        }
    }

    /// A lift moved the body: the path restarts from where it stands (V: bot_long3 p1 605 → 606,
    /// p3 1592 → 1593: the current and previous nodes cleared, the script's wait ended); the AI
    /// replans ([`Npc::placed`]).
    pub fn lifted(&mut self) {
        (self.cur, self.prev, self.wait, self.placed) = (NONE, NONE, 0, true);
    }

    /// `func_15077508` (command 0xD2): the next node toward the goal (or, `wander`, a random
    /// neighbour other than the previous node), and a waypoint on it up to twice its half-width to
    /// the side.
    pub fn next_node(&mut self, wander: bool, g: &Graph, pos: [f32; 3], rng: &mut Rng) {
        if !wander {
            if self.goal == self.cur {
                self.length = 0;
                self.flags |= AT_GOAL;
                return;
            }
            if self.goal == NONE {
                self.length = 0;
            }
        }
        self.flags &= !AT_GOAL;
        let goals: Vec<u8> = [self.goal].into_iter().filter(|&n| n != NONE).collect();
        let avoid: Vec<u8> = [self.avoid].into_iter().filter(|&n| n != NONE).collect();
        let mut next = NONE;
        let mut length = None;
        if self.cur == NONE {
            if wander {
                next = self.prev;
            }
            if next == NONE {
                next = g.approach(
                    pos,
                    if wander { &[] } else { &goals },
                    if wander { &[] } else { &avoid },
                );
                length = Some(g.nearest(pos, Some(0), Some(g.band(pos[1]))).1);
            }
        } else if wander {
            let mut best = (10.0f32, NONE);
            for &nb in &g.nodes[usize::from(self.cur)].next {
                if nb != NONE && nb != self.prev {
                    let f = rng.next_f32();
                    if f < best.0 {
                        best = (f, nb);
                    }
                }
            }
            if best.1 == NONE {
                self.flags |= AT_GOAL;
                best.1 = self.prev;
            }
            next = best.1;
        } else {
            let (n, len, _) = g.next_toward(self.cur, &goals, &avoid);
            next = n;
            length = Some(len);
        }
        let Some(node) = g.node(next) else { return };
        if let Some(l) = length {
            self.length = l;
        }
        (self.prev, self.cur) = (self.cur, next);
        let [mut x, y, mut z] = node.pos.map(f32::from);
        let (dx, dz) = (x - pos[0], z - pos[2]);
        let d = (dx * dx + dz * dz).sqrt();
        if d > 1.0 {
            let t = rng.next_f32() * (2.0 * f32::from(node.half)) / d;
            x -= dz * t;
            z += dx * t;
        }
        self.waypoint = [x as i32 as i16, y as i32 as i16, z as i32 as i16];
    }

    /// `func_15056B08`'s walk target and arrival: the waypoint (walking, wandering) or the chased
    /// player; arriving within `radius · 8` of the waypoint ends the script's wait.
    pub fn steer(&mut self, v: &View) -> Steer {
        let chase = self.mode == 1;
        let toward = if chase {
            v.players
                .get(usize::from(self.target))
                .copied()
                .unwrap_or(v.pos)
        } else {
            self.waypoint.map(f32::from)
        };
        let (dx, dz) = (toward[0] - v.pos[0], toward[2] - v.pos[2]);
        if !chase
            && (dx * dx + dz * dz).sqrt() < f32::from(self.radius) * 8.0
            && self.f4 & 0x10 == 0
        {
            self.wait = 0;
        }
        let speed = if self.f4 & 1 != 0 { 0.0 } else { self.speed };
        Steer { toward, speed }
    }

    /// `func_1504C854`: the virtual pad this frame (held, pressed), then its decay.
    pub fn pad(&mut self) -> (u16, u16) {
        let out = (self.held, self.pressed);
        if self.hold > 0 {
            self.hold -= 1;
        } else {
            self.held = 0;
        }
        if self.press > 0 {
            self.press -= 1;
        } else {
            self.pressed = 0;
        }
        out
    }
}

/// `func_15075400` with a label (≥ 0xF7): the command after the label; under, skip `label` commands.
fn jump(s: &Scripts, at: usize, label: u8) -> usize {
    if label < 0xF7 {
        return at + 5 * usize::from(label);
    }
    let mut p = at + 5;
    for _ in 0..100 {
        let b = s.byte(p);
        if b < 0xF7 {
            p += 5;
            continue;
        }
        p += 1;
        if b == label {
            return p - 5;
        }
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two scripts: 1 = walk 2 frames, goto 2; 2 = wait forever after a label jump.
    fn scripts() -> Scripts {
        // Header: the table's offset from the asset start (data + 0x30).
        let mut a = vec![0, 0, 0, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let body: Vec<u8> = [
            &[0; 0x30][..],
            // data + 0x30, the table: script 1 at data + 0x50, script 2 at data + 0x60
            &[
                0, 0, 0, 0x50, 1, 0, 0, 0, 0, 0, 0, 0x60, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
            &[0; 8],
            // +0x50: 01(02 00 23 01) 08(00 02 00 02) pad
            &[0x01, 2, 0, 0x23, 1, 0x08, 0, 2, 0, 2, 0, 0, 0, 0, 0, 0],
            // +0x60: d3(f7) 01(ff 00 00 01) [f7] 01(05 00 0a 01)
            &[
                0xD3, 0xF7, 0, 0, 0, 0x01, 0xFF, 0, 0, 1, 0xF7, 0x01, 5, 0, 10, 1,
            ],
        ]
        .concat();
        a.extend(body);
        Scripts::parse(&a).unwrap()
    }

    #[test]
    fn scripts_wait_jump_and_switch() {
        let s = scripts();
        assert_eq!(s.table[..2], [(1, 0x50), (2, 0x60)]);
        // An id the class lacks runs the game's default script.
        assert_eq!(s.start(9), s.table[2].1);
        assert_eq!(s.byte(s.start(9) + 15), 0x01);
        let g = Graph::default();
        let v = View {
            pos: [0.0; 3],
            facing: 0,
            players: &[],
        };
        let (mut n, mut rng) = (Npc::default(), Rng::default());
        n.run(&s, &g, &v, &mut rng);
        assert_eq!((n.wait, n.speed, n.mode), (200, 35.0, 0));
        n.run(&s, &g, &v, &mut rng); // 200 → 100
        assert_eq!(n.script, 1);
        n.run(&s, &g, &v, &mut rng); // runs out: goto 2, d3 (no goal) jumps over the wait-forever
        assert_eq!((n.script, n.wait, n.speed), (2, 500, 10.0));
        assert_ne!(n.flags & AT_GOAL, 0);
    }

    #[test]
    fn the_virtual_pad_holds_then_clears() {
        let mut n = Npc {
            held: 0x2000,
            hold: 2,
            pressed: 0x8000,
            ..Npc::default()
        };
        assert_eq!(n.pad(), (0x2000, 0x8000));
        assert_eq!(n.pad(), (0x2000, 0));
        assert_eq!(n.pad(), (0x2000, 0));
        assert_eq!(n.pad(), (0, 0));
    }
}
