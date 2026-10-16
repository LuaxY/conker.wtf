//! Replays the oracle's animation traces (`lab/oracle/trajectories/anim_*`, see
//! `lab/oracle/anim_README.md`) through `Ground`, `request` and `AnimPlayer`, player 1 only.
//! Skips when the lab files are absent (they are ROM-derived and live outside this tree).
//! `cargo test -p conker-anim --test oracle -- --nocapture` prints the per-scenario numbers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use conker_anim::{AnimPlayer, Ground, NONE, Request, SPAWN, request};

const SCENARIOS: [&str; 10] = [
    "anim_idle",
    "anim_idle_long",
    "anim_walk",
    "anim_run",
    "anim_turn",
    "anim_jump",
    "anim_jump_stand",
    "anim_ledge",
    "anim_crouch",
    "anim_creep",
];
/// States `func_1504C0E8` picks; the others are events the movement code decides.
const GROUND: [u16; 8] = [0, 1, 2, 15, 22, 27, 33, 56];

fn lab() -> Option<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lab");
    let ok = dir.join("oracle/trajectories/anim_run_fields.csv").exists();
    if !ok {
        eprintln!("{} has no oracle traces: skipping", dir.display());
    }
    ok.then_some(dir)
}

type Row = HashMap<String, String>;

/// CSV rows of player 0 (all rows when the file has no player column), keyed by `key`.
fn table(path: &Path, key: &str) -> Option<HashMap<i64, Row>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let head: Vec<String> = lines.next()?.split(',').map(str::to_owned).collect();
    let rows = lines.map(|l| {
        head.iter()
            .cloned()
            .zip(l.split(',').map(str::to_owned))
            .collect::<Row>()
    });
    Some(
        rows.filter(|r| r.get("player").is_none_or(|p| p == "0"))
            .map(|r| (r[key].parse().unwrap(), r))
            .collect(),
    )
}

fn num<T: std::str::FromStr>(r: &Row, k: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    let v = &r[k];
    match v.strip_prefix("0x") {
        Some(hex) => T::from_str(&i64::from_str_radix(hex, 16).unwrap().to_string()).unwrap(),
        None => v.parse().unwrap(),
    }
}

/// Logical id -> clip length in ticks, from the extractor's clips.json.
fn durations(lab: &Path) -> HashMap<u16, u16> {
    let path = lab.join("extract/slice/character-117/clips.json");
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut out = HashMap::new();
    for c in json["clips"].as_array().unwrap() {
        for l in c["logical_ids"].as_array().unwrap() {
            out.insert(
                l.as_u64().unwrap() as u16,
                c["duration_ticks"].as_u64().unwrap() as u16,
            );
        }
    }
    out
}

