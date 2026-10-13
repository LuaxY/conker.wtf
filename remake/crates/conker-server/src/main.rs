//! Authoritative multiplayer server: headless Bevy + lightyear, running a Deathmatch on scene 0x2D,
//! 0x30, 0x34, 0x3F or 0x2B (`--map`), or War: Total War on 0x2D (`--mode total-war`).
//!
//! ONE lightyear `Server` entity carries both transports on the same port number:
//! WebTransport (UDP) and WebSocket (TCP). With `--cert`/`--cert-key` (PEM, e.g. Let's Encrypt's)
//! both use that certificate (`wss://`, [`tls`]); in dev, a self-signed WebTransport certificate
//! pinned by its digest, and plain `ws://`. Clients connect with a netcode token the server signs
//! ([`token`]: `GET /token` on `--token-addr`, default port + 1).
//! Two `Server` entities silently stop server-side input processing (lightyear #1693); one
//! entity with two aeronet transports needs [`relink_evicted_session`] (tested 2026-10-08).
//!
//! The match is conker-rules' [`Match`] on the map's world, read from the asset pack (`--assets`):
//! every tick it moves the players (conker-sim, with the rules' masks), fires, flies the
//! projectiles, hits, kills, respawns and counts down. Each player also has its follow camera here
//! (conker-sim's [`Follow`]): R's aim camera holds the player's stick. Joining players take the
//! lowest free slot, else a new one, and start in their side's base ([`Game::placement`], mid-match
//! too); leaving frees the slot, and nobody else changes slot (`Match::remove_player`). A client
//! joins with its first message ([`Join`]): the side it asks for (squirrels or Tediz: the model and
//! its body), else the side with fewer players (the squirrels on a tie). Bots take the squirrels in
//! even slots, the Tediz in odd ones. In a Deathmatch every player is its own team, and only its
//! starts and respawns keep to its side's half; in Total War its side is its team (0 the squirrels,
//! 1 the Tediz), which the rules score and rank by.
//! Clients get [`Player`] (predicted by the owner) with conker-rules' [`Fighting`]
//! (the fighter, predicted by the owner: its knock-back, death and respawn; sent to the owner only,
//! [`FightingOwner`]), [`Combat`] and one [`MatchView`]. After each step every human's camera steps with what the rules know of it
//! (`Fighter::camera_view`: the weapon's aim, the scope, the twist), and its aim gate and scope go
//! into the fighter for the next step.
//!
//! Lag compensation keeps every slot's position per tick ([`History`]) and what each shooter showed
//! when it sampled its input (its [`Saw`]: lightyear's interpolation time; else its
//! `InterpolationDelay`). Every aimed hit test of the step (bullets, rockets, aimed shots, the
//! sniper's ray, the flame, the melee strike and lock-on) reads the targets there, per shooter
//! (conker-rules' `Match::step_seen`).
//!
//! Bots (`--bots N`, conker-bots): the original's multiplayer AI plays slots 0..N, each in its side's
//! base, from the server's start. Humans take the slots after them (a joining
//! human never replaces a bot; a leaving one frees its own slot, so bots keep theirs). Clients see
//! a bot as any other player (`PlayerId(PeerId::Local(slot))`, interpolated). The match start
//! starts the bots' AI afresh, as a new match does in the game; a join or leave mid-match keeps the
//! level script's state (the gas, the flags, Beach's timers), only the slot's own AI memory starts
//! over (conker-bots' `Bots::join`, `Bots::leave`).
//! Each tick: the lifts, `Bots::lifted` for the slots they moved, `Bots::tick` (the bots' inputs),
//! `Bots::scopes` (a bot aiming 0x14 or 0x24 looks through its scope), the step, `Bots::observe`.
//!
//! Settings (`--time`, `--lives`, `--kills`, `--skill`, `--radar`; the original's SET-UP options and
//! defaults, conker-rules' `Settings`) go to the clients in [`MatchView`]. The match ends as the
//! original's (`Match::ends`: the clock, the kill limit, the last one standing or no human left in
//! play, untimed with lives); over, every client shows the results, and the next match (the same
//! settings) starts once every human in it sent [`Restart`], or [`RESULTS`] ticks after the end
//! (`--wait` replays end as the game does). With the last human gone, a finished match waits and
//! the next human to join starts a fresh one.
//!
//! The level's lifts (conker-bots' `lift`, which conker-rules lacks) move every player, bots and
//! humans, before each step; a ride counts in `Combat.lives` so a client cuts its camera as after a
//! respawn.
//!
//! Connections: both transports listen on `[::]` (IPv4 and IPv6 on macOS and Linux, whose sockets are
//! dual-stack by default). A joined human stands as it joined ([`Waiting`]) until the first tick
//! its client sent an input for: its client predicts from that tick, from the state the server holds.
//! It leaves on its client's netcode disconnect, its transport closing (a closed page, a killed
//! WebSocket client) or the netcode timeout (3 s, carried in the connect token); every 10 s the
//! log has each client's traffic (`NET` lines).
//!
//! Usage: conker-server [--assets DIR] [--map 2d|30|34|3f|2b|1a|24] [--port 5888] [--digest-file PATH] [--seconds N]
//!   [--bots N] [--time 3-10|unlimited] [--lives 1-5] [--kills 0-10] [--skill inbred|crap|normal|bastard|einstein]
//!   [--radar on|off] [--spawn X,Y,Z[,FACING]]... [--give SLOT:READ:ID]... [--warp SLOT:READ:X,Y,Z]... [--wait N] [--margin TICKS]
//!   [--first-read R] [--no-lagcomp] [--cavemen] [--mode deathmatch|total-war|colors|beach|heist|tank|raptor] [--score 1-40|unlimited]
//!   [--seed N]
//!   [--cert PEM --cert-key PEM]
//!   [--token-key FILE] [--token-addr ADDR:PORT] [--public IP[:PORT]]...
//! - `--assets DIR`, else `CONKER_ASSETS`, else `../assets` (the pack next to `remake/`, run from
//!   there): the asset pack (`conker-extract pack`), which the page's static server serves too.
//! - `--map 2d|30|34|3f|2b`: the map (a folder under the pack's `maps/`, the scene in hex; default 2d). The
//!   clients learn it from `GET /info` on the token endpoint ([`token`]) before loading the level. 2b (the
//!   Beach, War: Beach's map) plays Deathmatch too: its start records, the villagers' two vitamin
//!   pickups (item 0x3E, no weapon there), its mounted gun. 1a (the Vault, Heist's map) plays
//!   Deathmatch only, as the original's scenario 6: every player the Wise Guy holding the bat, the
//!   four roaming pickups ([`Game::vault`]).
//! - `--token-key FILE` (or `CONKER_TOKEN_KEY`): 64 hex digits that sign the connect tokens; a
//!   random key per start without it. `--token-addr`: the token endpoint (default `[::]`, port + 1;
//!   production: loopback behind the static server).
//! - `--public IP[:PORT]` (repeatable, the port defaults to `--port`): the addresses clients reach
//!   the server at (a domain's A and AAAA records). The tokens name them and the server checks
//!   that a token was issued for them; without `--public` (dev) the tokens name loopback and the
//!   check is off.
//! - `--time` minutes on the clock (default 10) or `unlimited` (no clock: then each player has
//!   `--lives` lives, default 3, and the last one standing wins); `--kills N` ends the match at N
//!   kills (default 0, unlimited); `--skill` the bots' (default normal, or 0-4); `--radar off` hides
//!   the HUD's radar.
//! - `--spawn` (repeatable: slot 0, 1, ...) places a slot exactly, for oracle replays of the
//!   original's Deathmatch (the game's start jitter comes from an RNG state that can't be
//!   reproduced): on the slot's Deathmatch start record's facing (the default) and camera, and it
//!   respawns on any record.
//! - `--wait N` starts the match (clock, scripts) `--margin` ticks (default 150, 5 s) after the N-th
//!   human joined, every slot back on its spawn; `--first-read R`: that tick is oracle read R
//!   (default 23, the match going live).
//! - `--give SLOT:READ:ID` hands a weapon as its pickup does at that read (the oracle's MP+0xEB0 poke).
//! - `--warp SLOT:READ:X,Y,Z` (test only, as the gas research's position pokes): puts that slot there at
//!   that read, e.g. a Total War canister carrier into the shelter (a delivery).
//! - `--cavemen`: scene 0x3F's Deathmatch on any map: caveman bots, every player holding item 0x40.
//! - `--mode total-war`: the original's War: Total War (scenario 9), on map 2d only and without
//!   `--cavemen`: teams are the sides, friendly fire scores no kill, the match ends on the clock or
//!   (untimed) a team's last life, and the best team wins (conker-rules' `Settings::total_war`).
//! - `--mode colors`: the original's War: Colors (scenario 10, capture the flag), on map 30 only and
//!   without `--cavemen`: teams are the sides, lives are never lost, the level's AI script runs the
//!   flags (conker-rules' `colors`), and the match ends on the clock or at `--score` captures
//!   (1-40, default 20, `unlimited`: the clock only; untimed and unlimited, never).
//! - `--mode beach`: the original's War: Beach (scenario 8), on map 2b only and without `--cavemen`:
//!   the villagers (the squirrels' side, item 0x3E, no attack) against at most 2 Tediz; the level's
//!   AI script runs the escapes, the detonation, the weapon zones and the 90 s deadline (conker-rules'
//!   `beach`); lives are never lost, and the match ends on the clock or at `--score` (as Colors').
//! - `--mode heist`: the original's War: Heist (scenario 0), on map 1a only and without `--cavemen`:
//!   four teams by slot (slot & 3: REDS, BLUES, GREENS, YELLOWS; at most 8 players and 2 humans a
//!   team, 4 and 1 once 3 humans are in), the Wise Guy with the bat; the level's AI script runs the
//!   bag (conker-rules' `heist`), and the match ends on the clock, at `--score` bags (1-5, default
//!   3) or, untimed, once one team is left with lives.
//! - `--mode tank`: the original's Tank (scenario 1), on map 24 (which plays nothing else), without
//!   `--cavemen`, at most 7 bots: every player a tank (6 hp), free for all, ranked by kills as
//!   Deathmatch; the level's AI script runs the bomb, the bunkers' doors and the gas (conker-rules'
//!   `tank`); `TANK` lines log the bomb, the power-ups and the blasts' kills.
//! - `--mode raptor`: the original's War: Raptor (scenario 2), on map 3f only and without
//!   `--cavemen`: the cavemen (team 0, the club, the Ugas' camp) against the raptors (team 1, class
//!   9: a human asking for the Tediz, and bots in slots 2 and 6 of each 8), lives never lost on the
//!   clock; the level's AI script runs the eggs, the baby and the hazard (conker-rules' `raptor`);
//!   kills, fed bodies (+1) and delivered eggs (+2) score, and the match ends on the clock or at
//!   `--score` (1-40, default 20). `RAPTOR` lines log the script's egg and nest effects.
//! - `RUST_LOG=info,conker_server=debug` logs every player every tick (`STATE`, `COMBAT` lines),
//!   the events (`EVENT`) and, per bullet in play, the targets where its shooter saw them
//!   (`LAGCOMP`).

use aeronet_io::Session;
use bevy::app::ScheduleRunnerPlugin;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::{AppVisibilityExt, SingleComponent, VisibilityFilter};
use conker_assets::{PACK_CHARACTERS, PACK_MAP, PACK_WEAPONS, Slice};
use conker_bots::brain::g;
use conker_bots::lift::Lifts;
use conker_bots::{Assets, Bots, DEFAULT_SKILL};
use conker_rules::damage::{self, Hitbox};
use conker_rules::game::{Action, Event, Fighter, Match, Phase, Settings};
use conker_rules::game::{END_DELAY, FADE, SPAWN_IMMUNE, teams_result};
use conker_rules::gas::{self, Effect};
use conker_rules::net::Fighting;
use conker_rules::projectile::{Bullet, hits_body};
use conker_rules::spawn::{Starts, starts};
use conker_rules::weapon::{self, Attack, Gives, Pickup};
use conker_rules::{Arena, Placement, Rng, Spawn, gun, place};
use conker_rules::{beach, colors, heist, raptor, tank, traps};
use conker_sim::TICK_HZ;
use conker_sim::collision::CollisionWorld;
use conker_sim::follow::Follow;
use conker_sim::movement::{Input, Side, state};
use conker_sim::net::{
    BulletView, CashView, Collision, Combat, DEFAULT_PORT, FlagView, GunView, Join, MatchView,
    PROTOCOL_ID, Player, PlayerId, ProtocolPlugin, RESULTS, RaptorView, Restart, Saw, TankView,
    map_scene, scene_world,
};
use core::time::Duration;
use lightyear::input::server::InputSystems;
use lightyear::netcode::NetcodeServer;
use lightyear::prelude::input::native::{ActionState, NativeBuffer};
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use lightyear_aeronet::{AeronetLink, AeronetLinkOf};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

mod tls;
mod token;

/// Ticks between the last `--wait` player joining and the match start (`--margin`): every headless
/// client is synced by then.
const START_MARGIN: u32 = 150;
/// The oracle's match goes live at read 23; row R shows the clock after R − 22 frames.
const LIVE_READ: u32 = 23;
/// Ticks of position history: 2 s, more than any view lag a playable connection has.
const HISTORY: usize = 60;
/// RESTART's fade: the ticks from the votes to the next match (the original's: the STATS one more
/// frame, then black +16 a frame, 255 on the 17th, the match on the 18th read; V t5_r, C
/// `func_151E4EE8`, `func_151ED1E0`).
const RESTART_FADE: u32 = 17;
/// The bot skills' names (`--skill`), as the SET-UP screen shows them.
const SKILLS: [&str; 5] = ["inbred", "crap", "normal", "bastard", "einstein"];

