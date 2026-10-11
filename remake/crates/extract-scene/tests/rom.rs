//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.
//! Expected totals are DevOldSchool/conkers-bfd-decomp's (e2400b27dda6) for the US ROM.

use extract_core::Rom;
use extract_scene::{
    Layer, floor_below, parse_terrain, scene_collision, scene_placements, scene_spawns,
};
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// Entries in each per-scene bank (scene ids 0..0x45).
const SCENES: usize = 69;

fn rom() -> Option<&'static Rom> {
    static ROM: OnceLock<Option<Rom>> = OnceLock::new();
    let rom = ROM.get_or_init(|| {
        let path = std::env::var_os("CONKER_ROM")?;
        Some(
            Rom::new(std::fs::read(path).expect("read CONKER_ROM"))
                .expect("CONKER_ROM is the US ROM"),
        )
    });
    if rom.is_none() {
        eprintln!("CONKER_ROM unset: skipping");
    }
    rom.as_ref()
}

#[test]
fn placement_totals() {
    let Some(rom) = rom() else { return };
    let (mut dispatched, mut direct, mut direct_scenes, mut any_scenes) = (0, 0, 0, 0);
    let (mut included, mut excluded, mut unresolved, mut placement_faces) = (0, 0, 0, 0);
    for scene in 0..SCENES {
        let placements = scene_placements(rom, scene).unwrap();
        let d = placements.iter().filter(|p| p.dispatch == 0).count();
        dispatched += placements.len() - d;
        direct += d;
        direct_scenes += (d > 0) as usize;
        any_scenes += !placements.is_empty() as usize;
        assert!(
            placements
                .iter()
                .all(|p| p.dispatch <= 2 && p.scale.iter().all(|s| s.is_finite()))
        );
        included += placements.iter().filter(|p| p.has_collision()).count();
        excluded += placements.iter().filter(|p| !p.has_collision()).count();
        let collision = scene_collision(rom, scene, &placements).unwrap();
        unresolved += collision.unresolved.len();
        let faces =
            |l: fn(Layer) -> bool| collision.triangles.iter().filter(|t| l(t.layer)).count();
        placement_faces += faces(|l| matches!(l, Layer::Placement(_)));
        if !collision.unresolved.is_empty() {
            eprintln!(
                "scene {scene}: unresolved placements {:?}",
                collision.unresolved
            );
        }
    }
    eprintln!("placements in {any_scenes} scenes, direct ones in {direct_scenes}");
    assert_eq!((dispatched, direct), (727, 511));
    assert_eq!(direct_scenes, 48);
    assert_eq!((included, excluded), (1066, 172));
    assert_eq!(unresolved, 11);
    assert_eq!(placement_faces, 42_901);
}

#[test]
fn terrain_totals() {
    let Some(rom) = rom() else { return };
    let (mut primary, mut secondary, mut faces, mut secondary_faces) = (0, 0, 0, 0);
    let (mut words, mut distinct) = (0, BTreeSet::<u32>::new());
    let mut without_table = Vec::new();
    for scene in 0..SCENES {
        let Some(t) = parse_terrain(&rom.asset(&[0x04, scene]).unwrap()).unwrap() else {
            continue;
        };
        let Some(surfaces) = &t.surfaces else {
            // The game still builds collision from these; DevOldSchool's totals leave them out.
            without_table.push((
                scene,
                t.mesh.faces.len(),
                t.secondary.map_or(0, |s| s.faces.len()),
            ));
            continue;
        };
        primary += 1;
        faces += t.mesh.faces.len();
        words += surfaces.len();
        distinct.extend(surfaces);
        if let Some(s) = &t.secondary {
            secondary += 1;
            secondary_faces += s.faces.len();
        }
    }
    eprintln!("bundles without a surface table (scene, faces, secondary faces): {without_table:?}");
    assert_eq!((primary, secondary), (58, 38));
    assert_eq!((faces, secondary_faces), (97_071, 1_408));
    assert_eq!(faces + secondary_faces, 98_479);
    assert_eq!((words, distinct.len()), (97_071, 176));
}

#[test]
fn deathmatch_scene_0x2d() {
    let Some(rom) = rom() else { return };
    let placements = scene_placements(rom, 0x2D).unwrap();
    assert_eq!(placements.len(), 12 + 34); // D_800DBEF0 in oracle RAM
    let spawns = scene_spawns(rom, 0x2D).unwrap();
    assert_eq!(spawns.len(), 20); // D_800D2128 in oracle RAM
    let collision = scene_collision(rom, 0x2D, &placements).unwrap();
    let count = |l: fn(Layer) -> bool| collision.triangles.iter().filter(|t| l(t.layer)).count();
    assert_eq!(count(|l| l == Layer::Terrain), 1751);
    assert_eq!(count(|l| l == Layer::Secondary), 8);
    assert_eq!(count(|l| matches!(l, Layer::Placement(_))), 242 + 2298);
    // Every spawn point is on a floor: up to 4 units above it or slightly below a slope.
    for s in spawns {
        let p = s.position.map(f32::from);
        let (_, y) = floor_below(&collision.triangles, p, 2.0).expect("spawn has a floor");
        assert!(
            p[1] - y <= 4.0,
            "spawn {p:?} is {} above its floor",
            p[1] - y
        );
    }
}
