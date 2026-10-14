//! The modern controls' spread (CONTROLS_RESEARCH.md §3 "Accuracy"): under MODERN a shot leaves along
//! its aim turned by an offset within a cone, the hip's, tighter aimed (R held), none through a scope
//! standing still and wider the faster the scope moves. Damage, rate of fire and ammo stay the ROM's: only
//! the cone balances the mouse's precision against weapons tuned for the N64's lock-on. The offset is a
//! hash of (tick, slot, shot count), not the match's RNG (the bots and the oracle replays draw that), so
//! the server and the client's prediction turn a shot alike, bit for bit (integer hash, `cos_sin`,
//! `sqrt`). Bots never set MODERN: no cone, their shots as before.

use crate::game::{BUTTON_R, Fighter};
use conker_sim::movement::{RUN_SPEED, SCOPE_SHARE, action, cos_sin};

/// The hip's and the aimed (R held) cones' half angles in degrees, by weapon; none for the rest (the
/// shotgun's spheres, the flame, the rockets, the blades). ponytail: CoD/CS-shaped guesses tuned on the
/// harness below (`spread_hits_and_headshots_per_100_shots`); retune there.
pub fn cones(weapon: u8) -> (f32, f32) {
    match weapon {
        // The twin guns, the pistol, the tommy gun.
        0x09 => (4.5, 1.6),
        0x39 => (3.5, 1.0),
        0x38 => (5.0, 2.0),
        // 0x23 kills with one bullet and fires only from the hip (R held starts nothing).
        0x23 => (3.5, 3.5),
        // The crossbows' and the bone knives' hip shots (aimed, they shoot through the scope).
        0x18 | 0x41 | 0x3F => (7.5, 7.5),
        _ => (0.0, 0.0),
    }
}

/// Through a scope, the cone at its full speed ([`RUN_SPEED`] × [`SCOPE_SHARE`]), in proportion
/// below; standing still, none. ponytail: a guess, as [`cones`].
pub const SCOPE_MOVING: f32 = 10.0;
/// A mounted gun on the aim (decision 19). ponytail: a guess, as [`cones`].
pub const MOUNTED: f32 = 1.0;

/// The cone (degrees) of `f`'s shot this frame: none without MODERN.
pub fn cone(f: &Fighter) -> f32 {
    if f.actions & action::MODERN == 0 {
        return 0.0;
    }
    if f.gun.is_some() {
        return MOUNTED;
    }
    if f.body.scope_yaw.is_some() {
        return SCOPE_MOVING * (f.body.xz_speed / (RUN_SPEED * SCOPE_SHARE)).min(1.0);
    }
    let (hip, aimed) = f.weapon.map_or((0.0, 0.0), cones);
    if f.buttons & BUTTON_R != 0 {
        aimed
    } else {
        hip
    }
}

/// Player `f`'s (slot `slot`) shot direction `dir` in frame `t`, turned within its [`cone`]: `dir`
/// itself without one. A player fires at most one shot a frame (the twin guns' muzzles 2 frames
/// apart), so its shot count tells its shots apart.
pub fn turn(f: &Fighter, dir: [f32; 3], t: u32, slot: usize) -> [f32; 3] {
    scatter(dir, cone(f), t, slot, f.shots)
}