/// Replay and test setup from the command line.
#[derive(Clone, Debug, Default, PartialEq)]
struct Setup {
    spawns: Vec<([f32; 3], Option<u16>)>,
    gives: Vec<(usize, u32, u8)>,
    warps: Vec<(usize, u32, [f32; 3])>,
    wait: usize,
    margin: u32,
    first_read: u32,
    lagcomp: bool,
    bots: usize,
    settings: Settings,
    /// `--cavemen`: scene 0x3F's Deathmatch on any map ([`Game::cavemen`]).
    cavemen: bool,
    /// `--seed N`: the match RNG starts at the game's seed ^ N (0 the game's; runs to compare).
    seed: u64,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<(Setup, Net), String> {
    let mut args = args.into_iter();
    let mut score_given = false;
    let mut setup = Setup {
        first_read: LIVE_READ,
        margin: START_MARGIN,
        lagcomp: true,
        ..Setup::default()
    };
    let mut net = Net {
        port: DEFAULT_PORT,
        digest_file: None,
        seconds: None,
        assets: std::env::var("CONKER_ASSETS").unwrap_or_else(|_| "../assets".into()),
        map: PACK_MAP.into(),
        cert: None,
        cert_key: None,
        token_key: None,
        token_addr: None,
        public: Vec::new(),
    };
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        let bad = |v: &str| format!("bad value for {flag}: {v}");
        match flag.as_str() {
            "--port" => net.port = value()?.parse().map_err(|_| bad("port"))?,
            "--digest-file" => net.digest_file = Some(value()?),
            "--seconds" => net.seconds = Some(value()?.parse().map_err(|_| bad("seconds"))?),
            "--assets" => net.assets = value()?,
            "--map" => net.map = value()?,
            "--cert" => net.cert = Some(value()?),
            "--cert-key" => net.cert_key = Some(value()?),
            "--token-key" => net.token_key = Some(value()?),
            "--token-addr" => net.token_addr = Some(value()?.parse().map_err(|_| bad("address"))?),
            "--public" => {
                let v = value()?;
                let addr = v
                    .parse::<SocketAddr>()
                    .or_else(|_| v.parse::<IpAddr>().map(|ip| SocketAddr::new(ip, 0)));
                net.public.push(addr.map_err(|_| bad(&v))?);
            }
            "--spawn" => {
                let v = value()?;
                let n: Vec<f32> = v
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad(&v))?;
                let (pos, facing) = match n[..] {
                    [x, y, z] => ([x, y, z], None),
                    [x, y, z, f] => ([x, y, z], Some(f as u16)),
                    _ => return Err(bad(&v)),
                };
                setup.spawns.push((pos, facing));
            }
            "--give" => {
                let v = value()?;
                let w: Vec<&str> = v.split(':').collect();
                let [slot, read, id] = w[..] else {
                    return Err(bad(&v));
                };
                let id = id.trim_start_matches("0x");
                setup.gives.push((
                    slot.parse().map_err(|_| bad(&v))?,
                    read.parse().map_err(|_| bad(&v))?,
                    u8::from_str_radix(id, 16).map_err(|_| bad(&v))?,
                ));
            }
            "--warp" => {
                let v = value()?;
                let w: Vec<&str> = v.split(':').collect();
                let [slot, read, at] = w[..] else {
                    return Err(bad(&v));
                };
                let at: Vec<f32> = (at.split(',').map(str::parse))
                    .collect::<Result<_, _>>()
                    .map_err(|_| bad(&v))?;
                setup.warps.push((
                    slot.parse().map_err(|_| bad(&v))?,
                    read.parse().map_err(|_| bad(&v))?,
                    at.try_into().map_err(|_| bad(&v))?,
                ));
            }
            "--wait" => setup.wait = value()?.parse().map_err(|_| bad("wait"))?,
            "--margin" => setup.margin = value()?.parse().map_err(|_| bad("margin"))?,
            "--first-read" => setup.first_read = value()?.parse().map_err(|_| bad("first-read"))?,
            "--no-lagcomp" => setup.lagcomp = false,
            "--cavemen" => setup.cavemen = true,
            "--mode" => {
                let s = &mut setup.settings;
                let mode = value()?;
                let modes = ["total-war", "colors", "beach", "heist", "tank", "raptor"];
                if mode != "deathmatch" && !modes.contains(&mode.as_str()) {
                    return Err(bad(&mode));
                }
                let on = |m: &str| mode == m;
                (s.total_war, s.colors, s.beach, s.heist, s.tank, s.raptor) = (
                    on("total-war"),
                    on("colors"),
                    on("beach"),
                    on("heist"),
                    on("tank"),
                    on("raptor"),
                );
            }
            "--bots" => setup.bots = value()?.parse().map_err(|_| bad("bots"))?,
            "--seed" => setup.seed = value()?.parse().map_err(|_| bad("seed"))?,
            "--time" | "--lives" | "--kills" | "--skill" | "--radar" | "--score" => {
                let v = value()?;
                score_given |= flag == "--score";
                let n = |lo: u8, hi: u8| v.parse().ok().filter(|n| (lo..=hi).contains(n));
                let s = setup.settings;
                let set = match (flag.as_str(), v.as_str()) {
                    ("--time", "unlimited") => Some(Settings { time: None, ..s }),
                    ("--time", _) => n(3, 10).map(|m| Settings { time: Some(m), ..s }),
                    ("--lives", _) => n(1, 5).map(|lives| Settings { lives, ..s }),
                    ("--kills", "unlimited") => Some(Settings { kills: 0, ..s }),
                    ("--kills", _) => n(0, 10).map(|kills| Settings { kills, ..s }),
                    ("--skill", _) => (SKILLS.iter().position(|k| *k == v).map(|k| k as u8))
                        .or_else(|| n(0, 4))
                        .map(|skill| Settings { skill, ..s }),
                    ("--radar", "on" | "off") => Some(Settings {
                        radar: v == "on",
                        ..s
                    }),
                    ("--score", "unlimited") => Some(Settings { score: 0, ..s }),
                    ("--score", _) => n(1, 40).map(|score| Settings { score, ..s }),
                    _ => None,
                };
                setup.settings = set.ok_or_else(|| bad(&v))?;
            }
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    // Total War is scenario 9, scene 0x2D's: its bases and the squirrels and Tediz as the teams.
    if setup.settings.total_war && (net.map != "2d" || setup.cavemen) {
        return Err("--mode total-war: map 2d only, without --cavemen".into());
    }
    // Colors is scenario 10, scene 0x30's.
    if setup.settings.colors && (net.map != "30" || setup.cavemen) {
        return Err("--mode colors: map 30 only, without --cavemen".into());
    }
    // Beach is scenario 8, scene 0x2B's.
    if setup.settings.beach && (net.map != "2b" || setup.cavemen) {
        return Err("--mode beach: map 2b only, without --cavemen".into());
    }
    if setup.settings.beach && setup.bots > beach::MAX_VILLAGERS + beach::MAX_TEDIZ {
        return Err("--mode beach: at most 10 bots (8 villagers, 2 Tediz)".into());
    }
    // Heist is scenario 0, scene 0x1A's: its bag target 1-5 (opt[4], default 3), 8 players.
    let s = &mut setup.settings;
    if s.heist {
        if net.map != "1a" || setup.cavemen {
            return Err("--mode heist: map 1a only, without --cavemen".into());
        }
        if !score_given {
            s.score = heist::SCORE;
        }
        if !(1..=5).contains(&s.score) {
            return Err("--mode heist: --score 1-5".into());
        }
        if setup.bots > 7 {
            return Err("--mode heist: at most 7 bots (8 players)".into());
        }
    }
    // Tank is scenario 1, scene 0x24's, and the map plays nothing else.
    if (net.map == "24") != s.tank || (s.tank && setup.cavemen) {
        return Err(
            "--mode tank: map 24 only (and map 24 plays Tank only), without --cavemen".into(),
        );
    }
    if s.tank && setup.bots > 7 {
        return Err("--mode tank: at most 7 bots (8 players)".into());
    }
    // Raptor is scenario 2, scene 0x3F's: its score target 1-40 (opt[18], default 20).
    if s.raptor && (net.map != "3f" || setup.cavemen || s.score == 0) {
        return Err("--mode raptor: map 3f only, without --cavemen, --score 1-40".into());
    }
    for addr in net.public.iter_mut().filter(|a| a.port() == 0) {
        addr.set_port(net.port);
    }
    net.token_addr.get_or_insert(SocketAddr::new(
        Ipv6Addr::UNSPECIFIED.into(),
        net.port.wrapping_add(1),
    ));
    Ok((setup, net))
}

struct Net {
    port: u16,
    digest_file: Option<String>,
    seconds: Option<f64>,
    /// The asset pack's folder, and the map played (a folder under its `maps/`).
    assets: String,
    map: String,
    /// TLS: the certificate chain and its key (PEM).
    cert: Option<String>,
    cert_key: Option<String>,
    token_key: Option<String>,
    token_addr: Option<SocketAddr>,
    public: Vec<SocketAddr>,
}

fn main() {
    let fail = |msg: String| -> ! {
        eprintln!("conker-server: {msg}");
        std::process::exit(1)
    };
    let (setup, net) = parse_args(std::env::args().skip(1)).unwrap_or_else(|e| {
        fail(format!(
            "{e}; usage: conker-server [--assets DIR] [--map 2d|30|34|3f|2b|1a|24] [--port N] [--digest-file PATH] [--seconds N] \
             [--bots N] [--time 3-10|unlimited] [--lives 1-5] [--kills 0-10] \
             [--skill inbred|crap|normal|bastard|einstein] [--radar on|off] \
             [--spawn X,Y,Z[,FACING]]... [--give SLOT:READ:ID]... [--warp SLOT:READ:X,Y,Z]... [--wait N] [--margin TICKS] \
             [--first-read R] [--no-lagcomp] [--cavemen] [--mode deathmatch|total-war|colors|beach|heist|tank|raptor] [--score 1-40|unlimited] [--seed N] \
             [--cert PEM --cert-key PEM] [--token-key FILE] \
             [--token-addr ADDR:PORT] [--public IP[:PORT]]..."
        ))
    });
    let dir = std::fs::canonicalize(&net.assets).unwrap_or_else(|e| {
        fail(format!(
            "asset pack {}: {e} (export one: cargo run --release -p conker-extract -- pack; \
             or pass --assets DIR)",
            net.assets
        ))
    });
    let (scene, starts) = (map_scene(&net.map))
        .and_then(|s| Some((s, starts(s)?)))
        .unwrap_or_else(|| {
            fail(format!(
                "--map {}: not 2d, 30, 34, 3f, 2b, 1a or 24",
                net.map
            ))
        });
    let pack = Slice::load(&net.map, &PACK_CHARACTERS, PACK_WEAPONS, |p| {
        std::fs::read(dir.join(p))
    })
    .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())));
    let total_war = setup.settings.total_war;
    // Raptor: the bone weapons and grenades on their own placements (`raptor.json`).
    let raptor = (setup.settings.raptor).then(|| {
        conker_assets::raptor::Raptor::load(&net.map, |p| std::fs::read(dir.join(p)))
            .unwrap_or_else(|e| fail(format!("{}: {e}", dir.display())))
    });
    let pickups = raptor.as_ref().map_or(&pack.pickups, |r| &r.pickups);
    let arena = arena(&pack, pickups, total_war, setup.settings.tank);
    let world = scene_world(&pack, scene);

    let tick = Duration::from_secs_f64(1.0 / TICK_HZ);
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick / 2)),
        LogPlugin::default(),
        StatesPlugin, // lightyear registers states
        ServerPlugins {
            tick_duration: tick,
        },
        ProtocolPlugin,
    ));
    conker_rules::net::register(&mut app);
    app.add_visibility_filter::<FightingOwner>();
    app.insert_resource(ReplicationMetadata::new(tick));
    app.insert_resource(Collision(world));
    let mut game = Game::new(arena, setup);
    game.starts = starts;
    game.outros = (pack.outros.iter())
        .map(|o| (o.winner, o.cutscene, o.frames()))
        .collect();
    game.cavemen = game.setup.cavemen || scene == 0x3F;
    game.vault = scene == 0x1A && !game.setup.cavemen;
    game.ai = Ai::new([pack.nav, pack.triggers, pack.ai, pack.class], scene as u8)
        .unwrap_or_else(|| fail(format!("{}: bad bot data", dir.display())));
    game.m.guns = gun::spawn(usize::from(scene));
    game.m.traps = game.ai.traps();
    app.add_observer(on_link)
        .add_observer(on_connected)
        .add_observer(on_disconnected)
        .add_observer(on_left)
        .add_observer(relink_evicted_session);
    let (bots, settings) = (game.setup.bots, game.setup.settings);
    app.insert_resource(game);
    app.add_systems(
        Startup,
        move |mut commands: Commands,
              server: Single<Entity, With<Server>>,
              mut game: ResMut<Game>| {
            for slot in 0..bots {
                commands.spawn(game.join_bot(slot));
            }
            commands.trigger(Start { entity: *server });
            commands.spawn((
                Name::new("Match"),
                MatchView::default(),
                Replicate::to_clients(NetworkTarget::All),
            ));
        },
    );
    // Every frame (lightyear drops messages a frame leaves unread), before its fixed ticks.
    app.add_systems(
        RunFixedMainLoop,
        (join, collect_saws, restart_votes).in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
    );
    app.add_systems(
        FixedPreUpdate,
        first_inputs.before(InputSystems::UpdateActionState),
    );
    app.add_systems(FixedUpdate, (simulate, publish, log_players).chain());
    app.add_systems(Update, log_traffic);
    if let Some(seconds) = net.seconds {
        app.add_systems(
            Update,
            move |time: Res<Time<Real>>, mut exit: MessageWriter<AppExit>| {
                if time.elapsed_secs_f64() >= seconds {
                    exit.write(AppExit::Success);
                }
            },
        );
    }

    // Dev certificate: self-signed ECDSA P-256, 14-day validity, which is what browsers accept
    // through WebTransport `serverCertificateHashes`. Regenerated on every start.
    let (identity, wss) = match (&net.cert, &net.cert_key) {
        (Some(cert), Some(key)) => {
            let (identity, wss) = tls::load(cert, key).unwrap_or_else(|e| fail(e));
            (identity, Some(wss))
        }
        (None, None) => (
            Identity::self_signed(["localhost", "127.0.0.1", "::1"]).expect("self-signed cert"),
            None,
        ),
        _ => fail("--cert and --cert-key go together".into()),
    };
    let digest: String = identity.certificate_chain().as_slice()[0]
        .hash()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect();
    info!("WebTransport certificate digest: {digest}");
    if let Some(path) = net.digest_file {
        std::fs::write(&path, &digest).expect("write --digest-file");
    }

    let addr = SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), net.port);
    let key = token::key(
        net.token_key.as_deref(),
        std::env::var("CONKER_TOKEN_KEY").ok(),
    )
    .unwrap_or_else(|e| fail(e));
    // The tokens name the public addresses (in dev, loopback: no check).
    let named = match net.public.is_empty() {
        true => vec![
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), net.port),
            SocketAddr::new(Ipv6Addr::LOCALHOST.into(), net.port),
        ],
        false => net.public.clone(),
    };
    let token_addr = net.token_addr.expect("parse_args sets it");
    let listener = std::net::TcpListener::bind(token_addr).unwrap_or_else(|e| {
        fail(format!(
            "token endpoint {token_addr}: {e} (another server's port? pick one with --token-addr)"
        ))
    });
    let mode = match (settings.total_war, settings.colors, settings.beach) {
        (true, ..) => "War: Total War",
        (_, true, _) => "War: Colors",
        (.., true) => "War: Beach",
        _ if settings.heist => "War: Heist",
        _ if settings.tank => "Tank",
        _ if settings.raptor => "War: Raptor",
        _ => "Deathmatch",
    };
    info!("map {} (scene {scene:#X}), {mode}", net.map);
    let map = net.map.clone();
    token::serve(
        listener,
        token::Issuer {
            key,
            addrs: named,
            map,
        },
    );
    let ws = ServerConfig::builder().with_bind_address(addr);
    let (ws, scheme) = match wss {
        Some(identity) => (ws.with_identity(identity), "wss"),
        None => (ws.with_no_encryption(), "ws"),
    };
    app.world_mut().spawn((
        Name::new("Server"),
        NetcodeServer::new(NetcodeConfig {
            protocol_id: PROTOCOL_ID,
            private_key: key,
            client_timeout_secs: token::TIMEOUT_SECS,
            server_addr_check: !net.public.is_empty(),
            // The bind address ([::]) is not what the tokens name.
            additional_expected_addresses: net.public.clone(),
            ..default()
        }),
        LocalAddr(addr),
        WebTransportServerIo {
            certificate: identity,
        },
        WebSocketServerIo { config: ws },
    ));
    info!(
        "Listening on {addr}: WebTransport (UDP) + WebSocket (TCP, {scheme}://), one server entity; \
         connect tokens on http://{token_addr}/token{}",
        match net.public.is_empty() {
            true => String::new(),
            false => format!(", for {:?}", net.public),
        }
    );
    app.run();
}

