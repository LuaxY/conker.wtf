//! Replays the oracle's traces (`lab/oracle`, outside this tree; env `CONKER_ORACLE` overrides the path)
//! through the camera and compares eye and look-at frame by frame. Skips when the traces are absent.
//!
//! Row N of a trace is the state after game frame N-1, so the camera is stepped with row N's player and
//! compared with row N's camera. Where a trace has a raw dump (`raw/<name>.bin`, `CONKER_TRACE_RAW`), it
//! also supplies the inputs the CSV lacks: the heading `obj+0x40`, the head height `struct126+0x114`,
//! the floor under the camera `cam+0x35C`, and for the armed traces the weapon (`struct126+0x78`), the
//! twist (`+0x12`), the stun (`obj+0x104`) and the weapon in the hand (`struct126+0x11A`). The ROM
//! test computes that floor from the extracted collision instead, as the simulation will.
//!
//! Debugging: `CAMERA_DUMP=x:<first>:<last>` prints the replayed camera against the trace's rows.

use conker_camera::volume::{CameraLevel, CameraVolume, HINT_CUT, in_volume};
use conker_camera::{CameraState, CameraWorld, PlayerView, RayHit};
use extract_core::Rom;
use extract_scene::{Triangle, floor_below, scene_collision, scene_placements};
use std::collections::{HashMap, VecDeque};
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

/// Player 1's rows of a trace, by read.
struct Trace {
    rows: HashMap<u32, Row>,
    last: u32,
    /// The game's camera queries per frame (read), from `trajectories/<name>_camcalls.csv`.
    calls: HashMap<u32, Calls>,
}

/// One frame's recorded camera queries (tools/camera_lldb.py): rays and sweeps in call order, and
/// whether a level hint cut the camera.
#[derive(Clone, Default)]
struct Calls {
    rays: VecDeque<([f32; 7], Option<RayHit>)>,
    sweeps: VecDeque<([f32; 8], Option<[f32; 3]>)>,
    cut: bool,
}

