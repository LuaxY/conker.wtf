//! Checks against the real ROM (path in env `CONKER_ROM`); every test skips when it is unset.
//! Expected model counts are DevOldSchool/conkers-bfd-decomp's (MIT, e2400b27dda6,
//! `docs/evidence/assets/models/us_asset_inventory.md`), which count every G_TRI4 slot, so
//! they equal our triangles plus the skipped degenerate slots.
//! `dos_texture_pixels` also needs env `CONKER_DOS_TEXTURES` = DOS's `build/assets/textures`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use extract_core::{Rom, Table};
use extract_gbi::{Format, Image, Ir, Palette, Tlut, decode, flat_data, png, walk};

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

fn be32(b: &[u8], at: usize) -> usize {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap()) as usize
}

/// Segment 0 = model offsets, segment 1 = vertices relative to `model + vertex_base`.
fn walk_model(model: &[u8], dl: usize, vertex_base: usize) -> Ir {
    walk(dl as u32, |a| {
        let off = (a & 0xFF_FFFF) as usize;
        match a >> 24 {
            0 => model.get(off..),
            1 => model.get(vertex_base + off..),
            _ => None,
        }
    })
    .unwrap_or_else(|e| panic!("list at {dl:#x}: {e}"))
}

#[derive(Debug, Default)]
struct Totals {
    models: usize,
    source_vertices: usize,
    loaded_vertices: usize,
    triangles: usize,
    degenerate: usize,
    draws: usize,
    textured: usize,
    unresolved_seg8: usize,
    with_normal: usize,
    with_matrix: usize,
    skipped: BTreeMap<u8, u32>,
}

impl Totals {
    fn add(&mut self, ir: &Ir) {
        self.loaded_vertices += ir.vertices.len();
        self.with_normal += ir.vertices.iter().filter(|v| v.normal.is_some()).count();
        self.with_matrix += ir.vertices.iter().filter(|v| v.matrix.is_some()).count();
        self.triangles += ir.draws.iter().map(|d| d.triangles.len()).sum::<usize>();
        self.textured += ir
            .draws
            .iter()
            .filter(|d| d.material.texture.is_some())
            .map(|d| d.triangles.len())
            .sum::<usize>();
        self.degenerate += ir.degenerate as usize;
        self.draws += ir.draws.len();
        self.unresolved_seg8 += ir.unresolved.iter().filter(|a| *a >> 24 == 8).count();
        for (&op, &n) in &ir.skipped {
            *self.skipped.entry(op).or_default() += n;
        }
    }

    /// A 40-byte direct model (banks 03, 04): DL offset and size in words 0-1, vertices from 0x28.
    fn add_direct(&mut self, model: &[u8]) {
        let dl = be32(model, 0);
        self.models += 1;
        self.source_vertices += (dl - 0x28) / 16;
        self.add(&walk_model(model, dl, 0x28));
    }
}

#[test]
fn bank03_direct_models() {
    let Some(rom) = rom() else { return };
    let bank = rom.bank(3).unwrap();
    let mut t = Totals::default();
    for i in 0..bank.entries.len() {
        let model = bank.payload(i).unwrap();
        if !model.is_empty() {
            t.add_direct(&model);
        }
    }
    eprintln!("bank 03: {t:?}");
    assert_eq!((t.models, t.source_vertices), (77, 5_062));
    assert_eq!(t.triangles + t.degenerate, 3_894);
    assert_eq!(t.unresolved_seg8, 61);
    assert!(
        t.skipped.is_empty(),
        "uninterpreted opcodes {:x?}",
        t.skipped
    );
}

#[test]
fn bank04_scene_bundles() {
    let Some(rom) = rom() else { return };
    let bank = rom.bank(4).unwrap();
    let (mut bundles, mut segments) = (0, 0);
    let mut t = Totals::default();
    for i in 0..bank.entries.len() {
        let bundle = bank.payload(i).unwrap();
        if bundle.is_empty() {
            continue;
        }
        let table = Table::parse(&bundle).unwrap();
        bundles += 1;
        segments += table.entries.len();
        for j in 0..table.entries.len() {
            let model = table.raw(j).unwrap();
            if !model.is_empty() {
                t.add_direct(model);
            }
        }
    }
    eprintln!("bank 04: {bundles} bundles, {segments} segments, {t:?}");
    assert_eq!((bundles, segments, t.models), (59, 852, 765));
    assert_eq!(t.source_vertices, 209_274);
    assert_eq!(t.triangles + t.degenerate, 147_723);
    assert!(
        t.skipped.is_empty(),
        "uninterpreted opcodes {:x?}",
        t.skipped
    );
}