/// The match's static data from the pack: the spawn records, and the pickups (what each gives, and
/// where a player touches it; the gas masks only in Total War; Tank's power-ups, the pack's
/// weapons 0x80, 0x01, 0x10 and 0x02, as their bits).
fn arena(pack: &Slice, pickups: &[conker_assets::Pickup], total_war: bool, tank: bool) -> Arena {
    let spawns = (pack.spawns.iter())
        .map(|s| Spawn {
            pos: s.position,
            b6: s.byte_06,
            yaw: s.yaw,
            camera: s.camera,
            b14: s.byte_14,
            b15: s.byte_15,
        })
        .collect();
    // The roaming pickups lead the table and share their spots (0x1A's k0-k3).
    let roams = pickups.iter().take_while(|p| !p.roams.is_empty());
    let respawn = pickups.first().and_then(|p| p.respawn).map(|vi| vi / 2);
    let pickups = (pickups.iter())
        .filter(|p| total_war || p.gives != conker_assets::Gives::Mask)
        .map(|p| {
            let gives = match p.gives {
                conker_assets::Gives::Weapon(bit) if tank => Gives::Power(bit),
                conker_assets::Gives::Weapon(id) => Gives::Weapon(id),
                conker_assets::Gives::Grenades => Gives::Grenades,
                conker_assets::Gives::Mask => Gives::Mask,
            };
            let pickup = Pickup {
                placement: p.placement,
                gives,
            };
            (pickup, p.position)
        })
        .collect();
    Arena {
        spawns,
        pickups,
        roaming: weapon::Roaming {
            count: roams.clone().count(),
            spots: roams.map(|p| p.roams.clone()).next().unwrap_or_default(),
        },
        respawn,
    }
}

/// The bots' data (the pack's navigation graph, trigger table, AI and class scripts), their AI over
/// the current roster, and the level's lifts.
struct Ai {
    data: [Vec<u8>; 4],
    bots: Option<Bots>,
    lifts: Lifts,
    /// The map's scene (the lifts lead within it).
    scene: u8,
}

impl Ai {
    fn new(data: [Vec<u8>; 4], scene: u8) -> Option<Self> {
        let lifts = Lifts::new(&data[1], scene);
        let ai = Self {
            data,
            bots: None,
            lifts,
            scene,
        };
        // Parse once up front, so a bad pack fails at start and not at the first join.
        ai.assets()
            .map(|a| Bots::new(a, &[], &[0], DEFAULT_SKILL, Rng::default()))??;
        Some(ai)
    }

    /// The map's plates and traps, all at rest and off (scene 0x34's; none elsewhere).
    fn traps(&self) -> traps::Traps {
        traps::Traps::new(u16::from(self.scene), &self.data[1])
    }

    fn assets(&self) -> Option<Assets<'_>> {
        let [nav, triggers, ai, class] = &self.data;
        Some(Assets {
            nav,
            triggers,
            ai,
            class,
        })
    }

    /// A fresh AI over the slots of `humans` and `bots`, at `skill`; with no bot only in the War
    /// modes (`script`), whose AI script referees them.
    fn restart(&mut self, humans: &[usize], bots: &[usize], skill: u8, rng: Rng, script: bool) {
        self.bots = (!bots.is_empty() || script)
            .then(|| Bots::new(self.assets()?, humans, bots, skill, rng))
            .flatten();
    }
}

/// The match and what the server keeps around it.
#[derive(Resource)]
struct Game {
    arena: Arena,
    /// The map's start records and the sides' respawn masks.
    starts: Starts,
    m: Match,
    /// Players that ever joined (a Deathmatch's next team).
    joins: usize,
    setup: Setup,
    /// The tick the match (its clock) starts or started: the first join, or 5 s after the last
    /// `--wait` one; and whether it has.
    start: Option<u32>,
    started: bool,
    history: History,
    ai: Ai,
    /// Over, the slots whose human sent [`Restart`].
    votes: BTreeSet<usize>,
    /// War: the map's outros (the pack's `outros.json`): the winning team each plays for, its
    /// cutscene and its length in ticks ([`Game::outro`]).
    outros: Vec<(u8, u8, u32)>,
    /// Over, the winner as it stood on the first tick over ([`Game::winner`]), so a leaver changes
    /// neither the outro nor the STATS' header.
    won: Option<Option<u8>>,
    /// Scene 0x3F's Deathmatch (scenario 7, V oracle): the bots are cavemen, the humans Tediz, and
    /// every player starts holding item 0x40 ([`Fighter::start`]).
    cavemen: bool,
    /// Scene 0x1A's Deathmatch (scenario 6, V dm6): every player the Wise Guy (class 1, model 59,
    /// the squirrels' body) holding the bat, starting on its slot's Deathmatch record.
    vault: bool,
}

impl Game {
    fn new(arena: Arena, setup: Setup) -> Self {
        let rng = Rng::new(Rng::GAME_SEED ^ setup.seed);
        let m = Match::new(&arena, Vec::new(), rng, 0).with(setup.settings);
        Self {
            arena,
            starts: starts(0x2D).expect("0x2D's tables"),
            m,
            joins: 0,
            setup,
            start: None,
            started: false,
            history: History::default(),
            ai: Ai {
                data: Default::default(),
                bots: None,
                lifts: Lifts::default(),
                scene: 0x2D,
            },
            votes: BTreeSet::new(),
            outros: Vec::new(),
            won: None,
            cavemen: false,
            vault: false,
        }
    }

    /// The item a player on `side` starts holding (War: Beach's villagers: 0x3E), and its class
    /// (Beach's: the next villager's of [`beach::CLASSES`], the Tediz' 3; else 0).
    fn start_item(&self, side: Side) -> Option<u8> {
        match self.setup.settings.beach {
            true => (side == Side::Squirrels).then_some(beach::ITEM),
            false if self.vault => Some(weapon::BAT.id),
            false => self.cavemen.then_some(weapon::CLUB.id),
        }
    }

    fn class(&self, side: Side) -> u8 {
        match (self.setup.settings.beach, side) {
            _ if self.setup.settings.tank => tank::CLASS,
            _ if self.setup.settings.raptor => match side {
                Side::Tediz => raptor::CLASS,
                _ => raptor::CAVEMAN,
            },
            (false, _) => u8::from(self.vault),
            (true, Side::Squirrels) => beach::CLASSES[self.members(side) % beach::CLASSES.len()],
            (true, _) => beach::TEDI_CLASS,
        }
    }

    /// War: Beach caps the Tediz at [`beach::MAX_TEDIZ`] and the villagers at
    /// [`beach::MAX_VILLAGERS`]: `side` full, the other; both full, none.
    /// War: Heist's team row (C `func_151E1744`): with under 3 humans at most 8 players and 2 humans
    /// a team, else 4 and 1; the joiner's team is its slot's (I: the set-up's sizes, checked as
    /// players join).
    fn capped(&self, side: Side) -> Option<Side> {
        if self.setup.settings.heist {
            let humans = self.humans() + 1;
            let (most, per_team) = if humans >= 3 { (4, 1) } else { (8, 2) };
            let team = (self.m.next_slot() & 3) as u8;
            let taken = (0..self.m.players.len()).filter(|i| !self.m.free.contains(i));
            let mates = (taken.clone())
                .filter(|&i| !self.m.players[i].bot && self.m.players[i].team == team)
                .count();
            return (taken.count() < most && mates < per_team).then_some(side);
        }
        if !self.setup.settings.beach {
            return Some(side);
        }
        let other = match side {
            Side::Tediz => Side::Squirrels,
            _ => Side::Tediz,
        };
        let room = |s: Side| match s {
            Side::Tediz => self.members(s) < beach::MAX_TEDIZ,
            _ => self.members(s) < beach::MAX_VILLAGERS,
        };
        [side, other].into_iter().find(|&s| room(s))
    }

    /// Humans in the match (every taken slot after the bots).
    fn humans(&self) -> usize {
        let taken = self.m.players.len() - self.m.free.len();
        taken.saturating_sub(self.setup.bots)
    }

    /// The team of the next player to join, in `slot` on `side`: its own in a Deathmatch, its
    /// side's in Total War and Colors (`0x800E0C00`: 0 the squirrels, 1 the Tediz), its slot's in
    /// Heist (slot & 3, as the oracle's runs set them).
    fn team(&mut self, slot: usize, side: Side) -> u8 {
        let s = self.setup.settings;
        if s.heist {
            return (slot & 3) as u8;
        }
        if s.total_war || s.colors || s.beach {
            return side as u8;
        }
        if s.raptor {
            return u8::from(side == Side::Tediz);
        }
        self.joins += 1;
        (self.joins - 1) as u8
    }

    /// The bots' AI afresh over the roster as it stands (the server's start, the match start): the
    /// humans' slots, then the bots' (a free slot is no participant).
    fn restart_bots(&mut self) {
        let taken = (0..self.m.players.len()).filter(|i| !self.m.free.contains(i));
        let (bots, humans): (Vec<usize>, Vec<usize>) = taken.partition(|&i| self.m.players[i].bot);
        let s = self.setup.settings;
        let script = s.total_war || s.colors || s.beach || s.heist || s.tank || s.raptor;
        self.ai.restart(&humans, &bots, s.skill, self.m.rng, script);
    }

    /// Bot `slot` joins on its start record, with the squirrels in an even slot, else the Tediz
    /// (all cavemen in [`Game::cavemen`]): its fighter, and the entity clients see.
    fn join_bot(&mut self, slot: usize) -> impl Bundle + use<> {
        let side = match self.cavemen {
            // Raptor: 2 raptors in each 8 (I: the oracle's b1 and b2 had 2).
            true if self.setup.settings.raptor && slot % 4 == 2 => Side::Tediz,
            true => Side::Cavemen,
            false if self.one_side() => Side::Squirrels,
            false => (self.capped([Side::Squirrels, Side::Tediz][slot % 2]))
                .expect("parse_args caps Beach's bots"),
        };
        let (place, spawn_mask) = self.placement(slot, side);
        let fighter = Fighter {
            side,
            spawn_mask,
            bot: true,
            start: self.start_item(side),
            class: self.class(side),
            ..Fighter::new(&place, self.team(slot, side), 0)
        };
        self.m.players.push(fighter);
        self.restart_bots();
        info!(
            "bot in slot {slot} on record {} at {:?}, {side:?}",
            place.index, place.pos
        );
        (
            Bot,
            PlayerId(PeerId::Local(slot as u64)),
            Player(fighter.body),
            Fighting::new(slot, &fighter),
            FightingOwner(Entity::PLACEHOLDER),
            combat(&fighter, slot, 0, 0, self.m.settings.lives()),
            Slot { index: slot, place },
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        )
    }

    /// Where slot `slot` on `side` starts, and the records it respawns on (`Fighter::spawn_mask`):
    /// `--spawn`'s place on the slot's Deathmatch record, respawning anywhere (oracle replays). Else
    /// its side's base, as in the original's War mode on the map (Total War on 0x2D, Colors on 0x30,
    /// Raptor on 0x3F, Beach on 0x2B with the squirrels on the villagers' records; its Deathmatch has
    /// no sides; 0x34 has no War mode: the Deathmatch records by slot parity, respawning anywhere):
    /// the side's k-th player starts on `starts.sides[side][k]`, jittered (one RNG draw,
    /// `spawn::place`), and respawns in its base (`starts.side_masks`).
    fn placement(&mut self, slot: usize, side: Side) -> (Placement, u32) {
        let Some(&(pos, facing)) = self.setup.spawns.get(slot) else {
            // ponytail: k counts the side's players now, so after a leave two can share a record
            // (jittered, pushed apart); keep each player's record if that ever matters.
            // The cavemen take the squirrels' base (the Ugas' on 0x3F, `spawn::starts`).
            let base = usize::from(side == Side::Tediz);
            // Heist: the team's k-th player on its k-th record, respawning on the team's four.
            let team = heist::RECORDS[slot & 3];
            if self.setup.settings.heist {
                let place = place(
                    &self.arena.spawns,
                    usize::from(team[(slot >> 2) & 3]),
                    &mut self.m.rng,
                );
                return (place, team.iter().fold(0, |m, &r| m | 1 << r));
            }
            let record = match self.one_side() {
                true => self.starts.deathmatch[slot % 16],
                false => self.starts.sides[base][self.members(side) % 8],
            };
            let place = place(&self.arena.spawns, usize::from(record), &mut self.m.rng);
            // Beach: everyone respawns on its own record (the script's command 0x1F moves it).
            let mask = match self.setup.settings.beach {
                true => 1 << record,
                false => self.starts.side_masks[base],
            };
            return (place, mask);
        };
        let record = usize::from(self.starts.deathmatch[slot % 16]);
        let r = &self.arena.spawns[record];
        let place = Placement {
            index: record,
            pos,
            facing: facing.unwrap_or(u16::from(r.yaw) << 8),
            camera: r.camera,
        };
        (place, !0)
    }

    /// The oracle read that tick `t` stands for, once the match has started.
    fn read(&self, t: u32) -> Option<u32> {
        let start = self.start.filter(|_| self.started)?;
        (t >= start).then(|| self.setup.first_read + (t - start))
    }

    /// War: the winning team (`team_result`; `None` a DRAW, and outside the War modes), latched
    /// once over ([`Game::won`]).
    fn winner(&self) -> Option<u8> {
        let m = &self.m;
        let teams = m.teams();
        (self.won).unwrap_or_else(|| (teams > 0).then(|| teams_result(&m.standings(), teams))?)
    }

    /// Heist: a human's team ranks 1st (the outro's choice, scenario 0: cs2 if so, else cs3).
    fn human_won(&self) -> bool {
        let places = self.m.places();
        (self.m.players.iter().enumerate())
            .any(|(i, f)| !f.bot && !self.m.free.contains(&i) && places.get(i) == Some(&1))
    }

    /// The outro the match's end plays, in ticks: Heist's by whether a human's team won (cutscene
    /// 2, else 3), the other War modes' by the winning team (none on a DRAW), none if the map's
    /// pack has none.
    fn outro(&self) -> u32 {
        let heist = self.m.settings.heist;
        let cutscene = if self.human_won() { 2 } else { 3 };
        let winner = self.winner();
        (self.outros.iter())
            .find(|o| match heist {
                true => o.1 == cutscene,
                false => Some(o.0) == winner,
            })
            .map_or(0, |o| o.2)
    }

    /// The tick the results show from: the end fade's last frame, then in War its outro
    /// ([`Game::outro`]).
    fn results_at(&self) -> Option<u32> {
        Some(self.m.ended_at? + END_DELAY + FADE + 1 + self.outro())
    }

    /// Over, RESTART's fade: the ticks left before the next match, `RESTART_FADE` on the tick the
    /// votes were in down to 1 (`MatchView` phase 3); `None` otherwise.
    fn restarting(&self, t: u32) -> Option<u32> {
        let start = self
            .start
            .filter(|_| !self.started && self.m.phase() == Phase::Over)?;
        start.checked_sub(t).filter(|&left| left > 0)
    }

    /// Over: ticks before the next match starts anyway: [`RESULTS`] once the results show, more
    /// through the outro before them (a client joining mid-outro starts it there).
    fn restart_in(&self, t: u32) -> u32 {
        (self.results_at()).map_or(RESULTS, |r| (r + RESULTS).saturating_sub(t))
    }

    /// 0x1A's Deathmatch and Tank: one side (the squirrels), every slot on its Deathmatch record.
    fn one_side(&self) -> bool {
        self.vault || self.setup.settings.tank
    }

    /// The players on `side` in the match.
    fn members(&self, side: Side) -> usize {
        (self.m.players.iter().enumerate())
            .filter(|&(i, f)| f.side == side && !self.m.free.contains(&i))
            .count()
    }

    /// The side with fewer players in the match, the squirrels on a tie; the Tediz against the
    /// cavemen; on 0x1A always the squirrels ([`Game::vault`]).
    fn smaller_side(&self) -> Side {
        match !self.one_side()
            && (self.cavemen || self.members(Side::Tediz) < self.members(Side::Squirrels))
        {
            true => Side::Tediz,
            false => Side::Squirrels,
        }
    }

