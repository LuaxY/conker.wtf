//! Replays the oracle's scripted inputs (`lab/oracle/scripts/<name>.txt`) through [`movement::step`] on scene
//! 0x2D's collision and compares every frame with the original game's trace (`trajectories/<name>[_a].csv`,
//! player 0). Row N of a trace holds the input fed at read N and the state after frame N-1, so frame N is
//! simulated from row N's input and camera and checked against row N+1.
//!
//! Two modes, both asserted on every movement trace ([`TRACES`]):
//! - trace camera (`trace_camera`): the stick's camera angle is the trace camera's (eye → look-at of row N);
//! - closed loop (`camera_loop`), as server and client run it ([`Follow::tick`]): conker-camera, stepped after
//!   each movement step on the same collision (floor, wall ray, sweep, volumes), supplies the next frame's
//!   angle. Its eye and look-at are compared with the trace's too (reported, not asserted).
//!
//! Frames this crate does not simulate are listed in [`EXCLUDED`] (not counted; the traced state is copied
//! back after them).
//!
//! Needs `CONKER_ROM` (the collision) and the lab checkout next to this tree; skips otherwise.
//! `cargo test -p conker-sim --test oracle -- --nocapture` prints the per-scenario numbers;
//! `-- --include-ignored` adds `report_all` (both modes, every trace, not asserted).
//! Debugging: `ORACLE_DUMP=<name>:<first>:<last>` prints simulated vs traced rows, `ORACLE_RESYNC=1` lists
//! every bad row, copying the traced state back after each.

use conker_camera::CameraState;
use conker_camera::math::yaw_of;
use conker_camera::volume::CameraLevel;
use conker_sim::collision::{Body, CollisionWorld, Layer, Sweep};
use conker_sim::follow::Follow;
use conker_sim::movement::{self, Input, Other, PlayerState, Side};
use conker_sim::root::RootTrack;
use conker_sim::trig::AcosTable;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

const ORACLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../lab/oracle");
/// The match goes live at read 23; from read 30 the player has stood still for a while.
const START: u32 = 30;
const TOLERANCE: f32 = 0.1;

/// `scene`'s collision as the pack holds it: terrain, secondary, the placements a Deathmatch collides with.
fn scene_world(rom: &extract_core::Rom, scene: usize) -> CollisionWorld {
    let placements = extract_scene::scene_placements(rom, scene).unwrap();
    let collision = extract_scene::scene_collision(rom, scene, &placements).unwrap();
    let solid = extract_scene::deathmatch_solid(scene);
    let triangles = (collision.triangles.iter())
        .filter(|t| !matches!(t.layer, extract_scene::Layer::Placement(i) if !solid.contains(&i)));
    CollisionWorld::layered(triangles.map(|t| {
        let layer = match t.layer {
            extract_scene::Layer::Terrain => Layer::Terrain,
            extract_scene::Layer::Secondary => Layer::Secondary,
            extract_scene::Layer::Placement(i) => Layer::Placement(i as u16),
        };
        (t.points, t.surface.unwrap_or(0), layer)
    }))
}

fn world() -> Option<&'static CollisionWorld> {
    static WORLD: OnceLock<Option<CollisionWorld>> = OnceLock::new();
    WORLD
        .get_or_init(|| {
            let rom = std::fs::read(std::env::var_os("CONKER_ROM")?).expect("read CONKER_ROM");
            let rom = extract_core::Rom::new(rom).expect("CONKER_ROM is the US ROM");
            let mut world = scene_world(&rom, 0x2D);
            world.acos = AcosTable::from_game_data(&rom.game_data().unwrap());
            world.camera = Some(CameraLevel::new(
                0x2D,
                &rom.asset(&[0x0C, 0x2D, 5]).unwrap(),
            ));
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
            Some(world)
        })
        .as_ref()
}

#[derive(Clone, Debug)]
struct Row {
    read: u32,
    /// obj+0x4C, the animation's rate.
    rate: f32,
    input: (u16, i8, i8),
    state: PlayerState,
    /// The camera angle the stick reads ([`conker_camera::CameraState::stick_yaw`]) of the row's camera.
    yaw: u16,
    eye: [f32; 3],
    at: [f32; 3],
}

fn trace(name: &str) -> Option<Vec<Row>> {
    trace_of(name, "0")
}

/// Player `player`'s rows of a trace (`trace` takes player 1, the scripted human).
fn trace_of(name: &str, player: &str) -> Option<Vec<Row>> {
    let dir = PathBuf::from(ORACLE).join("trajectories");
    let text = [format!("{name}_a.csv"), format!("{name}.csv")]
        .iter()
        .find_map(|f| std::fs::read_to_string(dir.join(f)).ok())?;
    let mut lines = text.lines();
    let header: HashMap<&str, usize> = lines
        .next()?
        .split(',')
        .enumerate()
        .map(|(i, h)| (h, i))
        .collect();
    let rows = lines
        .map(|l| l.split(',').collect::<Vec<_>>())
        .filter(|c| {
            c[header["player"]] == player && c[header["mp"]] == "1" && !c[header["x"]].is_empty()
        })
        .map(|c| {
            let f = |k: &str| c[header[k]].parse::<f32>().unwrap();
            let u = |k: &str| c[header[k]].parse::<u32>().unwrap();
            let hex =
                |k: &str| u16::from_str_radix(c[header[k]].trim_start_matches("0x"), 16).unwrap();
            let (eye, at) = (
                ["cam_eye_x", "cam_eye_y", "cam_eye_z"].map(f),
                ["cam_at_x", "cam_at_y", "cam_at_z"].map(f),
            );
            let mut state = PlayerState::spawn(["x", "y", "z"].map(f), u("facing") as u16);
            state.y_vel = f("y_vel");
            state.gravity = f("gravity");
            state.xz_speed = f("xz_speed");
            state.target_speed = f("target_speed");
            state.move_angle = u("move_angle") as u16;
            state.state = u("state") as u8;
            state.floor_y = f("floor_y");
            Row {
                read: u("read"),
                rate: f("anim_speed"),
                input: (hex("in_buttons"), f("in_x") as i8, f("in_y") as i8),
                state,
                yaw: CameraState {
                    yaw: yaw_of(at[0] - eye[0], at[2] - eye[2]),
                    ..CameraState::default()
                }
                .stick_yaw(),
                eye,
                at,
            }
        })
        .collect();
    Some(rows)
}