#[test]
fn bank01_characters() {
    let Some(rom) = rom() else { return };
    let bank = rom.bank(1).unwrap();
    let mut t = Totals::default();
    let mut lists = 0;
    for i in 0..bank.entries.len() {
        let model = bank.payload(i).unwrap();
        if model.is_empty() {
            continue;
        }
        // 0x38-byte header of 7 (offset, size) pairs; vertices from 0x38, addressed from the
        // model start; display-list pointer tables in pairs 1 and 5, each list callable alone.
        let w: Vec<usize> = (0..14).map(|k| be32(&model, k * 4)).collect();
        t.models += 1;
        t.source_vertices += ([w[0], w[2]].into_iter().find(|&o| o != 0).unwrap() - 0x38) / 16;
        for (table, size) in [(w[2], w[3]), (w[10], w[11])] {
            for k in (0..size).step_by(4).take_while(|_| table != 0) {
                lists += 1;
                t.add(&walk_model(&model, be32(&model, table + k), 0));
            }
        }
    }
    eprintln!("bank 01: {lists} lists, {t:?}");
    assert_eq!((t.models, t.source_vertices), (183, 56_412));
    assert_eq!(t.triangles + t.degenerate, 62_073);
    assert!(t.with_matrix > 0 && t.with_normal > 0);
}

#[test]
fn texture_0xbb_from_its_display_list() {
    let Some(rom) = rom() else { return };
    let bank = rom.bank(3).unwrap();
    let material = (0..bank.entries.len())
        .find_map(|i| {
            let model = bank.payload(i).unwrap();
            if model.is_empty() {
                return None;
            }
            let ir = walk_model(&model, be32(&model, 0), 0x28);
            ir.draws.iter().map(|d| d.material).find(|m| {
                m.texture.and_then(|t| t.image)
                    == Some(Image::Flat {
                        index: 0xBB,
                        mode: 0,
                    })
            })
        })
        .expect("a bank-03 draw samples texture 0xBB");
    let texture = material.texture.unwrap();
    assert_eq!(material.texture_format(), Some(Format::Ci4(Tlut::Rgba16)));
    assert_eq!(texture.size(), [64, 64]);
    assert!(texture.interleaved);
    assert_eq!(
        texture.palette,
        Some(Palette {
            image: Image::Flat {
                index: 0xBB,
                mode: 2
            },
            offset: 0
        })
    );

    let payload = rom.textures().unwrap().get(0xBB).unwrap();
    assert_eq!(payload.len(), 2080);
    let tlut = flat_data(&payload, 2).unwrap();
    assert_eq!((tlut, tlut.len()), (&payload[0x800..], 32));
    let rgba = decode(Format::Ci4(Tlut::Rgba16), 64, 64, &payload, tlut, true).unwrap();
    assert_eq!(rgba.len(), 64 * 64 * 4);
    assert!(png(64, 64, &rgba).unwrap().starts_with(b"\x89PNG"));
}

/// RGBA8 of a PNG, with rows flipped back to memory order (DOS writes top-left origin).
fn read_png_flipped(path: &std::path::Path) -> (usize, usize, Vec<u8>) {
    let mut decoder =
        png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("{path:?}: unexpected {other:?}"),
    };
    let rows: Vec<&[u8]> = rgba.chunks(w * 4).rev().collect();
    (w, h, rows.concat())
}

