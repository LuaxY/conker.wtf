//! conker-extract CLI: ROM to inspectable files (see `USAGE`).

use std::path::PathBuf;

use anyhow::{Context, bail};
use extract_core::{ASSET_TABLE, BANK_COUNT, Rom};

const USAGE: &str = "usage: conker-extract [--rom <path>] [--out <dir>] [--force] <command>
commands:
  list                  bank table summary (printed, nothing written)
  textures              every display-list-drawn texture as PNG -> <out>/textures/
  scene <id>            placements, spawns, collision -> <out>/scene-<id>/
  level <id>            level glTF + textures -> <out>/level-<id>/
  character <name|id>   bank-01 character glTF + clips -> <out>/character-<id>/
  slice                 scene 0x2D + level 0x2D + character 117 (the vertical slice)
  pack                  the asset pack the server and the page load -> <out> (default <repo>/assets);
                        refuses to overwrite an existing pack (its pack.json) without --force;
                        --map <name>: a map, the scene in hex as its folder (repeatable; default 2d);
                        --background [<name>=]<wav>: a map's background recording (lab/audio), the
                        first map's without a name (repeatable); --song [<name>=]<wav>: the same for
                        the song its outro starts (op 8); --stem [<name>=]<wav>: the same for a raw
                        recording of its song's positional channels (0x2B's 13/14), cut as its loop
ids are decimal or 0x-prefixed hex (scene 0x2D = scene 45); character names: deathmatch (117), conker (0)
--rom defaults to env CONKER_ROM; --out defaults to <repo>/lab/extract/slice (outside the source tree)";

/// `<repo>/lab/extract/slice`: ROM-derived files never go into the source tree.
const DEFAULT_OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../lab/extract/slice");
/// `<repo>/assets`, next to the source tree (`<repo>/remake`), never in it.
const PACK_OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../assets");
const SOURCE_TREE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

#[derive(Debug, PartialEq)]
enum Command {
    List,
    Textures,
    Scene(usize),
    Level(usize),
    Character(usize),
    Slice,
    Pack,
}

/// Decimal, or hex with a `0x` prefix.
fn parse_id(s: &str) -> anyhow::Result<usize> {
    Ok(
        match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(hex) => usize::from_str_radix(hex, 16)?,
            None => s.parse()?,
        },
    )
}

fn parse_command(words: &[&str]) -> anyhow::Result<Command> {
    Ok(match words {
        ["list"] => Command::List,
        ["textures"] => Command::Textures,
        ["scene", id] => Command::Scene(parse_id(id)?),
        ["level", id] => Command::Level(parse_id(id)?),
        ["character", "deathmatch"] => Command::Character(conker_extract::SLICE_CHARACTER),
        ["character", "conker"] => Command::Character(0),
        ["character", id] => Command::Character(parse_id(id)?),
        ["slice"] => Command::Slice,
        ["pack"] => Command::Pack,
        _ => bail!("unknown command {words:?}"),
    })
}

