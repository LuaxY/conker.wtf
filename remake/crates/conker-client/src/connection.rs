//! The connection to the game server ([`build`]).
//! - **When:** once the level and the character have loaded (windowed) and [`CALM`] frames in a row
//!   took under [`CALM_SECS`]: the seconds the load and the first draws after it block the main
//!   thread (V: 7 s in a debug build under load; shaders compile in the browser) would eat into the
//!   netcode timeout. Headless clients have no assets, so they connect after the calm frames.
//! - **Status** ([`NetStatus`]): loading, connecting (the token fetch, then the handshake),
//!   connected, failed or disconnected, with the reason and what to do (the page's words in the
//!   browser, the command line's natively); logged on every change, and shown in the page's `#net`
//!   line or the native HUD.
//! - **Input** starts with the follow camera (the client synced, its player arrived): no input goes
//!   out before, `predict` steps nothing before (it reads the input), and the server holds the
//!   player as it joined until that first input, so both start on the same tick from the same state.
//! - **Leaving:** an exit (the window closed, `seconds`, natively Ctrl-C, SIGTERM or SIGHUP; the Esc
//!   menu's LEAVE MATCH, the results' QUIT, the lost connection's QUIT) first sends netcode's
//!   disconnect and exits [`LEAVE_DELAY`] later, once it has gone out; in the browser it goes to the
//!   landing page ([`LANDING`]) instead. A closed page runs no more frames: its transport closes
//!   (the browser's unload, and winit's exit dropping the session), which frees the slot on the
//!   server as well.
//! - **Lost or failed** ([`Reconnect`]): the menu's page offers RECONNECT (a fresh token and id, tried
//!   again for [`RETRY_SECS`]: the server may still hold the old slot) or QUIT ([`crate::menu`]).
//! - **Token:** the server signs each client's netcode connect token (its key never reaches the
//!   client). Without `token=`, the client fetches it as it connects, after the assets loaded (a
//!   token lasts 60 s): from `token-url`, else (an https page) `/token` on the page's own origin,
//!   else the game server's host, its port + 1 ([`token_source`]). In the background (natively a
//!   thread, plain http with [`FETCH_SECS`] limits), while [`connect`] waits for it.
//! - **TLS** ([`secure`]): an https page uses `wss://` and checks the WebTransport certificate as
//!   any site's (no digest); a native client does so with `ca=` (dev/test only). Else the dev
//!   server's self-signed certificate is pinned by its digest, and WebSocket is plain `ws://`.
//!   Natively WebTransport binds the server address's family (both for a host name) and, without a
//!   digest or `ca=`, checks the certificate against the system's roots ([`wt`]).

use crate::{Config, FollowCamera, Transport};
use bevy::prelude::*;
use conker_sim::movement::Input;
use conker_sim::net::{DEFAULT_PORT, Join, JoinChannel, PlayerId};
use core::time::Duration;
use lightyear::link::SendPayload;
use lightyear::netcode::client_plugin::NetcodeConfig;
use lightyear::netcode::{ConnectToken, NetcodeClient};
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::InputMarker;
use lightyear::prelude::*;
use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};

/// Netcode's timeout both ways (the connect token carries it, conker-server's `token.rs` sets it), and
/// the native WebTransport idle timeout: a client that sends nothing for this long loses its slot.
/// A browser tab in the background may run no frame for that long.
pub const TIMEOUT_SECS: i32 = 30;
/// Seconds a connection attempt may take: a WebTransport handshake that never completes (a stale
/// certificate digest in some browsers, blocked UDP) raises no error of its own.
pub const CONNECT_SECS: f64 = 10.0;
/// Seconds the native token and `/info` fetch waits to connect, then for the answer (under
/// [`CONNECT_SECS`] together: the watchdog counts the fetch too).
pub const FETCH_SECS: u64 = 4;
/// Where a leave goes in the browser: the landing page, next to `play.html`.
pub const LANDING: &str = "index.html";
/// Seconds a leaving client keeps running after its netcode disconnect, so the packets go out.
pub const LEAVE_DELAY: f64 = 0.2;
/// Seconds a reconnect keeps trying: past the old slot's timeout, as a server that missed the loss
/// holds it that long (a join it refuses, a full match, is tried again until then).
pub const RETRY_SECS: f64 = TIMEOUT_SECS as f64 + 5.0;
/// Seconds between a reconnect's failed attempt and the next (the token endpoint gives an address 10
/// tokens per 10 s).
const RETRY_EVERY: f64 = 2.0;
/// Frames in a row, each shorter than [`CALM_SECS`], before connecting.
pub const CALM: u32 = 10;
pub const CALM_SECS: f32 = 0.25;

/// Where the connection stands, with the reason of a failure or a disconnection.
#[derive(Resource, Clone, Debug, PartialEq)]
pub enum NetStatus {
    Loading,
    /// Since this many seconds after the start.
    Connecting(f64),
    Connected,
    Failed(String),
    Disconnected(String),
}

impl NetStatus {
    /// The status line, and whether it reports a problem.
    pub fn text(&self, cfg: &Config) -> (String, bool) {
        let (server, over) = (&cfg.server, transport_name(cfg.transport));
        match self {
            Self::Loading => ("loading the level".into(), false),
            Self::Connecting(_) => {
                // Bevy's default font (the native HUD) has ASCII only.
                let more = page("…", "...");
                (format!("connecting to {server} over {over}{more}"), false)
            }
            Self::Connected => (format!("connected to {server} over {over}"), false),
            Self::Failed(why) => {
                let hint = match cfg.transport {
                    Transport::WebTransport if secure(cfg) => page(
                        "Is the server running? ?transport=ws tries WebSocket",
                        "Is the server running? --transport ws tries WebSocket",
                    ),
                    Transport::WebTransport if cfg.digest.is_empty() => page(
                        "No certificate digest: pass ?digest=HEX, or run the server with \
                         --digest-file web/dist/digest.txt",
                        "No certificate digest: pass --digest HEX, the server's \
                         --digest-file. --transport ws tries WebSocket",
                    ),
                    Transport::WebTransport => page(
                        "Is the server running? A restarted server has a new certificate: \
                         reload the page. ?transport=ws tries WebSocket",
                        "Is the server running? A restarted server has a new certificate: \
                         pass its --digest. --transport ws tries WebSocket",
                    ),
                    Transport::WebSocket => "Is the server running?",
                };
                (
                    format!("could not connect to {server} over {over}: {why}. {hint}"),
                    true,
                )
            }
            // The menu's page offers RECONNECT and QUIT.
            Self::Disconnected(why) => (format!("disconnected from {server}: {why}"), true),
        }
    }
}