fn load_calls(dir: &Path, name: &str) -> HashMap<u32, Calls> {
    let mut out: HashMap<u32, Calls> = HashMap::new();
    let Ok(text) = std::fs::read_to_string(dir.join(format!("trajectories/{name}_camcalls.csv")))
    else {
        return out;
    };
    for line in text.lines() {
        let v: Vec<&str> = line.split(',').collect();
        let f = |i: usize| v[i].parse::<f32>().unwrap();
        let read: u32 = v[0].parse().unwrap();
        match v[1] {
            "ray" => {
                let args = std::array::from_fn(|k| f(3 + k));
                let hit = (v[10] != "0").then(|| RayHit {
                    dist: f(11),
                    point: [f(12), f(13), f(14)],
                    tri: std::array::from_fn(|i| {
                        std::array::from_fn(|k| v[15 + 3 * i + k].parse().unwrap())
                    }),
                });
                out.entry(read).or_default().rays.push_back((args, hit));
            }
            "sweep" => {
                let args = std::array::from_fn(|k| f(2 + k));
                let hit = (v[14] != "0").then(|| [f(15), f(16), f(17)]);
                out.entry(read).or_default().sweeps.push_back((args, hit));
            }
            "cut" if v[4] == "1" => out.entry(read).or_default().cut = true,
            _ => {}
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Row {
    pos: [f32; 3],
    floor_y: f32,
    facing: u16,
    move_angle: u16,
    state: u8,
    pad: (u16, i8, i8),
    eye: [f32; 3],
    at: [f32; 3],
    /// From the raw dump.
    raw: Option<Raw>,
}

/// Raw-dump fields of a row (the state after the frame, like the CSV).
#[derive(Clone, Copy)]
struct Raw {
    /// `obj+0x40`, `struct126+0x114`, `cam+0x35C`.
    heading: f32,
    head: f32,
    floor: f32,
    /// `cam+0x37C`: the yaw.
    yaw: f32,
    /// `cam+0x84`: bit 0x40000 is the volume flag.
    flags: u32,
    /// `cam+0x384`, `+0x5CC`, `+0x390`: the stick's yaw override, its blend, the heading's side.
    stick: [f32; 3],
    /// `struct126+0x78`, `+0x12`, `obj+0x104` != 0, `struct126+0x11A` = 2: the drawn weapon, the
    /// twist, stunned, the weapon in the hand.
    weapon: u8,
    twist: i16,
    stunned: bool,
    in_hand: bool,
}

fn load(dir: &Path, name: &str, raw_name: &str) -> Trace {
    let text = std::fs::read_to_string(dir.join(format!("trajectories/{name}.csv"))).unwrap();
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap().split(',').collect();
    let col = |n: &str| head.iter().position(|h| *h == n).unwrap();
    let raw = std::fs::read(raw_path(dir, raw_name)).ok();
    let raw_fields = raw_index(raw.as_deref());
    let mut rows = HashMap::new();
    for line in lines {
        let v: Vec<&str> = line.split(',').collect();
        if v[col("player")] != "0" || v[col("cam_eye_x")].is_empty() {
            continue;
        }
        let f = |n: &str| v[col(n)].parse::<f32>().unwrap();
        let hex = |n: &str| u16::from_str_radix(v[col(n)].trim_start_matches("0x"), 16).unwrap();
        let read: u32 = v[col("read")].parse().unwrap();
        rows.insert(
            read,
            Row {
                pos: [f("x"), f("y"), f("z")],
                floor_y: f("floor_y"),
                facing: f("facing") as u16,
                move_angle: f("move_angle") as u16,
                state: f("state") as u8,
                pad: (hex("pad_buttons"), f("pad_x") as i8, f("pad_y") as i8),
                eye: [f("cam_eye_x"), f("cam_eye_y"), f("cam_eye_z")],
                at: [f("cam_at_x"), f("cam_at_y"), f("cam_at_z")],
                raw: raw_fields.get(&read).copied(),
            },
        );
    }
    let last = *rows.keys().max().unwrap();
    Trace {
        rows,
        last,
        calls: load_calls(dir, name),
    }
}

/// A raw dump's path: `raw/<name>.bin`, or a path under the oracle directory (the combat traces keep
/// theirs in `trajectories/`).
fn raw_path(dir: &Path, name: &str) -> PathBuf {
    if name.contains('/') {
        dir.join(name)
    } else {
        dir.join(format!("raw/{name}.bin"))
    }
}

/// `CONKER_TRACE_RAW` records: u32 read, u32 player (LE), then the player object (0x32C), its
/// struct126 (0x1B0) and its camera (0x700), big-endian.
fn raw_index(data: Option<&[u8]>) -> HashMap<u32, Raw> {
    const OBJ: usize = 8;
    const S126: usize = OBJ + 0x32C;
    const CAM: usize = S126 + 0x1B0;
    const REC: usize = CAM + 0x700;
    let mut out = HashMap::new();
    for r in data.unwrap_or_default().as_chunks::<REC>().0 {
        let le = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        let be = |o: usize| f32::from_bits(u32::from_be_bytes(r[o..o + 4].try_into().unwrap()));
        if le(4) == 0 {
            let h = |o: usize| i16::from_be_bytes(r[o..o + 2].try_into().unwrap());
            let head = h(S126 + 0x114);
            let raw = Raw {
                heading: be(OBJ + 0x40),
                head: f32::from(head),
                floor: be(CAM + 0x35C),
                yaw: be(CAM + 0x37C),
                flags: be(CAM + 0x84).to_bits(),
                stick: [be(CAM + 0x384), be(CAM + 0x5CC), be(CAM + 0x390)],
                weapon: r[S126 + 0x78],
                twist: h(S126 + 0x12),
                stunned: r[OBJ + 0x104] != 0,
                in_hand: r[S126 + 0x11A] == 2,
            };
            out.insert(le(0), raw);
        }
    }
    out
}

/// Where the inputs the CSV lacks come from.
enum Inputs<'a> {
    /// The raw dump (falling back to the computed ones on rows it misses).
    Oracle,
    /// The same, but the volume test answered by these volumes instead of the dumped flag.
    OracleVolumes(&'a [CameraVolume]),
    /// Heading from the facing, head height from the state, floor from the scene's collision.
    Computed(&'a [Triangle]),
}

/// Answers the camera's queries for one frame with what the game recorded (the floor from the raw dump,
/// the volume flag from `cam+0x84`, rays and sweeps from the capture, in call order), and measures how
/// far the port's query inputs are from the game's.
struct OracleWorld<'a> {
    floor: f32,
    volume: bool,
    vols: Option<&'a [CameraVolume]>,
    calls: Calls,
    log: &'a mut CallLog,
}

#[derive(Default)]
struct CallLog {
    /// Queries answered from the capture, and the largest difference in their inputs.
    answered: usize,
    input_err: f32,
    /// Queries the port made that the game did not (on captured frames), and the reverse.
    extra: usize,
    unanswered: usize,
}

fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

impl CameraWorld for OracleWorld<'_> {
    fn floor_below(&mut self, _: [f32; 3], _: f32) -> Option<f32> {
        Some(self.floor)
    }
    fn ray(&mut self, o: [f32; 3], d: [f32; 3], len: f32) -> Option<RayHit> {
        let Some((args, hit)) = self.calls.rays.pop_front() else {
            self.log.extra += 1;
            return None;
        };
        let mine = [o[0], o[1], o[2], d[0], d[1], d[2], len];
        self.log.answered += 1;
        self.log.input_err = self.log.input_err.max(max_diff(&mine, &args));
        hit
    }
    fn sweep(&mut self, from: [f32; 3], to: [f32; 3], size: [f32; 2]) -> Option<[f32; 3]> {
        let Some((args, hit)) = self.calls.sweeps.pop_front() else {
            self.log.extra += 1;
            return None;
        };
        let mine = [
            from[0], from[1], from[2], to[0], to[1], to[2], size[0], size[1],
        ];
        self.log.answered += 1;
        self.log.input_err = self.log.input_err.max(max_diff(&mine, &args));
        hit
    }
    fn in_volume(&mut self, eye: [f32; 3]) -> bool {
        self.vols.map_or(self.volume, |v| in_volume(v, eye))
    }
}

/// The ROM-fed world: extract-scene's floor under the eye; no wall queries (the mover is conker-sim's),
/// the camera inside a volume.
struct RomWorld<'a>(&'a [Triangle]);

impl CameraWorld for RomWorld<'_> {
    fn floor_below(&mut self, eye: [f32; 3], start_y: f32) -> Option<f32> {
        let at = [eye[0], eye[1].max(start_y), eye[2]];
        floor_below(self.0, at, 50.0).map(|f| f.1)
    }
    fn ray(&mut self, _: [f32; 3], _: [f32; 3], _: f32) -> Option<RayHit> {
        None
    }
    fn sweep(&mut self, _: [f32; 3], _: [f32; 3], _: [f32; 2]) -> Option<[f32; 3]> {
        None
    }
    fn in_volume(&mut self, _: [f32; 3]) -> bool {
        true
    }
}

