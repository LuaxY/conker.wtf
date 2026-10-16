//! Times [`Slice::from_rom`] and writes its GLBs, for glTF validators and viewers:
//! `CONKER_ROM=<rom> cargo run --release -p conker-extract --example slice_glb [-- <out dir>]`
//! (default `<repo>/lab/extract/slice-glb`, outside the source tree).

use std::time::Instant;

use conker_extract::Slice;

fn main() -> anyhow::Result<()> {
    let rom = std::fs::read(std::env::var_os("CONKER_ROM").expect("set CONKER_ROM"))?;
    let out = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../lab/extract/slice-glb").into()
    });
    let start = Instant::now();
    let slice = Slice::from_rom(rom)?;
    println!(
        "Slice::from_rom: {:.0} ms ({} collision faces, {} spawns, {} clips)",
        start.elapsed().as_secs_f64() * 1e3,
        slice.collision.len(),
        slice.spawns.len(),
        slice.clips.len()
    );
    std::fs::create_dir_all(&out)?;
    for (name, glb) in [
        ("level-2d.glb", &slice.level_glb),
        ("character-117.glb", &slice.character_glb),
        ("held.glb", &slice.held_glb),
    ] {
        std::fs::write(format!("{out}/{name}"), glb)?;
        println!("{out}/{name}: {} bytes", glb.len());
    }
    Ok(())
}
