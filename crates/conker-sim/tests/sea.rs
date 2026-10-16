//! War: Beach's sea against the original (`lab/oracle/map2b/sea/`, E7b's runs on scenario 8, scene 0x2B):
//! player 1's rows of `sea` (dropped into the sea at read 100, floating, drifting with the current, moved
//! back into it at read 200, carried to the shore, standing up, the hard landing) and `swim1` (the same
//! drop, strokes to the shore, the wade, back into the water, the jump out, a landing on the sea floor
//! that goes back into the water, B and Z, a second swim out and the walk up the beach), and G4b's `east`
//! (the spawn in the sea's east camera hint, a walk and a swim), replayed through
//! [`movement::step_among`] on the ROM's collision with placement 1 as water and the current's path.
//! Frame N runs on row N's input and camera and is checked against row N+1, as `tests/oracle.rs` does;
//! `<run>_fields.csv` holds what the trace's columns do not (the water flag obj+0xAD, the drift
//! obj+0x16C/0x170, the rise s126+0x1C, obj+0xAE, the speed percentage obj+0xAA). Player 1 is a villager
//! with Beach's item 0x3E: its hold is fed as conker-rules' `beach::drive` gives it, its percentage from
//! the run's obj+0xAA (the handler sets it after the drive, so the frame after the wade's end drives at
//! 100 %).
//!
//! Needs `CONKER_ROM` and the lab checkout in `lab/`; skips otherwise. `SEA_DUMP=from:to` prints those reads.

use conker_camera::CameraState;
use conker_camera::math::yaw_of;
use conker_camera::volume::CameraLevel;
use conker_sim::collision::{CollisionWorld, Layer, NO_FLOOR};
use conker_sim::follow::{self, Follow};
use conker_sim::movement::{self, Input, Other, PlayerState};
use conker_sim::trig::AcosTable;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../lab/oracle/map2b/sea");
const TOLERANCE: f32 = 0.01;

fn world() -> Option<&'static CollisionWorld> {
    static WORLD: OnceLock<Option<CollisionWorld>> = OnceLock::new();
    WORLD
        .get_or_init(|| {
            let rom = std::fs::read(std::env::var_os("CONKER_ROM")?).expect("read CONKER_ROM");
            let rom = extract_core::Rom::new(rom).expect("CONKER_ROM is the US ROM");
            let placements = extract_scene::scene_placements(&rom, 0x2B).unwrap();
            let collision = extract_scene::scene_collision(&rom, 0x2B, &placements).unwrap();
            let (solid, water) = (
                extract_scene::deathmatch_solid(0x2B),
                extract_scene::deathmatch_water(0x2B),
            );
            let triangles = (collision.triangles.iter()).filter(
                |t| !matches!(t.layer, extract_scene::Layer::Placement(i) if !solid.contains(&i)),
            );
            let mut world = CollisionWorld::layered(triangles.map(|t| {
                let layer = match t.layer {
                    extract_scene::Layer::Terrain => Layer::Terrain,
                    extract_scene::Layer::Secondary => Layer::Secondary,
                    extract_scene::Layer::Placement(i) if water.contains(&i) => {
                        Layer::Water(i as u16)
                    }
                    extract_scene::Layer::Placement(i) => Layer::Placement(i as u16),
                };
                (t.points, t.surface.unwrap_or(0), layer)
            }));
            world.acos = AcosTable::from_game_data(&rom.game_data().unwrap());
            let points = conker_camera::volume::points(&rom.asset(&[0x0C, 0x2B, 8]).unwrap());
            world.camera = Some(CameraLevel::new(
                0x2B,
                &rom.asset(&[0x0C, 0x2B, 5]).unwrap(),
                &points,
            ));
            world.current = extract_scene::scene_path(&rom, 0x2B, 1).unwrap();
            Some(world)
        })
        .as_ref()
}

#[derive(Clone, Debug)]
struct Row {
    read: u32,
    input: (u16, i8, i8),
    yaw: u16,
    state: PlayerState,
    /// obj+0xAA at the frame's end.
    percent: u8,
    eye: [f32; 3],
    at: [f32; 3],
}