/// Answers the queries like the oracle world on frames the capture does not cover: no hits.
fn no_calls() -> Calls {
    Calls::default()
}

struct Errors {
    rows: usize,
    eye_max: (f32, u32),
    at_max: (f32, u32),
    eye_sum: f64,
    /// Rows where (yaw_u16 & 0xFF00) differs from the oracle camera's (wave 2's stick model).
    yaw_steps_off: usize,
    bad: Vec<(u32, f32)>,
    /// The replayed camera's yaw after each row.
    yaws: HashMap<u32, f32>,
    /// Rows with a raw dump, and those where the stick override, blend or heading side differ.
    stick_rows: usize,
    stick_off: usize,
    /// Rows with a raw dump, and those where the replayed volume flag differs from the dumped one.
    vol_rows: usize,
    vol_off: usize,
    /// Rows with a raw dump where the replayed yaw differs from `cam+0x37C` (the stick reads it).
    yaw_off: usize,
    calls: CallLog,
}

const START: u32 = 53; // last idle read before every scenario's input begins

fn replay(t: &Trace, inputs: &Inputs) -> Errors {
    let r0 = t.rows[&START];
    let mut cam = CameraState {
        eye: r0.eye,
        at: r0.at,
        floor: r0.raw.map_or(r0.floor_y, |r| r.floor),
        base: r0.pos[1],
        prev_pos: r0.pos,
        prev_eye: r0.eye,
        sweep_size: [30.0, 60.0],
        ..CameraState::default()
    };
    let mut e = Errors {
        rows: 0,
        eye_max: (0.0, 0),
        at_max: (0.0, 0),
        eye_sum: 0.0,
        yaw_steps_off: 0,
        bad: vec![],
        yaws: HashMap::new(),
        calls: CallLog::default(),
        stick_rows: 0,
        stick_off: 0,
        vol_rows: 0,
        vol_off: 0,
        yaw_off: 0,
    };
    for n in START + 1..=t.last {
        let r = t.rows[&n];
        let crouched = r.state == 27;
        let mut p = PlayerView {
            head: if crouched { 40.0 } else { 90.0 },
            strafing: r.pad.0 & 3 != 0,
            ..PlayerView::new(r.pos, r.floor_y, r.facing)
        };
        match inputs {
            Inputs::Oracle | Inputs::OracleVolumes(_) => {
                if let Some(raw) = r.raw {
                    (p.heading, p.head) = (raw.heading, raw.head);
                    (p.weapon, p.twist) = (raw.weapon, raw.twist);
                    (p.stunned, p.in_hand) = (raw.stunned, raw.in_hand);
                }
                // Row n is the state after frame n - 1: that frame's queries, and the volume flag
                // it computed at its start (still set at its end).
                let recorded = t.calls.get(&(n - 1));
                let mut world = OracleWorld {
                    floor: r.raw.map_or(r.floor_y, |r| r.floor),
                    volume: r.raw.is_some_and(|raw| raw.flags & 0x40000 != 0),
                    vols: match inputs {
                        Inputs::OracleVolumes(v) => Some(v),
                        _ => None,
                    },
                    calls: recorded.cloned().unwrap_or_else(no_calls),
                    log: &mut e.calls,
                };
                cam.hint_cut |= world.calls.cut;
                let extra = world.log.extra;
                cam.step_pad(&p, r.pad.0, [r.pad.1, r.pad.2], &mut world);
                if recorded.is_none() {
                    world.log.extra = extra; // nothing recorded: queries answered "no hit"
                }
                world.log.unanswered += world.calls.rays.len() + world.calls.sweeps.len();
            }
            Inputs::Computed(tris) => {
                cam.step_pad(&p, r.pad.0, [r.pad.1, r.pad.2], &mut RomWorld(tris));
            }
        }
        if std::env::var("CAMERA_DUMP").is_ok_and(|v| {
            let v: Vec<&str> = v.split(':').collect();
            v.len() == 3
                && v[1].parse().is_ok_and(|a: u32| a <= n)
                && v[2].parse().is_ok_and(|b: u32| n <= b)
        }) {
            eprintln!(
                "{n} eye {:?} want {:?} at {:?} want {:?} yaw {} want {:?} aim {} {} w {} dist {} {} off {} {} vel {} {} {} {} strafe {:?} {:?}",
                cam.eye,
                r.eye,
                cam.view_at(),
                r.at,
                cam.yaw,
                r.raw.map(|r| r.yaw),
                cam.aim.on,
                cam.aim.phase,
                cam.weapon,
                cam.dist_target,
                cam.height_target,
                cam.look_offset,
                cam.look_offset_target,
                cam.yaw_vel,
                cam.dist_vel,
                cam.height_vel,
                cam.at_vel,
                cam.strafe_spring,
                cam.strafe_blend
            );
        }
        let dist =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f32::max);
        let (de, da) = (dist(cam.eye, r.eye), dist(cam.view_at(), r.at));
        e.rows += 1;
        e.eye_sum += f64::from(de);
        if de > e.eye_max.0 {
            e.eye_max = (de, n);
        }
        if da > e.at_max.0 {
            e.at_max = (da, n);
        }
        if de.max(da) > 0.05 {
            e.bad.push((n, de.max(da)));
        }
        if cam.yaw_u16() & 0xFF00 != oracle_yaw(&r) & 0xFF00 {
            e.yaw_steps_off += 1;
        }
        e.yaws.insert(n, cam.yaw);
        if let Some(raw) = r.raw {
            e.vol_rows += 1;
            e.vol_off += (cam.volume != (raw.flags & 0x40000 != 0)) as usize;
            e.yaw_off += (cam.yaw.to_bits() != raw.yaw.to_bits()) as usize;
            e.stick_rows += 1;
            let mine = [cam.stick_override, cam.override_blend, cam.heading_rel];
            e.stick_off += (mine.map(f32::to_bits) != raw.stick.map(f32::to_bits)) as usize;
        }
    }
    e
}

