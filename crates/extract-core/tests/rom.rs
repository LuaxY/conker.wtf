//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.
//! Expected numbers are the ones `lab/research/assets-probe` measured on the US ROM.

use extract_core::{BANK_COUNT, Rom, TEXTURE_COUNT};
use sha1::Digest;
use std::sync::OnceLock;

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
fn asset_table_and_banks() {
    let Some(rom) = rom() else { return };
    let top = rom.asset_table().unwrap();
    assert_eq!(top.entries.len(), BANK_COUNT + 1);
    let extra = top.entries[BANK_COUNT];
    assert!(extra.is_last() && !extra.is_compressed());
    assert_eq!(extra.size(), 0x90);

    let counts: Vec<_> = (0..BANK_COUNT)
        .map(|b| rom.bank(b).unwrap().entries.len())
        .collect();
    assert_eq!(
        counts,
        [
            69, 187, 187, 114, 69, 138, 69, 1, 5, 483, 29, 69, 69, 7, 69, 187, 69, 187, 187, 2,
            204, 69, 462, 7, 1, 69, 5, 4, 5
        ]
    );
    for table in std::iter::once(top).chain((0..BANK_COUNT).map(|b| rom.bank(b).unwrap())) {
        let last: Vec<_> = table.entries.iter().map(|e| e.is_last()).collect();
        assert_eq!(last.iter().filter(|&&l| l).count(), 1);
        assert!(last[last.len() - 1]);
    }
}

#[test]
fn every_bank_entry_unpacks() {
    let Some(rom) = rom() else { return };
    let (mut compressed, mut total) = (0, 0);
    for b in 0..BANK_COUNT {
        let bank = rom.bank(b).unwrap();
        for i in 0..bank.entries.len() {
            // payload() fails unless an rzip entry inflates to exactly its stated size.
            total += bank.payload(i).unwrap().len();
            compressed += bank.entries[i].is_compressed() as usize;
        }
    }
    assert_eq!(compressed, 1670);
    assert_eq!(total, 60_507_058);
}

#[test]
fn nested_tables() {
    let Some(rom) = rom() else { return };
    let text = rom.asset(&[0x1C, 0]).unwrap();
    let table = extract_core::Table::parse(&text).unwrap();
    assert_eq!(table.entries.len(), 4);
    assert_eq!(table.payload(0).unwrap().len(), 2444); // nested rzip menu strings
    assert_eq!(rom.asset(&[0x1C, 0, 0]).unwrap().len(), 2444);
    assert_eq!(rom.asset(&[0x0C, 0x1A, 2]).unwrap().len(), 13 * 0x44); // Heist placements
    assert_eq!(rom.asset(&[0x04, 0x1A]).unwrap().len(), 72_040); // Heist bundle
}

#[test]
fn textures() {
    let Some(rom) = rom() else { return };
    let tex = rom.textures().unwrap();
    assert_eq!(tex.len(), TEXTURE_COUNT);
    let mut empty = 0;
    for id in 0..tex.len() {
        empty += tex.get(id).unwrap().is_empty() as usize;
    }
    assert_eq!(empty, 2);
    for (id, len) in [(0, 1536), (1, 4096), (2, 120), (0xBB, 2080)] {
        assert_eq!(tex.get(id).unwrap().len(), len, "texture {id:#x}");
    }
    // The size table covers the blob exactly up to 0xAB1941 (15 padding bytes before the asset table).
    let data = rom.game_data().unwrap();
    let sizes = &data[0xF200..0xF200 + TEXTURE_COUNT * 2];
    let total: usize = sizes
        .chunks(2)
        .map(|s| u16::from_be_bytes([s[0], s[1]]) as usize)
        .sum();
    assert_eq!(0x1A37E0 + total, 0xAB1941);
}

#[test]
fn game_code_and_data() {
    let Some(rom) = rom() else { return };
    let (code, data) = (rom.game_code().unwrap(), rom.game_data().unwrap());
    assert_eq!(code.len(), 2_072_880);
    assert_eq!(data.len(), 0x2E2A0);
    // The flat image N64Recomp reads (unpack_rom.py's IMAGE_SHA1).
    let b = rom.bytes();
    let image = [&b[..0x2D4B0], &code, &data, &b[0x19EA88..0x1A37E0]].concat();
    let sha1: String = sha1::Sha1::digest(&image)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    assert_eq!(sha1, "842e3d348e3c8ae0039e2ab367ad492f9b5266d8");
}

#[test]
fn accepts_swapped_byte_orders() {
    let Some(rom) = rom() else { return };
    let mut v64 = rom.bytes().to_vec();
    v64.as_chunks_mut::<2>()
        .0
        .iter_mut()
        .for_each(|w| w.swap(0, 1));
    assert_eq!(Rom::new(v64).unwrap().bytes(), rom.bytes());
    let mut n64 = rom.bytes().to_vec();
    n64.as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(|w| w.reverse());
    assert_eq!(Rom::new(n64).unwrap().bytes(), rom.bytes());
}
