//! Checks the port against the original game (`lab/oracle`, outside this tree; env `CONKER_ORACLE`
//! overrides the path). Skips when the traces are absent.
//! - Every spawn pick the oracle logged (`trajectories/spawn_*_lldb.txt`, `tools/spawn_lldb.py`): the
//!   port gets the logged RNG state, record table and participants, and must return the logged
//!   record, position, facing and camera, and leave the RNG in the logged state (so it took as many
//!   draws as the game).
//! - The players' rows in the matching traces (`spawn_*.csv`) at the first match frame.
//! - The sink rule on the surface runs (`trajectories/surface_*_lldb.txt`, `tools/surface_lldb.py`).
//!
//! Files these tests don't know are skipped: other agents add traces to the same directory.

use conker_rules::spawn::{Spawn, starts};
use conker_rules::{DEATHMATCH_MASKS, Rng, Surface, place, respawn, sink_target, step_sink};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn oracle_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CONKER_ORACLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../lab/oracle"));
    if dir.join("trajectories").is_dir() {
        Some(dir)
    } else {
        eprintln!("no oracle traces at {}: skipping", dir.display());
        None
    }
}

/// `key=value` tokens of one log line.
fn fields(line: &str) -> HashMap<&str, &str> {
    line.split_whitespace()
        .filter_map(|t| t.split_once('='))
        .collect()
}

fn int(s: &str) -> i64 {
    match s.strip_prefix("0x") {
        Some(h) => i64::from_str_radix(h, 16).unwrap(),
        None => s.parse().unwrap(),
    }
}

fn floats<const N: usize>(s: &str) -> [f32; N] {
    let v: Vec<f32> = s.split(',').map(|x| x.parse().unwrap()).collect();
    v.try_into().unwrap()
}

fn spawn(hex: &str) -> Spawn {
    let b: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let h = |o: usize| i16::from_be_bytes([b[o], b[o + 1]]);
    let f = |o: usize| f32::from_be_bytes(b[o..o + 4].try_into().unwrap());
    Spawn {
        pos: [h(0), h(2), h(4)],
        b6: b[6],
        yaw: b[7],
        camera: [f(8), f(12), f(16)],
        b14: b[0x14],
        b15: b[0x15] as i8,
    }
}

/// One logged pick in a match (scene 0x2D, 0x30 or 0x34) and what the game returned.
struct Pick {
    caller: String,
    player: usize,
    hint: i64,
    respawn: bool,
    masks: [u32; 2],
    rng: u64,
    s11b: usize,
    last: i64,
    team: u8,
    /// (object, x, z, team) of every participant.
    objs: Vec<(usize, f32, f32, u8)>,
    out: HashMap<String, String>,
}

struct Capture {
    name: String,
    scene: u16,
    table: Vec<Spawn>,
    picks: Vec<Pick>,
    /// (object, record) from `func_15085410`.
    set_index: Vec<(usize, usize)>,
}

fn captures(dir: &Path) -> Vec<Capture> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("trajectories"))
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| {
            Some(
                n.strip_prefix("spawn_")?
                    .strip_suffix("_lldb.txt")?
                    .to_string(),
            )
        })
        .collect();
    names.sort();
    let mut caps: Vec<Capture> = names
        .into_iter()
        .map(|name| {
            let text =
                std::fs::read_to_string(dir.join(format!("trajectories/spawn_{name}_lldb.txt")))
                    .unwrap();
            let (mut table, mut picks, mut set_index) = (Vec::new(), Vec::new(), Vec::new());
            let mut scene = 0x2D;
            let mut open: Option<Pick> = None;
            for line in text.lines() {
                let kv = fields(line);
                match line.split_whitespace().next() {
                    Some("table") => table = line.split_whitespace().skip(1).map(spawn).collect(),
                    Some("setidx") => {
                        set_index.push((int(kv["obj"]) as usize, int(kv["a1"]) as usize))
                    }
                    Some("pick")
                        if kv["mp"] == "1" && ["0x2d", "0x30", "0x34"].contains(&kv["scene"]) =>
                    {
                        scene = int(kv["scene"]) as u16;
                        let objs = kv["objs"]
                            .split(',')
                            .map(|o| {
                                let p: Vec<&str> = o.split(':').collect();
                                (
                                    p[0].parse().unwrap(),
                                    p[1].parse().unwrap(),
                                    p[2].parse().unwrap(),
                                    p[3].parse().unwrap(),
                                )
                            })
                            .collect();
                        open = Some(Pick {
                            caller: kv["caller"].to_string(),
                            player: int(kv["a0"]) as usize,
                            hint: int(kv["a1"]),
                            respawn: kv["d18a8"] == "1",
                            masks: [int(kv["m0"]) as u32, int(kv["m1"]) as u32],
                            rng: int(kv["rng"]) as u64,
                            s11b: int(kv["s11b"]) as usize,
                            last: int(kv["s1b0"]),
                            team: int(kv["team"]) as u8,
                            objs,
                            out: HashMap::new(),
                        });
                    }
                    Some("out") => {
                        if let Some(mut p) = open.take() {
                            p.out = kv
                                .iter()
                                .map(|(k, v)| (k.to_string(), v.to_string()))
                                .collect();
                            picks.push(p);
                        }
                    }
                    _ => {}
                }
            }
            Capture {
                name,
                scene,
                table,
                picks,
                set_index,
            }
        })
        .collect();
    // The first runs did not log the table: a scene's is the same in every run.
    let tables: HashMap<u16, Vec<Spawn>> = (caps.iter())
        .filter(|c| !c.table.is_empty())
        .map(|c| (c.scene, c.table.clone()))
        .collect();
    for c in &mut caps {
        if c.table.is_empty() {
            c.table = tables[&c.scene].clone();
        }
        assert!(c.table == tables[&c.scene], "{}: another table", c.name);
    }
    caps
}