/// The eye-to-look-at direction of the trace's camera as a u16 angle, truncated.
fn oracle_yaw(r: &Row) -> u16 {
    let (dx, dz) = (f64::from(r.at[0] - r.eye[0]), f64::from(r.at[2] - r.eye[2]));
    ((-dz).atan2(dx).to_degrees().rem_euclid(360.0) * 65536.0 / 360.0) as u32 as u16
}

/// Contiguous runs of rows off by more than 0.05: (first, last, max error).
fn runs(bad: &[(u32, f32)]) -> Vec<(u32, u32, f32)> {
    let mut out: Vec<(u32, u32, f32)> = vec![];
    for &(n, err) in bad {
        match out.last_mut() {
            Some(run) if run.1 + 1 == n => (run.1, run.2) = (n, run.2.max(err)),
            _ => out.push((n, n, err)),
        }
    }
    out
}

/// (trace, raw dump, its camera needs no walls or aim: the ROM-fed replay, which has no wall queries,
/// matches it too).
const TRACES: [(&str, &str, bool); 12] = [
    ("idle_a", "-", true),
    ("walk_a", "-", true),
    ("run_a", "-", true),
    ("turn_a", "-", true),
    ("jump_a", "-", true),
    ("camera_circle", "camera_circle", true),
    ("camera_swing", "camera_swing", true),
    ("camera_jump", "camera_jump", true),
    // ledge_a's run again, with a raw dump: wall push, the sphere sweep, then running off a ledge
    // (the blocked-view cut at read 411).
    ("camera_ledge", "camera_ledge", false),
    // The camera backing into a wall: the wall ray and the sweep.
    ("camera_wall", "camera_wall", false),
    // C strafes, Z, and R's aim camera twice (standing at read 353, running at 533), 30 reads each.
    ("camera_buttons", "camera_buttons", false),
    // R tapped for 6 reads; R held 160 reads, the stick aiming right, up, down, up-left; R held 90
    // reads after a run (stick forward, then right).
    ("camera_aim", "camera_aim", true),
];