/// Player 1's rows of run `name`, with its fields.
fn rows(name: &str) -> Option<Vec<Row>> {
    let dir = PathBuf::from(DIR);
    let table = |file: &str| -> Option<Vec<HashMap<String, String>>> {
        let text = std::fs::read_to_string(dir.join(file)).ok()?;
        let mut lines = text.lines();
        let header: Vec<String> = lines.next()?.split(',').map(String::from).collect();
        Some(
            lines
                .map(|l| {
                    header
                        .iter()
                        .cloned()
                        .zip(l.split(',').map(String::from))
                        .collect()
                })
                .filter(|r: &HashMap<String, String>| r["player"] == "0")
                .collect(),
        )
    };
    let fields: HashMap<String, HashMap<String, String>> = table(&format!("{name}_fields.csv"))?
        .into_iter()
        .map(|r| (r["read"].clone(), r))
        .collect();
    let rows = table(&format!("{name}.csv"))?
        .into_iter()
        .filter(|r| r["mp"] == "1" && !r["x"].is_empty())
        .filter_map(|r| {
            let m = fields.get(&r["read"])?;
            let f = |k: &str| r[k].parse::<f32>().unwrap();
            let u = |k: &str| r[k].parse::<u32>().unwrap();
            let g = |k: &str| m[k].parse::<f32>().unwrap();
            let hex = |k: &str| u16::from_str_radix(r[k].trim_start_matches("0x"), 16).unwrap();
            let mut s = PlayerState::spawn(["x", "y", "z"].map(f), u("facing") as u16);
            (s.y_vel, s.gravity) = (f("y_vel"), f("gravity"));
            (s.xz_speed, s.target_speed) = (f("xz_speed"), f("target_speed"));
            (s.move_angle, s.state, s.floor_y) =
                (u("move_angle") as u16, u("state") as u8, f("floor_y"));
            (s.swimming, s.water, s.rise) = (g("ad") == 1.0, g("water"), g("rise"));
            (s.drift, s.jump_out) = ([g("drift_x"), g("drift_z")], g("ae") as u8);
            let (eye, at) = (
                ["cam_eye_x", "cam_eye_y", "cam_eye_z"].map(f),
                ["cam_at_x", "cam_at_y", "cam_at_z"].map(f),
            );
            Some(Row {
                read: u("read"),
                input: (hex("in_buttons"), f("in_x") as i8, f("in_y") as i8),
                yaw: CameraState {
                    yaw: yaw_of(at[0] - eye[0], at[2] - eye[2]),
                    ..CameraState::default()
                }
                .stick_yaw(),
                state: s,
                percent: g("percent") as u8,
                eye,
                at,
            })
        })
        .collect();
    Some(rows)
}

#[derive(Debug, Default)]
struct Report {
    rows: usize,
    max_err: f32,
    max_drift_err: f32,
    /// The follow camera's eye and look-at, stepped as server and client step it.
    max_eye_err: f32,
    first_bad: Option<String>,
}