/// Every logged pick, through `place` or `respawn`: record, position, facing, camera and the RNG
/// state afterwards must equal the game's.
#[test]
fn every_logged_pick() {
    let Some(dir) = oracle_dir() else { return };
    let caps = captures(&dir);
    assert!(!caps.is_empty(), "no spawn_*_lldb.txt captures");
    let (mut placements, mut respawns, mut draws, mut bad) = (0, 0, 0u32, Vec::new());
    for c in &caps {
        for p in &c.picks {
            let mut rng = Rng::new(p.rng);
            let got = if p.respawn {
                respawns += 1;
                let enemies: Vec<[f32; 2]> = p
                    .objs
                    .iter()
                    .filter(|o| o.3 != p.team)
                    .map(|o| [o.1, o.2])
                    .collect();
                let last = (p.last >= 0).then_some(p.last as usize);
                respawn(
                    &c.table,
                    p.masks[usize::from(p.team != 0)],
                    last,
                    &enemies,
                    &mut rng,
                )
            } else {
                placements += 1;
                let index = if p.hint == -1 {
                    p.s11b
                } else {
                    p.hint as usize
                };
                place(&c.table, index, &mut rng)
            };
            draws += p.out["calls"].parse::<u32>().unwrap();
            let pos: [f32; 3] = floats(&p.out["pos"]);
            let cam: [f32; 3] = floats(&p.out["cam"]);
            let yaw = int(&p.out["yaw"]) as u16;
            let rec = &c.table[got.index];
            let ok = got.pos == pos
                && got.facing == yaw << 8
                && got.camera == cam
                && rng.state == int(&p.out["rng"]) as u64
                && i64::from(rec.b6) == int(&p.out["b6"])
                && i64::from(rec.b14) == int(&p.out["b14"])
                && i64::from(rec.b15) == int(&p.out["b15"]);
            if !ok {
                bad.push(format!(
                    "{} {} player {}: port {:?} rng {:#x}, game {:?}",
                    c.name, p.caller, p.player, got, rng.state, p.out
                ));
            }
        }
    }
    eprintln!(
        "{} captures: {placements} placements + {respawns} respawns ({draws} RNG draws), {} mismatches",
        caps.len(),
        bad.len()
    );
    assert!(bad.is_empty(), "{}", bad.join("\n"));
    assert!(respawns > 0 && placements > 0);
}

/// The record the level script places each slot on (struct126+0x11B at its pick, and the
/// `func_15085410` calls where logged), and the respawn masks, by scene and scenario (s9 Total War
/// and s10 Colors: the sides' tables; else Deathmatch's).
#[test]
fn start_records_and_masks() {
    let Some(dir) = oracle_dir() else { return };
    let (mut slots, mut picks) = (0, 0);
    for c in captures(&dir) {
        let colors = c.name.starts_with("s9") || c.name.starts_with("s10");
        let tables = starts(c.scene).unwrap();
        let script: Vec<&Pick> = c
            .picks
            .iter()
            .filter(|p| p.caller.contains("func_15097A8C"))
            .collect();
        for (k, p) in script.iter().enumerate() {
            let want = if colors {
                let rank = script[..k].iter().filter(|q| q.team == p.team).count();
                tables.sides[usize::from(p.team != 0)][rank]
            } else {
                tables.deathmatch[p.player]
            };
            assert_eq!(usize::from(want), p.s11b, "{} slot {}", c.name, p.player);
            if let Some(&(_, record)) = c.set_index.iter().find(|s| s.0 == p.player) {
                assert_eq!(record, p.s11b, "{} slot {}", c.name, p.player);
            }
            slots += 1;
        }
        for p in c.picks.iter().filter(|p| p.respawn) {
            assert_eq!(
                p.masks,
                if colors {
                    tables.side_masks
                } else {
                    DEATHMATCH_MASKS
                },
                "{}",
                c.name
            );
            picks += 1;
        }
    }
    eprintln!("{slots} start records and {picks} respawn masks as tabled");
    assert!(slots > 0);
}

