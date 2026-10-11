//! The bots against the original game's bot matches (`lab/oracle`, outside this tree). Needs
//! `CONKER_ROM` and the lab checkout; skips otherwise, and skips trace files it doesn't know (other
//! agents add traces concurrently). `cargo test -p conker-bots --release --test oracle -- --nocapture`
//! prints the numbers.

use conker_bots::nav::{Graph, NONE};
use std::io::Read;

const ORACLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../lab/oracle/trajectories"
);
/// `CONKER_TRACE_RAW` record: u32 read, u32 player (LE), object 0x32C, struct126 0x1B0, camera 0x700.
const RECORD: usize = 8 + 0x32C + 0x1B0 + 0x700;
/// The bot matches these tests know (1 human + bots; `combat_README.md`, `bot_README.md`).
const BOT_RAWS: [&str; 2] = ["combat_bots3.raw", "bot_long3.raw"];
// bot_long3 is the same match as combat_bots3 for its first 6,000 reads (same script and seed),
// 3,000 reads longer.

fn graph() -> Option<Graph> {
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return None;
    };
    let rom = extract_core::Rom::new(std::fs::read(path).unwrap()).unwrap();
    Graph::parse(
        &rom.asset(&[0x19, 0x2D]).unwrap(),
        &rom.asset(&[0x0C, 0x2D, 5]).unwrap(),
    )
}

/// One row of a bot: the fields the path search reads and writes.
#[derive(Clone, Copy, Debug)]
struct Row {
    read: u32,
    pos: [f32; 3],
    /// obj+0x20F (current node), +0x210 (previous), +0x211 (goal), +0x212 (avoid), +0x232 (script).
    cur: u8,
    goal: u8,
    avoid: u8,
    script: u8,
}

/// Every bot's rows of a raw trace, by player.
fn rows(name: &str) -> Option<Vec<Vec<Row>>> {
    let path = std::path::Path::new(ORACLE).join(name);
    let mut f = std::fs::File::open(&path).ok()?;
    let mut out: Vec<Vec<Row>> = Vec::new();
    let mut rec = vec![0u8; RECORD];
    while f.read_exact(&mut rec).is_ok() {
        let read = u32::from_le_bytes(rec[0..4].try_into().unwrap());
        let player = u32::from_le_bytes(rec[4..8].try_into().unwrap()) as usize;
        let obj = &rec[8..8 + 0x32C];
        let fl = |o: usize| f32::from_be_bytes(obj[o..o + 4].try_into().unwrap());
        if out.len() <= player {
            out.resize(player + 1, Vec::new());
        }
        out[player].push(Row {
            read,
            pos: [fl(0x14), fl(0x18), fl(0x1C)],
            cur: obj[0x20F],
            goal: obj[0x211],
            avoid: obj[0x212],
            script: obj[0x232],
        });
    }
    Some(out)
}

/// At every change of a bot's current node outside the wandering scripts (7 and 8 pick a random
/// neighbour), the path search from the row before (position, node, goal and avoided node) gives
/// the new node.
#[test]
fn next_node_as_the_game_picks_it() {
    let Some(g) = graph() else { return };
    let (mut total, mut same) = (0, 0);
    for name in BOT_RAWS {
        let Some(players) = rows(name) else {
            eprintln!("{name}: not found, skipped");
            continue;
        };
        let (mut n, mut ok, mut bad) = (0, 0, Vec::new());
        for (p, rows) in players.iter().enumerate().skip(1) {
            for w in rows.windows(2) {
                let (a, b) = (w[0], w[1]);
                if a.cur == b.cur
                    || b.cur == NONE
                    || b.read < 24
                    || matches!(b.script, 7 | 8)
                    || matches!(a.script, 7 | 8)
                {
                    continue;
                }
                let goals: Vec<u8> = [b.goal].into_iter().filter(|&x| x != NONE).collect();
                let avoid: Vec<u8> = [b.avoid].into_iter().filter(|&x| x != NONE).collect();
                // A node that is not a neighbour: the bot was moved (a lift's teleport, a respawn) and
                // searched again from where it landed.
                let moved = a.cur != NONE && !g.nodes[usize::from(a.cur)].next.contains(&b.cur);
                // A new goal (the script's command 0x60) forgets the current node: searched again too.
                let got = if a.cur == NONE || moved || a.goal != b.goal {
                    g.approach(if moved { b.pos } else { a.pos }, &goals, &avoid)
                } else {
                    let (next, _, _) = g.next_toward(a.cur, &goals, &avoid);
                    next
                };
                n += 1;
                if got == b.cur {
                    ok += 1;
                } else if bad.len() < 12 {
                    let alt = g.next_toward(a.cur, &[a.goal], &[a.avoid]).0;
                    bad.push(format!("p{p} read {} {}→{} goal {}/{} avoid {}/{} script {}/{}: got {got} (with the row before's {alt})", b.read, a.cur, b.cur, a.goal, b.goal, a.avoid, b.avoid, a.script, b.script));
                }
            }
        }
        println!("{name}: {ok}/{n} node changes as the game picks them");
        for l in &bad {
            println!("  {l}");
        }
        (total, same) = (total + n, same + ok);
    }
    if total > 0 {
        println!("all: {same}/{total}");
    }
    assert_eq!(same, total);
}

