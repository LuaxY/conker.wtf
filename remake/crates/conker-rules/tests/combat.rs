//! Deathmatch combat against the original game (`lab/oracle`, outside this tree; `combat_README.md`
//! there is the spec). Needs `CONKER_ROM` (scene 0x2D's collision, spawns and placements) and the
//! lab checkout; skips otherwise. Files these tests don't know are skipped: other agents add traces
//! concurrently. `cargo test -p conker-rules --test combat -- --nocapture` prints the numbers;
//! `COMBAT_DUMP=<trace>:<first>:<last>` prints simulated against traced rows.
//!
//! The encounters (`combat_e_*`) are replayed from the traced state at read 745 (both players
//! standing 146.5 apart, player 1 facing player 2): both players' scripts through conker-sim's
//! movement and [`Match::step`], each player's stick read with its own traced camera, the weapon
//! given at 745 as the oracle's lldb poke did. Row N of a trace is the state after frame N − 1.

use conker_camera::CameraState;
use conker_camera::math::yaw_of;
use conker_rules::Rng;
use conker_rules::game::{Arena, Event, Fighter, Match};
use conker_rules::parts::Poses;
use conker_rules::projectile::{Bullet, raycast};
use conker_rules::spawn::Spawn;
use conker_rules::weapon::BULLET_RANGE;
use conker_sim::collision::{CollisionWorld, Layer};
use conker_sim::movement::{Input, PlayerState};
use conker_sim::root::RootTrack;
use conker_sim::trig::AcosTable;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::OnceLock;

const ORACLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../lab/oracle");

struct Scene {
    world: CollisionWorld,
    arena: Arena,
}

fn scene() -> Option<&'static Scene> {
    static SCENE: OnceLock<Option<Scene>> = OnceLock::new();
    SCENE
        .get_or_init(|| {
            let Some(rom) = std::env::var_os("CONKER_ROM") else {
                eprintln!("CONKER_ROM unset: skipping");
                return None;
            };
            if !PathBuf::from(ORACLE).join("trajectories").is_dir() {
                eprintln!("no oracle traces at {ORACLE}: skipping");
                return None;
            }
            let rom = extract_core::Rom::new(std::fs::read(rom).expect("read CONKER_ROM"))
                .expect("CONKER_ROM is the US ROM");
            let placements = extract_scene::scene_placements(&rom, 0x2D).unwrap();
            let collision = extract_scene::scene_collision(&rom, 0x2D, &placements).unwrap();
            let solid = extract_scene::deathmatch_solid(0x2D);
            let triangles = (collision.triangles.iter()).filter(
                |t| !matches!(t.layer, extract_scene::Layer::Placement(i) if !solid.contains(&i)),
            );
            let mut world = CollisionWorld::layered(triangles.map(|t| {
                let layer = match t.layer {
                    extract_scene::Layer::Terrain => Layer::Terrain,
                    extract_scene::Layer::Secondary => Layer::Secondary,
                    extract_scene::Layer::Placement(i) => Layer::Placement(i as u16),
                };
                (t.points, t.surface.unwrap_or(0), layer)
            }));
            world.acos = AcosTable::from_game_data(&rom.game_data().unwrap());
            // Root motion (0x16's attack, the shot death 0x17E), as conker-sim's oracle test loads it.
            let player =
                extract_character::Character::load(&rom, extract_character::DEATHMATCH_PLAYER)
                    .unwrap();
            world.roots = player
                .playable_clips()
                .filter_map(|c| {
                    let rm = c.root_motion.as_ref()?;
                    Some(RootTrack {
                        ids: c.logical.clone(),
                        duration: c.duration,
                        step: c.step,
                        z: rm.z.clone(),
                        end: rm.end,
                    })
                })
                .collect();
            if std::env::var_os("COMBAT_ROOTS").is_some() {
                for t in &world.roots {
                    eprintln!(
                        "root ids {:x?} duration {} travel {}",
                        t.ids,
                        t.duration,
                        t.travel()
                    );
                }
            }
            let spawns = extract_scene::scene_spawns(&rom, 0x2D)
                .unwrap()
                .iter()
                .map(|s| Spawn {
                    pos: s.position,
                    b6: s.byte_06,
                    yaw: s.yaw,
                    camera: s.camera,
                    b14: s.byte_14,
                    b15: s.byte_15,
                })
                .collect();
            let arena = Arena::deathmatch(spawns, |n| {
                placements[usize::from(n)].position.map(f32::from)
            });
            Some(Scene { world, arena })
        })
        .as_ref()
}

/// `COMBAT_POSES=last|this`: the body parts posed on the pack's glTF (`conker_rules::parts`), from
/// the poses the last frame drew (the game's) or this frame's (the step run once on a copy first);
/// unset or without the pack (`CONKER_ASSETS`, else `<repo>/assets`), the fitted volumes.
fn poses() -> Option<(&'static Poses, bool)> {
    static POSES: OnceLock<Option<Poses>> = OnceLock::new();
    let this = match std::env::var("COMBAT_POSES").ok()?.as_str() {
        "this" => true,
        "last" => false,
        _ => return None,
    };
    let poses = POSES.get_or_init(|| {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let dir = PathBuf::from(dir);
        Poses::load(&[117, 136], |p| std::fs::read(dir.join(p))).ok()
    });
    Some((poses.as_ref()?, this))
}

fn world() -> Option<&'static CollisionWorld> {
    scene().map(|s| &s.world)
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(PathBuf::from(ORACLE).join(path)).ok()
}

/// One `call func_X` line of a `combat_*_lldb*.txt` capture (`tools/combat_lldb.py`): the frame
/// it ran in and its `key=value` / `key value` fields.
struct Call {
    frame: u32,
    func: String,
    kv: HashMap<String, String>,
}