/// The script's input at `read`: later lines win, uncovered reads are neutral.
fn script(name: &str) -> Option<impl Fn(u32) -> (u16, i8, i8)> {
    let dir = PathBuf::from(ORACLE).join("scripts");
    // The weapon-control probes `combat_ctlw_k<k>` share one script.
    let text = std::fs::read_to_string(dir.join(format!("{name}.txt")))
        .or_else(|e| match name.strip_prefix("combat_ctlw_k") {
            Some(_) => std::fs::read_to_string(dir.join("combat_ctlw.txt")),
            None => Err(e),
        })
        .ok()?;
    let lines: Vec<(u32, u32, u16, i8, i8)> = text
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

/// Read ranges whose frames this crate does not simulate, with the reason. Their rows are not counted; at
/// the last row the traced player state is copied back, so the rest of the trace is measured on its own.
const EXCLUDED: &[(&str, u32, u32, &str)] = &[
    (
        "camera_buttons",
        160,
        160,
        "the grenade (conker-rules) pushes its thrower 1.09 as it appears, 14 frames into the throw",
    ),
    (
        "move_wall6",
        730,
        u32::MAX,
        "hit by the bot's shot at 730 (combat)",
    ),
    (
        "combat_buttons",
        219,
        219,
        "the grenade appears 14 frames into the throw and pushes its thrower (conker-rules)",
    ),
    (
        "combat_ctl_cd_cr",
        99,
        99,
        "the grenade's push on its thrower (conker-rules)",
    ),
    (
        "combat_ctlw_k4",
        406,
        406,
        "the grenade's push on its thrower (conker-rules)",
    ),
    (
        "combat_ctlw_k7",
        406,
        406,
        "the grenade's push on its thrower (conker-rules)",
    ),
    (
        "camera4_w23",
        530,
        530,
        "0x23's scope hold lapses for a frame (its aim clip restarting, the rules'): the walk percentage clears",
    ),
];

#[derive(Debug, Default)]
struct Report {
    rows: usize,
    /// Rows inside [`EXCLUDED`] ranges.
    excluded: usize,
    /// Closed loop: the largest distance of the camera's eye or look-at from the trace's.
    eye_err: f32,
    max_err: f32,
    sum_err: f64,
    state_misses: usize,
    /// Rows whose facing differs (the camera follows it), and the first few: (read, state, got, want).
    facing_misses: usize,
    facing_first: Vec<(u32, u8, u16, u16)>,
    /// Closed loop: counted rows where the camera's 256-step differs from the one of the trace camera's
    /// eye → look-at (which misses the game's stick override, `cam+0x384`: camera_aim 243-330).
    yaw_steps_off: usize,
    /// Rows whose twist (s126+0x12) differs from the raw dump's.
    twist_misses: usize,
    /// The simulated heading (obj+0x40) after each frame: (read of the row, heading).
    headings: Vec<(u32, f32)>,
    first_bad: Option<String>,
}

impl Report {
    fn ok(&self) -> bool {
        self.rows > 0
            && self.max_err < TOLERANCE
            && self.state_misses == 0
            && self.twist_misses == 0
    }
}

/// The follow camera of the traced row `r` (at rest: the scenarios start from a standing player).
fn camera(r: &Row) -> Follow {
    let mut f = Follow::spawn(r.eye, &r.state);
    (f.cam.at, f.cam.cut) = (r.at, false);
    f
}

/// The animation ids of the movement's own states (the others are the match rules' actions).
const MOVEMENT: [u16; 12] = [0, 1, 2, 15, 22, 27, 33, 50, 54, 55, 56, 122];

/// Per-frame values a replay takes from the oracle instead of simulating them: what the match rules set
/// (the lock obj+0x89, the drawn weapon s126+0x78, from `CONKER_TRACE_RAW`) and the other player's body,
/// by read (the value at the start of that frame).
#[derive(Default)]
struct Feed {
    /// obj+0x89 and obj+0x83.
    lock: HashMap<u32, (u8, u8)>,
    armed: HashMap<u32, u8>,
    other: HashMap<u32, Vec<Other>>,
    /// The traced animation id (obj+0x84): the state is compared only where it is a movement state's,
    /// not an action's (whose low byte can look like one).
    anim: HashMap<u32, u16>,
    /// The scope's look yaw while it is open (s126+0x16C while s126+0x197 is set), as the camera leaves
    /// it for the next frame ([`PlayerState::scope_yaw`]).
    scope: HashMap<u32, Option<f32>>,
    /// The upper body's twist (s126+0x12), compared with [`PlayerState::twist`].
    twist: HashMap<u32, i16>,
    /// What the camera reads from the rules: the weapon in the hand (s126+0x11A = 2) and reeling
    /// (obj+0x104), as `Follow::step_view` takes them.
    in_hand: HashMap<u32, (bool, bool)>,
    /// Replay player 2 (`_p2` script) instead of player 1.
    second: bool,
}

/// Replays `name`; with `closed_loop` the yaw comes from conker-camera instead of the trace.
fn replay(name: &str, closed_loop: bool) -> Option<Report> {
    replay_fed(name, closed_loop, &raw_feed(name).unwrap_or_default())
}

/// The match rules' part of a trace with a raw dump: the lock and drawn weapon they set and the
/// animation they play (player 1).
fn raw_feed(name: &str) -> Option<Feed> {
    raw_feed_of(name, 0)
}

fn raw_feed_of(name: &str, player: u32) -> Option<Feed> {
    let raw = raw_bytes(
        name,
        player,
        &[
            (0, 0x89),
            (1, 0x78),
            (0, 0x84),
            (0, 0x85),
            (0, 0x83),
            (1, 0x197),
            (1, 0x16C),
            (1, 0x16D),
            (1, 0x16E),
            (1, 0x16F),
            (1, 0x12),
            (1, 0x13),
            (1, 0x11A),
            (0, 0x104),
        ],
    )?;
    Some(Feed {
        lock: raw.iter().map(|(&k, v)| (k, (v[0], v[4]))).collect(),
        armed: raw.iter().map(|(&k, v)| (k, v[1])).collect(),
        anim: raw
            .iter()
            .map(|(&k, v)| (k, u16::from_be_bytes([v[2], v[3]])))
            .collect(),
        scope: raw
            .iter()
            .map(|(&k, v)| {
                (
                    k,
                    (v[5] != 0).then(|| f32::from_be_bytes([v[6], v[7], v[8], v[9]])),
                )
            })
            .collect(),
        twist: raw
            .iter()
            .map(|(&k, v)| (k, i16::from_be_bytes([v[10], v[11]])))
            .collect(),
        in_hand: raw
            .iter()
            .map(|(&k, v)| (k, (v[12] == 2, v[13] != 0)))
            .collect(),
        other: HashMap::new(),
        second: player == 1,
    })
}

/// Player 2 of a two-player combat trace, idle where it stands: pushed by player 1, whose update comes
/// first (so player 2 sees its position at the frame's end, the next row).
fn second_feed(name: &str) -> Option<Feed> {
    let feed = raw_feed_of(name, 1)?;
    let other = trace(name)?
        .iter()
        .map(|r| (r.read - 1, vec![Other::player(Side::Tediz, r.state.pos)]))
        .collect();
    Some(Feed { other, ..feed })
}

fn replay_fed(name: &str, closed_loop: bool, feed: &Feed) -> Option<Report> {
    let p2 = format!("{name}_p2");
    let (world, rows, script) = if feed.second {
        (world()?, trace_of(name, "1")?, script(&p2)?)
    } else {
        (world()?, trace(name)?, script(name)?)
    };
    let start = rows.iter().position(|r| r.read == START)?;
    let mut p = rows[start].state;
    let mut report = Report::default();
    let mut cam = camera(&rows[start]);
    // ORACLE_DUMP=name:first:last prints simulated vs traced rows (debugging).
    let dump = std::env::var("ORACLE_DUMP").ok().and_then(|v| {
        let mut it = v.split(':');
        Some((
            it.next()?.to_string(),
            it.next()?.parse::<u32>().ok()?,
            it.next()?.parse::<u32>().ok()?,
        ))
    });
    let resync_env = std::env::var_os("ORACLE_RESYNC").is_some();
    for pair in rows[start..].windows(2) {
        let (row, next) = (&pair[0], &pair[1]);
        assert_eq!(
            next.read,
            row.read + 1,
            "{name}: gap after read {}",
            row.read
        );
        let (buttons, sx, sy) = script(row.read);
        // The trace's input columns are controller 0's.
        assert!(
            feed.second || (buttons, sx, sy) == row.input,
            "{name}: script and trace disagree at read {}",
            row.read
        );
        let yaw_off = closed_loop && cam.yaw() >> 8 != row.yaw >> 8;
        let camera_yaw = if closed_loop { cam.yaw() } else { row.yaw };
        let input = Input {
            stick_x: sx,
            stick_y: sy,
            buttons,
            camera_yaw,
        };
        // The rules' lock and hold while an action plays, and as it ends (the movement keeps its own:
        // skids, landings).
        let action = |read: u32| feed.anim.get(&read).is_some_and(|a| !MOVEMENT.contains(a));
        if (action(row.read) || action(row.read - 1))
            && let Some(&(lock, hold)) = feed.lock.get(&row.read)
        {
            (p.lock, p.timer) = (lock, hold);
        }
        let mut input = input;
        if let Some(&armed) = feed.armed.get(&row.read) {
            p.armed = armed;
            // As conker-rules' `Rules`: a drawn weapon takes A (it reloads) unless it is a melee weapon or
            // 0x14, which jump.
            if armed != 0 && !matches!(armed, 0x14 | 0x21 | 0x22) {
                input.buttons &= !movement::BUTTON_A;
            }
        }
        if let Some(&yaw) = feed.scope.get(&row.read) {
            p.scope_yaw = yaw;
        }
        let others = feed.other.get(&row.read).map_or(&[][..], Vec::as_slice);
        movement::step_among(&mut p, &input, world, Side::Tediz, others);
        // The clip the player plays this frame: the rules' action (fed, at its traced rate), else the
        // movement state's at conker-anim's rate, as server and client run it.
        let (id, rate, start) = match feed.anim.get(&next.read).filter(|a| !MOVEMENT.contains(a)) {
            Some(&id) => (id, next.rate, 0.0),
            None => {
                let r = conker_anim::request(u16::from(p.state), p.xz_speed, p.target_speed);
                (r.id, r.rate, r.start)
            }
        };
        movement::animate(&mut p, id, rate, start, world);
        // The camera as server and client step it, with what the rules know (the drawn weapon gives R's
        // armed aim or the scope instead of the unarmed aim camera).
        let (in_hand, stunned) = feed.in_hand.get(&next.read).copied().unwrap_or_default();
        let view = conker_camera::PlayerView {
            weapon: p.armed,
            in_hand,
            stunned,
            twist: p.twist,
            ..conker_sim::follow::view(&p)
        };
        cam.step_view(view, &input, world);
        p.aiming = cam.cam.aim.on && cam.cam.aim.phase != 0; // as Follow::tick
        report.headings.push((next.read, p.heading));
        let want = &next.state;
        if let Some(&(_, _, last, _)) = EXCLUDED
            .iter()
            .find(|e| e.0 == name && (e.1..=e.2).contains(&next.read))
        {
            report.excluded += 1;
            if next.read == last {
                resync(&mut p, want);
            }
            continue;
        }
        report.yaw_steps_off += yaw_off as usize;
        let err = (0..3)
            .map(|i| (p.pos[i] - want.pos[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        if closed_loop {
            let dist = |a: [f32; 3], b: [f32; 3]| {
                (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
            };
            let e = dist(cam.cam.eye, next.eye).max(dist(cam.cam.view_at(), next.at));
            report.eye_err = report.eye_err.max(e);
        }
        report.rows += 1;
        report.sum_err += err as f64;
        report.max_err = report.max_err.max(err);
        if dump
            .as_ref()
            .is_some_and(|(n, a, b)| n == name && (*a..=*b).contains(&next.read))
        {
            eprintln!(
                "{} yaw {} want {} eye {:?} want {:?} | sim {:?} st {} sp {} mv {} fc {} floor {:?} | want {:?} st {} sp {} mv {} fc {} | err {err:.4}",
                next.read,
                cam.yaw(),
                next.yaw,
                cam.cam.eye,
                next.eye,
                p.pos,
                p.state,
                p.xz_speed,
                p.move_angle,
                p.facing,
                p.floor.map(|f| (f.tri, f.surface)),
                want.pos,
                want.state,
                want.xz_speed,
                want.move_angle,
                want.facing
            );
        }
        if feed.twist.get(&next.read).is_some_and(|&t| t != p.twist) {
            report.twist_misses += 1;
        }
        if p.facing != want.facing {
            report.facing_misses += 1;
            if report.facing_first.len() < 6 {
                report
                    .facing_first
                    .push((next.read, want.state, p.facing, want.facing));
            }
        }
        let state_bad = p.state != want.state
            && (feed.anim.is_empty()
                || feed
                    .anim
                    .get(&next.read)
                    .is_some_and(|a| MOVEMENT.contains(a)));
        report.state_misses += state_bad as usize;
        if err >= TOLERANCE || state_bad {
            let line = format!(
                "row {}: err {err:.4} pos {:?} want {:?} state {} want {} speed {} want {} move {} want {} y_vel {} want {}",
                next.read,
                p.pos,
                want.pos,
                p.state,
                want.state,
                p.xz_speed,
                want.xz_speed,
                p.move_angle,
                want.move_angle,
                p.y_vel,
                want.y_vel
            );
            // ORACLE_RESYNC=1 (debugging): list every bad row, copying the traced state back after each.
            if resync_env {
                eprintln!("  {name} {line}");
                resync(&mut p, want);
            }
            report.first_bad.get_or_insert(line);
        }
    }
    let mean = report.sum_err / report.rows.max(1) as f64;
    eprintln!(
        "{name:16}{} rows {:4} (excluded {:3})  max {:9.4}  mean {:8.4}  eye {:8.4}  state misses {:3}  yaw steps off {:3}  twist misses {}  facing misses {:3} {:?}  {}",
        if closed_loop { " (camera)" } else { "" },
        report.rows,
        report.excluded,
        report.max_err,
        mean,
        report.eye_err,
        report.state_misses,
        report.yaw_steps_off,
        report.twist_misses,
        report.facing_misses,
        report.facing_first,
        report.first_bad.as_deref().unwrap_or("ok")
    );
    Some(report)
}

/// Copies the traced state back into the simulated player (the sim's own bookkeeping stays).
fn resync(p: &mut PlayerState, want: &PlayerState) {
    let floor = p.floor;
    *p = PlayerState {
        prev_buttons: p.prev_buttons,
        timer: p.timer,
        aiming: p.aiming,
        lock: p.lock,
        jumped: p.jumped,
        crouch_ok: p.crouch_ok,
        hard_landing: p.hard_landing,
        root: p.root,
        twist: p.twist,
        scope_yaw: p.scope_yaw,
        // An action's animation id is the rules' (its low byte is no movement state).
        state: if MOVEMENT.contains(&u16::from(want.state)) {
            want.state
        } else {
            p.state
        },
        ..*want
    };
    p.airborne = want.pos[1] > want.floor_y;
    p.floor = floor;
}

fn check(name: &str, closed_loop: bool) {
    if world().is_none() {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    }
    let Some(report) = replay(name, closed_loop) else {
        eprintln!("{name}: no trace or script under {ORACLE}: skipping");
        return;
    };
    assert!(report.ok(), "{name}: {report:?}");
}

/// The movement traces (`lab/oracle/scripts`), all asserted in both modes: wave 1's six, the movement
/// agent's `move_*`, the camera agent's `camera_*`, the animation agent's `anim_*` and the walls agent's
/// `wall_jump` (walls, wall pushes, jumps and Z against walls).
const TRACES: [&str; 28] = [
    "idle",
    "walk",
    "run",
    "turn",
    "jump",
    "ledge",
    "move_crouch",
    "move_jump",
    "move_release",
    "move_slope",
    "move_speeds",
    "move_turnrun",
    "move_wall1",
    "move_wall2",
    "move_wall3",
    "move_wall5",
    "move_wall6",
    "camera_aim",
    "camera_buttons",
    "camera_circle",
    "camera_jump",
    "camera_swing",
    "camera_wall",
    "anim_creep",
    "anim_crouch",
    "anim_idle_long",
    "anim_jump_stand",
    "wall_jump",
];

#[test]
fn trace_camera() {
    for name in TRACES {
        check(name, false);
    }
}

/// The camera in the loop, as server and client run it: the yaw each frame is conker-camera's, stepped
/// after the player as in the game.
#[test]
fn camera_loop() {
    for name in TRACES {
        check(name, true);
    }
}

/// Every traced single-player scenario in both modes, reported but not asserted (`-- --ignored
/// --nocapture`). Other agents' scenarios (combat, spawns, surfaces, two players) are skipped by name.
#[test]
#[ignore]
fn report_all() {
    if world().is_none() {
        return;
    }
    const MINE: [&str; 10] = [
        "idle", "walk", "run", "turn", "jump", "ledge", "move_", "camera_", "anim_", "wall_",
    ];
    let mut names: Vec<String> = std::fs::read_dir(PathBuf::from(ORACLE).join("scripts"))
        .into_iter()
        .flatten()
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .into_string()
                .ok()?
                .strip_suffix(".txt")
                .map(String::from)
        })
        .filter(|n| MINE.iter().any(|m| n.starts_with(m)))
        .collect();
    // ORACLE_EXTRA=a,b adds other agents' traces (player 1's rows) to the report.
    if let Ok(extra) = std::env::var("ORACLE_EXTRA") {
        names.extend(extra.split(',').map(String::from));
    }
    names.sort();
    for name in names {
        replay(&name, false);
        replay(&name, true);
    }
}

/// The wall layers' quadtrees equal the game's (`trajectories/wall_ram_tree{0,1}.txt`, dumped from the
/// oracle's RAM by `tools/wall_ram_tree.py`), and so do the triangle bounds the routine filters on.
#[test]
fn wall_trees_match_the_game() {
    let Some(world) = world() else {
        return;
    };
    let dir = PathBuf::from(ORACLE).join("trajectories");
    for k in 0..2 {
        let Ok(want) = std::fs::read_to_string(dir.join(format!("wall_ram_tree{k}.txt"))) else {
            eprintln!("no wall_ram_tree{k}.txt: skipping");
            return;
        };
        let got = world.wall_tree(k);
        let bad = got.lines().zip(want.lines()).position(|(a, b)| a != b);
        assert!(
            bad.is_none() && got.lines().count() == want.lines().count(),
            "layer {k}: first difference at node {bad:?} of {}/{}",
            got.lines().count(),
            want.lines().count()
        );
        eprintln!(
            "wall layer {k}: {} nodes equal the game's",
            want.lines().count()
        );
    }
    // Each static triangle's integer bounds (min x, min z, max x, max z, min y, max y): terrain then
    // secondary, as world.tris starts.
    let Ok(bounds) = std::fs::read_to_string(dir.join("wall_ram_tree_bounds.txt")) else {
        return;
    };
    for (i, line) in bounds.lines().enumerate() {
        let want: Vec<i32> = line
            .split(' ')
            .skip(2)
            .map(|v| v.parse().unwrap())
            .collect();
        let v = world.tris[i].v;
        let (lo, hi) = (
            [0, 1, 2].map(|k| v.iter().map(|p| p[k]).min().unwrap()),
            [0, 1, 2].map(|k| v.iter().map(|p| p[k]).max().unwrap()),
        );
        assert_eq!(
            want,
            [lo[0], lo[2], hi[0], hi[2], lo[1], hi[1]],
            "triangle {i}"
        );
    }
    eprintln!(
        "{} static triangles' bounds equal the game's",
        bounds.lines().count()
    );
}

/// One call of the game's wall routine for player 1, from `tools/wall_lldb.sh`.
#[derive(Debug, Default)]
struct WallCall {
    read: u32,
    old: [f32; 3],
    new: [f32; 3],
    y_vel: f32,
    dims: Option<(f32, f32)>,
    pushes: Vec<String>,
    ret: Option<([f32; 3], u32, f32)>,
}

fn wall_calls(name: &str) -> Option<Vec<WallCall>> {
    let path = PathBuf::from(ORACLE)
        .join("trajectories")
        .join(format!("wall_{name}_lldb.csv"));
    let text = std::fs::read_to_string(path).ok()?;
    let mut calls: Vec<WallCall> = Vec::new();
    for line in text.lines() {
        let c: Vec<&str> = line.split(',').collect();
        let f = |i: usize| c[i].parse::<f32>().unwrap();
        match (c[1], calls.last_mut()) {
            ("call", _) => calls.push(WallCall {
                read: c[0].parse().unwrap(),
                old: [f(3), f(4), f(5)],
                new: [f(6), f(7), f(8)],
                y_vel: f(9),
                ..Default::default()
            }),
            ("dims", Some(call)) if call.dims.is_none() => call.dims = Some((f(6), f(7))),
            ("push", Some(call)) => call.pushes.push(line.to_string()),
            ("ret", Some(call)) => {
                call.ret = Some(([f(3), f(4), f(5)], c[2].parse().unwrap(), f(6)))
            }
            _ => {}
        }
    }
    Some(calls)
}

/// The ported routine on each of the game's own calls (old, new, y velocity), compared bit for bit.
#[test]
fn wall_routine_matches_the_game_calls() {
    let Some(world) = world() else {
        return;
    };
    let names = [
        "ledge",
        "move_wall1",
        "move_wall2",
        "move_wall3",
        "move_wall5",
        "move_wall6",
        "move_turnrun",
        "camera_wall",
        "wall_jump",
    ];
    let (mut calls, mut pushed, mut bad) = (0, 0, Vec::new());
    for name in names {
        let Some(list) = wall_calls(name) else {
            eprintln!("{name}: no wall capture: skipping");
            continue;
        };
        for call in &list {
            let Some((pos, hits, y_vel)) = call.ret else {
                continue;
            };
            let body = Body::player(call.old[1], call.new[1]);
            if let Some((h, c)) = call.dims {
                assert_eq!(
                    (body.height, body.center_y),
                    (h, c),
                    "{name} read {}",
                    call.read
                );
            }
            let got = world.sweep(call.old, call.new, &body, &Sweep::default(), call.y_vel);
            calls += 1;
            pushed += (hits > 0) as usize;
            let same = got.pos.map(f32::to_bits) == pos.map(f32::to_bits)
                && got.hits == hits
                && got.y_vel.to_bits() == y_vel.to_bits();
            if !same {
                bad.push(format!(
                    "{name} read {}: got {:?} hits {} y_vel {} want {:?} hits {hits} y_vel {y_vel} | {}",
                    call.read,
                    got.pos,
                    got.hits,
                    got.y_vel,
                    pos,
                    call.pushes.join(" | ")
                ));
            }
        }
    }
    eprintln!(
        "wall routine: {calls} calls ({pushed} with pushes), {} differ",
        bad.len()
    );
    for b in bad.iter().take(20) {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty());
}

/// obj+0x40 of player 1 per read, from a `CONKER_TRACE_RAW` file (`raw/<name>.bin`): records of read and
/// player (u32 LE), then the object (0x32C), its +0x31C struct (0x1B0) and camera (0x700), N64 byte order.
fn raw_headings(name: &str) -> Option<HashMap<u32, (f32, u8)>> {
    let data = std::fs::read(
        PathBuf::from(ORACLE)
            .join("raw")
            .join(format!("{name}.bin")),
    )
    .ok()?;
    const REC: usize = 8 + 0x32C + 0x1B0 + 0x700;
    Some(
        data.as_chunks::<REC>()
            .0
            .iter()
            .filter(|r| r[4..8] == [0; 4])
            .map(|r| {
                let read = u32::from_le_bytes(r[..4].try_into().unwrap());
                let h = f32::from_be_bytes(r[8 + 0x40..8 + 0x44].try_into().unwrap());
                (read, (h, r[8 + 0x85]))
            })
            .collect(),
    )
}

/// The heading the camera follows (obj+0x40): the facing's, or square to the wall in state 122 (the push's
/// angle by the game's table arctangent, read from the ROM). Asserted on camera_wall and move_turnrun;
/// wall_jump is reported: the jump arc's 0.0008 leaves two rows (342, 352) one push-angle unit off.
#[test]
fn heading_matches_the_game() {
    if world().is_none() {
        return;
    }
    for (name, assert) in [
        ("camera_wall", true),
        ("move_turnrun", true),
        ("wall_jump", false),
    ] {
        let (Some(report), Some(raw)) = (replay(name, false), raw_headings(name)) else {
            eprintln!("{name}: no trace or raw dump: skipping");
            continue;
        };
        let (mut rows, mut wall, mut exact, mut max) = (0, 0, 0, 0.0f32);
        let mut misses = Vec::new();
        for &(read, h) in &report.headings {
            let Some(&(want, st)) = raw.get(&read) else {
                continue;
            };
            rows += 1;
            if st == 122 {
                wall += 1;
                exact += (h == want) as usize;
                max = max.max((h - want).abs());
                if h != want {
                    misses.push(read);
                }
            } else if h != want {
                misses.push(read);
            }
        }
        eprintln!(
            "{name:16} heading: {rows} rows, {wall} in state 122 ({exact} exact, max {max:.4} deg); rows off: {misses:?}",
        );
        assert!(!assert || misses.is_empty(), "{name}");
    }
}

/// The camera's eye sweep (`func_1512BB10` → `func_15044380`, object type 0x2D) on the mover: every call the
/// camera agent captured (`trajectories/camera_*_camcalls.csv`: from, to, the size words cam+0x95C/0x960,
/// the layers that pushed, where it stopped), compared bit for bit.
#[test]
fn camera_sweeps_match_the_game() {
    let Some(world) = world() else {
        return;
    };
    let (mut calls, mut pushed, mut bad) = (0, 0, Vec::new());
    for name in ["camera_wall", "camera_ledge", "camera_buttons"] {
        let path = PathBuf::from(ORACLE)
            .join("trajectories")
            .join(format!("{name}_camcalls.csv"));
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{name}: no capture: skipping");
            continue;
        };
        // cam+0x2C and cam+0x84 at the mode dispatch before each sweep: bit 31 skips the objects (a level
        // script, func_15103430), aiming (0x100) lifts the step cap.
        let (mut mode, mut flags) = (8u32, 0u32);
        for line in text.lines() {
            let c: Vec<&str> = line.split(',').collect();
            let hex = |i: usize| u32::from_str_radix(c[i].trim_start_matches("0x"), 16).unwrap();
            if c[1] == "mode" {
                (mode, flags) = (hex(3), hex(4));
            }
            if c[1] != "sweep" {
                continue;
            }
            let f = |i: usize| c[i].parse::<f32>().unwrap();
            let (from, to, size) = ([f(2), f(3), f(4)], [f(5), f(6), f(7)], [f(8), f(9)]);
            let (hits, want) = (c[14].parse::<u32>().unwrap(), [f(15), f(16), f(17)]);
            let got = world.sweep(
                from,
                to,
                &Body::camera(size[0], size[1]),
                &Sweep {
                    any_slope: true,
                    long_step: mode == 0x100,
                    no_secondary: flags & 0x1_0000 != 0,
                    no_objects: flags & 0x8000_0000 != 0,
                    ..Sweep::default()
                },
                0.0,
            );
            calls += 1;
            pushed += (hits > 0) as usize;
            if got.pos.map(f32::to_bits) != want.map(f32::to_bits) || (got.hits > 0) != (hits > 0) {
                bad.push(format!(
                    "{name} read {}: got {:?} hits {} want {want:?} hits {hits}",
                    c[0], got.pos, got.hits
                ));
            }
        }
    }
    eprintln!(
        "camera sweeps: {calls} calls ({pushed} pushed), {} differ",
        bad.len()
    );
    for b in bad.iter().take(20) {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty());
}

/// The camera's wall ray (`func_15123A54` → `func_150AC9C0`) on the ported line query: every ray the camera
/// agent captured (`trajectories/camera_*_camcalls.csv`: origin, direction, length; hit, distance, point and
/// the triangle's vertices), compared bit for bit.
#[test]
fn camera_rays_match_the_game() {
    let Some(world) = world() else {
        return;
    };
    let (mut calls, mut hits, mut bad) = (0, 0, Vec::new());
    for name in ["camera_wall", "camera_ledge", "camera_buttons"] {
        let path = PathBuf::from(ORACLE)
            .join("trajectories")
            .join(format!("{name}_camcalls.csv"));
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{name}: no capture: skipping");
            continue;
        };
        for line in text.lines() {
            let c: Vec<&str> = line.split(',').collect();
            if c[1] != "ray" {
                continue;
            }
            let f = |i: usize| c[i].parse::<f32>().unwrap();
            let got = world.ray([f(3), f(4), f(5)], [f(6), f(7), f(8)], f(9));
            let want = (c[10] != "0").then(|| {
                let tri: Vec<i32> = (15..24).map(|i| c[i].parse().unwrap()).collect();
                (f(11), [f(12), f(13), f(14)], tri)
            });
            calls += 1;
            hits += want.is_some() as usize;
            let got = got.map(|h| {
                let v = world.tris[h.tri as usize].v;
                (
                    h.dist,
                    h.point,
                    v.iter().flatten().copied().collect::<Vec<i32>>(),
                )
            });
            let same = match (&got, &want) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.0.to_bits() == b.0.to_bits()
                        && a.1.map(f32::to_bits) == b.1.map(f32::to_bits)
                        && a.2 == b.2
                }
                _ => false,
            };
            if !same {
                bad.push(format!("{name} read {}: got {got:?} want {want:?}", c[0]));
            }
        }
    }
    eprintln!(
        "camera rays: {calls} calls ({hits} hit), {} differ",
        bad.len()
    );
    for b in bad.iter().take(20) {
        eprintln!("  {b}");
    }
    assert!(bad.is_empty());
}