fn report(label: &str, name: &str, e: &Errors) {
    eprintln!(
        "{label} {name:15} rows {:4}  eye max {:9.4} @{:3}  mean {:8.5}  look-at max {:8.4} @{:3}  yaw steps off {:3}  runs > 0.05: {:?}",
        e.rows,
        e.eye_max.0,
        e.eye_max.1,
        e.eye_sum / e.rows as f64,
        e.at_max.0,
        e.at_max.1,
        e.yaw_steps_off,
        runs(&e.bad)
    );
    let c = &e.calls;
    if c.answered + c.extra + c.unanswered > 0 {
        eprintln!(
            "{label} {name:15} queries answered from the capture {}, max input difference {:.1e}, extra {}, unanswered {}",
            c.answered, c.input_err, c.extra, c.unanswered
        );
    }
}

#[test]
fn replays_oracle_traces() {
    let Some(dir) = oracle_dir() else { return };
    for (name, raw, _) in TRACES {
        let e = replay(&load(&dir, name, raw), &Inputs::Oracle);
        report("oracle-fed", name, &e);
        // Bit for bit: the game's own float code, in the game's order, its level queries answered
        // with the game's own results and asked with the game's arguments. Those match to 1.5e-8:
        // 33 rays of camera_ledge, camera_wall and camera_buttons point 1-4 ulps apart because the
        // replay starts the yaw spring at rest at read 53, while the game's velocity (`cam+0x7C8`,
        // past the raw dump's 0x700 bytes) keeps a residue of the spawn swing (V: seeding it with
        // 1e-5 leaves 21 rays, at most 7.5e-9 apart). The eye and look-at never show it.
        assert_eq!((e.eye_max.0, e.at_max.0), (0.0, 0.0), "{name}");
        assert!(e.calls.input_err < 1e-7 && e.calls.extra == 0, "{name}");
        if e.stick_rows > 0 {
            eprintln!(
                "oracle-fed {name:15} stick override, blend, side == cam+0x384/0x5CC/0x390 on {} of {} rows",
                e.stick_rows - e.stick_off,
                e.stick_rows
            );
        }
        assert_eq!(e.stick_off, 0, "{name}");
    }
}