/// RECONNECT ([`crate::menu`]'s page for a lost or failed connection): attempts until `until`, each
/// with a fresh token and client id (netcode refuses the old id while the server still holds its
/// slot), the next at `next`; `why` the last one's failure. Over once the local player is back.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct Reconnect {
    pub until: Option<f64>,
    next: Option<f64>,
    pub why: String,
}

impl Reconnect {
    pub fn active(&self) -> bool {
        self.until.is_some()
    }

    /// RECONNECT chosen: an attempt now, more until [`RETRY_SECS`] from now.
    pub fn start(&mut self, now: f64) {
        if !self.active() {
            *self = Reconnect {
                until: Some(now + RETRY_SECS),
                next: Some(now),
                why: String::new(),
            };
        }
    }
}

/// The page's words in the browser, the command line's natively.
fn page(browser: &'static str, native: &'static str) -> &'static str {
    if cfg!(target_family = "wasm") {
        browser
    } else {
        native
    }
}

fn transport_name(t: Transport) -> &'static str {
    match t {
        Transport::WebTransport => "WebTransport",
        Transport::WebSocket => "WebSocket",
    }
}

/// Why a connection ended or failed, in words (netcode's states and lightyear's reasons).
pub fn why(reason: &DisconnectedReason) -> String {
    match reason {
        DisconnectedReason::TransportError(state) => match state.as_str() {
            "ConnectionTimedOut" => format!("nothing from the server for {TIMEOUT_SECS} s"),
            "ConnectionRequestTimedOut" | "ChallengeResponseTimedOut" => {
                "the server did not answer".into()
            }
            "ConnectionDenied" => {
                "the server refused (full, or another version: reload the page)".into()
            }
            _ => tidy(state),
        },
        // The server's own words (a full match, conker-server's `join_player`), not aeronet's
        // placeholders (`(dropped)`: a stopping server's session).
        DisconnectedReason::LinkFailed(UnlinkReason::ByPeer(r))
            if !r.is_empty() && !r.starts_with('(') =>
        {
            r.clone()
        }
        DisconnectedReason::ByPeer(_) | DisconnectedReason::LinkFailed(UnlinkReason::ByPeer(_)) => {
            "the server closed the connection".into()
        }
        DisconnectedReason::LinkFailed(UnlinkReason::TransportError(e)) => tidy(e),
        DisconnectedReason::UserRequested(_) => "left".into(),
        other => other.to_string(),
    }
}

/// A transport's error on one line: its causes joined with `: ` (also anyhow's numbered ones,
/// `0: A 1: B`), without the browser's `[object Event]`, a cause that repeats the one before
/// (Chrome's WebTransport: `connection lost: Connection lost`) or a trailing period; WebSocket's
/// abnormal close (1006) in words.
fn tidy(error: &str) -> String {
    if error.contains("code 1006") {
        return "the connection dropped (WebSocket code 1006)".into();
    }
    let line = error.split_whitespace().collect::<Vec<_>>().join(" ");
    let line = line
        .replace(" Caused by: ", ": ")
        .replace(": [object Event]", "");
    let line = (0..10).fold(line, |l, n| {
        l.replace(&format!(": {n}: "), ": ")
            .replace(&format!(" {n}: "), ": ")
    });
    let mut causes: Vec<&str> = Vec::new();
    for cause in line.trim_end_matches('.').split(": ") {
        if !causes.last().is_some_and(|c| c.eq_ignore_ascii_case(cause)) {
            causes.push(cause);
        }
    }
    causes.join(": ")
}

/// The connection's systems and observers (see the module docs).
pub fn build(app: &mut App) {
    app.insert_resource(NetStatus::Loading);
    app.init_resource::<Reconnect>();
    app.add_systems(
        Update,
        (retry, connect.run_if(ready), watchdog, show).chain(),
    );
    app.add_systems(Last, leave.after(bevy::window::ExitSystems));
    app.add_systems(
        PostUpdate,
        delay_sends
            .after(ConnectionSystems::Send)
            .before(LinkSystems::Send),
    );
    #[cfg(not(target_family = "wasm"))]
    {
        app.add_observer(wt::link);
        // Ctrl-C, SIGTERM and SIGHUP (ctrlc's `termination`) become an exit, which [`leave`]
        // holds for the disconnect: Bevy's handler, set here unless its plugin (`DefaultPlugins`,
        // windowed) did, as it stands down when the shell started the client ignoring SIGINT (a
        // script's background job). Headless clients (`MinimalPlugins`) get its flag's system.
        use bevy::app::TerminalCtrlCHandlerPlugin as CtrlC;
        let _ = ctrlc::set_handler(CtrlC::gracefully_exit); // Err: Bevy's own is set already
        if !app.is_plugin_added::<CtrlC>() {
            app.add_systems(Update, CtrlC::exit_on_flag);
        }
    }
    app.add_observer(on_connecting)
        .add_observer(on_connected)
        .add_observer(on_disconnected)
        .add_observer(on_attached)
        .add_observer(on_player_left);
}