/// The camera's level flags (`cam+0x84`) from the ROM's trigger table, against every flag word the camera
/// captures logged at the mode dispatch of a frame: bit 0x40000 (inside a camera volume) and bit 31 (the
/// scene script's zones 0x58/0x59: the sweep skips the placements), both from last frame's view eye
/// (`cam+0x2F8`, from the raw dump).
#[test]
fn camera_flags_match_the_game() {
    let Some(level) = world().and_then(|w| w.camera.as_ref()) else {
        return;
    };
    assert_eq!(level.skip_objects.len(), 2);
    let (mut rows, mut bad) = (0, Vec::new());
    for name in ["camera_wall", "camera_ledge", "camera_buttons"] {
        let dir = PathBuf::from(ORACLE);
        let (Ok(calls), Some(eyes)) = (
            std::fs::read_to_string(dir.join(format!("trajectories/{name}_camcalls.csv"))),
            raw_view_eyes(name),
        ) else {
            eprintln!("{name}: no capture or raw dump: skipping");
            continue;
        };
        for line in calls.lines() {
            let c: Vec<&str> = line.split(',').collect();
            if c[1] != "mode" {
                continue;
            }
            let read: u32 = c[0].parse().unwrap();
            let flags = u32::from_str_radix(c[4].trim_start_matches("0x"), 16).unwrap();
            // Row `read` of the raw dump is the camera after frame read - 1.
            let Some(&eye) = eyes.get(&read) else {
                continue;
            };
            rows += 1;
            let got = (level.in_volume(eye), level.skips_objects(eye));
            let want = (flags & 0x4_0000 != 0, flags & 0x8000_0000 != 0);
            if got != want {
                bad.push(format!(
                    "{name} read {read}: got {got:?} want {want:?} eye {eye:?}"
                ));
            }
        }
    }
    eprintln!("camera flags: {rows} mode dispatches, {} differ", bad.len());
    let mut per_file: HashMap<String, usize> = HashMap::new();
    for b in &bad {
        *per_file
            .entry(b.split(' ').next().unwrap().to_string())
            .or_default() += 1;
    }
    eprintln!("{per_file:?}");
    for b in bad
        .iter()
        .filter(|b| std::env::var_os("ORACLE_ZONES").is_some() || !b.contains("bot"))
        .take(40)
    {
        eprintln!("  {b}");
    }
    assert!(rows > 0 && bad.is_empty());
}

