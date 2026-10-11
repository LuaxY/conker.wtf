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