/// Each participant's row at read 23 (the match's first frame) equals its script placement, and the
/// idle humans alone on their record are still there at read 30 (bots walk from read 24), in x
/// and z.
#[test]
fn traces_start_on_the_placements() {
    let Some(dir) = oracle_dir() else { return };
    let (mut rows, mut files) = (0, 0);
    for c in captures(&dir) {
        let Some(humans) = c
            .name
            .split_once('h')
            .and_then(|(_, r)| r.split('b').next()?.parse::<usize>().ok())
        else {
            continue;
        };
        let Ok(text) =
            std::fs::read_to_string(dir.join(format!("trajectories/spawn_{}.csv", c.name)))
        else {
            continue;
        };
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next().unwrap().split(',').collect();
        let col = |name: &str| head.iter().position(|h| *h == name).unwrap();
        let (read, player, x, y, z, facing) = (
            col("read"),
            col("player"),
            col("x"),
            col("y"),
            col("z"),
            col("facing"),
        );
        let placed: HashMap<usize, &Pick> = c
            .picks
            .iter()
            .filter(|p| p.caller.contains("func_15097A8C"))
            .map(|p| (p.player, p))
            .collect();
        // 16 players on 0x34's 12 records: two slots share a record and push each other apart.
        let alone = |i: usize| {
            placed
                .values()
                .filter(|q| q.s11b == placed[&i].s11b)
                .count()
                == 1
        };
        for line in lines {
            let v: Vec<&str> = line.split(',').collect();
            let (r, i): (u32, usize) = (v[read].parse().unwrap(), v[player].parse().unwrap());
            if !(r == 23 || r == 30 && i < humans && alone(i)) {
                continue;
            }
            let p = placed[&i];
            let pos: [f32; 3] = floats(&p.out["pos"]);
            let mut row = [v[x], v[y], v[z]].map(|s| s.parse::<f32>().unwrap());
            // By read 30 the floor has settled y: 0x30's record 12 sits 0.84 under its floor.
            if r == 30 {
                row[1] = pos[1];
            }
            assert_eq!(row, pos, "{} read {r} player {i}", c.name);
            assert_eq!(
                v[facing].parse::<u16>().unwrap(),
                (int(&p.out["yaw"]) as u16) << 8,
                "{} read {r} player {i}",
                c.name
            );
            rows += 1;
        }
        files += 1;
    }
    eprintln!(
        "{files} traces, {rows} rows (read 23: every participant; read 30: humans) on their placements"
    );
    assert!(rows > 0);
}

/// obj+0xCE (target) and obj+0xCC (sink) on every logged frame, from the previous frame's floor
/// word, speed and height above the floor.
#[test]
fn sink_follows_the_floor_word() {
    let Some(dir) = oracle_dir() else { return };
    let (mut frames, mut bad) = (0, Vec::new());
    for name in ["surface_5", "surface_9", "surface_a", "surface_slide"] {
        let Ok(text) = std::fs::read_to_string(dir.join(format!("trajectories/{name}_lldb.txt")))
        else {
            continue;
        };
        let rows: Vec<HashMap<&str, &str>> = text
            .lines()
            .filter(|l| l.starts_with("f "))
            .map(fields)
            .collect();
        for w in rows.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let surface = Surface(int(a["word"]) as u32);
            let target = sink_target(surface, a["spd"].parse().unwrap(), a["h"].parse().unwrap());
            let sink = step_sink(int(a["cc"]) as i16, target);
            frames += 1;
            if (i64::from(target), i64::from(sink)) != (int(b["ce"]), int(b["cc"])) {
                bad.push(format!(
                    "{name} t={}: port ce {target} cc {sink}, game ce {} cc {}",
                    b["t"], b["ce"], b["cc"]
                ));
            }
        }
    }
    eprintln!("{frames} frames, {} mismatches", bad.len());
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