/// Armed traces (trace, raw dump): `camera4_*` run `scripts/camera4_armed.txt` (draw, run and turn
/// armed, R held with the stick, an R tap, R while running; `tools/camera4_run.sh`), `combat_ctlw_k*`
/// hold R for 80 reads at the spawn with each weapon drawn (holstering at 190, firing at 215).
const ARMED: [(&str, &str); 18] = [
    ("camera4_w09", "camera4_w09"),
    ("camera4_w39", "camera4_w39"),
    ("camera4_w15", "camera4_w15"),
    ("camera4_w21", "camera4_w21"),
    ("camera4_w22", "camera4_w22"),
    ("camera4_w23", "camera4_w23"),
    ("camera4_w14", "camera4_w14"),
    ("camera4_w16", "camera4_w16"),
    ("camera4_w24", "camera4_w24"),
    ("combat_ctlw_k0", "trajectories/combat_ctlw_k0.raw"),
    ("combat_ctlw_k5", "trajectories/combat_ctlw_k5.raw"),
    ("combat_ctlw_k8", "trajectories/combat_ctlw_k8.raw"),
    ("combat_ctlw_k1", "trajectories/combat_ctlw_k1.raw"),
    ("combat_ctlw_k3", "trajectories/combat_ctlw_k3.raw"),
    ("combat_ctlw_k6", "trajectories/combat_ctlw_k6.raw"),
    ("combat_ctlw_k4", "trajectories/combat_ctlw_k4.raw"),
    ("combat_ctlw_k7", "trajectories/combat_ctlw_k7.raw"),
    ("combat_ctlw_k10", "trajectories/combat_ctlw_k10.raw"),
];

/// Oracle-fed like [`replays_oracle_traces`], with the weapon, twist, stun and hand from the raw dumps:
/// eye, look-at, yaw (`cam+0x37C`, which the stick reads; through the scope too) and the stick override
/// fields bit for bit,
/// except after the 3-frame C side steps of weapons 0x23 and 0x24 (combat_ctlw_k6/k10 from read 335:
/// the yaw 6e-5 off two frames after C-right, the eye 1 ulp, 0.08 units after C-down; the distance
/// spring's velocity `cam+0x258` stays exact, so the yaw spring's velocity differs; cause open).
#[test]
fn replays_armed_traces() {
    let Some(dir) = oracle_dir() else { return };
    let mut rows = 0;
    for (name, raw) in ARMED {
        if !dir.join(format!("trajectories/{name}.csv")).exists() {
            eprintln!("{name}: no trace: skipping");
            continue;
        }
        let e = replay(&load(&dir, name, raw), &Inputs::Oracle);
        report("armed     ", name, &e);
        eprintln!(
            "armed      {name:15} yaw == cam+0x37C on {} of {} rows, override, blend, side on {} of {}",
            e.vol_rows - e.yaw_off,
            e.vol_rows,
            e.stick_rows - e.stick_off,
            e.stick_rows
        );
        let side_steps = matches!(name, "combat_ctlw_k6" | "combat_ctlw_k10");
        let limit = if side_steps { 0.1 } else { 0.0 };
        assert!(e.eye_max.0 <= limit && e.at_max.0 <= limit, "{name}");
        assert!(side_steps || (e.yaw_off, e.stick_off) == (0, 0), "{name}");
        assert!(e.calls.extra == 0, "{name}");
        rows += e.rows;
    }
    eprintln!("armed traces: {rows} rows");
}