    /// Slot `i` left: it stays free for the next to join, nobody else changes slot; its own
    /// projectiles go (`Match::remove_player`).
    fn remove(&mut self, i: usize) {
        self.m.remove_player(i);
        self.votes.remove(&i);
        if let Some(bots) = &mut self.ai.bots {
            bots.leave(i);
        }
    }
}

/// Every slot's position after each of the last [`HISTORY`] ticks (game units).
#[derive(Default)]
struct History(VecDeque<(u32, Vec<[f32; 3]>)>);

impl History {
    fn push(&mut self, tick: u32, positions: Vec<[f32; 3]>) {
        if self.0.len() == HISTORY {
            self.0.pop_front();
        }
        self.0.push_back((tick, positions));
    }

    fn get(&self, tick: u32, slot: usize) -> Option<[f32; 3]> {
        let first = self.0.front()?.0;
        let row = self.0.get(tick.checked_sub(first)? as usize)?;
        (row.0 == tick).then(|| row.1.get(slot).copied())?
    }

    /// Slot `slot` as a client saw it at `tick` + `overstep` (its interpolation time): between the
    /// two recorded states around it, as the client's linear interpolation draws it.
    fn at(&self, slot: usize, tick: u32, overstep: f32) -> Option<[f32; 3]> {
        let a = self.get(tick, slot)?;
        let b = self.get(tick + 1, slot).unwrap_or(a);
        Some(core::array::from_fn(|i| a[i] + (b[i] - a[i]) * overstep))
    }
}

/// A joined player's slot in the match and its start place.
#[derive(Component, Clone, Copy, Debug)]
struct Slot {
    index: usize,
    place: Placement,
}

/// A slot the server's AI plays.
#[derive(Component, Clone, Copy, Debug)]
struct Bot;

/// The client a player's [`Fighting`] goes to: its own (it predicts it); the others read its
/// [`Combat`]. Bots' go to no one ([`Entity::PLACEHOLDER`]).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(immutable)]
struct FightingOwner(Entity);

impl VisibilityFilter for FightingOwner {
    type ClientComponent = RemoteId;
    type Scope = SingleComponent<Fighting>;

    fn is_visible(&self, client: Entity, _: Option<&RemoteId>) -> bool {
        client == self.0
    }
}

/// A human whose client has not sent an input yet (it is still loading or syncing): its fighter
/// stands as it joined, spawn immunity included, and its camera waits. Its client predicts from the
/// first tick it sends an input for, so both start from the same state on the same tick.
#[derive(Component, Clone, Copy, Debug)]
struct Waiting;

/// The player's follow camera, run here too: its aim camera holds the player's stick.
#[derive(Component, Clone, Copy, Debug)]
struct Cam(Follow);

/// A client's latest [`Saw`] messages (on its link entity), newest last.
#[derive(Component, Default, Debug)]
struct Saws(VecDeque<Saw>);

/// A transport accepted a session: let replication flow to it.
fn on_link(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(trigger.entity).insert(ReplicationSender);
}

/// Workaround for one `Server` entity carrying two aeronet transports (lightyear 0.30.1).
///
/// lightyear_aeronet ties each aeronet server entity to the lightyear `Server` with the one-to-one
/// `AeronetLinkOf` -> `AeronetLink` relationship, and Bevy removes the previous source when a second
/// one targets the same entity. So the transport that opens second evicts the first one's
/// `AeronetLinkOf`, lightyear's own `on_connection` never creates a `Link` for that transport's
/// sessions, and its clients time out. This does what `on_connection` would have done.
fn relink_evicted_session(
    trigger: On<Add, Session>,
    sessions: Query<(&ChildOf, &PeerAddr), Without<AeronetLinkOf>>,
    linked_servers: Query<(), With<AeronetLinkOf>>,
    names: Query<&Name>,
    server: Single<Entity, With<Server>>,
    mut commands: Commands,
) {
    let Ok((child_of, peer)) = sessions.get(trigger.entity) else {
        return;
    };
    if linked_servers.contains(child_of.parent()) {
        return; // lightyear links this one itself
    }
    let link = commands
        .spawn((
            LinkOf { server: *server },
            Link::default(),
            PeerAddr(peer.0),
        ))
        .id();
    let transport = names.get(child_of.parent()).map_or("?", |n| n.as_str());
    commands.entity(trigger.entity).insert((
        AeronetLinkOf(link),
        Name::new(format!("{transport} session (relinked)")),
    ));
}

/// A client passed the netcode handshake: it joins with its first message ([`join`]).
fn on_connected(
    trigger: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    mut commands: Commands,
) {
    if let Ok(remote) = clients.get(trigger.entity) {
        commands.entity(trigger.entity).insert(Saws::default());
        info!(
            "client {:?} passed the handshake; it joins with its side",
            remote.0
        );
    }
}

/// A client whose player is in the match.
#[derive(Component)]
struct Joined;

/// A connected client's [`Join`]: its player joins on the side it asked for, else on the side with
/// fewer players ([`Game::smaller_side`]), in the next slot, in its side's base.
#[allow(clippy::type_complexity)]
fn join(
    mut clients: Query<
        (
            Entity,
            &RemoteId,
            Option<&AeronetLink>,
            &mut MessageReceiver<Join>,
        ),
        (With<ClientOf>, With<Connected>, Without<Joined>),
    >,
    names: Query<&Name>,
    timeline: Res<LocalTimeline>,
    mut game: ResMut<Game>,
    mut commands: Commands,
) {
    for (client, remote, session, mut joins) in &mut clients {
        let Some(asked) = joins.receive().last() else {
            continue;
        };
        commands.entity(client).insert(Joined);
        let transport = session
            .and_then(|s| names.get(*s.collection()).ok())
            .map_or("?", |n| n.as_str());
        join_player(
            client,
            remote,
            transport,
            asked,
            &timeline,
            &mut game,
            &mut commands,
        );
    }
}

/// [`join`]'s player: its fighter in the match, and the entity every client sees.
fn join_player(
    client: Entity,
    remote: &RemoteId,
    transport: &str,
    asked: Join,
    timeline: &LocalTimeline,
    game: &mut Game,
    commands: &mut Commands,
) {
    let tick = timeline.tick().0;
    let index = game.m.next_slot();
    // The War modes have two teams: a caveman asked for takes the smaller side (Beach: at most
    // 2 Tediz and 8 villagers).
    let s = game.setup.settings;
    let two_sides = s.total_war || s.colors || s.beach;
    let side = (asked.side)
        .filter(|&s| !two_sides || s != Side::Cavemen)
        .filter(|_| !game.one_side())
        .unwrap_or_else(|| game.smaller_side());
    // Raptor: the Tediz' pick plays a raptor, any other a caveman.
    let side = match s.raptor && side != Side::Tediz {
        true => Side::Cavemen,
        false => side,
    };
    let Some(side) = game.capped(side) else {
        // ponytail: the client stays connected with no player; a kick needs netcode's per-client
        // disconnect, which lightyear's server doesn't expose.
        warn!("client {:?}: the match is full, no player", remote.0);
        return;
    };
    let (place, spawn_mask) = game.placement(index, side);
    let team = game.team(index, side);
    let fighter = Fighter {
        side,
        spawn_mask,
        start: game.start_item(side),
        class: game.class(side),
        ..Fighter::new(&place, team, tick)
    };
    // Its first ride test runs from where it stands, not from the slot's last player.
    game.ai.lifts.reset(index);
    game.m.join(fighter);
    if let Some(bots) = &mut game.ai.bots {
        bots.join(index, false);
    }
    // The clock starts with the first human, or 5 s after the last `--wait` one.
    if game.start.is_none() {
        match game.setup.wait {
            0 => game.start = Some(tick),
            n if n == game.humans() => {
                game.start = Some(tick + game.setup.margin);
                info!(
                    "match starts at tick {} (read {})",
                    tick + game.setup.margin,
                    game.setup.first_read
                );
            }
            _ => {}
        }
    }
    commands.spawn((
        PlayerId(remote.0),
        Player(fighter.body),
        Fighting::new(index, &fighter),
        FightingOwner(client),
        combat(&fighter, index, tick, 0, game.m.settings.lives()),
        Slot { index, place },
        Cam(Follow::spawn(place.camera, &fighter.body)),
        Waiting,
        Replicate::to_clients(NetworkTarget::All),
        PredictionTarget::to_clients(NetworkTarget::Single(remote.0)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(remote.0)),
        ControlledBy {
            owner: client,
            lifetime: default(),
        },
    ));
    info!(
        "client {:?} connected via {transport} as {side:?} (asked {:?}); slot {index} on record {} at {:?} facing {}",
        remote.0, asked.side, place.index, place.pos, place.facing
    );
}

/// A client left: its netcode disconnect or timeout, or its transport closing (lightyear then
/// despawns the player it controls, [`on_left`]).
fn on_disconnected(
    trigger: On<Add, Disconnected>,
    clients: Query<(&Disconnected, &RemoteId), With<ClientOf>>,
) {
    if let Ok((d, id)) = clients.get(trigger.entity) {
        info!("client {:?} left: {}", id.0, d.reason);
    }
}

/// Before lightyear takes this tick's inputs: a [`Waiting`] player whose client sent one for this
/// tick plays from it.
fn first_inputs(
    timeline: Res<LocalTimeline>,
    waiting: Query<(Entity, &Slot, &NativeBuffer<Input>), With<Waiting>>,
    mut commands: Commands,
) {
    let t = timeline.tick();
    for (entity, slot, buffer) in &waiting {
        if buffer.get(t).is_some() {
            commands.entity(entity).remove::<Waiting>();
            info!(
                "slot {} plays from tick {}, its first input",
                slot.index, t.0
            );
        }
    }
}

/// Bytes each client's session sent and received per second over the last 10 s (`NET` lines).
fn log_traffic(
    time: Res<Time<Real>>,
    sessions: Query<(Entity, &Session, &AeronetLinkOf)>,
    ids: Query<&RemoteId>,
    mut at: Local<f64>,
    mut last: Local<HashMap<Entity, (usize, usize)>>,
) {
    let now = time.elapsed_secs_f64();
    let secs = now - *at;
    if secs < 10.0 {
        return;
    }
    *at = now;
    for (entity, session, link) in &sessions {
        let (sent, recv) = (session.stats.bytes_sent.0, session.stats.bytes_recv.0);
        if let (Some((sent0, recv0)), Ok(id)) = (last.insert(entity, (sent, recv)), ids.get(link.0))
        {
            info!(
                "NET client {:?}: {:.0} B/s out, {:.0} B/s in",
                id.0,
                (sent - sent0) as f64 / secs,
                (recv - recv0) as f64 / secs
            );
        }
    }
    last.retain(|e, _| sessions.contains(*e));
}

/// A player's entity went (its client left): free its slot.
fn on_left(trigger: On<Remove, Slot>, slots: Query<&Slot>, mut game: ResMut<Game>) {
    let Ok(&Slot { index, .. }) = slots.get(trigger.entity) else {
        return;
    };
    game.remove(index);
    let players = game.m.players.len() - game.m.free.len();
    info!("slot {index} left; {players} players");
    // The last human gone: the next to join starts a fresh match (`--wait` replays end as they go).
    if game.humans() == 0 && game.setup.wait == 0 {
        (game.start, game.started) = (None, false);
    }
}

/// Over, a client's RESTART: its player's vote for the next match, once the results show.
fn restart_votes(
    mut clients: Query<(Entity, &mut MessageReceiver<Restart>), With<ClientOf>>,
    players: Query<(&Slot, &ControlledBy)>,
    mut game: ResMut<Game>,
) {
    for (client, mut votes) in &mut clients {
        let voted = votes.receive().count() > 0;
        let slot = players
            .iter()
            .find(|p| p.1.owner == client)
            .map(|p| p.0.index);
        let shown = game.results_at().is_some_and(|r| game.m.tick >= r);
        if let Some(slot) = slot.filter(|_| voted && shown) {
            game.votes.insert(slot);
            info!("slot {slot} votes to restart");
        }
    }
}

/// What a client needs of a fighter; `start_lives`: the lives each player starts with.
fn combat(f: &Fighter, slot: usize, tick: u32, lives: u16, start_lives: u8) -> Combat {
    let respawn_in = match f.action {
        Action::Hurt {
            lethal: true, step, ..
        } if f.class == tank::CLASS => u16::from(tank::RESPAWN.saturating_sub(step)),
        Action::Hurt {
            lethal: true, step, ..
        } if f.class == raptor::CLASS => u16::from(raptor::DOWN_AT.saturating_sub(step)),
        Action::Hurt {
            reaction,
            lethal: true,
            step,
        } => {
            let left: u32 = damage::sequence(reaction, true)[usize::from(step)..]
                .iter()
                .map(|s| u32::from(s.1))
                .sum();
            left.saturating_sub(tick - f.since) as u16
        }
        _ => 0,
    };
    Combat {
        slot: slot as u16,
        team: f.team,
        stun: f.stun,
        side: f.side,
        health: f.health,
        weapon: f.weapon.unwrap_or(0),
        drawn: f.drawn,
        rounds: f.rounds,
        grenades: f.grenades,
        kills: f.kills,
        suicides: f.suicides,
        immune: f.immune,
        anim: f.anim,
        respawn_in,
        lives,
        deaths: f.deaths,
        shots: f.shots,
        out: f.out,
        // A life goes at the respawn (V `func_15085710(i,4,1)`), not at the lethal hit.
        lives_left: if f.out > 0 {
            0
        } else {
            let dying = matches!(f.action, Action::Hurt { lethal: true, .. });
            (u16::from(start_lives) + f.extra + u16::from(dying)).saturating_sub(f.deaths) as u8
        },
        hits: f.hits,
        headshots: f.headshots,
        carrying: f.carrying,
        masked: f.masked,
        haze: f.haze(tick).map_or(0, |k| k as u8),
        // Colors' flags only (Heist's bag is `MatchView::cash`).
        flag: f.flag.filter(|&k| usize::from(k) < colors::FLAGS.len()),
        score: 0,
        class: f.class,
        freeze: 0,
        deadline: 0,
        bags: f.bags,
        hit: 0,
        cook: f.cook,
        power: f.power,
        power_left: if f.power == 0 { 0 } else { f.boost },
    }
}

/// The weapon behind a hit on `victim` by `attacker` (the victim itself when the hit has no owner),
/// for the clients' hit sounds ([`Combat::hit`]), from the match before (`pre`) and after (`m`)
/// the step: a blast (reactions 0x2B, 0x2C) is the attacker's rocket if one of its rockets went
/// off, its shotgun's while it holds one and none of its grenades went off, else a grenade; a
/// burning victim's is the flame; a seated attacker's the mounted gun (0x1B); else the weapon the
/// attacker held, none for an unowned hit. I: a weapon switched while its bullet flew names the
/// new one (an exact weapon would need conker-rules' `Event::Hit` to carry it).
fn hit_weapon(pre: &Match, m: &Match, victim: usize, attacker: usize) -> u8 {
    let (Some(a), Some(v)) = (pre.players.get(attacker), m.players.get(victim)) else {
        return 0;
    };
    // The attacker's projectiles (birth tick, owner) before the step that are gone after it.
    let went_off = |was: Vec<(u32, usize)>, is: Vec<(u32, usize)>| {
        was.into_iter().any(|r| r.1 == attacker && !is.contains(&r))
    };
    let rocket = went_off(
        pre.rockets.iter().map(|r| (r.0, r.1.owner)).collect(),
        m.rockets.iter().map(|r| (r.0, r.1.owner)).collect(),
    );
    let grenade = went_off(
        pre.grenades.iter().map(|g| (g.0, g.1.owner)).collect(),
        m.grenades.iter().map(|g| (g.0, g.1.owner)).collect(),
    );
    let blast = matches!(
        v.action,
        Action::Hurt {
            reaction: 0x2B | 0x2C,
            ..
        }
    );
    match () {
        _ if blast && rocket => 0x16,
        _ if blast && a.weapon == Some(0x37) && !grenade => 0x37,
        _ if blast => 0x26,
        _ if pre.players.get(victim).is_some_and(|v| v.burn.is_some()) => 0x15,
        _ if a.seated() => 0x1B,
        _ if attacker == victim => 0,
        _ => a.weapon.unwrap_or(0),
    }
}

