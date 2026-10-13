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
