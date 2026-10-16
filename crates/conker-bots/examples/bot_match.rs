//! A headless Deathmatch on scene 0x2D (`BOT_SCENE=30`, `34`: another scene): one idle human (slot 0)
//! and N bots, conker-sim's movement and conker-rules' `Match::step`, the bots driven by conker-bots.
//! The oracle's counterpart is `lab/oracle/tools/bot_run.sh bot_idle 300 bot_long3` (1 idle human +
//! 3 bots). Another scene runs its own AI script ([`conker_bots::script`]), as the server does from
//! the pack's `maps/<map>/ai.bin`; on 0x3F every player holds the club, as with the server's cavemen.
//!
//! `CONKER_ROM=... cargo run --release -p conker-bots --example bot_match -- [frames=9000] [bots=3]
//!   [out.csv] [seed]`
//! prints a summary per participant; `out.csv` gets one row per frame and participant
//! (`read,player,x,y,z,health,weapon,grenades,script,ai,goal,kills,sees,target,role,drawn,gdc,enemy,
//! attacking,shots,action,immune,stun`: the role G+0x15C, the drawn item s126+0x78, the script's goal
//! G+0xDC, G+0x11C, G+0x21C and the shot counter s126+0x1AB, the sniper's shots counted from its fire
//! clip; the rules' action, immune and stun frames), the columns `lab/oracle/tools/bot_stats.py` reads
//! for both sides.

use conker_bots::brain::g;
use conker_bots::lift::Lifts;
use conker_bots::{Assets, Bots, DEFAULT_SKILL};
use conker_camera::volume::CameraLevel;
use conker_rules::Rng;
use conker_rules::game::{Arena, Event, Fighter, Match};
use conker_rules::parts::{self, Poses};
use conker_rules::spawn::{self, Spawn};
use conker_rules::weapon;
use conker_sim::collision::{CollisionWorld, Layer};
use conker_sim::movement::Input;
use conker_sim::trig::AcosTable;
use std::io::Write;