/// War: Beach's script globals ([`brain::g`] has the shared struct's; these are 0xB9's own): per
/// participant the escape's freeze and the 90 s deadline (G+0x1574, `g::TIMER3`); the zones'
/// holders (participants, −1 none) and the plunger's cooldown.
mod beach_g {
    pub const FREEZE: i32 = 0x224C;
    pub const SNIPER: i32 = 0x230C;
    pub const BAZOOKA: i32 = 0x2308;
    pub const COOLDOWN: i32 = 0x22F4;
}

/// This tick's events, for [`publish`] and the logs, and each hit's victim and weapon
/// ([`hit_weapon`]).
#[derive(Resource, Default)]
struct Events(Vec<Event>, Vec<(usize, u8)>);

/// One match frame: the start of a `--wait` match, `--give`s, the step, the cameras, the history.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn simulate(
    mut world: ResMut<Collision>,
    timeline: Res<LocalTimeline>,
    mut game: ResMut<Game>,
    mut players: Query<(
        &Slot,
        &mut Cam,
        &ActionState<Input>,
        &ControlledBy,
        Has<Waiting>,
    )>,
    bots: Query<&Slot, With<Bot>>,
    waiting: Query<&Slot, With<Waiting>>,
    delays: Query<&InterpolationDelay>,
    saws: Query<&Saws>,
    mut commands: Commands,
) {
    let t = timeline.tick().0;
    let game = &mut *game;
    game.m.tick = t;
    // Over, the results: the next match (the same settings) once every human in it voted, or
    // `RESULTS` ticks on. Not in `--wait` replays (they end as the game does), nor with no human in
    // (the next to join starts it).
    let mut humans = players.iter().map(|p| p.0.index).peekable();
    let some_human = humans.peek().is_some();
    let voted = humans.all(|s| game.votes.contains(&s));
    if game.m.phase() == Phase::Over
        && game.started
        && game.setup.wait == 0
        && some_human
        && (voted || game.restart_in(t) == 0)
    {
        // RESTART fades to black first (phase 3, `RESTART_FADE` ticks); the deadline starts the
        // next match at once (the original's deadline quits, with no fade).
        let fade = if voted { RESTART_FADE } else { 0 };
        (game.start, game.started) = (Some(t + fade), false);
        game.votes.clear();
        // The RNG goes on from the bots' (they draw the most): a match no draw of the rules' RNG
        // ran in (a kill limit of 1, no respawn) would otherwise replay the last one.
        if let Some(bots) = &game.ai.bots {
            game.m.rng = bots.rng();
        }
        info!("next match at tick {t}");
    }
    if !game.started && game.start.is_some_and(|s| s <= t) {
        (game.start, game.started) = (Some(t), true);
        // Everyone back on its start place, the clock at the read it stands for.
        // A replay from later in a match starts with the spawn immunity as far run down, standing.
        let since_live = game.setup.first_read.saturating_sub(LIVE_READ);
        let mut fighters = std::mem::take(&mut game.m.players);
        for (slot, mut cam, ..) in &mut players {
            let f = &mut fighters[slot.index];
            *f = Fighter {
                side: f.side,
                spawn_mask: f.spawn_mask,
                bot: f.bot,
                start: f.start,
                class: f.class,
                ..Fighter::new(&slot.place, f.team, t)
            };
            if since_live > 0 {
                f.immune = SPAWN_IMMUNE.saturating_sub(since_live.min(255) as u8);
                f.anim = u16::from(f.body.state);
            }
            cam.0 = Follow::spawn(slot.place.camera, &f.body);
        }
        for slot in &bots {
            let f = &mut fighters[slot.index];
            *f = Fighter {
                side: f.side,
                spawn_mask: f.spawn_mask,
                bot: true,
                start: f.start,
                class: f.class,
                ..Fighter::new(&slot.place, f.team, t)
            };
        }
        game.m = Match {
            free: std::mem::take(&mut game.m.free),
            guns: gun::spawn(usize::from(game.ai.scene)),
            traps: game.ai.traps(),
            ..Match::new(&game.arena, fighters, game.m.rng, t)
        }
        .with(game.setup.settings);
        game.ai.lifts = Lifts::new(&game.ai.data[1], game.ai.scene);
        game.restart_bots();
        game.won = None;
        game.votes.clear();
        if game.m.settings.time.is_some() {
            game.m.timer -= 2 * (game.setup.first_read as i32 - (LIVE_READ as i32 - 1));
        }
        info!("match started at tick {t} = read {}", game.setup.first_read);
    }
    if let Some(read) = game.read(t) {
        for &(slot, at, id) in &game.setup.gives {
            if at == read && slot < game.m.players.len() {
                game.m.give(slot, id);
                info!("gave slot {slot} weapon {id:#x} at read {read}");
            }
        }
        for &(slot, at, pos) in &game.setup.warps {
            if at == read && slot < game.m.players.len() {
                game.m.players[slot].body.pos = pos;
                info!("warped slot {slot} to {pos:?} at read {read}");
            }
        }
    }
    let n = game.m.players.len();
    let mut inputs = vec![Input::default(); n];
    // What each human showed of the others when it sampled this tick's input: its Saw, else
    // lightyear's estimate (a fraction of a tick off).
    let mut views = vec![None; n];
    for (slot, _, action, owner, _) in &players {
        inputs[slot.index] = action.0;
        let held = saws.get(owner.owner).ok();
        let saw = held.and_then(|s| s.0.iter().rev().find(|s| s.tick == t).copied());
        if saw.is_none() {
            let newest = held.and_then(|s| s.0.back()).map(|s| s.tick);
            debug!("NOSAW t={t} slot={} newest={newest:?}", slot.index);
        }
        views[slot.index] = match saw {
            Some(s) => Some(View {
                tick: s.at,
                overstep: s.overstep,
                saw: true,
            }),
            None => delays.get(owner.owner).ok().map(|d| {
                let (tick, overstep) = d.tick_and_overstep(Tick(t));
                View {
                    tick: tick.0,
                    overstep,
                    saw: false,
                }
            }),
        };
    }
    game.m.waiting = waiting.iter().map(|s| s.index).collect();
    // A lift takes only the living, without a death: any other move is a kill gate's.
    let before: Vec<(u16, u8, [f32; 3])> = (game.m.players.iter())
        .map(|f| (f.deaths, f.health, f.body.pos))
        .collect();
    let rides = game.ai.lifts.ride(&mut game.m, &game.arena);
    for (p, f) in game.m.players.iter().enumerate() {
        let Some(&(deaths, health, pos)) = before.get(p) else {
            continue;
        };
        let human = matches!(
            f.action,
            Action::Hurt {
                reaction: damage::GATE,
                ..
            }
        ) && f.since == game.m.tick;
        let bot = rides.iter().any(|r| r.0 == p) && (deaths != f.deaths || health == 0);
        if human || bot {
            info!(
                "t={t} slot {p} falls through a kill gate at {pos:?}: deaths {}, scores {:?}",
                f.deaths, game.m.colors.scores
            );
        }
    }
    for &(p, place) in &rides {
        info!("t={t} slot {p} takes the lift to record {}", place.index);
    }
    // War: Beach's plunger and 0x34's plates: their collision where the props are this frame.
    if game.m.settings.beach {
        beach::seat(&mut world.0, game.m.plunger.y);
    }
    if !game.m.traps.list.is_empty() {
        traps::seat(&mut world.0, game.m.traps.offsets());
    }
    // The AI script's effects (Total War's gas) go in before the step, its hits' events first.
    let mut events = Vec::new();
    if let Some(bots) = &mut game.ai.bots {
        for &(p, _) in &rides {
            bots.lifted(p);
        }
        let effects = bots.tick(&game.m, &game.arena, &world, &mut inputs);
        let countdown = game.m.gas.countdown;
        let tag = match () {
            _ if game.m.settings.beach => "BEACH",
            _ if game.m.settings.heist => "HEIST",
            _ if game.m.settings.tank => "TANK",
            _ => "COLORS",
        };
        for e in &effects {
            match e {
                Effect::Countdown(300) if countdown != 300 => info!("GAS t={t} delivered"),
                Effect::Show(n @ (27 | 28)) if !game.m.gas.prop(*n).is_some_and(|p| p.shown) => {
                    info!("GAS t={t} canister on placement {n} back")
                }
                Effect::Carry(..) | Effect::Gas(_) | Effect::Release(_) | Effect::Haze(_) => {
                    info!("GAS t={t} {e:?}")
                }
                Effect::Flag(..)
                | Effect::LetGo(_)
                | Effect::Score(..)
                | Effect::Respawn(_)
                | Effect::Fade(..)
                | Effect::Gun(_) => info!("{tag} t={t} {e:?}"),
                // Beach's: the start records, the strikes, the ejections, and the plunger's hold
                // and the zones' gives when they change.
                Effect::Start(..) | Effect::Strike(_) | Effect::Place(..)
                    if game.m.settings.beach =>
                {
                    info!("BEACH t={t} {e:?}")
                }
                Effect::Plunger(held) if *held != game.m.plunger.held => info!("BEACH t={t} {e:?}"),
                // Heist's bag made and gone, and the voices.
                Effect::Cash(_) | Effect::Voice(..) => info!("HEIST t={t} {e:?}"),
                // Raptor's eggs, throws, nest drops and cooked eggs (not the turns).
                Effect::Raptor(op) if !matches!(op, raptor::Op::Face(..)) => {
                    info!("RAPTOR t={t} {op:?}")
                }
                Effect::Give(i, w)
                    if (game.m.players.get(*i)).is_some_and(|f| f.weapon.unwrap_or(0) != *w) =>
                {
                    info!("BEACH t={t} {e:?}")
                }
                _ => {}
            }
        }
        let carrier = game.m.tank.carrier;
        events = game.m.apply(&effects);
        bots.scopes(&mut game.m);
        // Tank: the bomb taken (the script's 0x4B), the plant, and the blast's kills (the bomber's).
        if game.m.settings.tank {
            if let (None, Some(c)) = (carrier, game.m.tank.carrier) {
                info!("TANK t={t} bomb taken by slot {c}");
            }
            if countdown <= 0 && game.m.gas.countdown > 0 {
                let (by, bunker) = (game.m.tank.last, game.m.tank.planted);
                info!("TANK t={t} bomb planted by slot {by:?} in bunker {bunker:?}");
            }
            for e in &events {
                if let Event::Hit {
                    victim,
                    attacker,
                    lethal: true,
                    ..
                } = *e
                {
                    info!("TANK t={t} blast kills slot {victim} (bomber {attacker})");
                }
            }
        }
    }
    // Tank: the bunkers' doors collide where they stand (shut: in place; open: gone).
    if game.m.settings.tank {
        tank::seat(&mut world.0, game.m.gas.lifts());
    }
    // Scene 0x2D's doors and covers: their collision where the props stand once the script's
    // effects are in (outside Total War the Deathmatch's set).
    // ponytail: the step's own slide (up 10 or 40, down 2 or 8, the hide) collides a frame late.
    if game.ai.scene == 0x2D {
        let g = &game.m.gas;
        gas::seat(
            &mut world.0,
            game.m.settings.total_war,
            g.shown(),
            g.lifts(),
        );
    }
    // A waiting player is restored after the step, so nothing moves it meanwhile (and nothing hurts
    // it: `Match::waiting`).
    let held: Vec<(usize, Fighter)> = (waiting.iter())
        .map(|s| (s.index, game.m.players[s.index]))
        .collect();
    let history = &game.history;
    let seen = |s: usize, v: usize| {
        let view: View = views.get(s).copied().flatten()?;
        history.at(v, view.tick, view.overstep)
    };
    let pre = game.m.clone();
    events.extend(if game.setup.lagcomp {
        game.m.step_seen(&game.arena, &inputs, &world, &seen)
    } else {
        game.m.step(&game.arena, &inputs, &world)
    });
    lag(&pre, &game.m, &events, &world, &seen, &views, t);
    for (p, (was, f)) in pre.players.iter().zip(&game.m.players).enumerate() {
        let high = |f: &Fighter| f.body.state == state::HIGH_JUMP;
        if high(was) != high(f) {
            let what = if high(f) { "takes" } else { "ends" };
            let ([x, y, z], st) = (f.body.pos, f.body.state);
            info!("t={t} slot {p} {what} a high jump at ({x:.0}, {y:.0}, {z:.0}) state {st}");
        }
    }
    // Beach's villager deaths score in the step.
    if game.m.colors.scores != pre.colors.scores && game.m.settings.beach {
        info!(
            "BEACH t={t} villager down: scores {:?}",
            game.m.colors.scores
        );
    }
    if game.m.colors.scores != pre.colors.scores && game.m.settings.raptor {
        info!("RAPTOR t={t} scores {:?}", game.m.colors.scores);
    }
    if game.m.ended_at.is_some() && pre.ended_at.is_none() {
        info!(
            "t={t} match ends: places {:?}, scores {:?}, bags {:?}",
            game.m.places(),
            game.m.colors.scores,
            game.m.heist.scores
        );
    }
    // Tank: the power-ups taken.
    if game.m.settings.tank {
        for e in &events {
            if let Event::Pickup { player, pickup } = *e {
                let gives = game.arena.pickups.get(pickup).map(|p| p.0.gives);
                info!("TANK t={t} slot {player} takes power-up {gives:?}");
            }
        }
    }
    // Heist: the bag taken.
    if let (None, Some(c)) = (pre.heist.cash.carrier, game.m.heist.cash.carrier) {
        let team = game.m.players.get(c).map(|f| f.team);
        info!("HEIST t={t} bag taken by slot {c} (team {team:?})");
    }
    for (k, (was, f)) in pre
        .colors
        .flags
        .iter()
        .zip(&game.m.colors.flags)
        .enumerate()
    {
        if let (None, Some(c)) = (was.carrier, f.carrier) {
            info!("COLORS t={t} flag {k} taken by slot {c}");
        }
    }
    // 0x34's plates going down and the traps they switch on, with the slot they credit.
    let (traps, was) = (&game.m.traps, &pre.traps);
    for k in 0..traps.plates.len() {
        if traps.plates[k].state == 3 && was.plates[k].state != 3 {
            let on =
                (0..traps.list.len()).filter(|&j| traps.list[j].plate == k && was.left[j] == 0);
            let owner = on.clone().find_map(|j| traps.owner[j]);
            info!(
                "TRAPS t={t} plate {k} down: {} traps on for slot {owner:?}",
                on.count()
            );
        }
    }
    for k in 0..game.m.guns.len() {
        let (was, now) = (pre.gun_state(k), game.m.gun_state(k));
        if was != now {
            let by = game.m.gunner(k).or(pre.gunner(k));
            info!("GUN t={t} gun {k} state {was:#x} -> {now:#x} slot {by:?}");
        }
    }
    for (i, f) in held {
        game.m.players[i] = f;
    }
    // A respawn is no move: the next trigger test starts from the record (a corpse's teleport
    // could cross a gate); nor is a takeover, for either body.
    for e in &events {
        match *e {
            Event::Respawn { player, .. } => game.ai.lifts.reset(player),
            Event::TakeOver { player, from } => {
                info!("TW t={t} slot {player} takes over bot {from}'s body");
                game.ai.lifts.reset(player);
                game.ai.lifts.reset(from);
            }
            _ => {}
        }
    }
    if game.won.is_none() && game.m.phase() == Phase::Over {
        game.won = Some(game.winner());
    }
    if let Some(bots) = &mut game.ai.bots {
        bots.observe(&events, &game.m);
    }
    // The cameras, with what the rules know of each player (its weapon's aim, the scope, the
    // twist); their aim gate and scope go into the next step.
    let next = game.m.tick;
    for (slot, mut cam, action, _, waiting) in &mut players {
        if waiting {
            continue;
        }
        let f = &mut game.m.players[slot.index];
        if let Some((_, place)) = rides.iter().find(|r| r.0 == slot.index) {
            cam.0 = Follow::spawn(place.camera, &f.body);
        }
        let respawn = events.iter().find_map(|e| match e {
            Event::Respawn { player, at } if *player == slot.index => Some(at),
            _ => None,
        });
        if let Some(at) = respawn {
            cam.0 = Follow::spawn(at.camera, &f.body);
        }
        // A takeover snaps the camera behind the new body (`func_15128774`, V t1 438).
        let took = |e: &Event| matches!(e, Event::TakeOver { player, .. } if *player == slot.index);
        if events.iter().any(took) {
            cam.0 = Follow::spawn(f.body.pos, &f.body);
        }
        cam.0.step_view(f.camera_view(next), &action.0, &world);
        f.take_camera(&cam.0);
    }
    let positions = game.m.players.iter().map(|f| f.body.pos).collect();
    game.history.push(t, positions);
    let mut hits = (events.iter())
        .filter_map(|e| match *e {
            Event::Hit {
                victim, attacker, ..
            } => Some((victim, hit_weapon(&pre, &game.m, victim, attacker))),
            _ => None,
        })
        .collect::<Vec<_>>();
    // A head hit's −1 above 2 health (`func_151D4DAC`) has no event and no sound: no weapon.
    let quiet = (pre.players.iter().zip(&game.m.players).enumerate())
        .filter(|(v, (was, is))| is.health < was.health && !hits.iter().any(|h| h.0 == *v))
        .map(|(v, _)| (v, 0))
        .collect::<Vec<_>>();
    hits.extend(quiet);
    commands.insert_resource(Events(events, hits));
    commands.insert_resource(Rides(rides.iter().map(|r| r.0).collect()));
}