/// cam+0x2F8 (the view eye) of player 1 per read, from `raw/<name>.bin`.
fn raw_view_eyes(name: &str) -> Option<HashMap<u32, [f32; 3]>> {
    let data = std::fs::read(
        PathBuf::from(ORACLE)
            .join("raw")
            .join(format!("{name}.bin")),
    )
    .ok()?;
    const CAM: usize = 8 + 0x32C + 0x1B0;
    const REC: usize = CAM + 0x700;
    Some(
        data.as_chunks::<REC>()
            .0
            .iter()
            .filter(|r| r[4..8] == [0; 4])
            .map(|r| {
                let f = |o: usize| f32::from_be_bytes(r[CAM + o..CAM + o + 4].try_into().unwrap());
                (
                    u32::from_le_bytes(r[..4].try_into().unwrap()),
                    [f(0x2F8), f(0x2FC), f(0x300)],
                )
            })
            .collect(),
    )
}

/// Fields of one player per read from a `CONKER_TRACE_RAW` file (`trajectories/<name>.raw` or
/// `raw/<name>.bin`): `(block, offset)` byte reads, block 0 the object, 1 its +0x31C struct.
fn raw_bytes(name: &str, player: u32, fields: &[(usize, usize)]) -> Option<HashMap<u32, Vec<u8>>> {
    let dir = PathBuf::from(ORACLE);
    let data = std::fs::read(dir.join(format!("trajectories/{name}.raw")))
        .or_else(|_| std::fs::read(dir.join(format!("raw/{name}.bin"))))
        .ok()?;
    const REC: usize = 8 + 0x32C + 0x1B0 + 0x700;
    let base = [8, 8 + 0x32C];
    Some(
        data.as_chunks::<REC>()
            .0
            .iter()
            .filter(|r| u32::from_le_bytes(r[4..8].try_into().unwrap()) == player)
            .map(|r| {
                let read = u32::from_le_bytes(r[..4].try_into().unwrap());
                (read, fields.iter().map(|&(b, o)| r[base[b] + o]).collect())
            })
            .collect(),
    )
}