fn calls(text: &str) -> Vec<Call> {
    text.lines()
        .filter_map(|l| {
            let head = l.split(" | ").next()?;
            let w: Vec<&str> = head.split_whitespace().collect();
            if w.get(2) != Some(&"call") {
                return None;
            }
            let mut kv = HashMap::new();
            let mut i = 4;
            while i < w.len() {
                if let Some((k, v)) = w[i].split_once('=') {
                    kv.insert(k.to_string(), v.to_string());
                    i += 1;
                } else if i + 1 < w.len() && !w[i + 1].contains('=') {
                    kv.insert(w[i].to_string(), w[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            Some(Call {
                frame: w[0].parse().ok()?,
                func: w[3].to_string(),
                kv,
            })
        })
        .collect()
}

fn f(kv: &HashMap<String, String>, k: &str) -> f32 {
    kv[k].parse().unwrap()
}

/// A bullet as the pool update (`func_151C2734`) saw it at the start of its frame's update.
#[derive(Clone, Debug)]
struct BulletRow {
    frame: u32,
    addr: String,
    pos: [f32; 3],
    vel: [f32; 3],
    life: f32,
}

fn bullet_rows(text: &str) -> Vec<BulletRow> {
    calls(text)
        .into_iter()
        .filter(|c| c.func == "func_151C2734")
        .map(|c| BulletRow {
            frame: c.frame,
            addr: c.kv["a0"].clone(),
            pos: ["a0+0x34", "a0+0x38", "a0+0x3C"].map(|k| f(&c.kv, k)),
            vel: ["a0+0x114", "a0+0x118", "a0+0x11C"].map(|k| f(&c.kv, k)),
            life: f(&c.kv, "a0+0x19C"),
        })
        .collect()
}

/// Grenades (class 0x7E objects) as the lldb object list saw them at the start of each frame:
/// frame → position.
fn grenade_rows(text: &str) -> HashMap<u32, [f32; 3]> {
    text.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w.get(2) != Some(&"obj") || w.get(5) != Some(&"0x7e") {
                return None;
            }
            let i = w.iter().position(|&x| x == "pos")?;
            let p = [1, 2, 3].map(|k| w[i + k].parse::<f32>().unwrap());
            Some((w[0].parse().ok()?, p))
        })
        .collect()
}

/// The traces with bullet logs.
const BULLET_LOGS: [&str; 7] = [
    "combat_ammo_cook_lldb",
    "combat_ctlw_k0_lldb",
    "combat_ctlw_k5_lldb",
    "combat_ctlw_k6_lldb",
    "combat_e_w09_lldb",
    "combat_e_w23_lldb",
    "combat_e_w39_lldb",
];

/// Bullets (`combat_README.md` §3.2): born with a life of (distance to the wall ahead) / 300, then
/// each frame they move by their velocity and lose 1. Every logged bullet, from its first row.
#[test]
fn bullets_move_as_the_game_moves_them() {
    let Some(world) = world() else { return };
    let (mut births, mut steps) = (0, 0);
    let (mut life_err, mut step_err) = (0f32, 0f32);
    for name in BULLET_LOGS {
        let Some(text) = read(&format!("trajectories/{name}.txt")) else {
            continue;
        };
        let rows = bullet_rows(&text);
        let mut last: HashMap<String, BulletRow> = HashMap::new();
        for r in rows {
            let speed = r.vel.iter().map(|v| v * v).sum::<f32>().sqrt();
            let dir = r.vel.map(|v| v / speed);
            match last.get(&r.addr) {
                Some(p) if p.frame + 1 == r.frame && p.life > r.life => {
                    let want = [0, 1, 2].map(|i| p.pos[i] + p.vel[i]);
                    let e = (0..3)
                        .map(|i| (want[i] - r.pos[i]).abs())
                        .fold(p.life - 1.0 - r.life, f32::max);
                    step_err = step_err.max(e.abs());
                    steps += 1;
                }
                _ => {
                    let wall = raycast(world, r.pos, dir, BULLET_RANGE);
                    let e = (wall / 300.0 - r.life).abs();
                    if e > 0.01 {
                        eprintln!(
                            "  {name} {} {}: life {} want {} (wall {wall})",
                            r.frame,
                            r.addr,
                            wall / 300.0,
                            r.life
                        );
                    }
                    life_err = life_err.max(e);
                    births += 1;
                }
            }
            last.insert(r.addr.clone(), r);
        }
    }
    eprintln!(
        "bullets: {births} births, life max error {life_err}; {steps} steps, max error {step_err}"
    );
    let _ = Bullet::reach;
}

/// One player's row of a trace (`trajectories/<name>.csv`) with its raw record (`.raw`).
#[derive(Clone, Debug)]
struct Row {
    timer: i32,
    state: PlayerState,
    health: u8,
    /// The stick's camera angle of this row's camera (conker-sim's oracle test does the same).
    yaw: u16,
    /// 0 playing, 7 the end fade, 3 over.
    phase: u8,
    /// obj+0x4C, the animation's rate.
    rate: f32,
    raw: Raw,
}

/// What the event tables read from the raw record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Raw {
    /// obj+0x84, +0x125, +0x104; s126+0x75, +0x19A.
    anim: u16,
    immune: u8,
    stun: u8,
    item: u8,
    grenades: u8,
    /// The scope's yaw and pitch (s126+0x16C, +0x170) while it is on (s126+0x197 = 2).
    scope: Option<[f32; 2]>,
    /// What the camera reads: s126+0x78 and s126+0x11A = 2.
    camera: (u8, bool),
    /// obj+0xB4: ticks into the clip.
    time: f32,
}

/// Match rows (mp 1) by (read, player).
fn trace(name: &str) -> Option<HashMap<(u32, usize), Row>> {
    let text = read(&format!("trajectories/{name}.csv"))?;
    // The camera agent's camera4 traces keep theirs in raw/<name>.bin.
    let dir = PathBuf::from(ORACLE);
    let raw = std::fs::read(dir.join(format!("trajectories/{name}.raw")))
        .or_else(|_| std::fs::read(dir.join(format!("raw/{name}.bin"))))
        .ok()?;
    const OBJ: usize = 0x32C;
    const REC: usize = 8 + OBJ + 0x1B0 + 0x700;
    let raws: HashMap<(u32, usize), Raw> = raw
        .as_chunks::<REC>()
        .0
        .iter()
        .map(|r| {
            let w = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
            let (o, s) = (&r[8..8 + OBJ], &r[8 + OBJ..]);
            let raw = Raw {
                anim: u16::from_be_bytes([o[0x84], o[0x85]]),
                immune: o[0x125],
                stun: o[0x104],
                item: s[0x75],
                grenades: s[0x19A],
                camera: (s[0x78], s[0x11A] == 2),
                time: f32::from_be_bytes(o[0xB4..0xB8].try_into().unwrap()),
                scope: (s[0x197] == 2).then(|| {
                    [0x16C, 0x170].map(|k| f32::from_be_bytes(s[k..k + 4].try_into().unwrap()))
                }),
            };
            ((w(0), w(4) as usize), raw)
        })
        .collect();
    let mut lines = text.lines();
    let header: HashMap<&str, usize> = lines
        .next()?
        .split(',')
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();
    let rows = lines
        .map(|l| l.split(',').collect::<Vec<_>>())
        .filter(|c| c[header["mp"]] == "1" && !c[header["x"]].is_empty())
        .filter_map(|c| {
            let f = |k: &str| c[header[k]].parse::<f32>().unwrap();
            let u = |k: &str| c[header[k]].parse::<u32>().unwrap();
            let (read, player) = (u("read"), u("player") as usize);
            let (eye, at) = (
                ["cam_eye_x", "cam_eye_y", "cam_eye_z"].map(f),
                ["cam_at_x", "cam_at_y", "cam_at_z"].map(f),
            );
            let pos = ["x", "y", "z"].map(f);
            let mut state = PlayerState::spawn(pos, u("facing") as u16);
            state.y_vel = f("y_vel");
            state.gravity = f("gravity");
            state.xz_speed = f("xz_speed");
            state.target_speed = f("target_speed");
            state.move_angle = u("move_angle") as u16;
            state.state = u("state") as u8;
            state.floor_y = f("floor_y");
            state.airborne = pos[1] > state.floor_y;
            let yaw = CameraState {
                yaw: yaw_of(at[0] - eye[0], at[2] - eye[2]),
                ..CameraState::default()
            }
            .stick_yaw();
            let row = Row {
                timer: u("timer") as i32,
                state,
                health: u("health") as u8,
                yaw,
                phase: u("phase") as u8,
                rate: f("anim_speed"),
                raw: *raws.get(&(read, player))?,
            };
            Some(((read, player), row))
        })
        .collect();
    Some(rows)
}

/// The script's input at `read` (later lines win, uncovered reads are neutral), as the oracle reads it.
fn script(name: &str) -> Option<impl Fn(u32) -> (u16, i8, i8) + use<>> {
    let lines: Vec<(u32, u32, u16, i8, i8)> = read(&format!("scripts/{name}.txt"))?
        .lines()
        .map(|l| {
            l.split('#')
                .next()
                .unwrap()
                .split_whitespace()
                .collect::<Vec<_>>()
        })
        .filter(|w| w.len() == 5)
        .map(|w| {
            (
                w[0].parse().unwrap(),
                w[1].parse().unwrap(),
                u16::from_str_radix(w[2], 16).unwrap(),
                w[3].parse().unwrap(),
                w[4].parse().unwrap(),
            )
        })
        .collect();
    Some(move |read: u32| {
        lines
            .iter()
            .rev()
            .find(|l| (l.0..=l.1).contains(&read))
            .map_or((0, 0, 0), |l| (l.2, l.3, l.4))
    })
}

/// An event as `tools/combat_table.py` writes it, reduced to what both sides can say:
/// (read, player, kind, values).
type Ev = (u32, usize, String, String);

fn oracle_events(name: &str, first: u32, last: u32) -> Option<BTreeSet<Ev>> {
    let text = read(&format!("trajectories/{name}_events.csv"))?;
    let mut out = BTreeSet::new();
    for line in text.lines().skip(1) {
        let mut it = line.splitn(4, ',');
        let (read, player, kind) = (
            it.next()?.parse::<u32>().ok()?,
            it.next()?.parse().ok()?,
            it.next()?,
        );
        let v = it.next()?.trim_matches('"');
        let t: Vec<&str> = v.split_whitespace().collect();
        let v = match kind {
            "hp" | "death" => format!("{} anim {}", t[0], t[t.len() - 1]),
            "weapon" => t[0].to_string(),
            "respawn" => format!("record {}", t[t.iter().position(|&w| w == "record")? + 1]),
            "teleport" => continue,
            _ => v.to_string(),
        };
        if (first..=last).contains(&read) {
            out.insert((read, player, kind.to_string(), v));
        }
    }
    Some(out)
}

/// The values the event tables track, per player.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    hp: u8,
    raw: Raw,
}