/// The unit `dir` turned by up to `degrees` (the tangent taken as the angle, under 0.3 % off at 5°):
/// CS's spread, a uniform radius and a uniform angle, so the centre is denser than the edge. The
/// pitch stays within ±80° (`AIM_PITCH_MAX`), so `dir` is never vertical.
pub fn scatter(dir: [f32; 3], degrees: f32, t: u32, slot: usize, shots: u16) -> [f32; 3] {
    if degrees <= 0.0 {
        return dir;
    }
    // lowbias32 (Chris Wellons) of the three, each spread by an odd constant.
    let mut x = t.wrapping_mul(0x9E37_79B9)
        ^ (slot as u32).wrapping_mul(0x85EB_CA6B)
        ^ u32::from(shots).wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    let r = degrees * (core::f32::consts::PI / 180.0) * (x >> 16) as f32 / 65536.0;
    let (c, s) = cos_sin(x as u16);
    let [dx, dy, dz] = dir;
    let h = (dx * dx + dz * dz).sqrt();
    // Across the line: level to its right, and up (right × dir).
    let right = [-dz / h, 0.0, dx / h];
    let up = [-dx * dy / h, h, -dz * dy / h];
    let v: [f32; 3] = core::array::from_fn(|i| dir[i] + r * (c * right[i] + s * up[i]));
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|x| x / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Action, Arena, BUTTON_R, Fighter, Match};
    use crate::parts::Poses;
    use crate::projectile::{BodyParts, HEAD, Sphere};
    use crate::{Placement, Rng, weapon};
    use conker_sim::collision::CollisionWorld;
    use conker_sim::movement::{AIM_PIVOT_HEIGHT, AIM_SHOULDER, Input, PlayerState, action};
    use std::f32::consts::TAU;

    /// No cone without MODERN (a bot's shot); the hip's, the aimed one tighter, none through a still
    /// scope and more the faster it moves; the mounted gun's.
    #[test]
    fn the_cone_is_modern_only_and_tightens_aimed_and_scoped() {
        let place = Placement {
            index: 0,
            pos: [0.0; 3],
            facing: 0,
            camera: [0.0; 3],
        };
        let mut f = Fighter::new(&place, 0, 0);
        f.weapon = Some(0x09);
        assert_eq!(cone(&f), 0.0);
        f.actions = action::MODERN;
        let (hip, aimed) = cones(0x09);
        assert!(cone(&f) == hip && aimed < hip);
        f.buttons = BUTTON_R;
        assert_eq!(cone(&f), aimed);
        f.body.scope_yaw = Some(0.0);
        assert_eq!(cone(&f), 0.0);
        f.body.xz_speed = RUN_SPEED * SCOPE_SHARE / 2.0;
        assert_eq!(cone(&f), SCOPE_MOVING / 2.0);
        f.gun = Some(crate::gun::Mount::new(0, [0.0; 3]));
        assert_eq!(cone(&f), MOUNTED);
    }

    /// The turn stays within its cone over the whole of it, the same for the same (tick, slot,
    /// shot) and different for another; none without a cone.
    #[test]
    fn the_scatter_fills_its_cone_from_tick_slot_and_shot() {
        let n = (0.36f32 + 0.04 + 0.64).sqrt();
        let dir = [0.6 / n, 0.2 / n, -0.8 / n];
        assert_eq!(scatter(dir, 0.0, 5, 1, 9), dir);
        let (k, mut far, mut quarters) = (3f32.to_radians(), 0, [0; 4]);
        for t in 0..1000 {
            let v = scatter(dir, 3.0, t, 1, 9);
            assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-5);
            let cos = v.iter().zip(dir).map(|(a, b)| a * b).sum::<f32>().min(1.0);
            let angle = cos.acos();
            assert!(angle <= k.atan() + 1e-3, "{t}: {angle}");
            far += usize::from(angle > 0.8 * k);
            let side = (v[0] - dir[0]) * -dir[2] + (v[2] - dir[2]) * dir[0];
            quarters[usize::from(side > 0.0) * 2 + usize::from(v[1] > dir[1])] += 1;
        }
        assert!(
            far > 100 && quarters.iter().all(|&q| q > 150),
            "{far} {quarters:?}"
        );
        assert_eq!(scatter(dir, 3.0, 7, 1, 9), scatter(dir, 3.0, 7, 1, 9));
        assert_ne!(scatter(dir, 3.0, 7, 1, 9), scatter(dir, 3.0, 7, 2, 9));
        assert_ne!(scatter(dir, 3.0, 7, 1, 9), scatter(dir, 3.0, 7, 1, 10));
    }

    /// The Tedi's poses from the pack (`CONKER_ASSETS`, else `<repo>/assets`); None without it.
    fn tedi() -> Option<Poses> {
        let dir = std::env::var("CONKER_ASSETS")
            .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets").into());
        let dir = std::path::PathBuf::from(dir);
        let poses = Poses::load(&[117], |p| std::fs::read(dir.join(p))).ok()?;
        poses.has(117).then_some(poses)
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Mode {
        Hip,
        Aimed,
        Scoped,
        /// Scoped, strafing left and right (A-D every 20 frames).
        Moving,
    }

    /// A scripted modern shooter with a perfect mouse: each frame it aims from where its shot's line
    /// starts at the centre of the target's posed head where the shot will meet it (the strafing
    /// target's own script ahead by the flight). The target, a Tedi `range` ahead, stands or strafes
    /// ±200 at `strafe` a frame, and is put back every frame (2 health, so a head hit is a type 0x2E
    /// kill, a headshot; never immune or stunned; the same idle pose), so no hit is refused. Returns
    /// (shots, hits, headshots).
    fn rate(head: &BodyParts, id: u8, mode: Mode, range: f32, strafe: f32) -> (u32, u32, u32) {
        let big = 20_000.0;
        let world = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ]);
        let at = |x: f32, facing| Placement {
            index: 0,
            pos: [x, 0.0, 0.0],
            facing,
            camera: [0.0; 3],
        };
        let players = vec![
            Fighter::new(&at(0.0, 0), 0, 0),
            Fighter::new(&at(range, 0x8000), 1, 0),
        ];
        let arena = Arena::default();
        let mut m = Match::new(&arena, players, Rng::new(1), 0);
        for _ in 0..30 {
            m.step(&arena, &[Input::default(); 2], &world);
        }
        let w = weapon::weapon(id).unwrap();
        m.give(0, id);
        let f = &mut m.players[0];
        (f.drawn, f.rounds, f.immune) = (true, w.rounds, 0);
        let target = Fighter {
            health: 2,
            immune: 0,
            ..m.players[1]
        };
        let a = 200.0;
        let script = |t: u32| {
            let p = (strafe * t as f32) % (4.0 * a);
            let z = if strafe == 0.0 {
                0.0
            } else if p < 2.0 * a {
                p - a
            } else {
                3.0 * a - p
            };
            [range, 0.0, z]
        };
        let scoped = matches!(mode, Mode::Scoped | Mode::Moving);
        // The flight's frames: bullets 300 a frame, the crossbow's bolts and shots 125 after their
        // first move (an even frame), the sniper's ray none.
        let flight = |d: f32| match w.attack {
            weapon::Attack::Ray { .. } => 0,
            weapon::Attack::Bolt { .. } => (d / 125.0) as u32 + 2,
            _ => (d / weapon::BULLET_SPEED) as u32,
        };
        let units = |r: f32| r * 65536.0 / TAU;
        // The sniper's aimed ray leaves `Fighter::shots` as it is: its shots are its fires.
        let ray = matches!(w.attack, weapon::Attack::Ray { .. });
        let (shots0, hits0) = (m.players[0].shots, m.players[0].hits);
        let (mut rays, mut last, mut done) = (0, m.players[0].body.pos, None);
        loop {
            let t = m.tick;
            assert!(t < 20_000, "{id:#x} {mode:?}: no shots");
            let shots = match ray {
                true => rays,
                false => u32::from(m.players[0].shots - shots0),
            };
            if done.is_none() && shots >= 100 {
                done = Some(t);
            }
            if done.is_some_and(|d| t > d + 90) {
                let f = &m.players[0];
                return (shots, u32::from(f.hits - hits0), u32::from(f.headshots));
            }
            m.players[1] = Fighter {
                body: PlayerState {
                    pos: script(t),
                    ..target.body
                },
                ..target
            };
            m.players[0].kills = 0;
            if scoped {
                m.players[0].body.scope_yaw.get_or_insert(0.0);
            }
            // The target's parts where it stands, from the one pose (`head` at the origin).
            let feet = script(t);
            let shift = |c: [f32; 3]| [c[0] + feet[0], c[1] + feet[1], c[2] + feet[2]];
            let spheres = (head.spheres.iter())
                .map(|s| Sphere {
                    centre: shift(s.centre),
                    ..*s
                })
                .collect();
            m.parts = vec![
                None,
                Some(BodyParts {
                    spheres,
                    origin: feet,
                    ..head.clone()
                }),
            ];
            let head = shift(head.spheres.iter().find(|s| s.part == HEAD).unwrap().centre);
            let pos = m.players[0].body.pos;
            let moved = [pos[0] - last[0], pos[1] - last[1], pos[2] - last[2]];
            last = pos;
            // The line's start: the scope's eye after this frame's move, else the aim ray's (the
            // shoulder's, from the frame's start).
            let pivot = [pos[0], pos[1] + AIM_PIVOT_HEIGHT, pos[2]];
            let dir_to = |from: [f32; 3]| {
                let d = flight((range - from[0]).abs());
                let ahead = script(t + d);
                let to = [head[0], head[1], head[2] + ahead[2] - feet[2]];
                let v = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
                let yaw = units((-v[2]).atan2(v[0])).round() as i32 as u16;
                let pitch = units(v[1].atan2(v[0].hypot(v[2]))).round() as i16;
                (yaw, pitch)
            };
            let (yaw, pitch) = if scoped {
                dir_to([
                    pivot[0] + moved[0],
                    pivot[1] + moved[1],
                    pivot[2] + moved[2],
                ])
            } else {
                let (yaw, _) = dir_to(pivot);
                let r = f32::from(yaw) * TAU / 65536.0;
                dir_to([
                    pivot[0] + AIM_SHOULDER * r.sin(),
                    pivot[1],
                    pivot[2] + AIM_SHOULDER * r.cos(),
                ])
            };
            // Automatics held (the tommy gun in 8-frame bursts, its climb kept low), the rest
            // pressed every other frame.
            let fire = done.is_none()
                && match w.id {
                    0x38 => t % 16 < 8,
                    _ => w.automatic() || t % 2 == 0,
                };
            let stick = match mode {
                Mode::Moving if (t / 20) % 2 == 0 => 80,
                Mode::Moving => -80,
                _ => 0,
            };
            let input = Input {
                stick_x: stick,
                buttons: if mode == Mode::Hip { 0 } else { BUTTON_R },
                camera_yaw: yaw.wrapping_sub(0x4000),
                actions: action::MODERN | if fire { action::FIRE } else { 0 },
                aim_pitch: pitch,
                ..Input::default()
            };
            m.step(&arena, &[input, Input::default()], &world);
            let f = &m.players[0];
            rays += u32::from(ray && f.since == t && matches!(f.action, Action::Fire { .. }));
        }
    }

    /// Hits and headshots per 100 shots of a perfect mouse on the head, by weapon and mode, at 300,
    /// 800 and 1,500, the target standing and strafing at 35 (the run). Without the pack: skipped.
    /// Slow in debug: `cargo test --release -p conker-rules --lib spread -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn spread_hits_and_headshots_per_100_shots() {
        let Some(mut poses) = tedi() else {
            eprintln!("no pack: skipping");
            return;
        };
        // The Tedi's idle pose, its parts at the origin.
        let at = |x: f32, facing| Placement {
            index: 0,
            pos: [x, 0.0, 0.0],
            facing,
            camera: [0.0; 3],
        };
        let players = vec![
            Fighter::new(&at(-300.0, 0), 0, 0),
            Fighter::new(&at(0.0, 0x8000), 1, 0),
        ];
        let (arena, big) = (Arena::default(), 20_000.0);
        let world = CollisionWorld::new([
            ([[-big, 0.0, -big], [-big, 0.0, big], [big, 0.0, big]], 5),
            ([[-big, 0.0, -big], [big, 0.0, big], [big, 0.0, -big]], 5),
        ]);
        let mut m = Match::new(&arena, players, Rng::new(1), 0);
        for _ in 0..30 {
            poses.place(&mut m);
            let ev = m.step(&arena, &[Input::default(); 2], &world);
            poses.tick(&m, &ev);
        }
        poses.place(&mut m);
        let head = m.parts[1].clone().expect("posed");
        assert_eq!(head.origin, [0.0; 3]);
        let rows = [
            ("twin guns", 0x09, Mode::Hip),
            ("twin guns", 0x09, Mode::Aimed),
            ("pistol", 0x39, Mode::Hip),
            ("pistol", 0x39, Mode::Aimed),
            ("0x23", 0x23, Mode::Hip),
            ("tommy gun", 0x38, Mode::Hip),
            ("tommy gun", 0x38, Mode::Aimed),
            ("sniper", 0x24, Mode::Scoped),
            ("sniper", 0x24, Mode::Moving),
            ("crossbow", 0x18, Mode::Hip),
            ("crossbow", 0x18, Mode::Scoped),
            ("crossbow", 0x18, Mode::Moving),
        ];
        eprintln!(
            "| Weapon | Mode | 300 | 800 | 1500 | 300 strafing | 800 strafing | 1500 strafing |"
        );
        for (name, id, mode) in rows {
            let mut line = format!("| {name} | {mode:?} |");
            for strafe in [0.0, 35.0] {
                for range in [300.0, 800.0, 1500.0] {
                    let (n, h, hs) = rate(&head, id, mode, range, strafe);
                    line += &format!(" {} / {} |", h * 100 / n, hs * 100 / n);
                }
            }
            eprintln!("{line}");
        }
    }
}