#[derive(Default, Debug)]
struct Score {
    frames: usize,
    /// (scenario, read) where `Ground::choose` disagreed.
    misses: Vec<(&'static str, i64)>,
    ground: usize,
    ground_ok: usize,
    rate_ok: usize,
    rate_err: f32,
    time_ok: usize,
    time_err: f32,
    clock: usize,
    clock_ok: usize,
}

fn replay(lab: &Path, name: &'static str, durations: &HashMap<u16, u16>) -> Score {
    let dir = lab.join("oracle/trajectories");
    let trace = table(&dir.join(format!("{name}.csv")), "read").unwrap();
    let fields = table(&dir.join(format!("{name}_fields.csv")), "read").unwrap();
    let clock = table(&dir.join(format!("{name}_clock.csv")), "timer").unwrap_or_default();
    let last = *fields.keys().max().unwrap();
    // Spawned in frame 22 (no advance that frame), one tick by read 24; blending is still off
    // in frame 24 (obj+0x2FF = 0).
    let mut p = AnimPlayer::default();
    p.play(SPAWN, durations[&SPAWN.id]);
    p.advance();
    let f24 = &fields[&24];
    assert_eq!(
        (p.id, p.time, p.rate),
        (
            num(f24, "anim_id"),
            num(f24, "anim_time"),
            num(f24, "anim_rate")
        )
    );
    let mut g = Ground::default();
    let mut s = Score::default();
    for frame in 24..last {
        // Row frame+1 holds the state after this frame: its speeds are what the frame decided with.
        let (now, after, row) = (&fields[&frame], &fields[&(frame + 1)], &trace[&(frame + 1)]);
        let traced: u16 = num(after, "anim_id");
        let (v, t): (f32, f32) = (num(row, "xz_speed"), num(row, "target_speed"));
        let z_held = num::<i64>(row, "pad_buttons") & 0x2000 != 0;
        let height = num::<f32>(row, "y") - num::<f32>(row, "floor_y");
        // obj+0x83 counts down the skid and the landing: the state is held while it stays > 0.
        let held = num::<u8>(now, "b83") > 1 && traced == num::<u16>(now, "anim_id");
        if GROUND.contains(&traced) && !held {
            s.ground += 1;
            let chosen = g.choose(v, t, z_held, height);
            if chosen == traced {
                s.ground_ok += 1;
            } else {
                s.misses.push((name, frame + 1));
                eprintln!(
                    "{name} read {}: chose {chosen}, traced {traced} (v {v} t {t} h {height})",
                    frame + 1
                );
            }
        }
        let mut req: Request = request(traced, v, t);
        if frame == 24 {
            req.blend = 0.0;
        }
        p.play(req, durations[&traced]);
        if let Some(c) = clock.get(&num::<i64>(&trace[&frame], "timer")) {
            s.clock += 1;
            let want = (
                num::<u16>(c, "id_raw") & 0x7FFF,
                num::<u16>(c, "prev_id"),
                num::<f32>(c, "time"),
                num::<f32>(c, "prev_time"),
                num::<f32>(c, "duration"),
                num::<i16>(c, "step"),
                num::<i16>(c, "weight"),
            );
            let prev = if p.prev == NONE { 0 } else { p.prev };
            let got = (
                p.id,
                prev,
                p.time,
                p.prev_time,
                p.duration,
                p.step,
                p.weight,
            );
            if got == want {
                s.clock_ok += 1;
            } else {
                eprintln!("{name} frame {frame} clock: got {got:?}, traced {want:?}");
            }
        }
        p.advance();
        s.frames += 1;
        let (rate, time): (f32, f32) = (num(after, "anim_rate"), num(after, "anim_time"));
        let (dr, dt) = ((p.rate - rate).abs(), (p.time - time).abs());
        s.rate_ok += (dr == 0.0) as usize;
        s.time_ok += (dt == 0.0) as usize;
        s.rate_err = s.rate_err.max(dr);
        s.time_err = s.time_err.max(dt);
        if dr != 0.0 || dt != 0.0 {
            eprintln!(
                "{name} read {}: id {traced} rate {} vs {rate}, time {} vs {time}",
                frame + 1,
                p.rate,
                p.time
            );
        }
    }
    s
}

#[test]
fn oracle_replay() {
    let Some(lab) = lab() else { return };
    let durations = durations(&lab);
    println!(
        "scenario         frames  ground id     rate exact (max err)  time exact (max err)  clock rows"
    );
    let mut total = Score::default();
    for name in SCENARIOS {
        let s = replay(&lab, name, &durations);
        println!(
            "{name:16} {:6}  {:5}/{:<5}  {:5}/{:<5} ({:.1e})    {:5}/{:<5} ({:.1e})    {}/{}",
            s.frames,
            s.ground_ok,
            s.ground,
            s.rate_ok,
            s.frames,
            s.rate_err,
            s.time_ok,
            s.frames,
            s.time_err,
            s.clock_ok,
            s.clock
        );
        total.frames += s.frames;
        total.ground += s.ground;
        total.ground_ok += s.ground_ok;
        total.rate_ok += s.rate_ok;
        total.time_ok += s.time_ok;
        total.clock += s.clock;
        total.clock_ok += s.clock_ok;
        total.misses.extend(s.misses);
    }
    println!(
        "total            {:6}  {:5}/{:<5}  {:5}/{:<5}             {:5}/{:<5}             {}/{}",
        total.frames,
        total.ground_ok,
        total.ground,
        total.rate_ok,
        total.frames,
        total.time_ok,
        total.frames,
        total.clock_ok,
        total.clock
    );
    // Stick reversed at full speed: the game skids because the target it passes to func_1504C0E8
    // is 0 while the stick points against the motion; obj+0x44 (traced) still says 35.
    assert_eq!(total.misses, [("anim_jump_stand", 354)]);
    assert_eq!(total.rate_ok, total.frames);
    assert_eq!(total.time_ok, total.frames);
    assert_eq!(total.clock_ok, total.clock);
}
