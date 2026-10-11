//! Client entry point. Native: options from CLI flags, the asset pack from `--assets DIR` (or
//! `CONKER_ASSETS`, else `../assets`), headless or windowed (prediction needs the pack's collision
//! either way) and its HUD (`hud/hud.json`). Browser: options from the page URL query; [`start`]
//! fetches the pack's data files from `assets/` next to the page, where Bevy loads its glTF models
//! and PNGs from too.

use conker_assets::audio::Audio;
use conker_assets::hud::Hud;
use conker_assets::{MANIFEST, PACK_CHARACTERS, PACK_MAP, PACK_WEAPONS, Slice};

#[cfg(not(target_family = "wasm"))]
fn main() {
    let fail = |msg: String| -> ! {
        eprintln!("conker-client: {msg}");
        std::process::exit(1)
    };
    let pairs = conker_client::arg_pairs(std::env::args().skip(1));
    let mut cfg = conker_client::Config::from_pairs(pairs).unwrap_or_else(|e| fail(e));
    let dir = (cfg.assets.clone())
        .or_else(|| std::env::var("CONKER_ASSETS").ok())
        .unwrap_or_else(|| "../assets".into());
    let dir = std::fs::canonicalize(&dir).unwrap_or_else(|e| {
        fail(format!(
            "asset pack {dir}: {e} (export one: cargo run --release -p conker-extract -- pack; \
             or pass --assets DIR)"
        ))
    });
    let slice = Slice::load(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS, |p| {
        std::fs::read(dir.join(p))
    })
    .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    let hud = Hud::load(|p| std::fs::read(dir.join(p)))
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    let manifest = std::fs::read(dir.join(MANIFEST)).unwrap_or_default();
    let sound = (conker_client::audio::has_sound(&manifest))
        .then(|| Audio::load(|p| std::fs::read(dir.join(p))))
        .transpose()
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    // Bevy's asset server reads the glTF models from there (an absolute path).
    cfg.assets = Some(dir.display().to_string());
    let script = match cfg.script.as_deref() {
        Some(file) if !file.is_empty() => std::fs::read_to_string(file)
            .map_err(|e| format!("{file}: {e}"))
            .and_then(|text| conker_client::parse_script(&text))
            .unwrap_or_else(|e| fail(e)),
        _ => Vec::new(),
    };
    conker_client::run(cfg, slice, hud, sound, script);
}

/// wasm-bindgen runs `main` on `init()`; the page then calls [`start`].
#[cfg(target_family = "wasm")]
fn main() {}

/// Fetches the asset pack's data files and starts the client. Errors (a missing or malformed file,
/// a bad URL option) come back to the page as a rejected string; nothing has started then.
#[cfg(target_family = "wasm")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn start() -> Result<(), String> {
    let location = web_sys::window().map(|w| w.location());
    let query = location.as_ref().and_then(|l| l.search().ok());
    let mut pairs = conker_client::query_pairs(&query.unwrap_or_default());
    // The game server defaults to the page's own host.
    if !pairs.iter().any(|(k, _)| k == "server")
        && let Some(host) = location.and_then(|l| l.hostname().ok())
    {
        pairs.insert(0, ("server".into(), conker_client::page_server(&host)));
    }
    let cfg = conker_client::Config::from_pairs(pairs)?;
    // Every file at once, from the folder Bevy's asset server reads the glTF models from.
    let root = bevy::asset::AssetPlugin::default().file_path;
    let mut files = Slice::files(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS);
    files.push(conker_assets::hud::HUD.into());
    let requests: Vec<_> = (files.iter())
        .map(|f| conker_client::web::fetch(&format!("{root}/{f}")))
        .collect();
    let mut bytes = std::collections::HashMap::new();
    for (file, request) in files.into_iter().zip(requests) {
        bytes.insert(file, request.await?);
    }
    let sound = conker_client::audio::has_sound(bytes.get(MANIFEST).map_or(&[][..], |m| &m[..]));
    let slice = Slice::load(PACK_MAP, &PACK_CHARACTERS, PACK_WEAPONS, |p| {
        bytes.remove(p).ok_or("not fetched")
    })
    .map_err(|e| format!("{root}/{e}"))?;
    let hud =
        Hud::load(|p| bytes.remove(p).ok_or("not fetched")).map_err(|e| format!("{root}/{e}"))?;
    // The sound's two data files, if the pack has sound (its WAVs load later, through Bevy).
    let sound = match sound {
        true => {
            let files = Audio::files();
            let requests = files.map(|f| conker_client::web::fetch(&format!("{root}/{f}")));
            let mut bytes = std::collections::HashMap::new();
            for (file, request) in files.into_iter().zip(requests) {
                bytes.insert(file, request.await?);
            }
            let sound = Audio::load(|p| bytes.remove(p).ok_or("not fetched"));
            Some(sound.map_err(|e| format!("{root}/{e}"))?)
        }
        false => None,
    };
    // `?script` drives the player with the test pattern; script files are native only.
    conker_client::run(cfg, slice, hud, sound, Vec::new());
    Ok(())
}
