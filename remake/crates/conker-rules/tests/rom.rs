//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.

use conker_rules::Surface;
use conker_rules::surface::SINK;
use extract_core::Rom;
use extract_scene::parse_terrain;
use std::collections::BTreeMap;

fn rom() -> Option<Rom> {
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return None;
    };
    Some(Rom::new(std::fs::read(path).expect("read CONKER_ROM")).expect("CONKER_ROM is the US ROM"))
}

/// `.game` data is loaded at 0x80082B20.
fn at(data: &[u8], vram: u32, n: usize) -> &[u8] {
    let o = (vram - 0x8008_2B20) as usize;
    &data[o..o + n]
}

fn f32_at(data: &[u8], vram: u32) -> f32 {
    f32::from_be_bytes(at(data, vram, 4).try_into().unwrap())
}

/// The constants the port hard-codes, read where the game's code loads them.
#[test]
fn constants_are_the_games() {
    let Some(rom) = rom() else { return };
    let data = rom.game_data().unwrap();
    let sink: Vec<u8> = SINK.iter().flatten().map(|&b| b as u8).collect();
    assert_eq!(at(&data, 0x8009_9140, 96), sink); // func_1504BA38
    assert_eq!(f32_at(&data, 0x8009_91D4).to_bits(), 0x3DCC_CCCD); // its 0.1
    assert_eq!(f32_at(&data, 0x8009_F740).to_bits(), 0x3780_0080); // random_float's scale
    assert_eq!(f32_at(&data, 0x8009_D960), 1e8); // the respawn's "far"
    assert_eq!(f32_at(&data, 0x8009_D964), std::f32::consts::PI); // the jitter's yaw to radians
}