const LOCO: [u16; 12] = [0, 1, 2, 15, 22, 27, 33, 50, 54, 55, 56, 122];

fn diff_events(
    read: u32,
    p: usize,
    old: Seen,
    cur: Seen,
    record: Option<usize>,
    out: &mut BTreeSet<Ev>,
) {
    let mut add = |k: &str, v: String| {
        out.insert((read, p, k.to_string(), v));
    };
    if cur.hp != old.hp {
        if old.hp == 0 && cur.hp > 0 {
            add(
                "respawn",
                format!("record {}", record.map_or("?".into(), |r| r.to_string())),
            );
        } else {
            add(
                if cur.hp == 0 { "death" } else { "hp" },
                format!("{}->{} anim {:#x}", old.hp, cur.hp, cur.raw.anim),
            );
        }
    }
    let (o, c) = (old.raw, cur.raw);
    if c.anim != o.anim && !(LOCO.contains(&c.anim) && LOCO.contains(&o.anim)) {
        add("state", format!("{:#x}->{:#x}", o.anim, c.anim));
    }
    if c.item != o.item {
        add("weapon", format!("{:#04x}->{:#04x}", o.item, c.item));
    }
    if c.grenades != o.grenades {
        add("grenades", format!("{}->{}", o.grenades, c.grenades));
    }
    if (c.immune == 0) != (o.immune == 0) {
        add("immune", format!("{}->{}", o.immune, c.immune));
    }
}

fn seen(m: &Match, p: usize) -> Seen {
    let f = &m.players[p];
    Seen {
        hp: f.health,
        raw: Raw {
            anim: conker_rules::characters::clip(f.class, f.anim),
            immune: f.immune,
            stun: f.stun,
            item: f.item(m.tick),
            grenades: f.grenades,
            scope: None,
            camera: (0, false),
            time: 0.0,
        },
    }
}

/// Kill counter changes: (frame, player, new value).
type Kills = BTreeSet<(u32, usize, u16)>;