/// After a lift the game searches the path afresh from where the bot landed (obj+0x20F/+0x210 in
/// bot_long3): p1 lifted up at read 605 goes from node 105 to 104, p3 lifted down at 1592 from 108
/// to 8, the previous node cleared both times.
#[test]
fn a_lift_restarts_the_path() {
    use conker_bots::npc::Npc;
    let Some(g) = graph() else { return };
    for (cur, prev, goal, avoid, landed, want) in [
        (105, 109, 60, NONE, [778.0, 1254.0, -3893.0], 104),
        (108, 104, 22, 46, [719.0, 46.0, -3983.0], 8),
    ] {
        let mut n = Npc {
            cur,
            prev,
            goal,
            avoid,
            ..Npc::default()
        };
        n.lifted();
        n.next_node(false, &g, landed, &mut conker_rules::Rng::default());
        assert_eq!((n.cur, n.prev), (want, NONE));
    }
}

/// The scene's collision world, as the server builds it.
fn world(rom: &extract_core::Rom) -> conker_sim::collision::CollisionWorld {
    use conker_sim::collision::{CollisionWorld, Layer};
    let placements = extract_scene::scene_placements(rom, 0x2D).unwrap();
    let collision = extract_scene::scene_collision(rom, 0x2D, &placements).unwrap();
    let mut w = CollisionWorld::layered(collision.triangles.iter().map(|t| {
        let layer = match t.layer {
            extract_scene::Layer::Terrain => Layer::Terrain,
            extract_scene::Layer::Secondary => Layer::Secondary,
            extract_scene::Layer::Placement(i) => Layer::Placement(i as u16),
        };
        (t.points, t.surface.unwrap_or(0), layer)
    }));
    w.acos = conker_sim::trig::AcosTable::from_game_data(&rom.game_data().unwrap());
    w
}

/// Per (read, player) of a raw trace: position, held item (s126+0x75 & 0x7F), head (obj+0x1A4).
type Eyes = std::collections::HashMap<(u32, usize), ([f32; 3], u8, [f32; 3])>;

fn eyes(name: &str) -> Option<Eyes> {
    let mut f = std::fs::File::open(std::path::Path::new(ORACLE).join(name)).ok()?;
    let mut out = std::collections::HashMap::new();
    let mut rec = vec![0u8; RECORD];
    while f.read_exact(&mut rec).is_ok() {
        let read = u32::from_le_bytes(rec[0..4].try_into().unwrap());
        let player = u32::from_le_bytes(rec[4..8].try_into().unwrap()) as usize;
        let obj = &rec[8..8 + 0x32C];
        let fl = |o: usize| f32::from_be_bytes(obj[o..o + 4].try_into().unwrap());
        let h = |o: usize| f32::from(i16::from_be_bytes([obj[o], obj[o + 1]]));
        let item = rec[8 + 0x32C + 0x75] & 0x7F;
        out.insert(
            (read, player),
            (
                [fl(0x14), fl(0x18), fl(0x1C)],
                item,
                [h(0x1A4), h(0x1A6), h(0x1A8)],
            ),
        );
    }
    Some(out)
}