/// The slots a lift moved this tick (a placement, as a respawn is: [`publish`] counts it in `lives`).
#[derive(Resource, Default)]
struct Rides(Vec<usize>);

/// Bullets tested this tick: the carried ones, then the ones born (rebuilt from the shooter's state
/// at the frame's start, as conker-rules fires them).
fn tested_bullets(pre: &Match, events: &[Event], world: &CollisionWorld) -> Vec<Bullet> {
    let mut out: Vec<Bullet> = pre.bullets.iter().map(|b| b.1).collect();
    for e in events {
        if let &Event::Shot { player, pos } = e
            && let Some(f) = pre.players.get(player).filter(|f| !f.seated())
            && let Some(Attack::Bullets { kind, .. }) = f.weapon().map(|w| w.attack)
        {
            out.push(Bullet::fire(world, player, pos, f.body.facing, kind));
        }
    }
    out
}

/// The first body a bullet meets among `bodies` (conker-rules' `first_body`, positions and
/// hitboxes given; standing).
fn first_hit(b: &Bullet, bodies: &[Option<[f32; 3]>], hitboxes: &[Hitbox]) -> Option<usize> {
    bodies
        .iter()
        .enumerate()
        .filter(|&(v, _)| v != b.owner)
        .filter_map(|(v, p)| {
            hits_body(b.pos, b.dir, b.reach(), (*p)?, hitboxes[v], false).map(|d| (d, v))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, v)| v)
}

/// A shooter's view: the interpolation time its screen showed the others at, and whether its
/// client said so ([`Saw`]) or lightyear's `InterpolationDelay` estimated it.
#[derive(Clone, Copy, Debug)]
struct View {
    tick: u32,
    overstep: f32,
    saw: bool,
}

/// Logs every bullet tested this tick (`LAGCOMP`): what it meets among the bodies where they are now
/// and where its shooter saw them (`seen`, the positions the step's hit tests used, per slot; dead
/// bodies `-`), and whether the rewind changed it.
fn lag(
    pre: &Match,
    post: &Match,
    events: &[Event],
    world: &CollisionWorld,
    seen: conker_rules::game::Seen,
    views: &[Option<View>],
    t: u32,
) {
    let dead = conker_anim::combat::DEAD;
    let alive = |v: usize| post.players[v].anim != dead;
    let now: Vec<Option<[f32; 3]>> = (0..post.players.len())
        .map(|v| alive(v).then_some(post.players[v].body.pos))
        .collect();
    let hitboxes: Vec<Hitbox> = post.players.iter().map(Fighter::hitbox).collect();
    for b in tested_bullets(pre, events, world) {
        let saw: Vec<Option<[f32; 3]>> = (0..post.players.len())
            .map(|v| alive(v).then(|| seen(b.owner, v).unwrap_or(post.players[v].body.pos)))
            .collect();
        let current = first_hit(&b, &now, &hitboxes);
        let rewound = first_hit(&b, &saw, &hitboxes);
        let (at, overstep, told) = views
            .get(b.owner)
            .copied()
            .flatten()
            .map_or((0, 0.0, false), |v| (v.tick, v.overstep, v.saw));
        let positions: Vec<String> = saw
            .iter()
            .map(|p| {
                // As f64: the exact value, as the clients' SEEN lines log it.
                p.map_or("-".into(), |p| {
                    let [x, y, z] = p.map(f64::from);
                    format!("{x},{y},{z}")
                })
            })
            .collect();
        debug!(
            "LAGCOMP t={t} shooter={} at={at} overstep={overstep} saw={told} current={current:?} rewound={rewound:?} differs={} seen={}",
            b.owner,
            current != rewound,
            positions.join(";")
        );
    }
}

/// Moves each client's [`Saw`] messages into its [`Saws`] (the last [`HISTORY`]).
fn collect_saws(mut links: Query<(&mut MessageReceiver<Saw>, &mut Saws)>) {
    for (mut received, mut saws) in &mut links {
        for saw in received.receive() {
            if saws.0.len() == HISTORY {
                saws.0.pop_front();
            }
            saws.0.push_back(saw);
        }
    }
}

/// Copies the match into the replicated components, and counts respawns.
#[allow(clippy::type_complexity)]
fn publish(
    game: Res<Game>,
    events: Res<Events>,
    rides: Res<Rides>,
    mut players: Query<(&Slot, &mut Player, &mut Fighting, &mut Combat)>,
    mut view: Single<&mut MatchView>,
) {
    let m = &game.m;
    let standings = m.standings();
    let beach = game.ai.bots.as_ref().filter(|_| m.settings.beach);
    for (slot, mut player, mut fighting, mut combat_state) in &mut players {
        let Some(f) = m.players.get(slot.index) else {
            continue;
        };
        // A takeover moves the camera as a respawn does (the client cuts on `lives`).
        let respawned = events.0.iter().any(|e| match *e {
            Event::Respawn { player, .. } | Event::TakeOver { player, .. } => player == slot.index,
            _ => false,
        });
        let rode = rides.0.contains(&slot.index);
        let lives = combat_state.lives + u16::from(respawned) + u16::from(rode);
        player.set_if_neq(Player(f.body));
        fighting.set_if_neq(Fighting::of(m, slot.index));
        let mut c = combat(f, slot.index, m.tick, lives, m.settings.lives());
        let hit = events.1.iter().rev().find(|h| h.0 == slot.index);
        c.hit = hit.map_or(combat_state.hit, |h| h.1);
        if m.settings.colors || m.settings.beach || m.settings.heist || m.settings.raptor {
            // Colors and Beach: no life is lost; the team's score ranks the player (Heist's and
            // Raptor's too).
            if m.settings.colors || m.settings.beach {
                c.lives_left = colors::LIVES;
            }
            c.score = standings
                .get(slot.index)
                .copied()
                .flatten()
                .map_or(0, |s| s.score);
        }
        if let Some((b, k)) = beach.and_then(|b| Some((b, b.by_slot(slot.index)?))) {
            let g = b.globals();
            c.freeze = g.get(beach_g::FREEZE, k).clamp(0, 255) as u8;
            c.deadline = g.get(g::TIMER3, k).clamp(0, 0xFFFF) as u16;
        }
        combat_state.set_if_neq(c);
    }
    let holder = |off| {
        let b = beach?;
        b.slot(usize::try_from(b.globals().i32(off)).ok()?)
            .map(|s| s as u16)
    };
    let taken = game
        .arena
        .pickups
        .iter()
        .zip(&m.pickups)
        .filter(|(_, s)| s.timer > 0)
        .fold(0u64, |acc, (p, _)| acc | 1 << p.0.placement);
    let s = m.settings;
    let voice = view.voice;
    view.set_if_neq(MatchView {
        timer: m.timer,
        time: s.time.unwrap_or(0),
        lives: s.lives(),
        kill_limit: s.kills,
        radar: s.radar,
        total_war: s.total_war,
        restart_in: match game.restarting(m.tick) {
            Some(left) => left as u16,
            None => game.restart_in(m.tick) as u16,
        },
        winner: game.winner(),
        phase: match m.phase() {
            _ if game.restarting(m.tick).is_some() => 3,
            Phase::Playing => 0,
            Phase::Fading => 1,
            Phase::Over => 2,
        },
        // Scripted clients key on it only in `--wait` replays (else they start on their own sync),
        // and idle until it is set (u32::MAX: not every player is in yet).
        start: (game.setup.wait > 0).then(|| game.start.unwrap_or(u32::MAX)),
        first_read: game.setup.first_read,
        bullets: (m.bullets.iter())
            .map(|&(born, b)| BulletView {
                pos: b.pos,
                vel: b.vel,
                life: b.life,
                born,
                owner: b.owner as u16,
            })
            .collect(),
        grenades: m.grenades.iter().map(|g| g.1.pos).collect(),
        rockets: m.rockets.iter().map(|r| r.1.pos).collect(),
        shots: m.shots.iter().map(|s| s.1.pos).collect(),
        taken,
        roaming: (0..game.arena.roaming.count)
            .map(|k| {
                (
                    game.arena.pickups[k].0.placement,
                    m.pickup_at(&game.arena, k),
                )
            })
            .collect(),
        countdown: m.gas.countdown,
        gas: m.gas.gas,
        props: m.gas.shown(),
        lifts: m.gas.lifts(),
        colors: s.colors,
        score_limit: s.score,
        scores: m.colors.scores,
        flags: m.colors.flags.map(|f| FlagView {
            shown: f.shown,
            carrier: f.carrier.map(|c| c as u16),
            home: f.home(),
            touchable: f.touchable(m.tick),
            pos: f.pos,
        }),
        fade: m.colors.fade,
        guns: (m.guns.iter().enumerate())
            .map(|(k, g)| GunView {
                pos: g.place.pos,
                yaw: g.yaw,
                pitch: g.pitch10 as f32 / 10.0,
                state: m.gun_state(k),
                gunner: m.gunner(k).map(|i| i as u16),
                shots: g.shots,
                barrel: g.last,
                trigger: m
                    .gunner(k)
                    .and_then(|i| m.players[i].gun)
                    .is_some_and(|g| g.trigger),
            })
            .collect(),
        beach: s.beach,
        holders: [holder(beach_g::SNIPER), holder(beach_g::BAZOOKA)],
        plunger: m.plunger.y,
        cooldown: beach.map_or(0, |b| b.globals().i32(beach_g::COOLDOWN).max(0) as u16),
        plates: m.traps.offsets(),
        traps: m.traps.on(),
        heist: s.heist,
        heist_scores: m.heist.scores,
        cash: {
            let c = &m.heist.cash;
            CashView {
                shown: c.shown,
                carrier: c.carrier.map(|i| i as u16),
                pos: c.pos,
                yaw: c.yaw,
                state: c.state as u8,
                since: c.since,
                touchable: c.touchable(m.tick),
            }
        },
        human_won: s.heist && m.ended_at.is_some() && game.human_won(),
        voice: (events.0.iter().rev())
            .find_map(|e| match *e {
                Event::Voice { player, id } => Some((m.tick, player as u16, id)),
                _ => None,
            })
            .or(voice),
        tank: s.tank,
        tanks: TankView {
            shells: (m.tank.shells.iter())
                .map(|sh| (sh.pos, sh.angle, sh.y_vel / sh.speed.max(1.0)))
                .collect(),
            bomb: m.tank.bomb.shown,
            carrier: m.tank.carrier.map(|c| c as u16),
            planted: m.tank.planted.filter(|_| m.gas.countdown > 0),
            wrecks: (m.wrecks().into_iter())
                .map(|(k, pos, facing)| (k as u16, pos, facing))
                .collect(),
        },
        raptor: s.raptor,
        raptors: RaptorView {
            eggs: (m.raptor.eggs).map(|e| (e.shown, e.carrier.map(|c| c as u16), e.pos)),
            cooked: m.raptor.cooked,
            fed: m.raptor.fed,
            prey: (m.players.iter().enumerate())
                .filter(|(i, f)| f.prey && !m.free.contains(i))
                .fold(0, |b, (i, _)| b | 1 << i),
            drops: m.raptor.drops.clone(),
        },
    });
}