/// Not connected yet, (windowed) the level and the characters loaded, and the frames since calm;
/// or the token on its way ([`connect`] polls it).
fn ready(
    status: Res<NetStatus>,
    assets: Option<Res<AssetServer>>,
    time: Res<Time<Real>>,
    mut calm: Local<u32>,
    cfg: Res<Config>,
    clients: Query<(), With<Client>>,
) -> bool {
    // The level and the map's own characters (`render`'s assets): windowed clients connect once
    // they loaded (the picks' load after, `render::load_characters`).
    let loaded = assets.is_none_or(|a| {
        let characters = crate::map_characters(&cfg.map).into_iter();
        std::iter::once(crate::render::level_scene(&cfg.map))
            .chain(characters.map(conker_assets::character_gltf))
            .all(|path| {
                a.get_handle_untyped(&path)
                    .is_some_and(|h| a.is_loaded_with_dependencies(&h))
            })
    });
    *calm = if loaded && time.delta_secs() < CALM_SECS {
        *calm + 1
    } else {
        0
    };
    match *status {
        NetStatus::Loading => *calm >= CALM,
        NetStatus::Connecting(_) => clients.is_empty(),
        _ => false,
    }
}

fn connect(
    mut commands: Commands,
    mut cfg: ResMut<Config>,
    mut status: ResMut<NetStatus>,
    time: Res<Time<Real>>,
    mut fetching: Local<Fetching>,
) {
    if cfg.token.is_empty() {
        // The fetch counts as connecting (the HUD says so; the watchdog bounds it). Each attempt
        // fetches its own (a reconnect's: [`retry`]).
        if *status == NetStatus::Loading {
            *status = NetStatus::Connecting(time.elapsed_secs_f64());
            *fetching = Fetching::default();
        }
        let Some(fetched) = fetching.poll(&cfg) else {
            return;
        };
        match fetched {
            Ok(hex) => cfg.token = hex,
            Err(why) => {
                *status = NetStatus::Failed(why);
                return;
            }
        }
    }
    // The token names the server's public addresses; the transports dial `server`.
    let server_addr = cfg
        .server
        .parse()
        .unwrap_or(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0));
    #[cfg(not(target_family = "wasm"))]
    let roots = match cfg.ca.as_deref().map(wt::roots).transpose() {
        Ok(roots) => roots,
        Err(why) => {
            *status = NetStatus::Failed(why);
            return;
        }
    };
    #[cfg(not(target_family = "wasm"))]
    let check = match (roots.clone(), &cfg.digest[..]) {
        (Some(roots), _) => wt::Check::Ca(roots),
        (None, "") => wt::Check::System,
        (None, digest) => match hex(digest).and_then(|h| <[u8; 32]>::try_from(h).ok()) {
            Some(hash) => wt::Check::Digest(hash),
            None => {
                *status = NetStatus::Failed("the certificate digest is not 64 hex digits".into());
                return;
            }
        },
    };
    let auth = match connect_token(&cfg.token) {
        Ok(token) => Authentication::Token(token),
        Err(why) => {
            *status = NetStatus::Failed(why);
            return;
        }
    };
    let netcode_config = NetcodeConfig {
        client_timeout_secs: TIMEOUT_SECS,
        ..default()
    };
    let netcode = NetcodeClient::new(auth, netcode_config).expect("netcode client");
    let mut client = commands.spawn((
        Name::new("Client"),
        Client,
        ReplicationReceiver,
        PeerAddr(server_addr),
        LocalAddr(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)),
        netcode,
    ));
    if let Some(ms) = cfg.lag {
        client.insert((
            Link::default().with_conditioner(RecvLinkConditioner::new(LinkConditionerConfig {
                incoming_latency: Duration::from_millis(ms),
                ..default()
            })),
            Outbox::default(),
        ));
    }
    match cfg.transport {
        #[cfg(not(target_family = "wasm"))]
        Transport::WebTransport => client
            .insert_if_new(Link::default())
            .insert(wt::WebTransport {
                check,
                server: cfg.server.parse().ok(),
                target: format!("https://{}", cfg.server),
            }),
        #[cfg(target_family = "wasm")]
        Transport::WebTransport => client.insert(WebTransportClientIo {
            certificate_digest: cfg.digest.clone(),
            target: Some(format!("https://{}", cfg.server)),
        }),
        Transport::WebSocket => {
            #[cfg(target_family = "wasm")]
            let ws = ClientConfig;
            #[cfg(not(target_family = "wasm"))]
            let ws = match roots {
                Some(roots) => wt::websocket(roots),
                None => ClientConfig::builder().with_no_encryption(),
            };
            let scheme = if secure(&cfg) { "wss" } else { "ws" };
            client.insert(WebSocketClientIo::from_url(
                ws,
                format!("{scheme}://{}", cfg.server),
            ))
        }
    };
    let entity = client.id();
    commands.trigger(Connect { entity });
    info!(
        "client {} connecting to {} over {:?}{}",
        cfg.id,
        cfg.server,
        cfg.transport,
        cfg.lag.map_or(String::new(), |ms| format!(
            ", {ms} ms added to every packet in and out"
        ))
    );
}

/// Over TLS with a checked certificate (`wss://`, WebTransport without a digest): in the browser
/// when the page is https, natively with `ca=`.
pub fn secure(cfg: &Config) -> bool {
    #[cfg(target_family = "wasm")]
    {
        let _ = cfg;
        web_sys::window().and_then(|w| w.location().protocol().ok()) == Some("https:".into())
    }
    #[cfg(not(target_family = "wasm"))]
    cfg.ca.is_some()
}

/// The game server's own token endpoint: its host, its port + 1 (`conker-server --token-addr`'s
/// default), `/token`.
pub fn token_url(server: &str) -> String {
    let (host, port) = server
        .rsplit_once(':')
        .and_then(|(host, port)| Some((host, port.parse::<u16>().ok()?)))
        .unwrap_or((server, DEFAULT_PORT));
    format!("http://{host}:{}/token", port.wrapping_add(1))
}