#[derive(Debug, Default)]
struct Report {
    rows: usize,
    /// Rows whose match phase (playing 0, fade 7, over 3) differs.
    phase: usize,
    /// Kill counter changes (frame, player, value): the game's (lldb writes) and the port's.
    kills: (Kills, Kills),
    /// Per player: max position error, rows whose health / animation id / immune differ.
    pos_err: [f32; 2],
    first_pos: [Option<u32>; 2],
    /// Grenade frames compared and their max position error; bullets likewise.
    grenade_rows: usize,
    grenade_err: f32,
    /// Logged bullet rows, those with a port bullet within 2 units (the game's muzzle follows the
    /// hand's animated pose, the port's offsets are fixed), and the max error over those.
    bullet_rows: usize,
    bullet_matched: usize,
    bullet_err: f32,
    health: usize,
    anim: usize,
    immune: usize,
    timer: usize,
    events: usize,
    matched: usize,
    missing: Vec<Ev>,
    extra: Vec<Ev>,
    /// Frames whose RNG state came from the game.
    rng_fed: usize,
    /// Rows whose facing differs (printed), and the first per player.
    facing: usize,
    first_facing: [Option<u32>; 2],
    /// Rows of armed locomotion (`conker_anim::armed_move`'s ids) whose id matches, and the
    /// largest rate error there (`conker_anim::request_at` with the speed percentage before the
    /// frame).
    armed_rows: usize,
    armed_rate: f32,
    /// Rows whose camera weapon or hand state (`Fighter::camera_weapon`, `in_hand`) differs.
    camera: usize,
    first_camera: Option<(u32, usize)>,
}

/// The game's RNG word at the first call of each frame to a function whose draws the port
/// reproduces (the idle pick `func_1507EFD0`, the swing pick `func_150641D8`, the respawn picker
/// `func_15084D70`, and the shotgun's pick `func_15065A5C` on the frames it fires, `func_150FF840`:
/// it runs every frame a weapon is held): frame → state.
fn rng_states(log: &str) -> HashMap<u32, u64> {
    let calls = calls(log);
    let shotgun: BTreeSet<u32> = (calls.iter())
        .filter(|c| c.func == "func_150FF840")
        .map(|c| c.frame)
        .collect();
    let mut out = HashMap::new();
    for c in calls {
        let pick = ["func_1507EFD0", "func_150641D8", "func_15084D70"].contains(&c.func.as_str())
            || (c.func == "func_15065A5C" && shotgun.contains(&c.frame));
        if !pick {
            continue;
        }
        let (Some(hi), Some(lo)) = (c.kv.get("0x800885B0"), c.kv.get("0x800885B4")) else {
            continue;
        };
        let w = |v: &str| u64::from_str_radix(v.trim_start_matches("0x"), 16).unwrap();
        out.entry(c.frame).or_insert(w(hi) << 32 | w(lo));
    }
    out
}

/// A replayed trace: from `start` (both players' traced state), with the weapon the oracle's lldb
/// poke gave player 1 (`give`: frame, id) and a forced match clock (`clock`: frame, from the
/// trace's timer column).
struct Case {
    name: &'static str,
    /// Events the port is known to miss or add (not ported; see `KNOWN`), `usize::MAX` for a
    /// weapon the port does not model.
    known: usize,
    /// The scripts (`scripts/<script>.txt` and `_p2`), when not the trace's name.
    script: Option<&'static str>,
    start: u32,
    give: Option<(u32, u8)>,
    clock: Option<u32>,
    /// An lldb capture logging the game's RNG word (`0x800885B0`/`B4`) at the entry of the
    /// functions that draw for the port's random picks (`tools/combat4_lldb.sh`): the port's RNG is
    /// set to it before each frame that has one, so the picks can match (the game's own sequence
    /// is not reproducible: hundreds of other draws a frame).
    rng: Option<&'static str>,
    /// The largest grenade position error allowed.
    grenade_tol: f32,
    /// The last row compared, when the trace runs into something conker-sim does not model.
    end: Option<u32>,
    /// The start item both players hold (holstered) and get back after a respawn, as the oracle's
    /// poke of MP+0x1170 at read 23 gave it (`Fighter::start`).
    start_item: Option<u8>,
    /// Both players are cavemen (`CONKER_MP_CHARS=6,6`; ctlw40c's idle player 2 was a Tedi).
    cavemen: bool,
}

const fn case(name: &'static str, start: u32, give: Option<(u32, u8)>) -> Case {
    Case {
        name,
        known: 0,
        script: None,
        start,
        give,
        clock: None,
        rng: None,
        grenade_tol: 0.5,
        end: None,
        start_item: None,
        cavemen: false,
    }
}

/// The caveman wave (`combat6_*`, `caveman_README.md`): item 0x40 held from the start.
const fn club(name: &'static str, start: u32, rng: &'static str) -> Case {
    Case {
        rng: Some(rng),
        start_item: Some(0x40),
        ..case(name, start, None)
    }
}

const fn encounter(name: &'static str, id: u8) -> Case {
    case(name, 745, Some((745, id)))
}

const fn probe(name: &'static str, id: u8) -> Case {
    Case {
        script: Some("combat_ctlw"),
        ..case(name, 30, Some((40, id)))
    }
}