fn main() -> anyhow::Result<()> {
    let mut rom_path = std::env::var_os("CONKER_ROM").map(PathBuf::from);
    let (mut out, mut force, mut maps, mut backgrounds) = (None, false, Vec::new(), Vec::new());
    let (mut songs, mut stems) = (Vec::new(), Vec::new());
    let mut words = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rom" => rom_path = Some(args.next().context(USAGE)?.into()),
            "--out" => out = Some(PathBuf::from(args.next().context(USAGE)?)),
            "--force" => force = true,
            "--map" => maps.push(usize::from_str_radix(&args.next().context(USAGE)?, 16)?),
            "--background" => backgrounds.push(args.next().context(USAGE)?),
            "--song" => songs.push(args.next().context(USAGE)?),
            "--stem" => stems.push(args.next().context(USAGE)?),
            _ => words.push(arg),
        }
    }
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    let command = parse_command(&words).with_context(|| USAGE)?;
    let out = match (out, &command) {
        (Some(out), _) => out,
        (None, Command::Pack) => PathBuf::from(PACK_OUT),
        (None, _) => PathBuf::from(DEFAULT_OUT),
    };
    if command == Command::Pack {
        // The pack is the user's to edit: never overwrite one by accident, and it is ROM-derived:
        // never write it into the source tree.
        let manifest = out.join(conker_assets::MANIFEST);
        if manifest.exists() && !force {
            bail!(
                "{} exists: there is a pack there (maybe edited); --force overwrites it",
                std::fs::canonicalize(&manifest)?.display()
            );
        }
        let mut existing = std::path::absolute(&out)?;
        while !existing.exists() {
            existing.pop();
        }
        if std::fs::canonicalize(existing)?.starts_with(std::fs::canonicalize(SOURCE_TREE)?) {
            bail!(
                "{}: the pack is ROM-derived and stays out of the source tree",
                out.display()
            );
        }
    }
    let started = std::time::Instant::now();
    let rom_path = rom_path.context(USAGE)?;
    let rom = Rom::new(
        std::fs::read(&rom_path).with_context(|| format!("reading {}", rom_path.display()))?,
    )?;

    let export = match command {
        Command::List => return list(&rom),
        Command::Textures => conker_extract::textures(&rom)?,
        Command::Scene(id) => conker_extract::scene(&rom, id)?,
        Command::Level(id) => conker_extract::level(&rom, id)?,
        Command::Character(id) => conker_extract::character(&rom, id)?,
        Command::Slice => conker_extract::slice(&rom)?,
        Command::Pack => {
            if maps.is_empty() {
                maps.push(conker_extract::SLICE_SCENE);
            }
            let recordings = |args: &[String],
                              flag: &str|
             -> anyhow::Result<Vec<Option<Vec<u8>>>> {
                let mut wavs = vec![None; maps.len()];
                for arg in args {
                    let (i, path) = match arg.split_once('=') {
                        Some((name, path)) => (
                            (maps.iter().map(|&m| conker_extract::map_name(m)))
                                .position(|m| m == name)
                                .with_context(|| format!("{flag} {arg}: no --map {name}"))?,
                            path,
                        ),
                        None => (0, arg.as_str()),
                    };
                    wavs[i] = Some(std::fs::read(path).with_context(|| format!("reading {path}"))?);
                }
                Ok(wavs)
            };
            let wavs = recordings(&backgrounds, "--background")?;
            let songs = recordings(&songs, "--song")?;
            let stems = recordings(&stems, "--stem")?;
            let maps: Vec<_> = (maps.iter().enumerate())
                .map(|(i, &scene)| conker_extract::PackMap {
                    scene,
                    background: wavs[i].as_deref(),
                    song: songs[i].as_deref(),
                    stem: stems[i].as_deref(),
                })
                .collect();
            conker_extract::pack(&rom, &maps)?
        }
    };
    let mut bytes = 0;
    for (path, data) in &export.files {
        let path = out.join(path);
        std::fs::create_dir_all(path.parent().expect("joined paths have a parent"))?;
        std::fs::write(&path, data).with_context(|| format!("writing {}", path.display()))?;
        bytes += data.len();
    }
    for line in &export.summary {
        println!("{line}");
    }
    println!(
        "wrote {} files, {bytes} bytes in {:.2} s -> {}",
        export.files.len(),
        started.elapsed().as_secs_f64(),
        std::fs::canonicalize(&out)?.display()
    );
    Ok(())
}

fn list(rom: &Rom) -> anyhow::Result<()> {
    let top = rom.asset_table()?;
    println!("bank  rom offset  entries  rzip   raw  empty");
    for b in 0..BANK_COUNT {
        let bank = rom.bank(b)?;
        let e = &bank.entries;
        let rzip = e.iter().filter(|e| e.is_compressed()).count();
        let empty = e.iter().filter(|e| e.size() == 0).count();
        let offset = ASSET_TABLE + top.entries[b].offset as usize;
        println!(
            "{b:02X}    {offset:#010X}  {:7}  {rzip:4}  {:4}  {empty:5}",
            e.len(),
            e.len() - rzip - empty
        );
    }
    for (i, e) in top.entries.iter().enumerate().skip(BANK_COUNT) {
        println!(
            "{i:02X}    {:#010X}  raw item, {:#x} bytes",
            ASSET_TABLE + e.offset as usize,
            e.size()
        );
    }
    println!("textures: {}", rom.textures()?.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_decimal_or_prefixed_hex() {
        assert_eq!(parse_id("45").unwrap(), 0x2D);
        assert_eq!(parse_id("0x2D").unwrap(), 0x2D);
        assert_eq!(parse_id("0X2d").unwrap(), 0x2D);
        assert!(parse_id("2D").is_err());
        assert!(parse_id("0x").is_err());
    }

    #[test]
    fn commands() {
        assert_eq!(parse_command(&["list"]).unwrap(), Command::List);
        assert_eq!(
            parse_command(&["scene", "0x2D"]).unwrap(),
            Command::Scene(0x2D)
        );
        assert_eq!(
            parse_command(&["level", "45"]).unwrap(),
            Command::Level(0x2D)
        );
        assert_eq!(
            parse_command(&["character", "deathmatch"]).unwrap(),
            Command::Character(117)
        );
        assert_eq!(
            parse_command(&["character", "conker"]).unwrap(),
            Command::Character(0)
        );
        assert_eq!(
            parse_command(&["character", "117"]).unwrap(),
            Command::Character(117)
        );
        assert_eq!(parse_command(&["slice"]).unwrap(), Command::Slice);
        assert_eq!(parse_command(&["pack"]).unwrap(), Command::Pack);
        for bad in [
            &[][..],
            &["scene"],
            &["scene", "x"],
            &["list", "extra"],
            &["bogus"],
        ] {
            assert!(parse_command(bad).is_err(), "{bad:?}");
        }
    }
}