/// The feed of a two-player combat trace for player 1: the lock and drawn weapon its rules set, and
/// player 2's body (updated after player 1, so player 1 sees its position from the frame's start).
fn combat_feed(name: &str) -> Option<Feed> {
    let feed = raw_feed(name)?;
    let mut other: HashMap<u32, Vec<Other>> = trace_of(name, "1")
        .map(|rows| {
            rows.iter()
                .map(|r| (r.read, vec![Other::player(Side::Tediz, r.state.pos)]))
                .collect()
        })
        .unwrap_or_default();
    // Grenades (class 0x7E: radius 22, centre 8 up, scale 1; V: RAM dumps) where the combat agent's lldb
    // log lists them, at the frame's start.
    let log = std::fs::read_to_string(
        PathBuf::from(ORACLE).join(format!("trajectories/{name}_lldb.txt")),
    )
    .unwrap_or_default();
    for l in log.lines() {
        let w: Vec<&str> = l.split_whitespace().collect();
        if w.get(2) != Some(&"obj") || w.get(5) != Some(&"0x7e") {
            continue;
        }
        let Some(i) = w.iter().position(|&x| x == "pos") else {
            continue;
        };
        let pos = [1, 2, 3].map(|k| w[i + k].parse::<f32>().unwrap());
        let grenade = Other {
            pos,
            radius: 22,
            lift: 8,
            y_scale: 1.0,
        };
        other
            .entry(w[0].parse().unwrap())
            .or_default()
            .push(grenade);
    }
    Some(Feed { other, ..feed })
}