/// The known gaps: e_w09 the twin guns' idle fidget 0x144 (4 events); e_w21 and ctlw_k1 the swing
/// 0x129 the game picks over 0x12A on 2 of 9 swings (4 events), and k1 its landing 0x19D one frame
/// shorter than conker-sim's 15 (2 events); e_w15 the flamethrower's flames (not modelled).
/// Positions are printed, not asserted: their gaps are conker-sim's (player-to-player pushes in
/// e_w14b and e_w21, C-left/right strafing in buttons, ctl_cd_cr and k6/k10, a wall-push reversal
/// in pick87) or root motion the port does not have (dead bodies in e_w22/e_w24, 0x16's Z in k7).
const CASES: [Case; 65] = [
    case("combat_e_gren", 745, None),
    Case {
        rng: Some("combat4_e_w09_lldb"),
        ..encounter("combat_e_w09", 0x09)
    },
    encounter("combat_e_w39", 0x39),
    Case {
        rng: Some("combat4_e_w23_lldb"),
        ..encounter("combat_e_w23", 0x23)
    },
    encounter("combat_e_w24", 0x24),
    Case {
        known: 2,
        ..encounter("combat_e_w15", 0x15)
    },
    Case {
        rng: Some("combat4_e_w21_lldb"),
        ..encounter("combat_e_w21", 0x21)
    },
    encounter("combat_e_w22", 0x22),
    encounter("combat_e_w14", 0x14),
    encounter("combat_e_w14b", 0x14),
    Case {
        rng: Some("combat4_e_w16_lldb"),
        ..encounter("combat_e_w16", 0x16)
    },
    Case {
        rng: Some("combat4_ctlw_k0_lldb"),
        ..probe("combat_ctlw_k0", 0x09)
    },
    Case {
        rng: Some("combat4_ctlw_k1_lldb"),
        ..probe("combat_ctlw_k1", 0x21)
    },
    probe("combat_ctlw_k3", 0x22),
    probe("combat_ctlw_k4", 0x14),
    probe("combat_ctlw_k5", 0x39),
    Case {
        rng: Some("combat4_ctlw_k6_lldb"),
        ..probe("combat_ctlw_k6", 0x23)
    },
    Case {
        rng: Some("combat4_ctlw_k7_lldb"),
        ..probe("combat_ctlw_k7", 0x16)
    },
    probe("combat_ctlw_k8", 0x15),
    probe("combat_ctlw_k10", 0x24),
    Case {
        rng: Some("combat4_ammo_cook_lldb"),
        ..case("combat_ammo_cook", 30, Some((40, 0x09)))
    },
    case("combat_pick57", 30, None),
    case("combat_pick87", 30, None),
    case("combat_ctl_cd", 30, None),
    case("combat_ctl_cdhold", 30, None),
    case("combat_ctl_cd_z", 30, None),
    case("combat_ctl_cd_cr", 30, None),
    case("combat_buttons", 30, None),
    Case {
        clock: Some(100),
        ..case("combat_end", 30, None)
    },
    // Wave 4 (lab/oracle/scripts/combat4_*.txt): weapon 0x14's aimed shot at 146.5 and at a wall,
    // the rocket at 146.5, a grenade cooked off in the hand next to player 2, the sniper over a
    // crouching player 2, three cooked throws.
    encounter("combat4_w14", 0x14),
    case("combat4_probe14", 30, Some((40, 0x14))),
    Case {
        rng: Some("combat4_w16_lldb"),
        ..encounter("combat4_w16", 0x16)
    },
    Case {
        rng: Some("combat4_dead2_lldb"),
        ..case("combat4_dead2", 745, None)
    },
    encounter("combat4_w24", 0x24),
    // The melee lock-on range: swings while strafing away from player 2 (lock-on at 141.6 and
    // 165.9, none at 201.1 and beyond); the scope pitched down with the sniper rifle and with weapon
    // 0x14 (head, body, misses).
    // (Compared to row 995: from 996 the strafe climbs a slope that slows the game's player and
    // not conker-sim's, a movement gap; the swings after 1000 are 600+ away anyway.)
    Case {
        rng: Some("combat4_lock_lldb"),
        end: Some(995),
        ..encounter("combat4_lock", 0x21)
    },
    encounter("combat4_pitch24", 0x24),
    encounter("combat4_pitch14", 0x14),
    // The camera agent's armed probes (camera4_armed.txt): each weapon drawn at the spawn, run and
    // turned (compared to row 249: from 250 R's aim camera holds the stick, the camera loop's).
    Case {
        end: Some(249),
        ..case("camera4_w09", 30, Some((40, 0x09)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w39", 30, Some((40, 0x39)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w23", 30, Some((40, 0x23)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w24", 30, Some((40, 0x24)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w15", 30, Some((40, 0x15)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w21", 30, Some((40, 0x21)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w22", 30, Some((40, 0x22)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w14", 30, Some((40, 0x14)))
    },
    Case {
        end: Some(249),
        ..case("camera4_w16", 30, Some((40, 0x16)))
    },
    // The cooked grenade bouncing up the ramp south of the spawn drifts 4 units in 15 frames (I:
    // the grenade's steps on a slope).
    Case {
        grenade_tol: 4.0,
        ..case("combat4_cook", 30, None)
    },
    // Weapons 2 (lab/oracle/combat5_README.md): the crossbow's controls, hip and aimed cadence,
    // bolts fired on odd and even frames, a body hit then a head kill at 146.5; the shotgun's
    // controls and cadence, its kill at 146.5 and at point blank, its blow-up and outer hits.
    case("combat5_ctlw18", 30, Some((40, 0x18))),
    case("combat5_cb3", 30, Some((40, 0x18))),
    case("combat5_cb4", 30, Some((40, 0x18))),
    encounter("combat5_cb_e1", 0x18),
    encounter("combat5_cb_e2", 0x18),
    Case {
        rng: Some("combat5_ctlw37_lldb"),
        ..case("combat5_ctlw37", 30, Some((40, 0x37)))
    },
    Case {
        rng: Some("combat5_sg3_lldb"),
        ..case("combat5_sg3", 30, Some((40, 0x37)))
    },
    Case {
        rng: Some("combat5_sg_e1_lldb"),
        ..encounter("combat5_sg_e1", 0x37)
    },
    // Player 2 runs scared from player 1's drawn shotgun (0x2B, then the skid 0x21).
    Case {
        rng: Some("combat5_sg_e2_lldb"),
        ..encounter("combat5_sg_e2", 0x37)
    },
    Case {
        rng: Some("combat5_sg_e3_lldb"),
        ..encounter("combat5_sg_e3", 0x37)
    },
    // e_w39's scripts with the tommy gun (0x38, map 0x1A's) on 0x2D: the dead pose ends in 0x18A
    // 63 frames in, which e_w39's kill by the same type 0x35 does not (not traced: 3 events).
    Case {
        known: 3,
        script: Some("combat_e_w39"),
        rng: Some("combat7_e_w38_lldb"),
        ..encounter("combat7_e_w38", 0x38)
    },
    // C-down with the twin guns and with the flamethrower drawn: the C strafe back, no grenade
    // (taps at 160 and 220, held 280-320).
    Case {
        script: Some("combat7_cdown"),
        rng: Some("combat7_cd09_lldb"),
        ..case("combat7_cd09", 30, Some((40, 0x09)))
    },
    Case {
        script: Some("combat7_cdown"),
        rng: Some("combat7_cd15_lldb"),
        ..case("combat7_cd15", 30, Some((40, 0x15)))
    },
    // sg_e1 at point blank (player 1 walks to 108.5 first): the muzzle's 0x3A hits before 0x32
    // kills, the same frame (V: the scorer's two hits at 790).
    Case {
        rng: Some("combat7_sgpb_lldb"),
        ..encounter("combat7_sgpb", 0x37)
    },
    // The caveman wave (lab/oracle/caveman_README.md): item 0x40's controls held by a Tedi and by a
    // caveman (its grenade's pull 0x175 and throw 0x16C: `characters::clip`), and its swings and
    // second swings to a kill, Tediz and cavemen.
    club("combat6_ctlw40", 30, "combat6_ctlw40_lldb"),
    Case {
        cavemen: true,
        ..club("combat6_ctlw40c", 30, "combat6_ctlw40c_lldb")
    },
    club("combat6_e40", 745, "combat6_e40_lldb"),
    Case {
        cavemen: true,
        ..club("combat6_e40c", 745, "combat6_e40c_lldb")
    },
];

fn phase_code(m: &Match) -> u8 {
    match m.phase() {
        conker_rules::Phase::Playing => 0,
        conker_rules::Phase::Fading => 7,
        conker_rules::Phase::Over => 3,
    }
}

/// Kill counter writes in an lldb capture (`write 0x800E0B36:2 a -> b`, player 1's; +2 per player).
fn kill_writes(log: &str) -> Kills {
    log.lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            let p = match (w.get(2), w.get(3)) {
                (Some(&"write"), Some(&"0x800E0B36:2")) => 0,
                (Some(&"write"), Some(&"0x800E0B38:2")) => 1,
                _ => return None,
            };
            Some((w[0].parse().ok()?, p, w[6].parse().ok()?))
        })
        .collect()
}

fn replay(c: &Case) -> Option<Report> {
    let (name, start) = (c.name, c.start);
    let sc = scene()?;
    let rows = trace(name)?;
    // Player 2 without a script of its own stands idle (no CONKER_INPUT_SCRIPT_P2).
    let sname = c.script.unwrap_or(name);
    let scripts = [script(sname), script(&format!("{sname}_p2"))];
    scripts[0].as_ref()?;
    let last = rows.keys().filter(|k| k.1 == 1).map(|k| k.0).max()?;
    let last = c.end.map_or(last, |e| e.min(last));
    let players = (0..2)
        .map(|p| {
            let r = &rows[&(start, p)];
            let place = conker_rules::Placement {
                index: 0,
                pos: r.state.pos,
                facing: r.state.facing,
                camera: [0.0; 3],
            };
            let mut f = Fighter::new(&place, p as u8, start);
            f.body = r.state;
            (f.start, f.weapon) = (c.start_item, c.start_item);
            if c.cavemen {
                f.side = conker_sim::movement::Side::Cavemen;
                f.class = conker_rules::raptor::CAVEMAN;
            }
            (f.health, f.immune, f.stun, f.anim, f.grenades) = (
                r.health,
                r.raw.immune,
                r.raw.stun,
                r.raw.anim,
                r.raw.grenades,
            );
            f
        })
        .collect();
    let mut m = Match::new(&sc.arena, players, Rng::default(), start);
    m.timer = rows[&(start, 0)].timer;
    // The clocks as the trace shows them at the start (id, time, rate; no blend).
    let mut posed = poses().map(|(p, this)| {
        let mut p = p.clone();
        p.clocks = (0..2)
            .map(|k| {
                let r = &rows[&(start, k)];
                let clock = conker_anim::AnimPlayer {
                    id: r.raw.anim,
                    time: r.raw.time,
                    rate: r.rate,
                    ..Default::default()
                };
                (clock, m.players[k].body.speed_percent)
            })
            .collect();
        (p, this)
    });
    let dump = std::env::var("COMBAT_DUMP").ok().and_then(|v| {
        let mut it = v.split(':');
        Some((
            it.next()?.to_string(),
            it.next()?.parse::<u32>().ok()?,
            it.next()?.parse::<u32>().ok()?,
        ))
    });
    let mut report = Report::default();
    let mut sim_events = BTreeSet::new();
    let log = read(&format!("trajectories/{name}_lldb.txt")).unwrap_or_default();
    let grenades = grenade_rows(&log);
    let mut bullets: HashMap<u32, Vec<[f32; 3]>> = HashMap::new();
    for b in bullet_rows(&log) {
        bullets.entry(b.frame).or_default().push(b.pos);
    }
    report.kills.0 = kill_writes(&log);
    let rng_states = c
        .rng
        .and_then(|n| read(&format!("trajectories/{n}.txt")))
        .map(|t| rng_states(&t))
        .unwrap_or_default();
    report.rng_fed = rng_states.len();
    let watched = log.contains("armed 0x800E0B36");
    for t in start..last {
        let (Some(_), Some(_)) = (rows.get(&(t + 1, 0)), rows.get(&(t + 1, 1))) else {
            break;
        };
        let before = [seen(&m, 0), seen(&m, 1)];
        if let Some((at, id)) = c.give
            && t == at
        {
            m.give(0, id);
        }
        if c.clock == Some(t) {
            m.timer = rows[&(t, 0)].timer;
        }
        let inputs: Vec<Input> = (0..2)
            .map(|p| {
                let (buttons, stick_x, stick_y) = scripts[p].as_ref().map_or((0, 0, 0), |s| s(t));
                Input {
                    stick_x,
                    stick_y,
                    buttons,
                    camera_yaw: rows[&(t, p)].yaw,
                }
            })
            .collect();
        if let Some(&state) = rng_states.get(&t) {
            m.rng = Rng::new(state);
        }
        // The scope as the camera left it at the end of the last frame (the game keeps it in
        // s126; conker-camera's `Scope` gives the same to a server).
        for p in 0..2 {
            let scope = rows[&(t, p)].raw.scope;
            let f = &mut m.players[p];
            f.body.scope_yaw = scope.map(|s| s[0]);
            f.scope_pitch = scope.map_or(0.0, |s| s[1]);
        }
        let kills = [m.players[0].kills, m.players[1].kills];
        // obj+0xAA before the frame (a strafing frame counts at 65, below).
        let percent = [0, 1].map(|p| m.players[p].body.speed_percent);
        // Bullets at the start of their update in frame t: the ones carried over, then the new ones.
        let carried: Vec<[f32; 3]> = m.bullets.iter().map(|b| b.1.pos).collect();
        if let Some((p, this)) = &posed {
            p.place(&mut m);
            if *this {
                let mut next = m.clone();
                let events = next.step(&sc.arena, &inputs, &sc.world);
                let mut q = p.clone();
                q.tick(&next, &events);
                q.place(&mut next);
                m.parts = next.parts;
            }
        }
        let events = m.step(&sc.arena, &inputs, &sc.world);
        if let Some((p, _)) = &mut posed {
            p.tick(&m, &events);
        }
        let mut sim_bullets = carried;
        sim_bullets.extend(events.iter().filter_map(|e| match e {
            Event::Shot { pos, .. } => Some(*pos),
            _ => None,
        }));
        let want = bullets.get(&t).cloned().unwrap_or_default();
        if sim_bullets.len() != want.len() && std::env::var_os("COMBAT_BULLETS").is_some() {
            eprintln!("  {name} frame {t}: sim {sim_bullets:?} want {want:?}");
        }
        for w in &want {
            report.bullet_rows += 1;
            let e = sim_bullets
                .iter()
                .map(|b| (0..3).map(|i| (b[i] - w[i]).powi(2)).sum::<f32>().sqrt())
                .fold(f32::MAX, f32::min);
            if e < 2.0 {
                report.bullet_matched += 1;
                report.bullet_err = report.bullet_err.max(e);
            }
        }
        if let (Some(g), Some(w)) = (m.grenades.first(), grenades.get(&(t + 1))) {
            report.grenade_rows += 1;
            let e = (0..3)
                .map(|i| (g.1.pos[i] - w[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            report.grenade_err = report.grenade_err.max(e);
            if dump
                .as_ref()
                .is_some_and(|(n, a, b)| n == name && (*a..=*b).contains(&(t + 1)))
            {
                eprintln!(
                    "{} grenade sim {:?} st {} yv {} v {} | want {w:?} | err {e:.4}",
                    t + 1,
                    g.1.pos,
                    g.1.state,
                    g.1.y_vel,
                    g.1.speed
                );
            }
        }
        report.rows += 1;
        let next = &rows[&(t + 1, 0)];
        report.timer += usize::from(m.timer != next.timer);
        report.phase += usize::from(phase_code(&m) != next.phase);
        for p in 0..2 {
            if watched && m.players[p].kills != kills[p] {
                report.kills.1.insert((t, p, m.players[p].kills));
            }
            let want = &rows[&(t + 1, p)];
            let f = &m.players[p];
            let err = (0..3)
                .map(|i| (f.body.pos[i] - want.state.pos[i]).powi(2))
                .sum::<f32>()
                .sqrt();
            if err > 0.1 && report.first_pos[p].is_none() {
                report.first_pos[p] = Some(t + 1);
            }
            report.pos_err[p] = report.pos_err[p].max(err);
            report.health += usize::from(f.health != want.health);
            report.anim +=
                usize::from(conker_rules::characters::clip(f.class, f.anim) != want.raw.anim);
            let armed = (1..=40).any(|t| {
                [
                    0x09, 0x39, 0x23, 0x24, 0x15, 0x21, 0x22, 0x16, 0x26, 0x18, 0x37, 0x40,
                ]
                .iter()
                .any(|&w| conker_anim::armed_move(w, t as f32, t as f32) == Some(f.anim))
            });
            if armed && f.anim == want.raw.anim {
                let percent = if f.body.prev_strafe {
                    conker_sim::movement::STRAFE_PERCENT
                } else {
                    percent[p]
                };
                let r =
                    conker_anim::request_at(f.anim, f.body.xz_speed, f.body.target_speed, percent);
                report.armed_rows += 1;
                report.armed_rate = report.armed_rate.max((r.rate - want.rate).abs());
                if std::env::var_os("COMBAT_RATES").is_some() && (r.rate - want.rate).abs() > 1e-3 {
                    eprintln!(
                        "  {name} {} p{p} {:#x} rate {} want {} (speed {} target {})",
                        t + 1,
                        f.anim,
                        r.rate,
                        want.rate,
                        f.body.xz_speed,
                        f.body.target_speed
                    );
                }
            }
            if (f.camera_weapon(), f.in_hand(m.tick)) != want.raw.camera {
                report.camera += 1;
                report.first_camera.get_or_insert((t + 1, p));
            }
            if f.body.facing != want.state.facing {
                report.facing += 1;
                report.first_facing[p].get_or_insert(t + 1);
            }
            report.immune += usize::from(f.immune != want.raw.immune);
            let record = events.iter().find_map(|e| match e {
                Event::Respawn { player, at } if *player == p => Some(at.index),
                _ => None,
            });
            diff_events(t + 1, p, before[p], seen(&m, p), record, &mut sim_events);
            if dump
                .as_ref()
                .is_some_and(|(n, a, b)| n == name && (*a..=*b).contains(&(t + 1)))
            {
                eprintln!(
                    "{} p{p} sim pos {:?} hp {} anim {:#x} imm {} stun {} sp {} mv {} fc {} | want pos {:?} hp {} anim {:#x} imm {} stun {} sp {} mv {} fc {} | err {err:.4}",
                    t + 1,
                    f.body.pos,
                    f.health,
                    f.anim,
                    f.immune,
                    f.stun,
                    f.body.xz_speed,
                    f.body.move_angle,
                    f.body.facing,
                    want.state.pos,
                    want.health,
                    want.raw.anim,
                    want.raw.immune,
                    want.raw.stun,
                    want.state.xz_speed,
                    want.state.move_angle,
                    want.state.facing
                );
            }
        }
    }
    // The camera4 probes have no event table: their animation ids are compared row by row.
    let oracle = oracle_events(name, start + 1, start + report.rows as u32)
        .or_else(|| name.starts_with("camera4_").then(|| sim_events.clone()))?;
    report.events = oracle.len();
    report.matched = oracle.intersection(&sim_events).count();
    report.missing = oracle.difference(&sim_events).cloned().collect();
    report.extra = sim_events.difference(&oracle).cloned().collect();
    Some(report)
}

#[test]
fn combat_traces() {
    if scene().is_none() {
        return;
    }
    for c in &CASES {
        let Some(r) = replay(c) else {
            eprintln!("{}: no trace", c.name);
            continue;
        };
        eprintln!(
            "{:18} rows {:4} pos {:8.4} {:8.4} (> 0.1 from {:?}) hp {} anim {} immune {} timer {} phase {} kills {}/{} | events {}/{} missing {} extra {} | grenade {} max {:.4} | bullets {}/{} max {:.5} | rng fed {} | facing {} from {:?} | camera {} from {:?} | armed rows {} rate {:.5}",
            c.name,
            r.rows,
            r.pos_err[0],
            r.pos_err[1],
            r.first_pos,
            r.health,
            r.anim,
            r.immune,
            r.timer,
            r.phase,
            r.kills.0.intersection(&r.kills.1).count(),
            r.kills.0.len().max(r.kills.1.len()),
            r.matched,
            r.events,
            r.missing.len(),
            r.extra.len(),
            r.grenade_rows,
            r.grenade_err,
            r.bullet_matched,
            r.bullet_rows,
            r.bullet_err,
            r.rng_fed,
            r.facing,
            r.first_facing,
            r.camera,
            r.first_camera,
            r.armed_rows,
            r.armed_rate
        );
        for e in r.missing.iter().take(10) {
            eprintln!("    missing {e:?}");
        }
        for e in r.extra.iter().take(10) {
            eprintln!("    extra   {e:?}");
        }
        // `COMBAT_ALL`: print every case, assert none.
        if c.known == usize::MAX || std::env::var_os("COMBAT_ALL").is_some() {
            continue;
        }
        assert!(
            r.missing.len() + r.extra.len() <= c.known,
            "{}: events",
            c.name
        );
        assert_eq!(
            (r.health, r.phase),
            (0, 0),
            "{}: health or phase rows",
            c.name
        );
        assert!(
            r.timer <= usize::from(c.clock.is_some()),
            "{}: timer",
            c.name
        );
        assert_eq!(r.kills.0, r.kills.1, "{}: kills", c.name);
        assert_eq!(r.camera, 0, "{}: camera weapon or hand", c.name);
        assert_eq!(r.armed_rate, 0.0, "{}: armed locomotion rate", c.name);
        assert!(r.grenade_err < c.grenade_tol, "{}: grenade", c.name);
        // e_w39: the pistol's shot 41 frames into the victim's fall passes the body in the game.
        let through_body = if c.name == "combat_e_w39" { 4 } else { 0 };
        assert!(
            r.bullet_rows - r.bullet_matched <= through_body,
            "{}: bullets",
            c.name
        );
    }
}

/// Every respawn in the combat event tables against [`conker_rules::respawn`]: the other
/// participants where the trace has them in the frame of the pick (row R − 1; dead ones where
/// they lie, and one that respawned in the same frame earlier in the slot order on its new
/// record), the player's previous respawn record as `last`, Deathmatch's masks. The game's RNG
/// state is not reproducible (`spawn` module), so each pick runs under 64 RNG states: the logged
/// record must be among the results, and a pick is RNG-independent when it is the only one.
#[test]
fn respawns_match_the_picker() {
    let Some(sc) = scene() else { return };
    let dir = PathBuf::from(ORACLE).join("trajectories");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter_map(|n| {
            let n = n.strip_suffix("_events.csv")?;
            (n.starts_with("combat_")
                || n.starts_with("combat4_")
                || n.starts_with("combat5_")
                || n.starts_with("combat6_"))
            .then(|| n.to_string())
        })
        .collect();
    names.sort();
    let (mut picks, mut independent) = (0, 0);
    for name in names {
        let events = read(&format!("trajectories/{name}_events.csv")).unwrap();
        let respawns: Vec<(u32, usize, usize)> = events
            .lines()
            .filter(|l| l.contains(",respawn,"))
            .filter_map(|l| {
                let w: Vec<&str> = l.split(',').collect();
                let rec = l
                    .split("record ")
                    .nth(1)?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()?;
                Some((w[0].parse().ok()?, w[1].parse().ok()?, rec))
            })
            .collect();
        if respawns.is_empty() {
            continue;
        }
        let Some(text) = read(&format!("trajectories/{name}.csv")) else {
            continue;
        };
        let mut lines = text.lines();
        let header: HashMap<&str, usize> = lines
            .next()
            .unwrap()
            .split(',')
            .enumerate()
            .map(|(i, h)| (h, i))
            .collect();
        let pos: HashMap<(u32, usize), [f32; 2]> = lines
            .map(|l| l.split(',').collect::<Vec<_>>())
            .filter(|c| c[header["mp"]] == "1" && !c[header["x"]].is_empty())
            .map(|c| {
                let f = |k: &str| c[header[k]].parse::<f32>().unwrap();
                ((f("read") as u32, f("player") as usize), [f("x"), f("z")])
            })
            .collect();
        let mut last: HashMap<usize, usize> = HashMap::new();
        for &(read, p, want) in &respawns {
            // A player that respawned in the same frame earlier in the slot order is already on its
            // new record (V combat4_dead2: player 2 picks against player 1 on record 6).
            let enemies: Vec<[f32; 2]> = (0..16)
                .filter(|&q| q != p)
                .filter_map(|q| {
                    let first = respawns.iter().any(|r| r.0 == read && r.1 == q && q < p);
                    pos.get(&(if first { read } else { read - 1 }, q)).copied()
                })
                .collect();
            let got: BTreeSet<usize> = (0..64u64)
                .map(|k| {
                    let mut rng = Rng::new(k.wrapping_mul(0x9E37_79B9_7F4A_7C15) & 0x1_FFFF_FFFF);
                    conker_rules::respawn(
                        &sc.arena.spawns,
                        conker_rules::DEATHMATCH_MASKS[0],
                        last.get(&p).copied(),
                        &enemies,
                        &mut rng,
                    )
                    .index
                })
                .collect();
            eprintln!(
                "  {name} read {read} player {p}: record {want}, picker gives {got:?} ({} others)",
                enemies.len()
            );
            assert!(
                got.contains(&want),
                "{name} read {read}: {want} not in {got:?}"
            );
            picks += 1;
            independent += usize::from(got.len() == 1);
            last.insert(p, want);
        }
    }
    eprintln!("respawns: {picks} picks consistent, {independent} independent of the RNG");
}