/// Where this client gets its token: `token-url=`, else (an https page) `/token` on the page's own
/// origin, which the static server proxies to the game server; else [`token_url`]. With its id.
pub fn token_source(cfg: &Config) -> String {
    let url = cfg.token_url.clone().unwrap_or_else(|| {
        match cfg!(target_family = "wasm") && secure(cfg) {
            true => "/token".into(),
            false => token_url(&cfg.server),
        }
    });
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}id={}", cfg.id)
}

/// The server's `/info` (its map and protocol): [`token_source`]'s endpoint, `info` for `token`.
pub fn info_url(cfg: &Config) -> String {
    let url = token_source(cfg);
    let url = url.split_once('?').map_or(&url[..], |(u, _)| u);
    format!("{}/info", url.rsplit_once('/').map_or("", |(base, _)| base))
}

/// The map named by the server's `/info` body (`{"map":"30","protocol":15}`), if this client
/// speaks its protocol and knows the map.
pub fn info_map(body: &[u8]) -> Result<String, String> {
    let info: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("the server's /info: {e}"))?;
    let protocol = info["protocol"].as_u64().unwrap_or(0);
    if protocol != conker_sim::net::PROTOCOL_ID {
        return Err(format!(
            "the server speaks protocol {protocol}, this client {}: reload the page",
            conker_sim::net::PROTOCOL_ID
        ));
    }
    let map = info["map"].as_str().unwrap_or_default();
    conker_sim::net::map_scene(map)
        .ok_or_else(|| format!("the server's map {map:?} is unknown"))?;
    Ok(map.into())
}

/// The fingerprint of the server's pack its `/info` names (`pack`; conker-sim's `pack_hash`).
pub fn info_pack(body: &[u8]) -> Option<String> {
    let info: serde_json::Value = serde_json::from_slice(body).ok()?;
    info["pack"].as_str().map(String::from)
}

/// This client's pack (`read`: its files) against the server's fingerprint (`server`, from
/// [`info_pack`]): Ok if they are the same, else why the client refuses to play (the collision and
/// the rules' data must match for the prediction to follow the server).
pub fn check_pack(
    map: &str,
    server: &str,
    read: impl FnMut(&str) -> Option<Vec<u8>>,
) -> Result<(), String> {
    let own = conker_sim::net::pack_hash(map, read).ok_or("a pack file is missing")?;
    match own == server {
        true => Ok(()),
        false => Err(format!(
            "this asset pack ({own}) is not the server's ({server}): play with the same export"
        )),
    }
}

/// Natively: the server's map and its pack's fingerprint, from its `/info` (asked again for 5 s
/// while the server starts).
#[cfg(not(target_family = "wasm"))]
pub fn server_info(cfg: &Config) -> Result<(String, Option<String>), String> {
    let get = || fetch(&info_url(cfg), "the server's map");
    let start = std::time::Instant::now();
    let mut body = get();
    while body.is_err() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(250));
        body = get();
    }
    let body = body?;
    Ok((info_map(body.as_bytes())?, info_pack(body.as_bytes())))
}

/// The token fetch, in flight (natively on a thread of its own, in the browser a promise):
/// [`connect`] polls it.
#[derive(Default)]
struct Fetching(Option<std::sync::Arc<std::sync::OnceLock<Result<String, String>>>>);

impl Fetching {
    /// The token once it has arrived; the first call starts the fetch.
    fn poll(&mut self, cfg: &Config) -> Option<Result<String, String>> {
        let done = self.0.get_or_insert_with(|| {
            let done = std::sync::Arc::new(std::sync::OnceLock::new());
            let out = std::sync::Arc::clone(&done);
            let url = token_source(cfg);
            #[cfg(not(target_family = "wasm"))]
            std::thread::spawn(move || out.set(fetch(&url, "a connect token")));
            #[cfg(target_family = "wasm")]
            {
                let request = crate::web::fetch(&url);
                wasm_bindgen_futures::spawn_local(async move {
                    let hex = request.await.and_then(|b| {
                        String::from_utf8(b).map_err(|_| "the token is not text".to_string())
                    });
                    let _ =
                        out.set(hex.map_err(|e| format!("could not get a connect token from {e}")));
                });
            }
            done
        });
        done.get().cloned()
    }
}