/// The oracle's sight bits (G+0x16C0 + 4p, p = 1..3) per read, from `bot_states_lldb.txt`
/// (`tools/bot_lldb.sh`, bot_README.md): the AI's view at the start of that read's frame.
fn sees(name: &str) -> Option<Vec<(u32, [u32; 3])>> {
    let text = std::fs::read_to_string(std::path::Path::new(ORACLE).join(name)).ok()?;
    Some(
        text.lines()
            .filter(|l| l.contains(" call func_1508A6FC "))
            .map(|l| {
                let read = l.split_whitespace().next().unwrap().parse().unwrap();
                let v = |off: &str| {
                    let at = l.find(&format!("*0x800D23B0+{off}=0x")).unwrap() + off.len() + 15;
                    let hex: String = l[at..]
                        .chars()
                        .take_while(char::is_ascii_hexdigit)
                        .collect();
                    u32::from_str_radix(&hex, 16).unwrap()
                };
                (read, [v("0x16C4"), v("0x16C8"), v("0x16CC")])
            })
            .collect(),
    )
}

/// Where a bot's sight bit for another participant flips, the pair was just tested: the game's
/// ray (`func_1508C1A4`) from eye to eye, with the positions of the row before, must agree.
#[test]
fn sight_as_the_game_tests_it() {
    let (Some(path), Some(eyes), Some(sees)) = (
        std::env::var_os("CONKER_ROM"),
        eyes("bot_long3.raw"),
        sees("bot_states_lldb.txt"),
    ) else {
        eprintln!("CONKER_ROM, bot_long3.raw or bot_states_lldb.txt missing: skipping");
        return;
    };
    let rom = extract_core::Rom::new(std::fs::read(path).unwrap()).unwrap();
    let w = world(&rom);
    let g = graph().unwrap();
    let eye = |read: u32, me: usize, other: usize| {
        let (pos, item, head) = eyes.get(&(read, me))?;
        Some(if me > 0 {
            [
                pos[0],
                pos[1] + 67.5 - if *item == 9 { 50.0 } else { 0.0 },
                pos[2],
            ]
        } else if other > 0 {
            *head
        } else {
            [pos[0], pos[1] + 40.0, pos[2]]
        })
    };
    // Row `read` is the state the frame's AI test saw (lag 0); older rows only for comparison.
    for lag in [0, 1, 2] {
        let (mut n, mut same, mut open_n, mut open_same) = (0, 0, 0, 0);
        let mut bad = Vec::new();
        for w2 in sees.windows(2) {
            let ((_, a), (read, b)) = (w2[0], w2[1]);
            for p in 1..=3usize {
                for q in 0..4usize {
                    if q == p {
                        continue;
                    }
                    let (bit, clear) = (0x10000u32 << q, 1u32 << q);
                    if (a[p - 1] ^ b[p - 1]) & (bit | clear) == 0 {
                        continue;
                    }
                    let at = read - lag;
                    let (Some(from), Some(to)) = (eye(at, p, q), eye(at, q, p)) else {
                        continue;
                    };
                    let got = conker_bots::brain::sight(&w, &g, from, to);
                    let want = (b[p - 1] & bit != 0, b[p - 1] & clear != 0);
                    n += 1;
                    if got.is_some() == want.0 {
                        same += 1;
                    } else if bad.len() < 8 {
                        bad.push(format!(
                            "read {read} p{p}-p{q}: game {want:?}, here {got:?}, {from:?} -> {to:?}"
                        ));
                    }
                    if want.0 && got.is_some() {
                        open_n += 1;
                        open_same += usize::from(got == Some(want.1));
                    }
                }
            }
        }
        println!(
            "rows read − {lag}: walls {same}/{n} as the game; barrier lines {open_same}/{open_n}"
        );
        for l in &bad {
            println!("  {l}");
        }
        if lag == 0 {
            // V: 137/142 and 70/79; the misses are lines thousands of units long.
            assert!(n > 100 && same * 100 >= n * 95, "{same}/{n}");
            assert!(open_same * 100 >= open_n * 85, "{open_same}/{open_n}");
        }
    }
}