/// `STATE` (position) and `COMBAT` lines per player and tick (debug), `EVENT` lines (info), and a
/// summary once a second.
#[allow(clippy::type_complexity)]
fn log_players(
    timeline: Res<LocalTimeline>,
    game: Res<Game>,
    events: Res<Events>,
    players: Query<(
        &PlayerId,
        &Slot,
        &Player,
        &Combat,
        Option<&ActionState<Input>>,
    )>,
) {
    let tick = timeline.tick().0;
    let read = game.read(tick).map_or(String::from("-"), |r| r.to_string());
    if let Some(b) = &game.ai.bots
        && tick.is_multiple_of(300)
        && (b.faults > 0 || !b.unknown.is_empty())
    {
        warn!("bots: {} faults, unknown natives {:?}", b.faults, b.unknown);
    }
    for e in &events.0 {
        if !matches!(e, Event::Shot { .. }) {
            info!("EVENT t={tick} read={read} {e:?}");
        }
    }
    for (id, slot, p, c, input) in &players {
        // As f64: the exact value (an f32's shortest decimal reads back up to half an ulp off).
        let [x, y, z] = p.pos.map(f64::from);
        debug!(
            "STATE t={tick} p={:?} pos={x},{y},{z} st={} fy={}",
            id.0, p.state, p.floor_y
        );
        debug!(
            "COMBAT t={tick} read={read} slot={} hp={} anim={:#x} w={:#04x} drawn={} rounds={} gren={} imm={} kills={} timer={}",
            slot.index,
            c.health,
            c.anim,
            c.weapon,
            c.drawn,
            c.rounds,
            c.grenades,
            c.immune,
            c.kills,
            game.m.timer
        );
        if tick.is_multiple_of(30) {
            // A bot: no input; its AI state, goal node and behaviour script, its node and waypoint.
            let k = game.ai.bots.as_ref().and_then(|b| b.by_slot(slot.index));
            let bots = game.ai.bots.as_ref();
            let ai = bots.and_then(|b| b.state(k?));
            let walk = bots.and_then(|b| b.npc(k?)).map(|n| (n.cur, n.waypoint));
            // Its enemy (G+0x11C) and target (G+0x31C − 2), as slots.
            let fight = bots.zip(k).map(|(b, k)| {
                let slot = |p: i32| usize::try_from(p).ok().and_then(|p| b.slot(p));
                let at = |off| b.globals().get(off, k);
                (slot(at(g::ENEMY)), slot(at(g::TARGET) - 2))
            });
            info!(
                "tick {tick} player {:?} input {:?} ai {ai:?} fight {fight:?} walk {walk:?} pos ({x:.2}, {y:.2}, {z:.2}) state {} hp {} weapon {:#04x} kills {}",
                id.0,
                input.map(|a| a.0),
                p.state,
                c.health,
                c.weapon,
                c.kills
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conker_rules::game::PickupState;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn replay_flags_parse() {
        let (setup, net) = parse_args(args(&[
            "--spawn",
            "-123.5,-293.75,112.25,25345",
            "--spawn",
            "1,2,3",
            "--give",
            "0:745:0x09",
            "--warp",
            "0:1000:-163,-1448,-33",
            "--wait",
            "2",
            "--first-read",
            "745",
            "--port",
            "6000",
            "--bots",
            "2",
        ]))
        .unwrap();
        assert_eq!(setup.bots, 2);
        assert_eq!(setup.spawns[0], ([-123.5, -293.75, 112.25], Some(25345)));
        assert_eq!(setup.spawns[1], ([1.0, 2.0, 3.0], None));
        assert_eq!(setup.gives, vec![(0, 745, 0x09)]);
        assert_eq!(setup.warps, vec![(0, 1000, [-163.0, -1448.0, -33.0])]);
        assert_eq!(
            (setup.wait, setup.first_read, setup.lagcomp, net.port),
            (2, 745, true, 6000)
        );
        assert!(parse_args(args(&["--spawn", "1,2"])).is_err());
        assert!(parse_args(args(&["--give", "0:745"])).is_err());
        assert!(parse_args(args(&["--warp", "0:745:1,2"])).is_err());
        assert!(!parse_args(args(&["--no-lagcomp"])).unwrap().0.lagcomp);
    }

    /// The SET-UP flags, the original's defaults without them, and their ranges.
    #[test]
    fn match_settings_parse() {
        let settings = |list: &[&str]| parse_args(args(list)).map(|(s, _)| s.settings);
        assert_eq!(settings(&[]), Ok(Settings::default()));
        let s = settings(&[
            "--time",
            "unlimited",
            "--lives",
            "5",
            "--kills",
            "10",
            "--skill",
            "einstein",
            "--radar",
            "off",
            "--mode",
            "total-war",
        ]);
        let want = Settings {
            time: None,
            lives: 5,
            kills: 10,
            skill: 4,
            radar: false,
            total_war: true,
            ..Settings::default()
        };
        assert_eq!(s, Ok(want));
        assert_eq!(
            settings(&["--time", "3", "--skill", "0"]).unwrap().time,
            Some(3)
        );
        for bad in [
            ["--time", "2"],
            ["--time", "11"],
            ["--lives", "0"],
            ["--lives", "6"],
            ["--kills", "11"],
            ["--skill", "5"],
            ["--skill", "genius"],
            ["--radar", "yes"],
            ["--mode", "ffa"],
            ["--score", "0"],
            ["--score", "41"],
        ] {
            assert!(settings(&bad).is_err(), "{bad:?}");
        }
        // Total War is scene 0x2D's, with its squirrels and Tediz.
        assert!(settings(&["--mode", "total-war", "--map", "30"]).is_err());
        assert!(settings(&["--cavemen", "--mode", "total-war"]).is_err());
        assert!(!settings(&["--mode", "deathmatch"]).unwrap().total_war);
        // Colors is scene 0x30's: map 30 only; the score target 1-40 or unlimited (0).
        let c = settings(&["--mode", "colors", "--map", "30", "--score", "unlimited"]).unwrap();
        assert!(c.colors && !c.total_war && c.score == 0);
        assert_eq!(
            settings(&["--mode", "colors", "--map", "30"])
                .unwrap()
                .score,
            20
        );
        assert!(settings(&["--mode", "colors"]).is_err());
        assert!(settings(&["--mode", "colors", "--map", "30", "--cavemen"]).is_err());
        // Beach is scene 0x2B's, with Colors' score target.
        let b = settings(&["--mode", "beach", "--map", "2b"]).unwrap();
        assert!(b.beach && !b.colors && !b.total_war && b.score == 20);
        assert!(settings(&["--mode", "beach"]).is_err());
        assert!(settings(&["--mode", "beach", "--map", "2b", "--cavemen"]).is_err());
        let beach = |bots| parse_args(args(&["--mode", "beach", "--map", "2b", "--bots", bots]));
        assert!(
            beach("10").is_ok() && beach("11").is_err(),
            "8 villagers, 2 Tediz"
        );
    }

    /// War: Beach: the bots alternate sides until the Tediz are 2, then every one is a villager (a
    /// human asking for the Tediz too), each villager with item 0x3E and the next start class,
    /// respawning on its own record; the Tediz class 3. At 8 villagers, no one else joins.
    #[test]
    fn beach_caps_the_tediz() {
        let mut g = game(Setup {
            settings: Settings {
                beach: true,
                ..Settings::default()
            },
            ..Setup::default()
        });
        g.starts = starts(0x2B).unwrap();
        for slot in 0..7 {
            let _ = g.join_bot(slot);
        }
        let (s, t) = (Side::Squirrels, Side::Tediz);
        let f = &g.m.players;
        assert_eq!(
            f.iter().map(|f| f.side).collect::<Vec<_>>(),
            [s, t, s, t, s, s, s]
        );
        assert_eq!(
            f.iter().map(|f| f.team).collect::<Vec<_>>(),
            [0, 1, 0, 1, 0, 0, 0]
        );
        let classes: Vec<u8> = f.iter().map(|f| f.class).collect();
        assert_eq!(classes, [0x1E, 3, 0x14, 3, 0x1D, 0x1F, 0x1E]);
        assert_eq!((f[0].start, f[1].start), (Some(beach::ITEM), None));
        assert_eq!((f[0].spawn_mask, f[1].spawn_mask), (1 << 1, 1 << 10));
        assert_eq!(g.capped(Side::Tediz), Some(Side::Squirrels));
        let _ = (g.join_bot(7), g.join_bot(8));
        assert_eq!(g.capped(Side::Tediz), Some(Side::Squirrels));
        let _ = g.join_bot(9);
        assert_eq!(g.members(Side::Squirrels), 8);
        assert_eq!(
            (g.capped(Side::Tediz), g.capped(Side::Squirrels)),
            (None, None)
        );
    }

    /// Total War: every player's team is its side (the bots alternate); a Deathmatch gives each
    /// its own.
    #[test]
    fn total_war_teams_are_the_sides() {
        for (total_war, colors, teams) in [
            (true, false, [0, 1, 0]),
            (false, true, [0, 1, 0]),
            (false, false, [0, 1, 2]),
        ] {
            let mut g = game(Setup {
                settings: Settings {
                    total_war,
                    colors,
                    ..Settings::default()
                },
                ..Setup::default()
            });
            let _ = (g.join_bot(0), g.join_bot(1), g.join_bot(2));
            let got: Vec<u8> = g.m.players.iter().map(|f| f.team).collect();
            assert_eq!(got, teams);
        }
    }

    /// Over, the results last `RESULTS` ticks from the end fade's last frame; through a War outro
    /// the countdown runs on from the outro's length (a client joining mid-outro starts there).
    #[test]
    fn the_results_count_down() {
        let mut g = game(Setup::default());
        assert_eq!(g.restart_in(500), RESULTS);
        g.m.ended_at = Some(100);
        let over = 100 + END_DELAY + FADE + 1;
        assert_eq!(g.restart_in(over), RESULTS);
        assert_eq!(g.restart_in(over + 30), RESULTS - 30);
        assert_eq!(g.restart_in(over + RESULTS + 5), 0);
        // Total War: after the winner's outro (325 or 400 ticks), none on a DRAW.
        let mut g = game(Setup {
            settings: Settings {
                total_war: true,
                ..Settings::default()
            },
            ..Setup::default()
        });
        g.outros = vec![(0, 2, 325), (1, 3, 400)];
        let _ = (g.join_bot(0), g.join_bot(1));
        g.m.ended_at = Some(100);
        assert_eq!(g.results_at(), Some(over));
        g.m.players[1].kills = 1;
        assert_eq!(g.results_at(), Some(over + 400));
        assert_eq!(g.restart_in(over), RESULTS + 400);
        assert_eq!(g.restart_in(over + 400 - 30), RESULTS + 30);
        assert_eq!(g.restart_in(over + 400 + 30), RESULTS - 30);
        // Latched once over: the winners' kills going (a leaver) change neither.
        g.won = Some(g.winner());
        g.m.players[1].kills = 0;
        assert_eq!((g.winner(), g.results_at()), (Some(1), Some(over + 400)));
    }

    /// RESTART voted at tick D (over): phase 3 for 17 ticks, `restart_in` 17 down to 1, the next
    /// match on D + 17 (V t5_r: A read at R, the STATS on R + 1, black on R + 17, the new match on
    /// R + 18).
    #[test]
    fn restart_fades_for_17_ticks() {
        let mut g = game(Setup::default());
        g.m.ended_at = Some(100);
        let d = 100 + END_DELAY + FADE + 50;
        g.m.tick = d;
        assert_eq!(g.restarting(d), None);
        (g.start, g.started) = (Some(d + RESTART_FADE), false);
        let left: Vec<_> = (d..d + 18).map(|t| g.restarting(t)).collect();
        assert_eq!(left[0], Some(17));
        assert_eq!(left[16], Some(1));
        assert_eq!(left[17], None);
    }

    /// `--public` takes the game port unless it names one; the token endpoint defaults to port + 1.
    #[test]
    fn deploy_flags_parse() {
        let (_, net) = parse_args(args(&[
            "--public",
            "203.0.113.7",
            "--public",
            "[2001:db8::7]:7000",
            "--port",
            "6000",
            "--public",
            "2001:db8::8",
            "--cert",
            "a.pem",
            "--cert-key",
            "b.pem",
        ]))
        .unwrap();
        let addrs = [
            "203.0.113.7:6000",
            "[2001:db8::7]:7000",
            "[2001:db8::8]:6000",
        ];
        assert_eq!(net.public, addrs.map(|a| a.parse().unwrap()));
        assert_eq!(net.token_addr, Some("[::]:6001".parse().unwrap()));
        assert_eq!(
            (net.cert.as_deref(), net.cert_key.as_deref()),
            (Some("a.pem"), Some("b.pem"))
        );
        let (_, net) = parse_args(args(&["--token-addr", "127.0.0.1:5889"])).unwrap();
        assert_eq!(net.token_addr, Some("127.0.0.1:5889".parse().unwrap()));
        assert!(parse_args(args(&["--public", "game.example.org"])).is_err());
    }

    fn record(position: [i16; 3], yaw: u8) -> Spawn {
        Spawn {
            pos: position,
            b6: 0,
            yaw,
            camera: [0.0, 100.0, 0.0],
            b14: 0,
            b15: 0,
        }
    }

    fn game(setup: Setup) -> Game {
        let spawns = (0..20).map(|i| record([i * 100, 0, 0], 64)).collect();
        Game::new(
            Arena::deathmatch(spawns, |n| [f32::from(n), 0.0, 0.0]),
            setup,
        )
    }

    /// A side's k-th player starts on its base's k-th record (squirrels 0, 6, 7, ...; Tediz 1, 13,
    /// 14, ...) with the game's jitter and respawns in its half; `--spawn` overrides, on the slot's
    /// Deathmatch record and respawning anywhere.
    #[test]
    fn each_side_starts_in_its_base() {
        let mut g = game(Setup::default());
        let (a, squirrels) = g.placement(0, Side::Squirrels);
        let (b, tediz) = g.placement(0, Side::Tediz);
        assert_eq!((a.index, b.index), (0, 1));
        assert_eq!((squirrels, tediz), (0xFFF0_1FFD, 0xFFFF_E03E));
        // The jitter moves along the record's yaw (64: sin 1, cos 0, so along x) by at most 40.
        assert!(
            b.pos[2] == 0.0 && (b.pos[0] - 100.0).abs() <= 40.0,
            "{:?}",
            b.pos
        );
        assert_eq!(b.facing, 64 << 8);
        let mut g = game(Setup {
            spawns: vec![([5.0, 6.0, 7.0], Some(9))],
            ..Setup::default()
        });
        let (a, mask) = g.placement(0, Side::Squirrels);
        assert_eq!(
            (a.index, a.pos, a.facing, a.camera, mask),
            (1, [5.0, 6.0, 7.0], 9, [0.0, 100.0, 0.0], !0)
        );
    }

    /// Scene 0x3F's Deathmatch (`--cavemen`): the bots are cavemen in the squirrels' base, holding
    /// item 0x40; the humans join as Tediz.
    #[test]
    fn cavemen_bots_hold_the_club() {
        let mut g = game(Setup::default());
        g.cavemen = true;
        let _ = (g.join_bot(0), g.join_bot(1));
        let f = &g.m.players;
        assert_eq!([f[0].side, f[1].side], [Side::Cavemen; 2]);
        assert_eq!([f[0].start, f[1].start], [Some(0x40); 2]);
        let (place, mask) = g.placement(2, Side::Cavemen);
        assert_eq!(
            (place.index, mask),
            (7, 0xFFF0_1FFD),
            "the squirrels' third record"
        );
        assert_eq!(g.smaller_side(), Side::Tediz);
    }

    /// Scene 0x1A's Deathmatch: everyone the Wise Guy (class 1, the squirrels' body) holding the
    /// bat, slot s on the s-th Deathmatch record (0-3, then 20-31), respawning anywhere.
    #[test]
    fn vault_players_hold_the_bat() {
        let spawns = (0..32).map(|i| record([i * 100, 0, 0], 64)).collect();
        let arena = Arena {
            spawns,
            ..Default::default()
        };
        let mut g = Game::new(arena, Setup::default());
        (g.vault, g.starts) = (true, starts(0x1A).unwrap());
        let _: Vec<_> = (0..6).map(|s| g.join_bot(s)).collect();
        for (s, f) in g.m.players.iter().enumerate() {
            let x = 100.0 * f32::from([0u8, 1, 2, 3, 20, 21][s]);
            assert_eq!(
                (f.side, f.start, f.class, f.spawn_mask),
                (Side::Squirrels, Some(0x19), 1, !0)
            );
            assert!((f.body.pos[0] - x).abs() <= 40.0, "{s}: {:?}", f.body.pos);
        }
        assert_eq!(g.smaller_side(), Side::Squirrels);
    }

    /// War: Heist on 0x1A: `--score` 1-5 (default 3), map 1a only, at most 7 bots; teams by slot
    /// (slot & 3), the team's k-th on its k-th record, respawning on its four; at most 8 players
    /// and 2 humans a team, 4 players from the 3rd human; the outro by whether a human's team won
    /// (cutscene 2, else 3), GREENS's and YELLOWS's wins included.
    #[test]
    fn heist_teams_records_and_outros() {
        let settings = |list: &[&str]| parse_args(args(list)).map(|(s, _)| s.settings);
        let h = settings(&["--mode", "heist", "--map", "1a"]).unwrap();
        assert!(h.heist && !h.colors && h.score == 3);
        let five = settings(&["--mode", "heist", "--map", "1a", "--score", "5"]);
        assert_eq!(five.unwrap().score, 5);
        for bad in [
            &["--mode", "heist"][..],
            &["--mode", "heist", "--map", "1a", "--score", "6"],
            &["--mode", "heist", "--map", "1a", "--score", "unlimited"],
            &["--mode", "heist", "--map", "1a", "--bots", "8"],
            &["--mode", "heist", "--map", "1a", "--cavemen"],
        ] {
            assert!(parse_args(args(bad)).is_err(), "{bad:?}");
        }
        let spawns = (0..32).map(|i| record([i * 100, 0, 0], 64)).collect();
        let arena = Arena {
            spawns,
            ..Default::default()
        };
        let setup = Setup {
            settings: h,
            bots: 6,
            ..Setup::default()
        };
        let mut g = Game::new(arena, setup);
        (g.vault, g.starts) = (true, starts(0x1A).unwrap());
        for slot in 0..6 {
            let _ = g.join_bot(slot);
        }
        let human = |g: &mut Game| {
            let slot = g.m.next_slot();
            let side = g.capped(Side::Squirrels)?;
            let (place, spawn_mask) = g.placement(slot, side);
            let team = g.team(slot, side);
            Some(g.m.join(Fighter {
                spawn_mask,
                start: g.start_item(side),
                class: g.class(side),
                ..Fighter::new(&place, team, 0)
            }))
        };
        assert_eq!(human(&mut g), Some(6));
        let f = &g.m.players;
        assert_eq!(
            f.iter().map(|f| f.team).collect::<Vec<_>>(),
            [0, 1, 2, 3, 0, 1, 2]
        );
        for (s, record) in [0u8, 1, 2, 3, 8, 11, 14].into_iter().enumerate() {
            let x = 100.0 * f32::from(record);
            assert!(
                (f[s].body.pos[0] - x).abs() <= 40.0,
                "{s}: {:?}",
                f[s].body.pos
            );
            assert_eq!((f[s].start, f[s].class), (Some(0x19), 1));
        }
        assert_eq!(f[2].spawn_mask, 1 << 2 | 1 << 14 | 1 << 15 | 1 << 16);
        // The outros: GREENS (the human's team) won, cutscene 2; YELLOWS, cutscene 3.
        g.outros = vec![(0, 2, 325), (1, 3, 775)];
        g.m.ended_at = Some(100);
        let over = 100 + END_DELAY + FADE + 1;
        g.m.heist.scores = [0, 0, 3, 0];
        assert_eq!(
            (g.winner(), g.human_won(), g.results_at()),
            (Some(2), true, Some(over + 325))
        );
        g.m.heist.scores = [0, 0, 0, 3];
        assert_eq!(
            (g.winner(), g.human_won(), g.results_at()),
            (Some(3), false, Some(over + 775))
        );
        // The second human; then, a third would make 9 players (4 at most with 3 humans).
        assert_eq!(human(&mut g), Some(7));
        assert_eq!(human(&mut g), None);
    }

    /// Tank: map 24 only (and it plays nothing else); one side, each a tank (class 8) on its slot's
    /// record (0-3, then 0x10-0x13), its own team, respawning anywhere.
    #[test]
    fn tank_mode_records_and_class() {
        let settings = |list: &[&str]| parse_args(args(list)).map(|(s, _)| s.settings);
        let t = settings(&["--mode", "tank", "--map", "24"]).unwrap();
        assert!(t.tank && !t.heist);
        for bad in [
            &["--map", "24"][..],
            &["--mode", "tank"],
            &["--mode", "tank", "--map", "24", "--bots", "8"],
            &["--mode", "tank", "--map", "24", "--cavemen"],
        ] {
            assert!(parse_args(args(bad)).is_err(), "{bad:?}");
        }
        let arena = Arena {
            spawns: (0..20).map(|i| record([i * 100, 0, 0], 64)).collect(),
            ..Default::default()
        };
        let setup = Setup {
            settings: t,
            bots: 6,
            ..Setup::default()
        };
        let mut g = Game::new(arena, setup);
        g.starts = starts(0x24).unwrap();
        for slot in 0..6 {
            let _ = g.join_bot(slot);
        }
        let f = &g.m.players;
        for (s, record) in [0u8, 1, 2, 3, 16, 17].into_iter().enumerate() {
            assert!((f[s].body.pos[0] - 100.0 * f32::from(record)).abs() <= 40.0);
            assert_eq!(
                (f[s].side, f[s].class, f[s].team),
                (Side::Squirrels, 8, s as u8)
            );
            assert_eq!(f[s].spawn_mask, !0);
        }
    }

    /// Raptor: map 3f only, `--score` 1-40 (default 20); bots in slots 2 and 6 of each 8 are
    /// raptors (team 1, class 9, the raptors' base), the others cavemen (team 0, class 0xB, the
    /// Ugas'), all with the club (a raptor never draws it, V); the outro by the winning team.
    #[test]
    fn raptor_teams_and_outros() {
        let settings = |list: &[&str]| parse_args(args(list)).map(|(s, _)| s.settings);
        let r = settings(&["--mode", "raptor", "--map", "3f"]).unwrap();
        assert!(r.raptor && !r.colors && r.score == raptor::SCORE);
        let forty = settings(&["--mode", "raptor", "--map", "3f", "--score", "40"]);
        assert_eq!(forty.unwrap().score, 40);
        for bad in [
            &["--mode", "raptor"][..],
            &["--mode", "raptor", "--map", "2d"],
            &["--mode", "raptor", "--map", "3f", "--score", "41"],
            &["--mode", "raptor", "--map", "3f", "--score", "unlimited"],
            &["--mode", "raptor", "--map", "3f", "--cavemen"],
        ] {
            assert!(parse_args(args(bad)).is_err(), "{bad:?}");
        }
        let arena = Arena {
            spawns: (0..32).map(|i| record([i * 100, 0, 0], 64)).collect(),
            ..Default::default()
        };
        let setup = Setup {
            settings: r,
            bots: 7,
            ..Setup::default()
        };
        let mut g = Game::new(arena, setup);
        (g.cavemen, g.starts) = (true, starts(0x3F).unwrap());
        for slot in 0..7 {
            let _ = g.join_bot(slot);
        }
        let (mut cavemen, mut raptors) = (0, 0);
        for (s, f) in g.m.players.iter().enumerate() {
            let raptor = s == 2 || s == 6;
            let k = if raptor { &mut raptors } else { &mut cavemen };
            let record = g.starts.sides[usize::from(raptor)][*k];
            *k += 1;
            assert!((f.body.pos[0] - 100.0 * f32::from(record)).abs() <= 40.0);
            let want = match raptor {
                true => (Side::Tediz, 1, raptor::CLASS),
                false => (Side::Cavemen, 0, raptor::CAVEMAN),
            };
            assert_eq!((f.side, f.team, f.class), want, "slot {s}");
            assert_eq!(f.start, Some(weapon::CLUB.id));
        }
        g.outros = vec![(0, 2, 765), (1, 4, 415)];
        g.m.ended_at = Some(100);
        let over = 100 + END_DELAY + FADE + 1;
        g.m.colors.scores = [20, 7];
        assert_eq!((g.winner(), g.results_at()), (Some(0), Some(over + 765)));
        g.m.colors.scores = [9, 20];
        assert_eq!((g.winner(), g.results_at()), (Some(1), Some(over + 415)));
    }

    /// Bots take slots 0..N, on alternate sides, each in its side's base; humans join after them,
    /// on the side with fewer players unless they ask, on their side's next record; a leaving human
    /// never moves a bot.
    #[test]
    fn bots_take_the_first_slots_and_humans_follow() {
        let mut g = game(Setup {
            bots: 2,
            ..Setup::default()
        });
        let _ = (g.join_bot(0), g.join_bot(1));
        assert_eq!((g.m.players.len(), g.humans()), (2, 0));
        let sides: Vec<Side> = g.m.players.iter().map(|f| f.side).collect();
        assert_eq!(sides, [Side::Squirrels, Side::Tediz]);
        let x: Vec<f32> = g.m.players.iter().map(|f| f.body.pos[0]).collect();
        assert!(
            x[0].abs() <= 40.0 && (x[1] - 100.0).abs() <= 40.0,
            "records 0, 1: {x:?}"
        );
        assert_eq!(g.smaller_side(), Side::Squirrels, "a tie");
        let next = [Side::Squirrels, Side::Tediz].map(|side| g.placement(2, side).0.index);
        assert_eq!(next, [6, 13], "each side's second start record");
        let (place, _) = g.placement(2, Side::Squirrels);
        g.m.players.push(Fighter {
            side: Side::Squirrels,
            ..Fighter::new(&place, 2, 0)
        });
        assert_eq!((g.humans(), g.smaller_side()), (1, Side::Tediz));
        assert_eq!(g.placement(3, Side::Squirrels).0.index, 7);
        let bot1 = g.m.players[1].body.pos;
        g.remove(2);
        assert_eq!((g.humans(), g.m.players[1].body.pos), (0, bot1));
        assert_eq!(
            g.smaller_side(),
            Side::Squirrels,
            "a free slot is on no side"
        );
    }

    #[test]
    fn reads_count_from_the_start() {
        let mut g = game(Setup {
            first_read: 745,
            ..Setup::default()
        });
        assert_eq!(g.read(1000), None);
        (g.start, g.started) = (Some(1000), true);
        assert_eq!(
            (g.read(999), g.read(1000), g.read(1080)),
            (None, Some(745), Some(825))
        );
    }

    /// A leaver's slot stays free and nobody else moves: projectiles, pickups and the history keep
    /// their slots; the next to join takes the free slot, in its side's base.
    #[test]
    fn a_leaver_frees_its_slot_for_the_next_joiner() {
        let mut g = game(Setup::default());
        for i in 0..3 {
            let (place, _) = g.placement(i, Side::Tediz);
            g.m.join(Fighter::new(&place, i as u8, 0));
        }
        let bullet = |owner| {
            Bullet::fire(
                &CollisionWorld::new([]),
                owner,
                [0.0; 3],
                0,
                damage::TWIN_BULLET,
            )
        };
        g.m.bullets = vec![(0, bullet(0)), (0, bullet(1)), (0, bullet(2))];
        g.m.pickups[0] = PickupState {
            timer: 9,
            taken: Some((2, 5)),
            ..Default::default()
        };
        g.m.pickups[1] = PickupState {
            timer: 9,
            taken: Some((1, 5)),
            ..Default::default()
        };
        g.history.push(7, vec![[0.0; 3], [1.0; 3], [2.0; 3]]);
        g.remove(1);
        assert_eq!(
            g.m.players.iter().map(|f| f.team).collect::<Vec<_>>(),
            [0, u8::MAX, 2]
        );
        assert_eq!(
            g.m.bullets.iter().map(|b| b.1.owner).collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(
            (g.m.pickups[0].taken, g.m.pickups[1].taken),
            (Some((2, 5)), None)
        );
        assert_eq!(g.history.get(7, 2), Some([2.0; 3]));
        assert_eq!((g.humans(), g.m.next_slot()), (2, 1));
        let next = g.m.next_slot();
        assert_eq!(
            g.placement(next, Side::Squirrels).0.index,
            0,
            "the first squirrel's"
        );
    }

    #[test]
    fn history_rewinds_between_ticks() {
        let mut h = History::default();
        for t in 0..(HISTORY as u32 + 5) {
            h.push(t, vec![[t as f32, 0.0, 0.0]]);
        }
        assert_eq!(h.get(4, 0), None, "older than the window");
        assert_eq!(h.at(0, 30, 0.0), Some([30.0, 0.0, 0.0]));
        assert_eq!(h.at(0, 30, 0.25), Some([30.25, 0.0, 0.0]));
        assert_eq!(
            h.at(0, HISTORY as u32 + 4, 0.5),
            Some([64.0, 0.0, 0.0]),
            "no later tick"
        );
        assert_eq!(h.at(1, 30, 0.0), None);
    }

    /// A bullet across a moving target: it misses where the target is now and hits where the
    /// shooter saw it, and `first_hit` skips the shooter and takes the nearest body.
    #[test]
    fn the_rewound_test_hits_where_the_shooter_saw_the_target() {
        let world = CollisionWorld::new([]);
        let b = Bullet::fire(&world, 0, [0.0, 40.0, 0.0], 0, damage::TWIN_BULLET);
        let now = [
            Some([0.0, 0.0, 0.0]),
            Some([150.0, 0.0, 120.0]),
            Some([250.0, 0.0, 0.0]),
        ];
        let seen = [
            Some([0.0, 0.0, 0.0]),
            Some([150.0, 0.0, 0.0]),
            Some([250.0, 0.0, 0.0]),
        ];
        let tedi = [Hitbox::of(Side::Tediz); 3];
        assert_eq!(first_hit(&b, &now, &tedi), Some(2));
        assert_eq!(first_hit(&b, &seen, &tedi), Some(1));
        assert_eq!(first_hit(&b, &[Some([0.0; 3]), None, None], &tedi), None);
    }

    /// A hit names its attacker's weapon, not the nearest armed player's: its gun, its rocket or
    /// grenade gone off for a blast (its shotgun's when none went off), the flame on a burning
    /// victim, none for an unowned hit.
    #[test]
    fn a_hit_names_its_attackers_weapon() {
        let place = |x| Placement {
            index: 0,
            pos: [x, 0.0, 0.0],
            facing: 0,
            camera: [0.0; 3],
        };
        let mut pre = game(Setup::default()).m;
        pre.players = [0.0, 100.0, 900.0]
            .map(|x| Fighter::new(&place(x), 0, 0))
            .to_vec();
        (pre.players[1].weapon, pre.players[2].weapon) = (Some(0x09), Some(0x39));
        let hurt = |m: &mut Match, reaction| {
            m.players[0].action = Action::Hurt {
                reaction,
                lethal: false,
                step: 0,
            };
        };
        let mut m = pre.clone();
        hurt(&mut m, 0x21);
        assert_eq!(hit_weapon(&pre, &m, 0, 2), 0x39, "the far shooter's");
        assert_eq!(hit_weapon(&pre, &m, 0, 0), 0, "no attacker");
        pre.players[0].burn = Some(conker_rules::flame::Burn {
            acc: 0,
            life: 10,
            attacker: Some(2),
        });
        assert_eq!(hit_weapon(&pre, &m, 0, 2), 0x15, "a burn");
        pre.players[0].burn = None;
        hurt(&mut m, 0x2B);
        let rocket = conker_rules::projectile::Rocket {
            pos: [0.0; 3],
            dir: [1.0, 0.0, 0.0],
            life: 1.0,
            owner: 2,
        };
        pre.rockets = vec![(5, rocket)];
        assert_eq!(hit_weapon(&pre, &m, 0, 2), 0x16, "its rocket went off");
        pre.rockets.clear();
        assert_eq!(hit_weapon(&pre, &m, 0, 2), 0x26, "a grenade");
        pre.players[2].weapon = Some(0x37);
        assert_eq!(hit_weapon(&pre, &m, 0, 2), 0x37, "the shotgun's blast");
    }

    #[test]
    fn the_respawn_counts_down() {
        let place = Placement {
            index: 0,
            pos: [0.0; 3],
            facing: 0,
            camera: [0.0; 3],
        };
        let mut f = Fighter::new(&place, 0, 0);
        f.action = Action::Hurt {
            reaction: 24,
            lethal: true,
            step: 0,
        };
        f.since = 10;
        let total: u32 = damage::sequence(24, true)
            .iter()
            .map(|s| u32::from(s.1))
            .sum();
        assert_eq!(u32::from(combat(&f, 0, 15, 0, 99).respawn_in), total - 5);
    }
}