#[test]
fn spawn_matches_the_first_match_frames() {
    let Some(dir) = oracle_dir() else { return };
    let t = load(&dir, "idle_a", "-");
    let (r23, r24) = (t.rows[&23], t.rows[&24]);
    let p = |r: &Row| PlayerView::new(r.pos, r.floor_y, r.facing);
    let mut cam = CameraState::spawn(r23.eye, &p(&r23));
    assert_eq!((cam.eye, cam.at), (r23.eye, r23.at));
    cam.step(&p(&r24), 0, |_, _| Some(r24.floor_y));
    assert_eq!(cam.at, r24.at);
    let err = (0..3)
        .map(|i| (cam.eye[i] - r24.eye[i]).abs())
        .fold(0.0, f32::max);
    assert!(err < 1e-3, "{err}");
}

/// The camera-relative stick, as the game reads it: target = stick angle + [`CameraState::stick_yaw`] of
/// the previous tick, full u16 precision; the move angle then turns by whole 256-steps,
/// `((target - angle) as i16 >> 8) << 8`, keeping its low byte. Checked on every steering frame (state 2
/// on both rows, stick pushed, no button, turn under the speed cap): with `cam+0x37C` from the raw
/// dumps, and with this crate's replayed camera on the bit-exact traces without one (after checking
/// that its yaw equals `cam+0x37C` bit for bit where both exist).
#[test]
fn stick_rule() {
    let Some(dir) = oracle_dir() else { return };
    // The movement traces with raw dumps, beside the camera ones.
    let extra = [
        "move_jump",
        "move_release",
        "move_slope",
        "move_speeds",
        "move_turnrun",
    ];
    let traces = TRACES
        .iter()
        .copied()
        .chain(extra.iter().map(|&n| (n, n, false)));
    let (mut rows, mut hits, mut yaw_rows, mut capped) = (0, 0, 0, 0);
    for (name, raw, ported) in traces {
        if !dir.join(format!("trajectories/{name}.csv")).exists() {
            continue;
        }
        let t = load(&dir, name, raw);
        let ported = ported || TRACES.iter().any(|t| t.0 == name);
        let replayed = ported.then(|| replay(&t, &Inputs::Oracle).yaws);
        if let Some(yaws) = &replayed {
            for (n, r) in &t.rows {
                if let (Some(raw), Some(yaw)) = (r.raw, yaws.get(n)) {
                    assert_eq!(
                        raw.yaw.to_bits(),
                        yaw.to_bits(),
                        "{name} read {n}: yaw != cam+0x37C"
                    );
                    yaw_rows += 1;
                }
            }
        }
        for n in START + 2..=t.last {
            let (r, prev) = (t.rows[&n], t.rows[&(n - 1)]);
            let (buttons, sx, sy) = r.pad;
            if (sx, sy) == (0, 0) || buttons != 0 || r.state != 2 || prev.state != 2 {
                continue;
            }
            let yaw = match (prev.raw, &replayed) {
                (Some(raw), _) => raw.yaw,
                (None, Some(yaws)) => yaws[&(n - 1)],
                (None, None) => continue,
            };
            let cam = CameraState {
                yaw,
                ..CameraState::default()
            };
            // The traced sticks are axis-aligned or diagonal, where atan2 is exact.
            let stick =
                (f64::from(sy).atan2(f64::from(sx)).to_degrees() * 65536.0 / 360.0).round() as i32;
            let target = (stick as u16).wrapping_add(cam.stick_yaw());
            let d = target.wrapping_sub(prev.move_angle) as i16;
            if d.unsigned_abs() >= 1792 {
                capped += 1;
                continue; // capped at full speed: the turn rule's business
            }
            rows += 1;
            hits += (r.move_angle == prev.move_angle.wrapping_add(((d >> 8) << 8) as u16)) as usize;
        }
    }
    eprintln!(
        "stick rule: {hits} of {rows} steering rows exact ({capped} more turn at the cap); replayed yaw == cam+0x37C on {yaw_rows} rows"
    );
    assert!(rows > 2000 && hits == rows, "{hits}/{rows}");
}