/// Replays `name` from row `from` to row `to`; `pokes` move the player before a read's frame, as the
/// run's `CONKER_POKE` did. The camera is checked on the rows of `camera`.
fn replay(
    name: &str,
    from: u32,
    to: u32,
    pokes: &[(u32, [f32; 3])],
    camera: &[(u32, u32)],
) -> Option<Report> {
    let world = world()?;
    let rows = rows(name)?;
    let start = rows.iter().position(|r| r.read == from)?;
    let mut p = rows[start].state;
    p.airborne = !p.swimming && p.pos[1] > p.floor_y;
    p.prev_buttons = rows[start - 1].input.0;
    let mut cam = Follow::spawn(rows[start].eye, &p);
    (cam.cam.at, cam.cam.cut) = (rows[start].at, false);
    // The camera as the run had it: in a kind-5 hint's fixed camera (mode 0x40) if the last move ended in one.
    let level = world.camera.as_ref().unwrap();
    cam.cam.fixed = level.fixed(rows[start - 1].state.pos, p.pos, p.floor_y);
    if cam.cam.fixed.is_some() {
        // Its eye springs' velocities, from the last move (`cam+0x3C0`..; the eye moves by v·dt; the
        // look-at's height spring is not seeded, so a start needs it at rest).
        let (eye, last) = (rows[start].eye, rows[start - 1].eye);
        cam.cam.fixed_vel = std::array::from_fn(|k| (eye[k] - last[k]) / conker_camera::DT);
    }
    let mut report = Report::default();
    for pair in rows[start..].windows(2).take_while(|w| w[1].read <= to) {
        let (row, next) = (&pair[0], &pair[1]);
        assert_eq!(
            next.read,
            row.read + 1,
            "{name}: gap after read {}",
            row.read
        );
        if let Some((_, pos)) = pokes.iter().find(|(r, _)| *r == row.read) {
            p.pos = *pos;
        }
        // Beach's item 0x3E on a villager (`func_15065A5C`, not in the water): the hold kept, and the
        // drive at the percentage the last frame ended with (75 %, none after the wade's end: the
        // handler sets it after the drive, conker-rules' `beach::drive` before).
        let crouched = matches!(
            p.state,
            movement::state::CROUCH | movement::state::CROUCH_WALK
        );
        if !p.swimming && !crouched {
            (p.timer, p.speed_percent) = (p.timer.max(1), row.percent);
        }
        let (buttons, stick_x, stick_y) = row.input;
        let input = Input {
            stick_x,
            stick_y,
            buttons,
            camera_yaw: row.yaw,
            ..Default::default()
        };
        movement::step_among(
            &mut p,
            &input,
            world,
            Other::new([0.0; 3], [30, 66, 0]),
            &[],
        );
        let r = conker_anim::request(u16::from(p.state), p.xz_speed, p.target_speed);
        movement::animate(&mut p, r.id, r.rate, r.start, world);
        cam.step_view(follow::view(&p), &input, world);
        let dist =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt();
        let eye_err = match camera.iter().any(|r| (r.0..=r.1).contains(&next.read)) {
            true => dist(cam.cam.eye, next.eye).max(dist(cam.cam.view_at(), next.at)),
            false => 0.0,
        };
        report.max_eye_err = report.max_eye_err.max(eye_err);
        let want = &next.state;
        let err = (0..3)
            .map(|i| (p.pos[i] - want.pos[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        let drift_err = (0..2)
            .map(|i| (p.drift[i] - want.drift[i]).abs())
            .fold(0.0, f32::max);
        report.rows += 1;
        report.max_err = report.max_err.max(err);
        report.max_drift_err = report.max_drift_err.max(drift_err);
        let dump = |read: u32| {
            std::env::var("SEA_DUMP").ok().is_some_and(|v| {
                let (a, b) = v.split_once(':').unwrap();
                (a.parse::<u32>().unwrap()..=b.parse().unwrap()).contains(&read)
            })
        };
        let bad = dump(next.read)
            || err >= TOLERANCE
            || drift_err >= TOLERANCE
            || eye_err >= TOLERANCE
            || p.state != want.state
            || p.swimming != want.swimming
            || (p.xz_speed - want.xz_speed).abs() >= TOLERANCE
            || (p.water != want.water && want.water != NO_FLOOR);
        if bad {
            let line = format!(
                "row {}: err {err:.4} eye {eye_err:.4} pos {:?} want {:?} state {} want {} speed {} want {} move {} want {} facing {} want {} y_vel {} want {} drift {:?} want {:?} swimming {} want {} percent {} want {}",
                next.read,
                p.pos,
                want.pos,
                p.state,
                want.state,
                p.xz_speed,
                want.xz_speed,
                p.move_angle,
                want.move_angle,
                p.facing,
                want.facing,
                p.y_vel,
                want.y_vel,
                p.drift,
                want.drift,
                p.swimming,
                want.swimming,
                p.speed_percent,
                next.percent,
            );
            if std::env::var_os("SEA_ALL").is_some() || dump(next.read) {
                eprintln!("  {name} {line}");
            }
            report.first_bad.get_or_insert(line);
        }
    }
    eprintln!(
        "{name}: {} rows, max {:.5}, drift {:.5}, eye {:.5}: {}",
        report.rows,
        report.max_err,
        report.max_drift_err,
        report.max_eye_err,
        report.first_bad.as_deref().unwrap_or("ok")
    );
    Some(report)
}

fn check(name: &str, from: u32, to: u32, pokes: &[(u32, [f32; 3])], camera: &[(u32, u32)]) {
    if world().is_none() {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    }
    let Some(report) = replay(name, from, to, pokes, camera) else {
        eprintln!("{name}: no run under {DIR}: skipping");
        return;
    };
    assert!(report.first_bad.is_none(), "{name}: {report:?}");
}

/// The fall into the sea, the entry (state 209, y velocity 0.4 of the fall's against gravity −3), the
/// float to `water − 41.67` (204), the drift to the current, the second drop (read 200), the carry to the
/// shore, standing up (the wade 139, the brake to 12.5) and the fall's hard landing (54), then the drift
/// fading on the sand; the camera from 400, once its spring has caught up with the two moves (the eye
/// 100 over the sea's surface behind the player on the beach).
#[test]
fn sea_floats_drifts_and_stands_up() {
    check(
        "sea",
        101,
        659,
        &[(200, [-1000.0, -520.0, 9000.0])],
        &[(400, 659)],
    );
}

/// Strokes (34 at 40 %: +2 a frame to 14, the move angle 4 steps a frame), the shore (the wade at 75 %,
/// its clip's end standing the player idle for a frame: 236, 418, 514, 528), the landing's move angle
/// onto the facing (208, 498), back in from the sand, the jump out (y velocity 46, gravity 3.3 while A is
/// held, speed 15 braking 0.175 for 12 frames), the landing on the sea floor and in again, B and Z doing
/// nothing, the swimmer's 90-wide box against the shore's wall (476), the second swim out and the walk up
/// the beach onto the shelf (540). The camera from 300 (the swimmer's look-at 67.5 over it, the eye 100
/// over 50 above the surface), with 450-482 in the sea's west camera hint (trigger 0x27, kind 5: point
/// 1, reach 707), the fixed camera, and the follow camera taking over again from 483 (its distance and
/// height springs' constants back from 1 and 2 to 6 and 8).
#[test]
fn swim_strokes_wades_and_jumps_out() {
    check("swim1", 101, 749, &[], &[(300, 749)]);
}

/// The spawn on the east beach inside the sea's east camera hint (trigger 0x26, kind 5: point 0, reach
/// 751; `east.txt`, no poke), which the player never leaves: from the spawn's cut (read 24, the eye on
/// the fixed camera's target, still at 25) the walk east until the wall (86-104, where the player is
/// 0.25 off), and from the fall into the sea (225) the swim.
#[test]
fn east_spawns_in_the_fixed_camera() {
    check("east", 25, 85, &[], &[(26, 85)]);
    check("east", 225, 660, &[], &[(226, 660)]);
}
