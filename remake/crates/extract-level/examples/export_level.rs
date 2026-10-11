//! Writes one scene's level geometry as glTF for inspection:
//!
//! `CONKER_ROM=<rom> cargo run -p extract-level --example export_level -- <out dir> [scene]`
//!
//! `scene` is hex (default 0x2D). Output: `level-<scene>.gltf`, `level-<scene>.bin` and
//! `textures/*.png`.

use extract_core::Rom;
use extract_level::{primitive, scene_level, to_gltf};

const USAGE: &str = "usage: export_level <out dir> [scene]";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::PathBuf::from(args.first().ok_or(USAGE)?);
    let scene = match args.get(1) {
        Some(s) => usize::from_str_radix(s.trim_start_matches("0x"), 16)?,
        None => 0x2D,
    };
    let rom = Rom::new(std::fs::read(std::env::var_os("CONKER_ROM").ok_or(USAGE)?)?)?;
    let level = scene_level(&rom, scene)?;
    let stem = format!("level-{scene:02x}");
    let gltf = to_gltf(&level, &rom.textures()?, &format!("{stem}.bin"))?;

    std::fs::create_dir_all(out.join("textures"))?;
    std::fs::write(out.join(format!("{stem}.gltf")), &gltf.json)?;
    std::fs::write(out.join(format!("{stem}.bin")), &gltf.bin)?;
    for (path, png) in &gltf.images {
        std::fs::write(out.join(path), png)?;
    }

    println!(
        "scene {scene:#04x}: {} instances, {} meshes, {} missing placements {:?}",
        level.instances.len(),
        level.meshes.len(),
        level.missing.len(),
        level.missing
    );
    for mesh in &level.meshes {
        let prims: Vec<_> = mesh
            .ir
            .draws
            .iter()
            .map(|d| primitive(&mesh.ir, d))
            .collect();
        let uses = level
            .instances
            .iter()
            .filter(|i| std::ptr::eq(&level.meshes[i.mesh], mesh))
            .count();
        println!(
            "  {:<10} x{uses}: {} draws, {} vertices, {} faces, {} unresolved calls",
            mesh.name(),
            mesh.ir.draws.len(),
            prims.iter().map(|p| p.positions.len()).sum::<usize>(),
            prims.iter().map(|p| p.indices.len() / 3).sum::<usize>(),
            mesh.ir.unresolved.len(),
        );
    }
    println!(
        "{} PNGs, {} bytes of buffer",
        gltf.images.len(),
        gltf.bin.len()
    );
    for w in &gltf.warnings {
        println!("warning: {w}");
    }
    Ok(())
}