/// Player 1's movement in the combat traces where nothing the sim leaves to the match rules moves it,
/// with the rules' lock and drawn weapon and player 2's body fed from the oracle (trace camera). The
/// rows the rules move (the grenade's push on its thrower, hits) are in [`COMBAT_EXCLUDED`].
#[test]
fn combat_movement() {
    if world().is_none() {
        return;
    }
    for name in COMBAT_TRACES {
        let Some(feed) = combat_feed(name) else {
            eprintln!("{name}: no trace or raw dump: skipping");
            continue;
        };
        let Some(report) = replay_fed(name, false, &feed) else {
            eprintln!("{name}: no trace or script: skipping");
            continue;
        };
        assert!(report.ok(), "{name}: {report:?}");
    }
    // Player 2 standing in the middle, pushed by player 1 (V: combat_e_w14b from 820, up to 25 units).
    for name in [
        "combat_e_w14b",
        "combat_e_w14",
        "combat_e_w16",
        "combat_approach",
    ] {
        let report = second_feed(name).and_then(|feed| replay_fed(name, false, &feed));
        let Some(report) = report else {
            eprintln!("{name}: no player-2 trace or script: skipping");
            continue;
        };
        assert!(report.ok(), "{name} player 2: {report:?}");
    }
    // ORACLE_COMBAT=a,b reports more combat traces the same way (not asserted), ORACLE_COMBAT2 their
    // second players.
    let names = |var| std::env::var(var).unwrap_or_default();
    for name in names("ORACLE_COMBAT").split(',').filter(|n| !n.is_empty()) {
        if let Some(feed) = combat_feed(name) {
            replay_fed(name, false, &feed);
        }
    }
    for name in names("ORACLE_COMBAT2").split(',').filter(|n| !n.is_empty()) {
        if let Some(feed) = second_feed(name) {
            replay_fed(name, false, &feed);
        }
    }
}