/// The oracle's match goes live at read 23 (row 23 shows the first frame).
const LIVE: u32 = 23;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let frames: u32 = args.first().map_or(9000, |a| a.parse().unwrap());
    let bots: usize = args.get(1).map_or(3, |a| a.parse().unwrap());
    let out = args.get(2).filter(|s| *s != "-");
    let seed: Option<u64> = args.get(3).map(|a| a.parse().unwrap());
    // BOT_DEBUG=k: participant k's walk state every 5 frames.
    let debug: Option<usize> = std::env::var("BOT_DEBUG").ok().map(|v| v.parse().unwrap());
    // BOT_GIVE=new:old (hex ids): the pickups giving weapon `old` give `new` instead.
    let give: Option<(u8, u8)> = std::env::var("BOT_GIVE").ok().map(|v| {
        let (a, b) = v.split_once(':').expect("BOT_GIVE=new:old");
        let id = |s: &str| u8::from_str_radix(s.trim_start_matches("0x"), 16).unwrap();
        (id(a), id(b))
    });
    let rom = std::env::var_os("CONKER_ROM").expect("CONKER_ROM");
    let rom = extract_core::Rom::new(std::fs::read(rom).unwrap()).unwrap();
    let scene = std::env::var("BOT_SCENE").map_or(0x2D, |s| usize::from_str_radix(&s, 16).unwrap());
    let placements = extract_scene::scene_placements(&rom, scene).unwrap();
    let collision = extract_scene::scene_collision(&rom, scene, &placements).unwrap();
    let solid = extract_scene::deathmatch_solid(scene);
    let triangles = (collision.triangles.iter())
        .filter(|t| !matches!(t.layer, extract_scene::Layer::Placement(i) if !solid.contains(&i)));
    let mut world = CollisionWorld::layered(triangles.map(|t| {
        let layer = match t.layer {
            extract_scene::Layer::Terrain => Layer::Terrain,
            extract_scene::Layer::Secondary => Layer::Secondary,
            extract_scene::Layer::Placement(i) => Layer::Placement(i as u16),
        };
        (t.points, t.surface.unwrap_or(0), layer)
    }));
    world.acos = AcosTable::from_game_data(&rom.game_data().unwrap());
    world.camera = Some(CameraLevel::new(
        scene as u16,
        &rom.asset(&[0x0C, scene, 5]).unwrap(),
        &conker_camera::volume::points(&rom.asset(&[0x0C, scene, 8]).unwrap()),
    ));
    let spawns = extract_scene::scene_spawns(&rom, scene)
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
    let (picks, spots) = weapon::pickups(scene as u16).expect("the scene's pickups");
    let mut arena = Arena {
        spawns,
        pickups: (picks.iter().enumerate())
            .filter(|(_, p)| p.gives != conker_rules::weapon::Gives::Mask)
            .map(|(k, p)| {
                let at = spots.get(k).copied();
                let at = at.unwrap_or(placements[usize::from(p.placement)].position);
                (*p, at.map(f32::from))
            })
            .collect(),
        ..Default::default()
    };
    for (p, _) in &mut arena.pickups {
        if let (conker_rules::weapon::Gives::Weapon(w), Some((new, old))) = (&mut p.gives, give)
            && *w == old
        {
            *w = new;
        }
    }
    let assets: Vec<Vec<u8>> = Assets::paths(scene)
        .iter()
        .map(|p| rom.asset(p).unwrap().into_owned())
        .collect();
    let assets = Assets {
        nav: &assets[0],
        triggers: &assets[1],
        ai: &assets[2],
        class: &assets[3],
    };

    let players = 1 + bots;
    let rng = seed.map_or_else(Rng::default, |s| Rng { state: s });
    // `Match::deathmatch` with the scene's start table.
    let mut starts = spawn::starts(scene as u16).expect("the scene's start tables");
    // 0x34's and 0x3F's first record as the oracle's boot draws it for the roster (V: 1 + 3 drew 0,
    // 1 + 5 3, 1 + 15 7; others unseen, 7), so both sides start alike (`Starts::draw`).
    if matches!(scene, 0x34 | 0x3F) {
        starts.deathmatch = spawn::ring(match players {
            4 => 0,
            6 => 3,
            _ => 7,
        });
    }
    let mut r = rng;
    spawn::place(&arena.spawns, 0, &mut r);
    let fighters = (0..players)
        .map(|i| {
            let at = usize::from(starts.deathmatch[i % 16]);
            let mut f = Fighter::new(&spawn::place(&arena.spawns, at, &mut r), i as u8, LIVE);
            // As the server's cavemen: on 0x3F everyone holds the club (the script's MP+0x1170).
            f.start = (scene == 0x3F).then_some(weapon::CLUB.id);
            // As the server's bots (s126+0x84: the pickups' longer reach, `tank::touches`).
            f.bot = i > 0;
            f
        })
        .collect();
    let mut m = Match::new(&arena, fighters, r, LIVE);
    // The mounted guns and 0x34's plates and traps, as the server has them.
    m.guns = conker_rules::gun::spawn(scene);
    m.traps = conker_rules::traps::Traps::new(scene as u16, assets.triggers);
    // The body parts the hits test, posed from the pack (`CONKER_ASSETS`, else `<repo>/assets`) as
    // the server has them; without a pack (or `BOT_PARTS=0`) the fitted cylinder and spheres.
    let dir = std::env::var("CONKER_ASSETS")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into());
    let mut poses = match std::env::var("BOT_PARTS").as_deref() {
        Ok("0") => Poses::default(),
        _ => Poses::load(&parts::models(scene as u16, 0), |p| {
            std::fs::read(std::path::Path::new(&dir).join(p))
        })
        .unwrap_or_default(),
    };
    // The mounted guns' recoil (the pack's model 138), as the server has it.
    let read = |p: &str| std::fs::read(std::path::Path::new(&dir).join(p)).ok();
    if let (false, [Some(gltf), Some(bin)]) =
        (m.guns.is_empty(), conker_rules::gun::FILES.map(read))
    {
        arena.fire_clip = conker_rules::gun::fire_clip(&gltf, &bin).unwrap_or_default();
    }
    let slots: Vec<usize> = (1..players).collect();
    let mut ai = Bots::new(assets, &[0], &slots, DEFAULT_SKILL, rng).expect("bot assets");
    let mut lifts = Lifts::new(assets.triggers, scene as u8);
    let mut rides = 0;
    let mut csv = out.map(|p| std::io::BufWriter::new(std::fs::File::create(p).unwrap()));
    if let Some(w) = &mut csv {
        writeln!(
            w,
            "read,player,x,y,z,health,weapon,grenades,script,ai,goal,kills,sees,target,role,drawn,gdc,enemy,attacking,shots,action,immune,stun"
        )
        .unwrap();
    }
    let mut stats = vec![Stats::default(); players];
    let mut snipes = vec![0u16; players];
    // 0x34: plate presses, deaths in a trap that is on, and the living's frames in one (at the
    // frame's start, where the traps hit).
    let (mut presses, mut trap_kills, mut in_trap, mut plates) = (0, [0; 2], [0; 2], [0u8; 4]);
    let mut zones = [0; 2];
    let mut last = m.players.iter().map(|f| f.body.pos).collect::<Vec<_>>();
    for _ in 0..frames {
        let mut inputs = vec![Input::default(); players];
        for (slot, _) in lifts.ride(&mut m, &arena) {
            ai.lifted(slot);
            rides += 1;
        }
        if !m.traps.list.is_empty() {
            conker_rules::traps::seat(&mut world, m.traps.offsets());
        }
        ai.tick(&m, &arena, &world, &mut inputs);
        ai.scopes(&mut m);
        if let Some(k) = debug
            && m.tick.is_multiple_of(5)
            && let (Some(n), Some(slot)) = (ai.npc(k), ai.slot(k))
        {
            let f = &m.players[slot];
            let [x, y, z] = f.body.pos;
            println!(
                "{} pos {x:.0} {y:.0} {z:.0} cur {} prev {} goal {} avoid {} script {} mode {} spd {} xz {:.1} wait {} wp {:?} in {:?} state {} ai {:?} action {:?} hp {} stun {} immune {}",
                m.tick,
                n.cur,
                n.prev,
                n.goal,
                n.avoid,
                n.script,
                n.mode,
                n.speed,
                f.body.xz_speed,
                n.wait,
                n.waypoint,
                inputs[slot],
                f.body.state,
                ai.state(k),
                f.action,
                f.health,
                f.stun,
                f.immune
            );
            let t = usize::try_from(ai.globals().get(g::TARGET, k) - 2)
                .ok()
                .and_then(|t| m.players.get(t));
            if let Some(t) = t {
                let (dx, dz) = (t.body.pos[0] - x, t.body.pos[2] - z);
                let to = conker_sim::movement::atan2_u16(-dz, dx);
                let b: Vec<String> = m
                    .bullets
                    .iter()
                    .filter(|b| b.1.owner == slot)
                    .map(|b| {
                        format!(
                            "{:.0},{:.0},{:.0} life {:.2}",
                            b.1.pos[0], b.1.pos[1], b.1.pos[2], b.1.life
                        )
                    })
                    .collect();
                println!(
                    "    target at {:?} dist {:.0} facing {} to {} action {:?} anim {:#x} drawn {} bullets {b:?}",
                    t.body.pos,
                    (dx * dx + dz * dz).sqrt(),
                    f.body.facing,
                    to,
                    f.action,
                    f.anim,
                    f.drawn
                );
            }
        }
        for (k, s) in stats.iter_mut().enumerate() {
            let sees = ai.globals().get(g::SEES, k);
            s.sees_wall_free += u32::from(sees >> 16 != 0);
            s.sees_clear += u32::from(sees & 0xFFFF != 0);
        }
        // Per player alive: in a trap that is on (1 a jet, 2 the electric floor), else 0.
        let trapped: Vec<usize> = (m.players.iter())
            .map(|f| {
                let b = &f.body;
                (0..m.traps.list.len())
                    .find(|&k| m.traps.hits(k) && m.traps.list[k].contains(b.pos, b.floor_y))
                    .filter(|_| f.health > 0)
                    .map_or(0, |k| 1 + usize::from(m.traps.list[k].electric))
            })
            .collect();
        for &t in trapped.iter().filter(|&&t| t != 0) {
            in_trap[t - 1] += 1;
        }
        for f in m.players.iter().filter(|f| f.health > 0) {
            for e in [false, true] {
                let b = &f.body;
                zones[usize::from(e)] += usize::from(
                    (m.traps.list.iter()).any(|t| t.electric == e && t.contains(b.pos, b.floor_y)),
                );
            }
        }
        poses.place(&mut m);
        let anims: Vec<u16> = m.players.iter().map(|f| f.anim).collect();
        let events = m.step(&arena, &inputs, &world);
        // The sniper's shots, its fire clip's starts: the game's counter s126+0x1AB steps for them,
        // the rules' `shots` leaves the ray out.
        for (p, f) in m.players.iter().enumerate() {
            if f.anim == weapon::SNIPER.fire.0 && anims[p] != f.anim {
                snipes[p] += 1;
                stats[p].shots += 1;
            }
        }
        poses.tick(&m, &events);
        ai.observe(&events, &m);
        for e in &events {
            match *e {
                Event::Hit {
                    victim,
                    attacker,
                    lethal: true,
                    ..
                } => {
                    stats[victim].deaths += 1;
                    if trapped[victim] != 0 {
                        trap_kills[trapped[victim] - 1] += 1;
                    }
                    if victim == attacker {
                        stats[victim].suicides += 1;
                    }
                }
                Event::Hit { attacker, .. } => {
                    stats[attacker].hits += 1;
                    let w = m.players[attacker].weapon.unwrap_or(0);
                    *stats[attacker].hits_by.entry(w).or_default() += 1;
                }
                Event::Shot { player, .. } => stats[player].shots += 1,
                Event::Pickup { player, pickup } => {
                    let gives = arena.pickups[pickup].0.gives;
                    stats[player].pickups.push(match gives {
                        conker_rules::weapon::Gives::Weapon(w) => w,
                        conker_rules::weapon::Gives::Grenades => 0xAA,
                        conker_rules::weapon::Gives::Mask => conker_rules::gas::MASK,
                        conker_rules::weapon::Gives::Power(bit) => bit,
                    });
                }
                _ => {}
            }
        }
        let state = m.traps.plates.map(|p| p.state);
        presses += (0..4).filter(|&k| plates[k] == 2 && state[k] == 3).count();
        plates = state;
        for (p, f) in m.players.iter().enumerate() {
            let s = &mut stats[p];
            let (dx, dz) = (f.body.pos[0] - last[p][0], f.body.pos[2] - last[p][2]);
            let d = (dx * dx + dz * dz).sqrt();
            if d < 300.0 {
                s.distance += d;
            }
            last[p] = f.body.pos;
            // s126+0x78, as the oracle's traces count it (the mounted gun 0x1B while seated).
            let drawn = f.camera_weapon();
            if drawn != 0 {
                *s.drawn.entry(drawn).or_default() += 1;
            }
            let k = ai.bot_slots().iter().position(|&b| b == p).map(|i| i + 1);
            let (state, goal, script) = k.and_then(|k| ai.state(k)).unwrap_or((-1, 0xFF, 0));
            if k.is_some() {
                *s.scripts.entry(script).or_default() += 1;
                *s.states.entry(state).or_default() += 1;
            }
            if let Some(w) = &mut csv {
                let [x, y, z] = f.body.pos;
                let g = ai.globals();
                let (sees, target) = (g.get(g::SEES, p), g.get(g::TARGET, p));
                writeln!(
                    w,
                    "{},{p},{x},{y},{z},{},{},{},{script},{state},{goal},{},{sees},{target},{},{drawn},{},{},{},{},{},{},{}",
                    m.tick,
                    f.health,
                    f.weapon.unwrap_or(0),
                    f.grenades,
                    f.kills,
                    g.get(0x15C, p),
                    g.get(0xDC, p),
                    g.get(g::ENEMY, p),
                    g.get(g::ATTACKING, p),
                    f.shots + snipes[p],
                    // The action's name (its fields dropped: the CSV's commas).
                    format!("{:?}", f.action).split([' ', '(']).next().unwrap_or(""),
                    f.immune,
                    f.stun
                )
                .unwrap();
            }
        }
    }
    let minutes = frames as f32 / 1800.0;
    if !m.traps.list.is_empty() {
        println!(
            "traps: {presses} presses, deaths in a jet {} and on the electric floor {}, the living's \
             frames in a jet {} and on the electric floor {}; in the zones at all: jets {}, electric {}",
            trap_kills[0], trap_kills[1], in_trap[0], in_trap[1], zones[0], zones[1]
        );
    }
    println!(
        "{frames} frames ({minutes:.2} min), {bots} bots, lift rides {rides}, faults {}, unknown natives {:?}",
        ai.faults, ai.unknown
    );
    for (p, (s, f)) in stats.iter().zip(&m.players).enumerate() {
        println!(
            "p{p}: kills {} ({:.2}/min) deaths {} suicides {} shots {} hits {} distance {:.0} ({:.0}/min) pickups {:02x?}",
            f.kills,
            f32::from(f.kills) / minutes,
            s.deaths,
            s.suicides,
            s.shots,
            s.hits,
            s.distance,
            s.distance / minutes,
            s.pickups
        );
        if p > 0 {
            println!("    scripts {:?}\n    ai states {:?}", s.scripts, s.states);
            println!(
                "    frames drawn by weapon {:02x?}, hits by weapon {:02x?}",
                s.drawn, s.hits_by
            );
        }
        if p > 0 {
            let g = ai.globals();
            println!(
                "    role G+0x15C {} sniper timer G+0x2378 {} wanted G+0x2338 {:#x}",
                g.get(0x15C, p),
                g.get(0x2378, p),
                g.get(0x2338, p)
            );
        }
        if std::env::var_os("BOT_SEES").is_some() {
            println!(
                "    frames seeing someone: no wall {} clear {}",
                s.sees_wall_free, s.sees_clear
            );
        }
    }
}

#[derive(Clone, Default)]
struct Stats {
    deaths: u32,
    suicides: u32,
    shots: u32,
    hits: u32,
    distance: f32,
    pickups: Vec<u8>,
    scripts: std::collections::BTreeMap<u8, u32>,
    states: std::collections::BTreeMap<i32, u32>,
    sees_wall_free: u32,
    sees_clear: u32,
    drawn: std::collections::BTreeMap<u8, u32>,
    hits_by: std::collections::BTreeMap<u8, u32>,
}