#[test]
fn dos_texture_pixels() {
    let Some(rom) = rom() else { return };
    let Some(root) = std::env::var_os("CONKER_DOS_TEXTURES") else {
        eprintln!("CONKER_DOS_TEXTURES unset: skipping");
        return;
    };
    let root = std::path::PathBuf::from(root);
    let textures = rom.textures().unwrap();
    let mut compared = BTreeMap::<&str, usize>::new();
    // Format and size our walker derives for each flat texture drawn by banks 03 and 04.
    let mut drawn = BTreeMap::<usize, Vec<(Format, [u32; 2])>>::new();
    let mut add = |model: &[u8]| {
        if model.is_empty() {
            return;
        }
        for d in walk_model(model, be32(model, 0), 0x28).draws {
            let (Some(t), Some(f)) = (d.material.texture, d.material.texture_format()) else {
                continue;
            };
            if let Some(Image::Flat { index, mode: 0 }) = t.image {
                drawn.entry(index as usize).or_default().push((f, t.size()));
            }
        }
    };
    let bank = rom.bank(3).unwrap();
    for i in 0..bank.entries.len() {
        add(&bank.payload(i).unwrap());
    }
    let bank = rom.bank(4).unwrap();
    for i in 0..bank.entries.len() {
        let bundle = bank.payload(i).unwrap();
        if let Ok(table) = Table::parse(&bundle) {
            for j in 0..table.entries.len() {
                add(table.raw(j).unwrap());
            }
        }
    }
    let (mut agree, mut disagree) = (0, Vec::new());
    for family in [
        "us",
        "us-ci8-proven",
        "us-rgba16-proven",
        "us-native-proven",
    ] {
        let dir = root.join(family);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("manifest.json")).unwrap()).unwrap();
        for entry in manifest["textures"].as_array().unwrap() {
            let get = |k: &str| entry[k].as_u64().or(manifest[k].as_u64()).unwrap() as usize;
            let (ordinal, w, h) = (get("flat_index"), get("width"), get("height"));
            // DOS numbers its texture families by stream ordinal; runtime ids keep the two
            // empty slots 1767 and 1768 (DOS `rzip_archive.py` `iter_indexed_flat_rzip_entries`).
            let index = if ordinal >= 1767 {
                ordinal + 2
            } else {
                ordinal
            };
            if entry
                .get("preview_width")
                .is_some_and(|p| p.as_u64() != Some(w as u64))
            {
                continue; // DOS transposed the preview; nothing to compare pixel-for-pixel
            }
            let payload = textures.get(index).unwrap();
            let (format, tlut): (Format, &[u8]) = match family {
                "us" => (Format::Ci4(Tlut::Rgba16), flat_data(&payload, 2).unwrap()),
                "us-ci8-proven" => {
                    let tlut = flat_data(&payload, 1).unwrap();
                    assert_eq!(tlut, &payload[get("palette_offset")..][..512], "{index}");
                    (Format::Ci8(Tlut::Rgba16), tlut)
                }
                "us-rgba16-proven" => (Format::Rgba16, &[]),
                _ => match entry["format"].as_str().unwrap() {
                    "rgba32" => (Format::Rgba32, &[]),
                    "ia8" => (Format::Ia8, &[]),
                    "ia16" => (Format::Ia16, &[]),
                    "i4" => (Format::I4, &[]),
                    "i8" => (Format::I8, &[]),
                    f => panic!("format {f}"),
                },
            };
            let interleaved = entry["row_layout"] != "linear";
            let ours = decode(format, w, h, &payload, tlut, interleaved).unwrap();
            let file = dir.join(entry["file"].as_str().unwrap());
            let (dw, dh, theirs) = read_png_flipped(&file);
            assert_eq!((dw, dh), (w, h), "{file:?}");
            // 5-bit channels: we replicate bits, DOS rounds v * 255 / 31 down (<= 1 apart).
            let off = ours
                .iter()
                .zip(&theirs)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(off <= 1, "{file:?}: channels differ by {off}");
            *compared.entry(family).or_default() += 1;
            if let Some(seen) = drawn.get(&index) {
                if seen.contains(&(format, [w as u32, h as u32])) {
                    agree += 1;
                } else {
                    disagree.push((index, format, w, h, seen[0]));
                }
            }
        }
    }
    eprintln!("compared with DOS: {compared:?}");
    eprintln!(
        "walker format+size: {agree} agree, {} differ: {disagree:?}",
        disagree.len()
    );
    assert_eq!(compared.values().sum::<usize>(), 704 + 471 + 6 + 91);
    // The 10 differences, measured 2026-10-08: 1260, 3994, 4000 are drawn as CI4 128x32 and 737
    // with 64x32 bounds (DOS assumes 64x64 from the 2080-byte size). The other six are ids
    // >= 1767, where DOS's DL-derived format and size belong to the id two lower (its ordinal):
    // our walker draws 2259, 4423, 7223, 7224 exactly as DOS describes 2261, 4425, 7225, 7226.
    assert_eq!((agree, disagree.len()), (966, 10));
}
