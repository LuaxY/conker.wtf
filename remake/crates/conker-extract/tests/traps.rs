//! Scene 0x34's traps in the pack (`conker_extract::traps`; the ROM's path in env `CONKER_ROM`, the
//! test skips when it is unset).

use std::collections::{BTreeMap, BTreeSet};

use conker_assets::audio::Audio;
use extract_core::Rom;

/// With map 34 the electric floor's sound 0x236 is in its folder, the fire jets' 0x1AA stays in
/// `common` (the flamethrower's too), and the traps' three textures are PNGs of their sizes.
#[test]
fn pack_has_the_traps() {
    let Some(path) = std::env::var_os("CONKER_ROM") else {
        eprintln!("CONKER_ROM unset: skipping");
        return;
    };
    let rom = Rom::new(std::fs::read(path).unwrap()).unwrap();
    let maps = [("34".to_string(), None, None, None)];
    let (files, folders) =
        conker_extract::audio::pack_audio(&rom, &BTreeSet::new(), &maps, &[], &[]).unwrap();
    let files: BTreeMap<&str, &[u8]> = (files.iter())
        .map(|(p, d)| (p.as_str(), d.as_slice()))
        .collect();
    let audio = Audio::load(|p: &str| files.get(p).map(|d| d.to_vec()).ok_or("missing")).unwrap();
    let file = |id: u16| audio.sounds.sounds[&id].file.as_str();
    assert_eq!(
        (file(0x236), file(0x1AA)),
        ("maps/34/0566.wav", "common/0426.wav")
    );
    assert!(files.contains_key("audio/maps/34/0566.wav"));
    assert!(folders.contains(&"maps/34".to_string()));

    let pngs = conker_extract::traps::textures(&rom).unwrap();
    let sizes: Vec<(&str, [u32; 2])> = (pngs.iter())
        .map(|(path, png)| {
            let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
            (path.as_str(), [word(16), word(20)])
        })
        .collect();
    assert_eq!(
        sizes,
        [
            ("maps/34/traps/flame.png", [32, 32]),
            ("maps/34/traps/puff.png", [16, 16]),
            ("maps/34/traps/arc.png", [16, 16])
        ]
    );
}