/// Bytes from hex digits (any case, surrounding space ignored).
fn hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(text.get(2 * i..2 * i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()
        .filter(|b| b.len() * 2 == text.len())
}

/// The netcode connect token from its hex.
fn connect_token(text: &str) -> Result<ConnectToken, String> {
    let bytes = hex(text)
        .filter(|b| b.len() == 2048)
        .ok_or("the connect token is not 2048 bytes of hex")?;
    ConnectToken::try_from_bytes(&bytes).map_err(|e| format!("bad connect token: {e}"))
}

/// A plain-HTTP GET's body (native: the token and `/info` endpoints on the game server's host),
/// [`FETCH_SECS`] to connect and as long for the answer.
#[cfg(not(target_family = "wasm"))]
fn fetch(url: &str, what: &str) -> Result<String, String> {
    use std::io::{ErrorKind, Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let fail = |e: &dyn std::fmt::Display| format!("could not get {what} from {url}: {e}");
    let said = |e: std::io::Error| match e.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => {
            fail(&format!("no answer in {FETCH_SECS} s"))
        }
        _ => fail(&e),
    };
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| fail(&"natively only http:// (or pass token=HEX)"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let wait = Duration::from_secs(FETCH_SECS);
    let mut stream = Err(fail(&"no address"));
    for addr in host.to_socket_addrs().map_err(|e| fail(&e))? {
        stream = TcpStream::connect_timeout(&addr, wait).map_err(said);
        if stream.is_ok() {
            break;
        }
    }
    let mut stream = stream?;
    stream.set_read_timeout(Some(wait)).map_err(|e| fail(&e))?;
    write!(stream, "GET /{path} HTTP/1.0\r\nHost: {host}\r\n\r\n").map_err(|e| fail(&e))?;
    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(said)?;
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    match head.split_whitespace().nth(1) {
        Some("200") => Ok(body.to_string()),
        _ => Err(fail(&head.lines().next().unwrap_or("no answer"))),
    }
}

/// The native WebTransport link (lightyear's binds IPv4 only, and only pins a digest, so this one
/// spawns the transport itself): it binds the server address's family (both for a host name), and
/// checks the certificate against [`Check`]. Also `ca=`'s `wss://` ([`websocket`]).
#[cfg(not(target_family = "wasm"))]
mod wt {
    use aeronet_webtransport::client::WebTransportClient;
    use aeronet_webtransport::wtransport;
    use bevy::prelude::*;
    use core::time::Duration;
    use lightyear::prelude::client::ClientConfig;
    use lightyear::prelude::{LinkStart, Linked, Linking};
    use lightyear::websocket::prelude::rustls;
    use lightyear_aeronet::AeronetLinkOf;
    use rustls::RootCertStore;
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::pem::PemObject;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use wtransport::config::IpBindConfig;

    /// How the server's certificate is checked.
    pub enum Check {
        /// `ca=` (dev/test only): against this CA, as a browser checks a real one.
        Ca(Arc<RootCertStore>),
        /// The dev server's self-signed certificate, pinned by its SHA-256 (`digest=`).
        Digest([u8; 32]),
        /// Neither: against the system's roots (a real certificate).
        System,
    }

    pub fn roots(path: &str) -> Result<Arc<RootCertStore>, String> {
        let pem = std::fs::read(path).map_err(|e| format!("ca {path}: {e}"))?;
        let mut roots = RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(&pem) {
            let cert = cert.map_err(|e| format!("ca {path}: {e}"))?;
            roots.add(cert).map_err(|e| format!("ca {path}: {e}"))?;
        }
        if roots.is_empty() {
            return Err(format!("ca {path}: no certificate"));
        }
        Ok(Arc::new(roots))
    }

    pub fn websocket(roots: Arc<RootCertStore>) -> ClientConfig {
        ClientConfig::builder().with_tls_config(
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    }

    /// A WebTransport link to `target` (`server` when it is an address; lightyear's settings
    /// otherwise).
    #[derive(Component)]
    pub struct WebTransport {
        pub check: Check,
        pub server: Option<SocketAddr>,
        pub target: String,
    }

    /// The local socket for `server`: its family, both for a host name (dual-stack).
    pub fn bind(server: Option<SocketAddr>) -> IpBindConfig {
        match server {
            Some(SocketAddr::V4(_)) => IpBindConfig::InAddrAnyV4,
            Some(SocketAddr::V6(_)) => IpBindConfig::InAddrAnyV6,
            None => IpBindConfig::InAddrAnyDual,
        }
    }

    pub fn link(
        trigger: On<LinkStart>,
        links: Query<&WebTransport, (Without<Linking>, Without<Linked>)>,
        mut commands: Commands,
    ) -> Result {
        let Ok(wt) = links.get(trigger.entity) else {
            return Ok(());
        };
        let config = wtransport::ClientConfig::builder().with_bind_config(bind(wt.server));
        let config = match &wt.check {
            Check::Ca(roots) => config.with_custom_tls(
                wtransport::tls::client::build_default_tls_config(roots.clone(), None),
            ),
            Check::Digest(hash) => {
                config.with_server_certificate_hashes([wtransport::tls::Sha256Digest::new(*hash)])
            }
            Check::System => config.with_native_certs(),
        };
        let config = config
            .keep_alive_interval(Some(Duration::from_secs(1)))
            .max_idle_timeout(Some(Duration::from_secs(super::TIMEOUT_SECS as u64)))?
            .build();
        let (link, target) = (trigger.entity, wt.target.clone());
        commands.queue(move |world: &mut World| {
            let entity = world.spawn((AeronetLinkOf(link), Name::new("WebTransportClient")));
            WebTransportClient::connect(config, target).apply(entity);
        });
        Ok(())
    }
}

/// `lag=`'s outgoing half: lightyear's conditioner delays only what arrives, so what the client
/// sends waits here until it is due (each payload `lag` ms after netcode wrote it).
#[derive(Component, Default)]
struct Outbox(VecDeque<(f64, SendPayload)>);

fn delay_sends(
    cfg: Res<Config>,
    time: Res<Time<Real>>,
    mut links: Query<(&mut Link, &mut Outbox)>,
) {
    let Some(ms) = cfg.lag else {
        return;
    };
    let now = time.elapsed_secs_f64();
    for (mut link, mut out) in &mut links {
        while let Some(payload) = link.send.pop() {
            out.0.push_back((now + ms as f64 / 1000.0, payload));
        }
        while out.0.front().is_some_and(|(due, _)| *due <= now) {
            if let Some((_, payload)) = out.0.pop_front() {
                link.send.push(payload);
            }
        }
    }
}

/// Connecting (already so since a token fetch: the watchdog counts both).
fn on_connecting(
    trigger: On<Add, Connecting>,
    clients: Query<(), With<Client>>,
    time: Res<Time<Real>>,
    mut status: ResMut<NetStatus>,
) {
    if clients.contains(trigger.entity) && !matches!(*status, NetStatus::Connecting(_)) {
        *status = NetStatus::Connecting(time.elapsed_secs_f64());
    }
}

/// Connected: the client joins the match with the side it asks for (`side=`, else the server's
/// pick).
fn on_connected(
    trigger: On<Add, Connected>,
    mut clients: Query<&mut MessageSender<Join>, With<Client>>,
    cfg: Res<Config>,
    mut status: ResMut<NetStatus>,
) {
    if let Ok(mut joins) = clients.get_mut(trigger.entity) {
        info!(
            "client {} connected over {:?}; joins on side {:?} with pick {:?}",
            cfg.id, cfg.transport, cfg.side, cfg.character
        );
        joins.send::<JoinChannel>(Join {
            side: cfg.side,
            character: cfg.character,
        });
        *status = NetStatus::Connected;
    }
}

/// A failed attempt or a lost connection (the client spawns `Disconnected` before it connects:
/// that one is ignored, as is any after a [`watchdog`] failure, and a leave's: [`leave`] goes on
/// without the lost connection's page).
fn on_disconnected(
    trigger: On<Insert, Disconnected>,
    clients: Query<&Disconnected, With<Client>>,
    mut status: ResMut<NetStatus>,
) {
    let Ok(d) = clients.get(trigger.entity) else {
        return;
    };
    if matches!(d.reason, DisconnectedReason::UserRequested(_)) {
        return;
    }
    match *status {
        NetStatus::Connecting(_) => *status = NetStatus::Failed(why(&d.reason)),
        NetStatus::Connected => *status = NetStatus::Disconnected(why(&d.reason)),
        _ => {}
    }
}

/// A reconnect's attempts ([`Reconnect`]): the old client goes and a new one starts as the first did
/// ([`connect`]: a token fetched), under a fresh id; a failure tries again [`RETRY_EVERY`] later
/// until [`RETRY_SECS`] ran out (then the menu offers RECONNECT again); the local player's arrival
/// ends it (not netcode's connection: a full match refuses the join after it).
fn retry(
    time: Res<Time<Real>>,
    mut r: ResMut<Reconnect>,
    mut status: ResMut<NetStatus>,
    mut cfg: ResMut<Config>,
    clients: Query<Entity, With<Client>>,
    playing: Query<(), With<FollowCamera>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let Some(until) = r.until else {
        return;
    };
    if !playing.is_empty() {
        info!("reconnected as client {}", cfg.id);
        *r = Reconnect::default();
        return;
    }
    if let NetStatus::Failed(why) | NetStatus::Disconnected(why) = &*status
        && r.next.is_none()
    {
        if now >= until {
            info!("reconnect: giving up after {RETRY_SECS} s");
            *r = Reconnect::default();
            return;
        }
        (r.why, r.next) = (why.clone(), Some(now + RETRY_EVERY));
    }
    if r.next.is_some_and(|t| now >= t) {
        r.next = None;
        for entity in &clients {
            commands.entity(entity).despawn();
        }
        cfg.token.clear();
        cfg.id = getrandom::u64().unwrap_or(cfg.id.wrapping_add(1)).max(1);
        *status = NetStatus::Loading;
        info!("reconnecting as client {}", cfg.id);
    }
}

/// Gives up on an attempt after [`CONNECT_SECS`].
fn watchdog(
    time: Res<Time<Real>>,
    clients: Query<Entity, With<Client>>,
    mut status: ResMut<NetStatus>,
    mut commands: Commands,
) {
    let NetStatus::Connecting(since) = *status else {
        return;
    };
    if time.elapsed_secs_f64() - since > CONNECT_SECS {
        *status = NetStatus::Failed(format!("no answer in {CONNECT_SECS} s"));
        for entity in &clients {
            commands.trigger(Disconnect { entity });
        }
    }
}

/// Logs each new status; in the browser, writes it into the page's `#net` element.
fn show(status: Res<NetStatus>, cfg: Res<Config>) {
    if !status.is_changed() {
        return;
    }
    let (text, problem) = status.text(&cfg);
    if problem {
        warn!("connection: {text}");
    } else {
        info!("connection: {text}");
    }
    #[cfg(target_family = "wasm")]
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("net"))
    {
        el.set_text_content(Some(&text));
        el.set_class_name(if problem { "error" } else { "" });
    }
}

/// Another player left (the server despawned it).
fn on_player_left(trigger: On<Remove, PlayerId>, players: Query<&PlayerId, Without<Predicted>>) {
    if let Ok(id) = players.get(trigger.entity) {
        info!("player {:?} left", id.0);
    }
}

/// Input starts with the follow camera (`attach`: the client synced and its player arrived).
fn on_attached(trigger: On<Add, FollowCamera>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert(InputMarker::<Input>::default());
}

/// Holds any exit until netcode's disconnect has gone out: the first exit request sends it and
/// comes back [`LEAVE_DELAY`] later (a closed window asks again every frame meanwhile). In the
/// browser the exit is the landing page ([`LANDING`], once). Every leave comes here: the Esc menu's
/// LEAVE MATCH, the results' QUIT and the lost connection's QUIT write an exit.
fn leave(
    mut exits: ResMut<Messages<AppExit>>,
    time: Res<Time<Real>>,
    clients: Query<Entity, (With<Client>, With<Connected>)>,
    mut leaving: Local<Option<(f64, AppExit)>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    if leaving.is_none()
        && let Some(exit) = exits.drain().last()
    {
        let at = match clients.single() {
            Ok(entity) => {
                info!("leaving: disconnecting first");
                commands.trigger(Disconnect { entity });
                now + LEAVE_DELAY
            }
            Err(_) => now,
        };
        *leaving = Some((at, exit));
    }
    let Some((at, exit)) = leaving.clone() else {
        return;
    };
    exits.clear();
    if now < at {
        return;
    }
    #[cfg(target_family = "wasm")]
    {
        let _ = exit;
        *leaving = Some((f64::INFINITY, AppExit::Success));
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_href(LANDING);
        }
    }
    #[cfg(not(target_family = "wasm"))]
    exits.write(exit);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Natively in the command line's words, in ASCII (Bevy's default font has nothing else).
    #[test]
    fn statuses_read_with_their_reasons_and_hints() {
        let mut cfg = Config::from_pairs([("id".to_string(), "1".to_string())]).unwrap();
        cfg.server = "[::1]:5888".into();
        let text = |s: NetStatus, cfg: &Config| s.text(cfg);
        assert_eq!(
            text(NetStatus::Connected, &cfg),
            ("connected to [::1]:5888 over WebTransport".into(), false)
        );
        assert_eq!(
            text(NetStatus::Connecting(0.0), &cfg),
            (
                "connecting to [::1]:5888 over WebTransport...".into(),
                false
            )
        );
        let (t, problem) = text(NetStatus::Failed("the server did not answer".into()), &cfg);
        assert!(
            problem && t.contains("did not answer. No certificate digest: pass --digest"),
            "{t}"
        );
        cfg.digest = "AB".into();
        let (t, _) = text(NetStatus::Failed("x".into()), &cfg);
        assert!(t.contains("new certificate: pass its --digest"), "{t}");
        cfg.transport = Transport::WebSocket;
        let (t, problem) = text(NetStatus::Disconnected("left".into()), &cfg);
        assert!(problem && t.ends_with("[::1]:5888: left"), "{t}");
        assert!(t.is_ascii(), "{t}");
    }

    /// The native fetch gives up on a server that takes the connection and never answers, and
    /// says so; the WebTransport socket takes the server address's family.
    #[test]
    fn a_silent_endpoint_times_out_and_sockets_follow_the_family() {
        let quiet = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/token", quiet.local_addr().unwrap());
        let start = std::time::Instant::now();
        let why = fetch(&url, "a connect token").unwrap_err();
        assert!(
            why.ends_with(&format!("no answer in {FETCH_SECS} s")),
            "{why}"
        );
        assert!(start.elapsed().as_secs() < FETCH_SECS + 2);
        use aeronet_webtransport::wtransport::config::IpBindConfig::*;
        let bind = |s: &str| format!("{:?}", wt::bind(s.parse().ok()));
        assert_eq!(bind("127.0.0.1:5888"), format!("{InAddrAnyV4:?}"));
        assert_eq!(bind("[::1]:5888"), format!("{InAddrAnyV6:?}"));
        assert_eq!(bind("game.example.org:5888"), format!("{InAddrAnyDual:?}"));
        assert_eq!(hex(" 0aFf\n"), Some(vec![0x0a, 0xff]));
        assert_eq!(hex("0aF"), None);
    }

    #[test]
    fn netcode_states_read_as_words() {
        let e = |s: &str| why(&DisconnectedReason::TransportError(s.into()));
        assert_eq!(e("ConnectionTimedOut"), "nothing from the server for 30 s");
        assert_eq!(e("ConnectionRequestTimedOut"), "the server did not answer");
        assert!(e("ConnectionDenied").contains("refused"));
        assert_eq!(e("Other"), "Other");
        assert_eq!(
            e("failed to await connection\n\nCaused by:\n    Opening handshake failed."),
            "failed to await connection: Opening handshake failed"
        );
        assert_eq!(
            e("connection lost Caused by: [object Event]"),
            "connection lost"
        );
        assert_eq!(
            e("connection lost\n\nCaused by:\n    Connection lost."),
            "connection lost"
        );
        assert!(e("connection closed with code 1006").contains("dropped"));
        assert_eq!(
            e(
                "connection lost: 0: WebSocket protocol error: Connection reset without closing \
               handshake 1: Connection reset without closing handshake"
            ),
            "connection lost: WebSocket protocol error: Connection reset without closing handshake"
        );
        assert_eq!(why(&DisconnectedReason::UserRequested(None)), "left");
        let link = |r| why(&DisconnectedReason::LinkFailed(r));
        assert_eq!(
            link(UnlinkReason::TransportError("connection lost".into())),
            "connection lost"
        );
        assert_eq!(
            link(UnlinkReason::ByPeer(String::new())),
            "the server closed the connection"
        );
        assert_eq!(link(UnlinkReason::ByPeer("full".into())), "full");
        assert_eq!(
            link(UnlinkReason::ByPeer("(dropped)".into())),
            "the server closed the connection"
        );
    }

    #[test]
    fn tokens_come_from_the_option_or_the_servers_port_plus_one() {
        assert_eq!(token_url("127.0.0.1:5931"), "http://127.0.0.1:5932/token");
        assert_eq!(token_url("[::1]:5888"), "http://[::1]:5889/token");
        assert_eq!(
            token_url("game.example.org"),
            "http://game.example.org:5889/token"
        );
        assert_eq!(token_url("[::1]"), "http://[::1]:5889/token");
        let pairs = [
            ("token", "ab"),
            ("token-url", "http://127.0.0.1:1/t"),
            ("ca", "/ca.pem"),
        ];
        let mut cfg =
            Config::from_pairs(pairs.map(|(k, v)| (k.to_string(), v.to_string()))).unwrap();
        assert_eq!(
            (cfg.token_url.as_deref(), cfg.ca.as_deref(), secure(&cfg)),
            (Some("http://127.0.0.1:1/t"), Some("/ca.pem"), true)
        );
        assert!(
            connect_token(&cfg.token)
                .err()
                .unwrap()
                .contains("2048 bytes")
        );
        let token = ConnectToken::build("[::1]:5888", PROTOCOL, 9, [7; 32])
            .generate()
            .unwrap()
            .try_into_bytes()
            .unwrap();
        cfg.token = token.iter().map(|b| format!("{b:02x}")).collect();
        assert!(connect_token(&cfg.token).is_ok());
        cfg.token.replace_range(..2, "zz");
        assert!(connect_token(&cfg.token).is_err());
        // Natively, without `token=`, it asks `token-url` (nothing listens on port 1).
        cfg.id = 9;
        assert_eq!(token_source(&cfg), "http://127.0.0.1:1/t?id=9");
        let why = fetch(&token_source(&cfg), "a connect token").err().unwrap();
        assert!(
            why.starts_with("could not get a connect token from http://127.0.0.1:1/t?id="),
            "{why}"
        );
        // The map comes from `/info` next to the token endpoint.
        assert_eq!(info_url(&cfg), "http://127.0.0.1:1/info");
        let info = |s: &str| info_map(s.replace("P", &PROTOCOL.to_string()).as_bytes());
        assert_eq!(info(r#"{"map":"30","protocol":P}"#), Ok("30".into()));
        assert!(
            info(r#"{"map":"30","protocol":1}"#)
                .unwrap_err()
                .contains("protocol")
        );
        assert!(info(r#"{"map":"zz","protocol":P}"#).is_err());
        assert_eq!(
            info_pack(br#"{"map":"30","pack":"00ff"}"#).as_deref(),
            Some("00ff")
        );
        assert_eq!(info_pack(br#"{"map":"30"}"#), None);
        // The pack check: the same files pass, any other byte refuses.
        let read = |extra: &'static str| move |f: &str| Some(format!("{f}{extra}").into_bytes());
        let server = conker_sim::net::pack_hash("2d", read("")).unwrap();
        assert_eq!(check_pack("2d", &server, read("")), Ok(()));
        let why = check_pack("2d", &server, read(" ")).unwrap_err();
        assert!(why.contains("not the server's"), "{why}");
    }
    const PROTOCOL: u64 = conker_sim::net::PROTOCOL_ID;

    /// Another player's stand-in fighter carries what the local frame reads of it.
    #[test]
    fn rivals_come_from_combat() {
        let c = conker_sim::net::Combat {
            slot: 3,
            team: 3,
            stun: 2,
            health: 4,
            immune: 7,
            weapon: 0x09,
            drawn: true,
            ..default()
        };
        let body = conker_sim::movement::PlayerState::spawn([1.0, 2.0, 3.0], 9);
        let f = crate::rival(&c, body);
        assert_eq!(
            (
                f.body, f.team, f.stun, f.health, f.immune, f.weapon, f.drawn
            ),
            (body, 3, 2, 4, 7, Some(0x09), true)
        );
    }

    /// RECONNECT: the old client goes and an attempt starts under a fresh id and token; a failure
    /// waits [`RETRY_EVERY`] and tries again, until [`RETRY_SECS`] ran out (the last reason kept
    /// for the page meanwhile).
    #[test]
    fn reconnect_tries_again_until_it_gives_up() {
        let mut app = App::new();
        let start = std::time::Instant::now();
        let mut cfg = Config::from_pairs([("token".to_string(), "ab".to_string())]).unwrap();
        cfg.id = 7;
        app.insert_resource(Time::<Real>::new(start))
            .insert_resource(cfg)
            .insert_resource(NetStatus::Disconnected("connection lost".into()))
            .init_resource::<Reconnect>()
            .add_systems(Update, retry);
        let at = |app: &mut App, secs: f64| {
            let now = start + Duration::from_secs_f64(secs);
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .update_with_instant(now);
            app.update();
        };
        let old = app.world_mut().spawn(Client).id();
        app.world_mut().resource_mut::<Reconnect>().start(0.0);
        at(&mut app, 0.0);
        let cfg = app.world().resource::<Config>();
        assert!(cfg.id != 7 && cfg.token.is_empty());
        assert!(app.world().get_entity(old).is_err(), "the old client went");
        assert_eq!(*app.world().resource::<NetStatus>(), NetStatus::Loading);
        // A full match refuses it: again 2 s later, the reason shown meanwhile.
        *app.world_mut().resource_mut::<NetStatus>() = NetStatus::Disconnected("full".into());
        at(&mut app, 1.0);
        assert_eq!(app.world().resource::<Reconnect>().why, "full");
        at(&mut app, 2.5);
        assert!(matches!(
            app.world().resource::<NetStatus>(),
            NetStatus::Disconnected(_)
        ));
        at(&mut app, 3.0);
        assert_eq!(*app.world().resource::<NetStatus>(), NetStatus::Loading);
        // Failing past RETRY_SECS: given up, the failure left for the page's buttons.
        *app.world_mut().resource_mut::<NetStatus>() = NetStatus::Failed("no answer".into());
        at(&mut app, RETRY_SECS);
        assert!(!app.world().resource::<Reconnect>().active());
        assert!(matches!(
            app.world().resource::<NetStatus>(),
            NetStatus::Failed(_)
        ));
    }

    /// Closing the window: Bevy despawns it and asks to exit every frame; the first request sends
    /// the disconnect, and the exit goes through [`LEAVE_DELAY`] later.
    #[test]
    fn closing_the_window_disconnects_first() {
        let mut app = App::new();
        app.add_plugins((
            bevy::time::TimePlugin,
            bevy::window::WindowPlugin::default(),
        ));
        app.add_systems(Last, leave.after(bevy::window::ExitSystems));
        let client = app
            .world_mut()
            .spawn((Client, RemoteId(PeerId::Server), Connected))
            .id();
        app.add_observer(move |t: On<Disconnect>, mut commands: Commands| {
            commands.entity(t.entity).insert(Name::new("disconnected"));
        });
        app.update();
        let window = app
            .world_mut()
            .query_filtered::<Entity, With<Window>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .write_message(bevy::window::WindowCloseRequested { window });
        let start = std::time::Instant::now();
        while app.should_exit().is_none() && start.elapsed().as_secs_f64() < 2.0 {
            app.update();
        }
        let took = start.elapsed().as_secs_f64();
        assert_eq!(app.should_exit(), Some(AppExit::Success));
        assert!(
            app.world().get::<Window>(window).is_none(),
            "the window closed"
        );
        assert!(
            app.world().get::<Name>(client).is_some(),
            "disconnected first"
        );
        assert!((LEAVE_DELAY..1.0).contains(&took), "{took} s");
    }
}