/// The combat traces [`combat_movement`] asserts (player 1): C-button taps during the grenade throw, the
/// pickup runs, the meeting run and every encounter up to its end (player 1 attacks; weapon 0x16's attack
/// moves by its root, 0x77), armed strafes (k6, k10: C-up with 0x23, 0x24), a cooked grenade's push, and
/// the camera agent's armed runs (`camera4_w<id>`: R with each weapon, the stick twisting a gun's upper
/// body and turning the facing, or turning the scope's look).
const COMBAT_TRACES: [&str; 28] = [
    "combat_buttons",
    "combat_ctl_cd_cr",
    "combat_pick57",
    "combat_pick87",
    "combat_meet",
    "combat_approach",
    "combat_e_w09",
    "combat_e_w14",
    "combat_e_w14b",
    "combat_e_w16",
    "combat_e_w21",
    "combat_e_w22",
    "combat_e_w24",
    "combat_e_gren",
    "combat_ammo_cook",
    "combat_ctlw_k4",
    "combat_ctlw_k6",
    "combat_ctlw_k7",
    "combat_ctlw_k10",
    "camera4_w09",
    "camera4_w14",
    "camera4_w15",
    "camera4_w16",
    "camera4_w21",
    "camera4_w22",
    "camera4_w23",
    "camera4_w24",
    "camera4_w39",
];

/// The shot death 0x17E drifts by its root (combat_e_w22 player 2 from 852, combat_e_w24 from 905):
/// [`movement::root_motion`] each frame along the traced facing (its spin is the match rules'), with
/// [`movement::animate`] on 0x17E at the traced rate, from the traced clock and root fields.
#[test]
fn death_drifts_by_its_root() {
    let Some(world) = world() else {
        return;
    };
    // e_w22 ends where player 1 starts pushing the body (866, the rules' pushes).
    for (name, first, last) in [("combat_e_w22", 852, 865), ("combat_e_w24", 905, 960)] {
        let (Some(rows), Some(raw)) = (
            trace_of(name, "1"),
            raw_bytes(name, 1, &[(0, 0xB4), (0, 0xB5), (0, 0xB6), (0, 0xB7)]),
        ) else {
            eprintln!("{name}: no trace or raw dump: skipping");
            continue;
        };
        let at_row = |r: u32| rows.iter().find(|row| row.read == r).unwrap();
        let fields = raw_floats(name, 1, &[0x1F8, 0x1F4]).unwrap();
        let flags = raw_bytes(name, 1, &[(0, 0x1FC)]).unwrap();
        let time = |r: u32| f32::from_be_bytes(raw[&r][..4].try_into().unwrap());
        let mut p = at_row(first).state;
        p.floor = world.floor(p.pos[0], p.pos[2], p.pos[1], p.pos[1], None);
        p.root = conker_sim::root::Root {
            id: 0x17E,
            time: time(first),
            at: fields[&first][0],
            advance: fields[&first][1],
            moves: flags[&first][0] & 1 != 0,
            wrapped: flags[&first][0] & 4 != 0,
            ..Default::default()
        };
        let mut max = 0f32;
        for k in first..last {
            let next = at_row(k + 1);
            p.facing = next.state.facing;
            movement::root_motion(&mut p, world);
            movement::animate(&mut p, 0x17E, next.rate, 0.0, world);
            let e = ((p.pos[0] - next.state.pos[0]).powi(2)
                + (p.pos[2] - next.state.pos[2]).powi(2))
            .sqrt();
            if std::env::var_os("ORACLE_DEATH").is_some() {
                eprintln!(
                    "{} {:?} want {:?} adv {} t {} fc {} err {e}",
                    k + 1,
                    p.pos,
                    next.state.pos,
                    p.root.advance,
                    p.root.time,
                    p.facing
                );
            }
            max = max.max(e);
        }
        eprintln!("{name} player 2 death 0x17E rows {first}-{last}: max {max:.5}");
        assert!(max < 0.001, "{name}: {max}");
    }
}