/// Every scene's surface words, split into the fields the game reads: scene 0x2D uses kinds 0, 1,
/// 3, 4, 5, 9, 0xA, slide classes 0 and 6 and no bits 12-15; the whole ROM's spread is printed.
#[test]
fn surface_fields() {
    let Some(rom) = rom() else { return };
    let (mut kinds, mut classes, mut other) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut scene_2d = Vec::new();
    for scene in 0..69 {
        let Some(t) = parse_terrain(&rom.asset(&[0x04, scene]).unwrap()).unwrap() else {
            continue;
        };
        for &w in t.surfaces.iter().flatten() {
            let s = Surface(w);
            *kinds.entry(s.kind()).or_insert(0) += 1;
            *classes.entry(s.slide()).or_insert(0) += 1;
            *other.entry(w & !0x7000_001F).or_insert(0) += 1;
            if scene == 0x2D {
                scene_2d.push(s);
            }
        }
    }
    eprintln!("kinds {kinds:x?}\nslide classes {classes:?}\nother bits {other:x?}");
    let set = |f: fn(&Surface) -> u32| {
        scene_2d
            .iter()
            .map(f)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(set(|s| s.kind() as u32), [0, 1, 3, 4, 5, 9, 0xA].into());
    assert_eq!(set(|s| s.slide().into()), [0, 6].into());
    assert_eq!(set(|s| s.0 >> 12 & 0xF), [0].into());
    assert_eq!(scene_2d.iter().filter(|s| s.slide() != 0).count(), 12);
}

/// The kill gates (trigger type 1, action 4) of the packed maps: 0x2D's two at y −185, 0x30's at
/// −2071, 0x2B's west pit at −1641 (record 0x0F); 0x34 and 0x3F have none. fall6's drop at
/// (−1150, −1614 → −1690, −2430) crosses 0x2B's (V: hp 0 at read 91).
#[test]
fn kill_gates_of_the_maps() {
    let Some(rom) = rom() else { return };
    let gates =
        |scene: usize| conker_rules::lift::kill_gates(&rom.asset(&[0x0C, scene, 5]).unwrap());
    let ys = |scene| {
        gates(scene)
            .iter()
            .map(|g| g.zone.center[1])
            .collect::<Vec<_>>()
    };
    assert_eq!(ys(0x2D), [-185, -185]);
    assert_eq!(ys(0x30), [-2071]);
    assert_eq!(ys(0x2B), [-1641]);
    assert!(ys(0x34).is_empty() && ys(0x3F).is_empty());
    let pit = &gates(0x2B)[0];
    assert!(pit.takes(
        [-1150.0, -1614.0, -2430.0],
        [-1150.0, -1689.9, -2430.0],
        -1689.9
    ));
    assert!(!pit.takes(
        [-1150.0, -1689.9, -2430.0],
        [-1150.0, -1614.0, -2430.0],
        -1614.0
    ));
}

/// Scene 0x2D's cover 2 (placement 2, a wall at x = 548 across mask k22's alcove) stops a player
/// running in while shown and lets it through hidden; lifted 500 it lets it under ([`gas::seat`];
/// the pack keeps the doors' and covers' collision, conker-extract's `solid_placements`).
#[test]
fn a_cover_is_solid_while_shown() {
    use conker_rules::gas;
    use conker_sim::collision::{CollisionWorld, Layer};
    use conker_sim::movement::{self, Input, PlayerState};
    use conker_sim::trig::AcosTable;
    let Some(rom) = rom() else { return };
    let placements = extract_scene::scene_placements(&rom, 0x2D).unwrap();
    let collision = extract_scene::scene_collision(&rom, 0x2D, &placements).unwrap();
    let solid = [&[0, 1][..], extract_scene::deathmatch_solid(0x2D)].concat();
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
    // Full stick toward -x (move angle 0x8000).
    let run = Input {
        stick_y: 82,
        camera_yaw: 0x4000,
        ..Input::default()
    };
    for (shown, lift, through) in [(0xFF, 0, false), (0xFB, 0, true), (0xFF, 500, true)] {
        gas::seat(&mut world, true, shown, [0, 0, lift, 0, 0, 0, 0, 0]);
        let mut p = PlayerState::spawn([700.0, -2.0, -5556.0], 0x8000);
        for _ in 0..30 {
            movement::step(&mut p, &run, &world);
        }
        assert_eq!(p.pos[0] < 500.0, through, "{shown:#x} {lift}: {:?}", p.pos);
    }
    // Outside Total War the covers stand home whatever the gas says.
    gas::seat(&mut world, false, 0, [500; 8]);
    let mut p = PlayerState::spawn([700.0, -2.0, -5556.0], 0x8000);
    for _ in 0..30 {
        movement::step(&mut p, &run, &world);
    }
    assert!(p.pos[0] > 548.0, "{:?}", p.pos);
}

/// Scene 0x34's traps (`conker_rules::traps`): 29 kind-12 records, all off at load, 27 fire jets
/// (ids 0-26, each in one plate's list) and the electric floor's two boxes (records 43 and 82);
/// the plates' lists are the game's 9-byte arrays at 0x800A2380, 0x800A238C and 0x800A2398.
#[test]
fn the_traps_of_0x34() {
    use conker_rules::traps;
    let Some(rom) = rom() else { return };
    let data = rom.game_data().unwrap();
    for (k, list) in traps::FIRES.iter().enumerate() {
        assert_eq!(
            at(&data, 0x800A_2380 + 12 * k as u32, 9),
            *list,
            "plate {k}"
        );
    }
    let table = rom.asset(&[0x0C, 0x34, 5]).unwrap();
    let list = traps::parse(&table);
    assert!(
        table
            .chunks(0x34)
            .all(|r| r.len() < 0x15 || r[0x15] >> 2 != 12 || r[0x14] == 1)
    );
    let fires: Vec<u8> = list.iter().filter(|t| !t.electric).map(|t| t.id).collect();
    assert_eq!(fires.len(), 27);
    assert!((0..27).all(|id| fires.contains(&id)));
    let electric: Vec<usize> = list
        .iter()
        .filter(|t| t.electric)
        .map(|t| t.record)
        .collect();
    assert_eq!(electric, [43, 82]);
    assert_eq!(list[0].record, 27);
    assert!(list[0].contains([-2442.0, -370.0, 3257.0], -382.0));
    assert!(traps::Traps::new(0x2D, &table).list.is_empty());
}

/// Scene 0x34's plates replayed on the oracle's 60 s capture (1 human, 15 bots,
/// `../lab/oracle/map34/s4_h1b15`): each read's contacts are the players standing on a plate
/// ([`traps::CONTACT`]), each plate steps with them, and every grounded row's floor is queried with
/// the plates' collision where the replay has them. The contacts and the plates equal box34's RAM
/// (the same match, its first 335 reads: `dm-maps-results-scratch/out/box34/mem.bin`), and the
/// rows conker-sim's `floors_match_the_game_on_other_maps` finds off on 0x34 (891 with the plates
/// at rest) drop. Its plates go down 14 times and its traps kill no one (the 15 bots step on the
/// plates on their way, their script 0xBD knows nothing of the traps: `conker-bots`'
/// `bots_set_off_the_0x34_traps`).
#[test]
fn the_plates_hold_the_0x34_floors() {
    use conker_rules::traps;
    let Some(rom) = rom() else { return };
    let lab = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../lab");
    let Ok(text) = std::fs::read_to_string(format!("{lab}/oracle/map34/s4_h1b15/trace.csv")) else {
        return;
    };
    let mem = std::fs::read(format!(
        "{lab}/research/mp-original/dm-maps-results-scratch/out/box34/mem.bin"
    ))
    .unwrap_or_default();
    let ram: BTreeMap<u32, [(i16, u8, bool); 4]> = (mem.chunks_exact(68))
        .map(|r| {
            let plate = |k: usize| {
                let b = &r[4 + 16 * k..20 + 16 * k];
                (i16::from_be_bytes([b[2], b[3]]), b[11] & 3, b[15] & 4 != 0)
            };
            (
                u32::from_le_bytes(r[..4].try_into().unwrap()),
                [0, 1, 2, 3].map(plate),
            )
        })
        .collect();
    let mut world = world_34(&rom);
    let mut lines = text.lines();
    let header: std::collections::HashMap<&str, usize> =
        (lines.next().unwrap().split(',').enumerate())
            .map(|(i, h)| (h, i))
            .collect();
    let mut reads: BTreeMap<u32, Vec<[f32; 6]>> = BTreeMap::new();
    for c in lines.map(|l| l.split(',').collect::<Vec<_>>()) {
        if c[header["mp"]] != "1" || c[header["x"]].is_empty() {
            continue;
        }
        let f = |k: &str| c[header[k]].parse::<f32>().unwrap();
        let row = ["x", "y", "z", "floor_y", "player", "health"].map(f);
        reads.entry(f("read") as u32).or_default().push(row);
    }
    // A plate's footprint (its triangles' x and z bounds): the oracle's own floor queries, not the
    // replay's, say where its players stood (they lag a moving plate by a frame at times: V box34
    // 58-60, 99-101).
    let footprint = traps::PLATES.map(|n| {
        let tris = (0..world.tris.len() as u32).filter(|&i| world.placement(i) == Some(n));
        let v: Vec<[i32; 3]> = tris.flat_map(|i| world.tris[i as usize].v).collect();
        let (x, z) = (v.iter().map(|p| p[0]), v.iter().map(|p| p[2]));
        [x.clone().min(), x.max(), z.clone().min(), z.max()].map(|c| c.unwrap() as f32)
    });
    let mut traps = traps::Traps::new(0x34, &rom.asset(&[0x0C, 0x34, 5]).unwrap());
    let (mut rows, mut off, mut at_rest, mut checked) = (0, 0, 0, 0);
    let (mut presses, mut kills, mut alive, mut on) = (0, 0, [true; 16], vec![false; 29]);
    for (&read, players) in reads.range(23..) {
        let plates = traps.plates;
        if let Some(r) = ram.get(&read) {
            let want = r.map(|(y, s, _)| (y, s));
            let got = std::array::from_fn(|k| (traps::REST[k] + plates[k].dy, plates[k].state));
            assert_eq!(got, want, "plates at read {read}");
            checked += 1;
        }
        let mut stood = [false; 4];
        for &[x, y, z, floor_y, p, health] in players {
            // Killed in the last frame inside a trap that was on.
            let p = p as usize;
            let inside = (0..on.len()).any(|k| on[k] && traps.list[k].contains([x, y, z], floor_y));
            kills += u32::from(alive[p] && health == 0.0 && inside);

            alive[p] = health > 0.0;
            for (k, [x0, x1, z0, z1]) in footprint.iter().enumerate() {
                let rest = f32::from(traps::REST[k]);
                stood[k] |= (*x0..=*x1).contains(&x)
                    && (*z0..=*z1).contains(&z)
                    && (rest - f32::from(traps::DEPTH)..=rest).contains(&floor_y)
                    && y - floor_y <= traps::CONTACT;
            }
            if y != floor_y || read < 30 {
                continue;
            }
            rows += 1;
            traps::seat(&mut world, plates.map(|p| p.dy as i8));
            let got = world.floor(x, z, y, y, None);
            off += got.is_none_or(|h| (h.y - y).abs() > 0.5) as u32;
            traps::seat(&mut world, [0; 4]);
            let rest = world.floor(x, z, y, y, None);
            at_rest += rest.is_none_or(|h| (h.y - y).abs() > 0.5) as u32;
        }
        if let Some(r) = ram.get(&read) {
            assert_eq!(stood, r.map(|p| p.2), "contacts at read {read}");
        }
        on = (0..on.len()).map(|k| traps.hits(k)).collect();
        traps.step(stood.map(|s| s.then_some(0)));
        presses += (0..4)
            .filter(|&k| plates[k].state == 2 && traps.plates[k].state == 3)
            .count();
    }
    eprintln!(
        "0x34: {rows} grounded rows, {off} off with the plates ({at_rest} at rest); {checked} reads of box34's RAM equal; {presses} presses, {kills} trap kills"
    );
    assert!(checked > 300 && off * 4 < at_rest);
    assert_eq!((presses, kills), (14, 0));
}

/// Scene 0x34's collision as a Deathmatch has it (the placements `deathmatch_solid` keeps).
fn world_34(rom: &Rom) -> conker_sim::collision::CollisionWorld {
    use conker_sim::collision::{CollisionWorld, Layer};
    let placements = extract_scene::scene_placements(rom, 0x34).unwrap();
    let collision = extract_scene::scene_collision(rom, 0x34, &placements).unwrap();
    let solid = extract_scene::deathmatch_solid(0x34);
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
    world.acos = conker_sim::trig::AcosTable::from_game_data(&rom.game_data().unwrap());
    world
}

/// A 0x34 match from frame `tick` with fighters standing at `at` (health 6, no spawn immunity).
fn match_34(
    rom: &Rom,
    at: &[[f32; 3]],
    tick: u32,
) -> (conker_rules::game::Arena, conker_rules::game::Match) {
    use conker_rules::game::{Arena, Fighter, Match};
    let spawns = (extract_scene::scene_spawns(rom, 0x34).unwrap().iter())
        .map(|s| conker_rules::spawn::Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    let arena = Arena {
        spawns,
        pickups: Vec::new(),
        ..Default::default()
    };
    let players = (at.iter().enumerate())
        .map(|(i, &pos)| {
            let place = conker_rules::Placement {
                index: 0,
                pos,
                facing: 0x4100,
                camera: [0.0; 3],
            };
            let mut f = Fighter::new(&place, i as u8, tick);
            f.immune = 0;
            f
        })
        .collect();
    let mut m = Match::new(&arena, players, conker_rules::Rng::default(), tick);
    m.traps = conker_rules::traps::Traps::new(0x34, &rom.asset(&[0x0C, 0x34, 5]).unwrap());
    (arena, m)
}

/// V trap34 and elec34 (`dm-maps-results-scratch/out`): a player put on plate 6 (and one on plate
/// 9) at read 80 sets off the fire jets, whose first hit is in frame 83 (read 84: P2 in jet 0 at
/// health 0), the electric floor one frame later (frame 84, read 85); the touch kills and the
/// presser is credited. Fire jets hit 120 frames, the electric floor 151.
#[test]
fn a_plate_sets_off_its_traps() {
    use conker_rules::traps;
    use conker_sim::movement::Input;
    let Some(rom) = rom() else { return };
    let mut world = world_34(&rom);
    let at = [
        [-2575.0, -385.0, 4149.0],
        [-2587.0, -376.0, 3247.0],
        [-53.0, -32.0, 3326.0],
        [301.0, -399.0, 2624.0],
    ];
    let (arena, mut m) = match_34(&rom, &at, 80);
    let fire = (0..m.traps.list.len())
        .find(|&k| m.traps.list[k].contains(at[1], at[1][1]))
        .unwrap();
    let spark = (0..m.traps.list.len())
        .find(|&k| m.traps.list[k].contains(at[3], at[3][1]))
        .unwrap();
    assert!(
        !m.traps.list[fire].electric
            && m.traps.list[fire].plate == 0
            && m.traps.list[spark].electric
    );
    let (mut hit, mut on) = ([None; 4], [Vec::new(), Vec::new()]);
    while m.tick < 260 {
        let t = m.tick;
        for (k, v) in [fire, spark].into_iter().zip(&mut on) {
            if m.traps.hits(k) {
                v.push(t);
            }
        }
        traps::seat(&mut world, m.traps.offsets());
        m.step(&arena, &[Input::default(); 4], &world);
        for (i, h) in hit.iter_mut().enumerate() {
            if h.is_none() && m.players[i].health < 6 {
                *h = Some((t, m.players[i].health));
            }
        }
    }
    assert_eq!(hit, [None, Some((83, 0)), None, Some((84, 0))]);
    assert_eq!(
        m.players.iter().map(|f| f.kills).collect::<Vec<_>>(),
        [1, 0, 1, 0]
    );
    assert_eq!(
        (on[0].first(), on[0].last(), on[0].len()),
        (Some(&83), Some(&202), 120)
    );
    assert_eq!(
        (on[1].first(), on[1].last(), on[1].len()),
        (Some(&84), Some(&234), 151)
    );
    assert_eq!(m.traps.owner[fire], Some(0));
}

/// V cone34k: a player at health 1 put on cone 1 at read 40 takes its hit in frame 40 (health
/// 0, thrown up 55 at 7, set alight with no attacker) and hops off it; the death starts on the
/// normal floor where it lands (the fall, not 0x2D8).
#[test]
fn a_cone_throws_and_burns() {
    use conker_anim::combat as anim;
    use conker_sim::movement::Input;
    let Some(rom) = rom() else { return };
    let lab = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../lab");
    let Ok(text) = std::fs::read_to_string(format!(
        "{lab}/research/mp-original/dm-maps-results-scratch/out/cone34k/trace.csv"
    )) else {
        return;
    };
    let mut lines = text.lines();
    let header: std::collections::HashMap<&str, usize> =
        (lines.next().unwrap().split(',').enumerate())
            .map(|(i, h)| (h, i))
            .collect();
    let ys: BTreeMap<u32, f32> = (lines.map(|l| l.split(',').collect::<Vec<_>>()))
        .filter(|c| {
            c[header["mp"]] == "1" && c[header["player"]] == "1" && !c[header["x"]].is_empty()
        })
        .map(|c| {
            (
                c[header["read"]].parse().unwrap(),
                c[header["y"]].parse().unwrap(),
            )
        })
        .collect();
    let world = world_34(&rom);
    let (arena, mut m) = match_34(&rom, &[[-1164.0, 392.0, -2970.0]], 40);
    m.players[0].health = 1;
    let mut worst: f32 = 0.0;
    while m.tick < 140 {
        m.step(&arena, &[Input::default()], &world);
        let f = &m.players[0];
        if m.tick == 41 {
            assert_eq!((f.health, f.body.y_vel), (0, 55.0), "{:?}", f.body);
            assert!(f.burn.is_some_and(|b| b.attacker.is_none()));
        }
        // 0x69 to read 113 (I: the remake's ends a frame early), then the fall (0x58), not 0x2D8.
        let want = if m.tick <= 112 {
            anim::FLAME_HIT
        } else {
            anim::DEATH_FALL
        };
        assert_eq!(f.anim, want, "read {}", m.tick);
        worst = worst.max((f.body.pos[1] - ys[&m.tick]).abs());
    }
    eprintln!("cone34k: y within {worst} of the oracle's over reads 41-139");
    assert!(worst < 0.01);
}