/// The volume rule (`cam+0x84` bit 0x40000) from the ROM's camera volumes (`[0x0C, 0x2D, 5]`):
/// the replay answers the volume test with them instead of the dumped flag, and still matches the
/// game bit for bit, its flag equal to the dumped one on every row.
#[test]
fn volumes_from_the_rom() {
    let (Some(dir), Some(path)) = (oracle_dir(), std::env::var_os("CONKER_ROM")) else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let vols = CameraVolume::parse_table(&rom.asset(&[0x0C, 0x2D, 5]).unwrap());
    assert_eq!(vols.len(), 49); // the game's list on scene 0x2D (D_800D3274, RAM dump)
    let (mut rows, mut off) = (0, 0);
    for (name, raw, _) in TRACES {
        let e = replay(&load(&dir, name, raw), &Inputs::OracleVolumes(&vols));
        assert_eq!((e.eye_max.0, e.at_max.0), (0.0, 0.0), "{name}");
        (rows, off) = (rows + e.vol_rows, off + e.vol_off);
    }
    eprintln!(
        "camera volumes from the ROM: flag == cam+0x84 bit 0x40000 on {} of {rows} rows, eye and look-at bit-exact",
        rows - off
    );
    assert!(rows > 3000 && off == 0, "{off}");
}

/// The hint cut from the ROM's hints ([`CameraLevel::hint_cut`], `[0x0C, 0x2D, 5]`) and the traced
/// positions: frame n tests the move from row n - 1 to row n. It fires on exactly the frames the game
/// cut on (the captures' `cut` writes: camera_ledge 410) and on no frame of the other traces, whose
/// replays are bit-exact without a cut.
#[test]
fn hints_from_the_rom() {
    let (Some(dir), Some(path)) = (oracle_dir(), std::env::var_os("CONKER_ROM")) else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let level = CameraLevel::new(0x2D, &rom.asset(&[0x0C, 0x2D, 5]).unwrap());
    let [hint] = level.hints[..] else {
        panic!("scene 0x2D has one hint, record 82")
    };
    assert_eq!((hint.kind, hint.command, hint.zone.shape), (4, HINT_CUT, 3));
    // The level's own sines (`func_150AD78C`) give the bits the game filled in (RAM dump).
    assert_eq!(
        hint.zone.pitch.map(f32::to_bits),
        [0x3F7F_D814, 0xBD0E_F2C8]
    );
    assert_eq!(hint.zone.yaw.map(f32::to_bits), [0x3EE6_D96A, 0x3F64_8081]);
    let (mut frames, mut fired) = (0, vec![]);
    for (name, raw, _) in TRACES {
        let t = load(&dir, name, raw);
        let mut got: Vec<u32> = (START..=t.last)
            .filter(|n| {
                let (Some(a), Some(b)) = (t.rows.get(&(n - 1)), t.rows.get(n)) else {
                    return false;
                };
                frames += 1;
                level.hint_cut(a.pos, b.pos, b.floor_y)
            })
            .collect();
        let mut want: Vec<u32> = t.calls.iter().filter(|c| c.1.cut).map(|c| *c.0).collect();
        got.sort();
        want.sort();
        assert_eq!(got, want, "{name}");
        fired.extend(got.into_iter().map(|n| (name, n)));
    }
    eprintln!("hints from the ROM: {frames} frames, cuts {fired:?}");
    assert_eq!(fired, [("camera_ledge", 410)]);
}

#[test]
fn replays_with_rom_collision() {
    let (Some(dir), Some(path)) = (oracle_dir(), std::env::var_os("CONKER_ROM")) else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let placements = scene_placements(&rom, 0x2D).unwrap();
    let tris = scene_collision(&rom, 0x2D, &placements).unwrap().triangles;
    for (name, raw, ported) in TRACES {
        let e = replay(&load(&dir, name, raw), &Inputs::Computed(&tris));
        report("computed  ", name, &e);
        if ported {
            assert!(e.eye_max.0 < 0.2 && e.at_max.0 < 0.01, "{name}");
        }
    }
}