/// f32 fields of one player's object per read from a raw dump.
fn raw_floats(name: &str, player: u32, offsets: &[usize]) -> Option<HashMap<u32, Vec<f32>>> {
    let fields: Vec<(usize, usize)> = offsets
        .iter()
        .flat_map(|&o| (0..4).map(move |i| (0, o + i)))
        .collect();
    let raw = raw_bytes(name, player, &fields)?;
    Some(
        raw.into_iter()
            .map(|(k, v)| {
                (
                    k,
                    v.chunks(4)
                        .map(|c| f32::from_be_bytes(c.try_into().unwrap()))
                        .collect(),
                )
            })
            .collect(),
    )
}

/// The wall routine's zone gate (`obj+0xF8` bit 0x4: `func_150829D8` sets it every frame, then
/// `func_150A11C4` clears it inside the trigger table's type-1 zones of kind 0 and sets it in those of
/// kind 6, the last one containing the player deciding): the camera volumes, tested with the player's
/// position at the frame's start ([`conker_sim::movement::walls_on`]). Compared with the bit in every raw
/// dump of a known name, all players: humans asserted, bots (s126+0x84 set) reported. Rows after a
/// teleport (a respawn, the lifts: more than 100 units in a frame) and `combat_end` (it ends in scene
/// 0x1D) are skipped.
#[test]
fn wall_zones_match_the_game() {
    let Some(world) = world() else {
        return;
    };
    let dir = PathBuf::from(ORACLE);
    let mut files: Vec<PathBuf> = ["raw", "trajectories"]
        .iter()
        .flat_map(|d| std::fs::read_dir(dir.join(d)).into_iter().flatten())
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "bin" || x == "raw"))
        .filter(|p| {
            // The dumps of scene 0x2D this test knows (other agents add theirs concurrently);
            // `combat_end` ends in scene 0x1D.
            const KNOWN: [&str; 11] = [
                "camera_", "camera4_", "move_", "anim_", "wall_", "combat_", "combat4_", "bot_",
                "ex_", "explore", "probe_",
            ];
            let name = p.file_name().unwrap().to_string_lossy();
            KNOWN.iter().any(|k| name.starts_with(k)) && !name.starts_with("combat_end")
        })
        .collect();
    files.sort();
    const REC: usize = 8 + 0x32C + 0x1B0 + 0x700;
    let (mut rows, mut off_rows, mut bot_rows, mut bad, mut bot_bad) =
        (0, 0, 0, Vec::new(), Vec::new());
    for path in files {
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        if data.is_empty() || data.len() % REC != 0 {
            continue;
        }
        // (player, read) → (position, bit, bot)
        let mut at: HashMap<(u32, u32), ([f32; 3], bool, bool)> = HashMap::new();
        for r in data.as_chunks::<REC>().0 {
            let (read, player) = (
                u32::from_le_bytes(r[..4].try_into().unwrap()),
                u32::from_le_bytes(r[4..8].try_into().unwrap()),
            );
            let f = |o: usize| f32::from_be_bytes(r[8 + o..12 + o].try_into().unwrap());
            let flags = u32::from_be_bytes(r[8 + 0xF8..12 + 0xF8].try_into().unwrap());
            let bot = r[8 + 0x32C + 0x84] != 0;
            at.insert(
                (player, read),
                ([f(0x14), f(0x18), f(0x1C)], flags & 4 != 0, bot),
            );
        }
        for (&(player, read), &(now, bit, bot)) in &at {
            let Some(&(prev, ..)) = at.get(&(player, read.wrapping_sub(1))) else {
                continue;
            };
            let jump = (0..3).map(|k| (now[k] - prev[k]).abs()).fold(0.0, f32::max);
            if read < 25 || jump > 100.0 {
                continue;
            }
            let p = movement::PlayerState::spawn(prev, 0);
            let line = format!(
                "{} p{player} read {read}: bit {bit} at {prev:?}",
                path.file_name().unwrap().to_string_lossy()
            );
            let miss = bit != movement::walls_on(&p, world);
            if bot {
                bot_rows += 1;
                if miss {
                    bot_bad.push(line);
                }
                continue;
            }
            rows += 1;
            off_rows += !bit as usize;
            if miss {
                bad.push(line);
            }
        }
    }
    bad.sort();
    eprintln!(
        "wall zones: humans {rows} rows ({off_rows} with the routine off), {} differ; bots {bot_rows} rows, {} differ {bot_bad:?}",
        bad.len(),
        bot_bad.len()
    );
    for b in bad.iter().take(20) {
        eprintln!("  {b}");
    }
    assert!(rows > 0 && bad.is_empty());
}

/// The other maps' Deathmatch captures (1 human + 15 bots, 60 s): on every grounded row (y = floor_y)
/// from read 30, the floor query at the player's position (full, then the last-triangle fast path the
/// game uses while grounded) finds the game's floor, on the scene's collision with only its solid
/// placements. The rows still off: 0x34's boxes 6-9 stand 10 lower in the game while stood on (placement
/// dispatch 2, static here), and bots on lifts and slopes (untraced). With every placement's model the
/// pickups' tops become floors: 134 and 24 rows off on 0x30 and 0x3F.
#[test]
fn floors_match_the_game_on_other_maps() {
    let Some(rom) = std::env::var_os("CONKER_ROM") else {
        return;
    };
    let rom = extract_core::Rom::new(std::fs::read(rom).unwrap()).unwrap();
    for (scene, run, most) in [
        (0x30, "map30/s5_h1b15", 127),
        (0x34, "map34/s4_h1b15", 891),
        (0x3F, "map3f/s7_h1b15", 17),
    ] {
        let Ok(text) = std::fs::read_to_string(PathBuf::from(ORACLE).join(run).join("trace.csv"))
        else {
            return;
        };
        let world = scene_world(&rom, scene);
        let mut lines = text.lines();
        let header: HashMap<&str, usize> = (lines.next().unwrap().split(',').enumerate())
            .map(|(i, h)| (h, i))
            .collect();
        let (mut rows, mut off) = (0, Vec::new());
        for c in lines.map(|l| l.split(',').collect::<Vec<_>>()) {
            let f = |k: &str| c[header[k]].parse::<f32>().unwrap();
            if c[header["mp"]] != "1" || c[header["y"]] != c[header["floor_y"]] || f("read") < 30.0
            {
                continue;
            }
            let [x, y, z] = ["x", "y", "z"].map(f);
            let full = world.floor(x, z, y, y, None);
            let got = full.and_then(|h| world.floor(x, z, y, y, Some(h.tri)));
            rows += 1;
            if got.is_none_or(|h| (h.y - y).abs() > 0.5) {
                off.push((
                    f("read") as u32,
                    f("player") as u8,
                    [x, y, z],
                    got.map(|h| h.y),
                ));
            }
        }
        eprintln!(
            "{run}: {rows} grounded rows, {} off {:?}",
            off.len(),
            &off[..off.len().min(3)]
        );
        assert!(rows > 0 && off.len() <= most);
    }
}
