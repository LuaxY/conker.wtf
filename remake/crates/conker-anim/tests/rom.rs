//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.

use std::sync::OnceLock;

use extract_character::{Character, DEATHMATCH_PLAYER};
use extract_core::Rom;

fn rom() -> Option<&'static Rom> {
    static ROM: OnceLock<Option<Rom>> = OnceLock::new();
    let rom = ROM.get_or_init(|| {
        let path = std::env::var_os("CONKER_ROM")?;
        Some(Rom::new(std::fs::read(path).expect("read CONKER_ROM")).expect("the US ROM"))
    });
    if rom.is_none() {
        eprintln!("CONKER_ROM unset: skipping");
    }
    rom.as_ref()
}

/// The float constants `request` and `Ground` use are the game's own `.game` data words.
#[test]
fn constants_are_the_games() {
    let Some(rom) = rom() else { return };
    let data = rom.game_data().unwrap();
    let word = |vram: usize| {
        let o = vram - 0x8008_2B20; // .game data load address
        f32::from_bits(u32::from_be_bytes(data[o..o + 4].try_into().unwrap()))
    };
    // func_1504C0E8: speed and target weights, run/walk/creep rate offsets, crouch-walk target.
    let used = [0.4, 0.6, 0.8, 0.3, 0.1, 2.6];
    let addrs = [
        0x8009_91DC,
        0x8009_91E0,
        0x8009_91F4,
        0x8009_91F8,
        0x8009_91FC,
        0x8009_9200,
    ];
    for (a, v) in addrs.into_iter().zip(used) {
        assert_eq!(word(a), v, "{a:08X}");
    }
    assert_eq!(word(0x8009_97FC), 0.6); // func_15064B94: wall-push rate offset
}

/// Root motion: the locomotion clips keep the root's x and z still (only y bobs), so they only
/// show motion; the creep clip (0) travels forward, and the game moves the player by it
/// (anim_creep trace, see the README). Prints the per-clip ranges, in model units (x0.5 in the
/// world: obj+0x14C).
#[test]
fn root_channels() {
    let Some(rom) = rom() else { return };
    let player = Character::load(rom, DEATHMATCH_PLAYER).unwrap();
    let range = |v: &[f32]| {
        v.iter().fold(f32::MAX, |a, &b| a.min(b))..v.iter().fold(f32::MIN, |a, &b| a.max(b))
    };
    println!("id  dur        root x            root y            root z");
    for id in [0u16, 1, 2, 15, 22, 27, 33, 50, 54, 55, 56, 122] {
        let clip = player
            .clips
            .iter()
            .find(|c| c.logical.contains(&id))
            .unwrap();
        let axis = |k: usize| {
            clip.frames
                .iter()
                .map(|f| f.root[k] as f32 / 1024.0)
                .collect::<Vec<_>>()
        };
        let [x, y, z] = [range(&axis(0)), range(&axis(1)), range(&axis(2))];
        let r = |r: &std::ops::Range<f32>| format!("{:8.3}..{:<8.3}", r.start, r.end);
        println!(
            "{id:3} {:4}  {}  {}  {}",
            clip.duration,
            r(&x),
            r(&y),
            r(&z)
        );
        if [1, 2, 27, 122].contains(&id) {
            assert!(
                x.start == x.end && z.start == z.end,
                "clip {id} moves in xz"
            );
        }
        if id == 0 {
            assert!(z.end - z.start > 100.0);
        }
    }
}

/// While creeping the player moves by clip 0's root z (x0.5, along its facing), two frames late,
/// and not by its speed: compares each creeping frame's traced displacement with the root advance
/// of the frame two before, over the anim_creep trace (needs the lab trace too).
#[test]
fn creep_moves_by_root_motion() {
    let Some(rom) = rom() else { return };
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../lab/oracle/trajectories");
    let (Ok(trace), Ok(fields)) = (
        std::fs::read_to_string(dir.join("anim_creep.csv")),
        std::fs::read_to_string(dir.join("anim_creep_fields.csv")),
    ) else {
        eprintln!("no anim_creep trace: skipping");
        return;
    };
    let player = Character::load(rom, DEATHMATCH_PLAYER).unwrap();
    let clip = player
        .clips
        .iter()
        .find(|c| c.logical.contains(&0))
        .unwrap();
    let keys: Vec<f32> = clip
        .frames
        .iter()
        .map(|f| f.root[2] as f32 / 1024.0)
        .collect();
    let step = clip.step as f32;
    // Linear between stored frames; the last stretch (past the last key) is not covered here.
    let root_z = |t: f32| {
        let i = (t / step) as usize;
        let (a, b) = (keys[i], keys[i + 1]);
        a + (b - a) * (t / step - i as f32)
    };
    // Player-0 rows: read -> (x, z, facing) and read -> (anim id, anim time).
    let rows = |text: &str, cols: &[&str]| -> std::collections::HashMap<i64, Vec<f64>> {
        let mut lines = text.lines();
        let head: Vec<&str> = lines.next().unwrap().split(',').collect();
        let at = |c: &str| head.iter().position(|h| *h == c).unwrap();
        let (read, player) = (at("read"), at("player"));
        let idx: Vec<usize> = cols.iter().map(|c| at(c)).collect();
        lines
            .map(|l| l.split(',').collect::<Vec<_>>())
            .filter(|v| v[player] == "0")
            .map(|v| {
                (
                    v[read].parse().unwrap(),
                    idx.iter().map(|&i| v[i].parse().unwrap()).collect(),
                )
            })
            .collect()
    };
    let pos = rows(&trace, &["x", "z", "facing"]);
    let anim = rows(&fields, &["anim_id", "anim_time"]);
    let (mut n, mut max_err) = (0, 0f64);
    for k in 60..=400 {
        let (Some(a), Some(b), Some(t0), Some(t1)) = (
            pos.get(&(k - 1)),
            pos.get(&k),
            anim.get(&(k - 3)),
            anim.get(&(k - 2)),
        ) else {
            continue;
        };
        // Still creeping; both times inside clip 0, no loop between them, before the last key.
        let last = ((keys.len() - 1) as f32 * step) as f64;
        let still_creeping = anim.get(&k).is_some_and(|r| r[0] == 0.0);
        if !still_creeping || t0[0] != 0.0 || t1[0] != 0.0 || t1[1] < t0[1] || t1[1] >= last {
            continue;
        }
        let ang = b[2] * std::f64::consts::TAU / 65536.0;
        let forward = (b[0] - a[0]) * ang.cos() - (b[1] - a[1]) * ang.sin();
        let want = 0.5 * (root_z(t1[1] as f32) - root_z(t0[1] as f32)) as f64;
        n += 1;
        max_err = max_err.max((forward - want).abs());
    }
    println!("creep frames compared: {n}, max |displacement - 0.5 root dz| = {max_err:.4} units");
    assert!(n > 150 && max_err < 0.02);
}

/// The idle clips the rules' fidget times its picks with (`combat::IDLE_CLIPS`).
#[test]
fn idle_clip_lengths() {
    let Some(rom) = rom() else { return };
    let player = Character::load(rom, DEATHMATCH_PLAYER).unwrap();
    for (id, ticks) in conker_anim::combat::IDLE_CLIPS {
        let clip = player
            .clips
            .iter()
            .find(|c| c.logical.contains(&id))
            .unwrap();
        assert_eq!(clip.duration, ticks, "{id:#x}");
    }
}
