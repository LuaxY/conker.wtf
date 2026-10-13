//! Client entry point. Native: options from CLI flags, the asset pack from `--assets DIR` (or
//! `CONKER_ASSETS`, else `../assets`), headless or windowed (prediction needs the pack's collision
//! either way) and its HUD (`hud/hud.json`). Browser: options from the page URL query; [`start`]
//! fetches the pack's data files from `assets/` next to the page, where Bevy loads its glTF models
//! and PNGs from too. Both first ask the server's `/info` which map it plays (`maps/<map>/`).

use conker_assets::audio::Audio;
use conker_assets::hud::{End, Hud};
use conker_assets::race::Race;
use conker_assets::raptor::Raptor;
use conker_assets::{MANIFEST, PACK_WEAPONS, Slice};

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
    // The server's map and pack, unless `map=` names one.
    let mut pack = None;
    if cfg.map.is_empty() {
        (cfg.map, pack) = conker_client::connection::server_info(&cfg).unwrap_or_else(|e| fail(e));
    }
    let characters = conker_client::character_files(&cfg.map);
    let slice = Slice::load(&cfg.map, &characters, PACK_WEAPONS, |p| {
        std::fs::read(dir.join(p))
    })
    .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    if let Some(server) = pack {
        conker_client::connection::check_pack(&cfg.map, &server, |p| {
            std::fs::read(dir.join(p)).ok()
        })
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    }
    let mut hud = Hud::load(|p| std::fs::read(dir.join(p)))
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    let end = End::load(|p| std::fs::read(dir.join(p)))
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    hud.end = Some(end);
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
    let raptor = (cfg.map == "3f")
        .then(|| Raptor::load(&cfg.map, |p| std::fs::read(dir.join(p))))
        .transpose()
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    let race = (cfg.map == "33")
        .then(|| Race::load(&cfg.map, |p| std::fs::read(dir.join(p))))
        .transpose()
        .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    conker_client::run(cfg, slice, hud, sound, script, raptor, race);
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
    let mut cfg = conker_client::Config::from_pairs(pairs)?;
    // The server's map and pack first (its `/info`), unless `?map=` names one.
    let mut pack = None;
    if cfg.map.is_empty() {
        let info = conker_client::connection::info_url(&cfg);
        let body = (conker_client::web::fetch(&info).await)
            .map_err(|e| format!("could not get the server's map from {e}"))?;
        cfg.map = conker_client::connection::info_map(&body)?;
        pack = conker_client::connection::info_pack(&body);
    }
    // Every file at once, from the folder Bevy's asset server reads the glTF models from.
    let root = bevy::asset::AssetPlugin::default().file_path;
    let characters = conker_client::character_files(&cfg.map);
    let mut files = Slice::files(&cfg.map, &characters, PACK_WEAPONS);
    files.extend([conker_assets::hud::HUD, conker_assets::hud::END].map(String::from));
    let raptor = cfg.map == "3f";
    if raptor {
        files.push(conker_assets::raptor::raptor_json(&cfg.map));
    }
    let race = cfg.map == "33";
    if race {
        files.push(conker_assets::race::race_json(&cfg.map));
    }
    let requests: Vec<_> = (files.iter())
        .map(|f| conker_client::web::fetch(&format!("{root}/{f}")))
        .collect();
    let mut bytes = std::collections::HashMap::new();
    for (file, request) in files.into_iter().zip(requests) {
        bytes.insert(file, request.await?);
    }
    if let Some(server) = pack {
        let read = |p: &str| bytes.get(p).cloned();
        conker_client::connection::check_pack(&cfg.map, &server, read)
            .map_err(|e| format!("{root}/: {e}"))?;
    }
    let sound = conker_client::audio::has_sound(bytes.get(MANIFEST).map_or(&[][..], |m| &m[..]));
    let slice = Slice::load(&cfg.map, &characters, PACK_WEAPONS, |p| {
        bytes.remove(p).ok_or("not fetched")
    })
    .map_err(|e| format!("{root}/{e}"))?;
    let mut hud =
        Hud::load(|p| bytes.remove(p).ok_or("not fetched")).map_err(|e| format!("{root}/{e}"))?;
    let end = End::load(|p| bytes.remove(p).ok_or("not fetched"));
    hud.end = Some(end.map_err(|e| format!("{root}/{e}"))?);
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
    let raptor = raptor
        .then(|| Raptor::load(&cfg.map, |p| bytes.remove(p).ok_or("not fetched")))
        .transpose()
        .map_err(|e| format!("{root}/{e}"))?;
    let race = race
        .then(|| Race::load(&cfg.map, |p| bytes.remove(p).ok_or("not fetched")))
        .transpose()
        .map_err(|e| format!("{root}/{e}"))?;
    // `?script` drives the player with the test pattern; script files are native only.
    conker_client::run(cfg, slice, hud, sound, Vec::new(), raptor, race);
    Ok(())
}
