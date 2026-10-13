//! The connection to the game server ([`build`]).
//! - **When:** once the level and the character have loaded (windowed) and [`CALM`] frames in a row
//!   took under [`CALM_SECS`]: the seconds the load and the first draws after it block the main
//!   thread (V: 7 s in a debug build under load; shaders compile in the browser) would trip the 3 s
//!   netcode timeout. Headless clients have no assets, so they connect after the calm frames.
//! - **Status** ([`NetStatus`]): loading, connecting, connected, failed or disconnected, with the
//!   reason; logged on every change, and in the browser shown in the page's `#net` line.
//! - **Input** starts with the follow camera (the client synced, its player arrived): no input goes
//!   out before, `predict` steps nothing before (it reads the input), and the server holds the
//!   player as it joined until that first input, so both start on the same tick from the same state.
//! - **Leaving:** an exit (the window closed, `seconds`) first sends netcode's disconnect
//!   and exits [`LEAVE_DELAY`] later, once it has gone out. A closed page runs no more frames: its
//!   transport closes (the browser's unload, and winit's exit dropping the session), which frees
//!   the slot on the server as well.
//! - **Token:** the server signs each client's netcode connect token (its key never reaches the
//!   client). Without `token=`, the client fetches it as it connects, after the assets loaded (a
//!   token lasts 60 s): from `token-url`, else (an https page) `/token` on the page's own origin,
//!   else the game server's host, its port + 1 ([`token_source`]). Natively at once; in the browser
//!   in the background, while [`connect`] waits for it.
//! - **TLS** ([`secure`]): an https page uses `wss://` and checks the WebTransport certificate as
//!   any site's (no digest); a native client does so with `ca=` (dev/test only). Else the dev
//!   server's self-signed certificate is pinned by its digest, and WebSocket is plain `ws://`.

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

/// Netcode's timeout both ways (the connect token carries it to the server): a client that sends
/// nothing for this long loses its slot.
pub const TIMEOUT_SECS: i32 = 3;
/// Seconds a connection attempt may take: a WebTransport handshake that never completes (a stale
/// certificate digest in some browsers, blocked UDP) raises no error of its own.
pub const CONNECT_SECS: f64 = 10.0;
/// Seconds a leaving client keeps running after its netcode disconnect, so the packets go out.
pub const LEAVE_DELAY: f64 = 0.2;
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
            Self::Connecting(_) => (format!("connecting to {server} over {over}…"), false),
            Self::Connected => (format!("connected to {server} over {over}"), false),
            Self::Failed(why) => {
                let hint = match cfg.transport {
                    Transport::WebTransport if secure(cfg) => {
                        "Is the server running? Reload the page for a new connect token. \
                         ?transport=ws tries WebSocket"
                    }
                    Transport::WebTransport if cfg.digest.is_empty() => {
                        "No certificate digest: pass ?digest=HEX, or run the server with \
                         --digest-file web/dist/digest.txt"
                    }
                    Transport::WebTransport => {
                        "Is the server running? A restarted server has a new certificate: \
                         reload the page. ?transport=ws tries WebSocket"
                    }
                    Transport::WebSocket => "Is the server running?",
                };
                (
                    format!("could not connect to {server} over {over}: {why}. {hint}"),
                    true,
                )
            }
            Self::Disconnected(why) => (
                format!("disconnected from {server}: {why}. Reload to reconnect"),
                true,
            ),
        }
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
        DisconnectedReason::ByPeer(_) | DisconnectedReason::LinkFailed(UnlinkReason::ByPeer(_)) => {
            "the server closed the connection".into()
        }
        DisconnectedReason::LinkFailed(UnlinkReason::TransportError(e)) => tidy(e),
        DisconnectedReason::UserRequested(_) => "left".into(),
        other => other.to_string(),
    }
}

/// A transport's error on one line: its causes joined with `: `, without the browser's
/// `[object Event]`, a cause that repeats the one before (Chrome's WebTransport: `connection
/// lost: Connection lost`) or a trailing period; WebSocket's abnormal close (1006) in words.
fn tidy(error: &str) -> String {
    if error.contains("code 1006") {
        return "the connection dropped (WebSocket code 1006)".into();
    }
    let line = error.split_whitespace().collect::<Vec<_>>().join(" ");
    let line = line
        .replace(" Caused by: ", ": ")
        .replace(": [object Event]", "");
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
    app.add_systems(Update, (connect.run_if(ready), watchdog, show).chain());
    app.add_systems(Last, leave.after(bevy::window::ExitSystems));
    app.add_systems(
        PostUpdate,
        delay_sends
            .after(ConnectionSystems::Send)
            .before(LinkSystems::Send),
    );
    #[cfg(not(target_family = "wasm"))]
    app.add_observer(ca::link);
    app.add_observer(on_connecting)
        .add_observer(on_connected)
        .add_observer(on_disconnected)
        .add_observer(on_attached)
        .add_observer(on_player_left);
}

/// Not connected yet, (windowed) the level and the characters loaded, and the frames since calm.
fn ready(
    status: Res<NetStatus>,
    assets: Option<Res<AssetServer>>,
    time: Res<Time<Real>>,
    mut calm: Local<u32>,
    cfg: Res<Config>,
) -> bool {
    // The level and the characters (`render`'s assets): windowed clients connect once they loaded.
    let loaded = assets.is_none_or(|a| {
        let characters = crate::characters(&cfg.map).into_iter();
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
    *status == NetStatus::Loading && *calm >= CALM
}

fn connect(
    mut commands: Commands,
    mut cfg: ResMut<Config>,
    mut status: ResMut<NetStatus>,
    #[cfg(target_family = "wasm")] mut fetching: Local<Fetching>,
) {
    if cfg.token.is_empty() {
        #[cfg(not(target_family = "wasm"))]
        let fetched = fetch(&token_source(&cfg), "a connect token");
        #[cfg(target_family = "wasm")]
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
    let roots = match cfg.ca.as_deref().map(ca::roots).transpose() {
        Ok(roots) => roots,
        Err(why) => {
            *status = NetStatus::Failed(why);
            return;
        }
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
        Transport::WebTransport if let Some(roots) = roots => client
            .insert_if_new(Link::default())
            .insert(ca::WebTransportCa {
                roots,
                target: format!("https://{}", cfg.server),
            }),
        Transport::WebTransport => client.insert(WebTransportClientIo {
            certificate_digest: cfg.digest.clone(),
            target: Some(format!("https://{}", cfg.server)),
        }),
        Transport::WebSocket => {
            #[cfg(target_family = "wasm")]
            let ws = ClientConfig;
            #[cfg(not(target_family = "wasm"))]
            let ws = match roots {
                Some(roots) => ca::websocket(roots),
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

/// Natively: the server's map, from its `/info` (for 5 s while the server starts).
#[cfg(not(target_family = "wasm"))]
pub fn server_map(cfg: &Config) -> Result<String, String> {
    let get = || fetch(&info_url(cfg), "the server's map");
    let mut body = get();
    for _ in 0..20 {
        if body.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
        body = get();
    }
    info_map(body?.as_bytes())
}

/// The browser's token fetch, in flight: [`connect`] polls it.
#[cfg(target_family = "wasm")]
#[derive(Default)]
struct Fetching(Option<std::sync::Arc<std::sync::OnceLock<Result<String, String>>>>);

#[cfg(target_family = "wasm")]
impl Fetching {
    /// The token once it has arrived; the first call starts the fetch.
    fn poll(&mut self, cfg: &Config) -> Option<Result<String, String>> {
        let done = self.0.get_or_insert_with(|| {
            let done = std::sync::Arc::new(std::sync::OnceLock::new());
            let out = std::sync::Arc::clone(&done);
            let request = crate::web::fetch(&token_source(cfg));
            wasm_bindgen_futures::spawn_local(async move {
                let hex = request.await.and_then(|b| {
                    String::from_utf8(b).map_err(|_| "the token is not text".to_string())
                });
                let _ = out.set(hex.map_err(|e| format!("could not get a connect token from {e}")));
            });
            done
        });
        done.get().cloned()
    }
}

/// The netcode connect token from its hex.
fn connect_token(hex: &str) -> Result<ConnectToken, String> {
    let hex = hex.trim();
    let bytes = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
        .collect::<Option<Vec<u8>>>()
        .filter(|b| b.len() == 2048)
        .ok_or("the connect token is not 2048 bytes of hex")?;
    ConnectToken::try_from_bytes(&bytes).map_err(|e| format!("bad connect token: {e}"))
}

/// A plain-HTTP GET's body (native: the token and `/info` endpoints on the game server's host).
#[cfg(not(target_family = "wasm"))]
fn fetch(url: &str, what: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    let fail = |e: &dyn std::fmt::Display| format!("could not get {what} from {url}: {e}");
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| fail(&"natively only http:// (or pass token=HEX)"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    // ponytail: no connect timeout (a dev tool on loopback or a LAN); add one for remote hosts.
    let mut stream = std::net::TcpStream::connect(host).map_err(|e| fail(&e))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| fail(&e))?;
    write!(stream, "GET /{path} HTTP/1.0\r\nHost: {host}\r\n\r\n").map_err(|e| fail(&e))?;
    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|e| fail(&e))?;
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    match head.split_whitespace().nth(1) {
        Some("200") => Ok(body.to_string()),
        _ => Err(fail(&head.lines().next().unwrap_or("no answer"))),
    }
}

/// Dev/test only (`ca=PATH`): a native client checks the server's certificate against this CA, as
/// a browser checks a real one: `wss://`, and WebTransport without a digest (lightyear's own
/// WebTransport link only pins a digest, so this one spawns the transport itself).
#[cfg(not(target_family = "wasm"))]
mod ca {
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
    use std::sync::Arc;

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

    /// A WebTransport link checked against [`roots`]'s CA (lightyear's settings otherwise).
    #[derive(Component)]
    pub struct WebTransportCa {
        pub roots: Arc<RootCertStore>,
        pub target: String,
    }

    pub fn link(
        trigger: On<LinkStart>,
        links: Query<&WebTransportCa, (Without<Linking>, Without<Linked>)>,
        mut commands: Commands,
    ) -> Result {
        let Ok(ca) = links.get(trigger.entity) else {
            return Ok(());
        };
        let tls = wtransport::tls::client::build_default_tls_config(ca.roots.clone(), None);
        let config = wtransport::ClientConfig::builder()
            .with_bind_config(wtransport::config::IpBindConfig::InAddrAnyDual)
            .with_custom_tls(tls)
            .keep_alive_interval(Some(Duration::from_secs(1)))
            .max_idle_timeout(Some(Duration::from_secs(5)))?
            .build();
        let (link, target) = (trigger.entity, ca.target.clone());
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

fn on_connecting(
    trigger: On<Add, Connecting>,
    clients: Query<(), With<Client>>,
    time: Res<Time<Real>>,
    mut status: ResMut<NetStatus>,
) {
    if clients.contains(trigger.entity) {
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
/// that one is ignored, as is any after a [`watchdog`] failure).
fn on_disconnected(
    trigger: On<Insert, Disconnected>,
    clients: Query<&Disconnected, With<Client>>,
    mut status: ResMut<NetStatus>,
) {
    let Ok(d) = clients.get(trigger.entity) else {
        return;
    };
    match *status {
        NetStatus::Connecting(_) => *status = NetStatus::Failed(why(&d.reason)),
        NetStatus::Connected => *status = NetStatus::Disconnected(why(&d.reason)),
        _ => {}
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
/// comes back [`LEAVE_DELAY`] later (a closed window asks again every frame meanwhile).
fn leave(
    mut exits: ResMut<Messages<AppExit>>,
    time: Res<Time<Real>>,
    clients: Query<Entity, (With<Client>, With<Connected>)>,
    mut leaving: Local<Option<(f64, AppExit)>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    if let Some((at, exit)) = leaving.clone() {
        exits.clear();
        if now >= at {
            exits.write(exit);
        }
        return;
    }
    let Some(exit) = exits.drain().last() else {
        return;
    };
    match clients.single() {
        Ok(entity) => {
            info!("leaving: disconnecting first");
            commands.trigger(Disconnect { entity });
            *leaving = Some((now + LEAVE_DELAY, exit));
        }
        Err(_) => {
            exits.write(exit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_read_with_their_reasons_and_hints() {
        let mut cfg = Config::from_pairs([("id".to_string(), "1".to_string())]).unwrap();
        cfg.server = "[::1]:5888".into();
        let text = |s: NetStatus, cfg: &Config| s.text(cfg);
        assert_eq!(
            text(NetStatus::Connected, &cfg),
            ("connected to [::1]:5888 over WebTransport".into(), false)
        );
        let (t, problem) = text(NetStatus::Failed("the server did not answer".into()), &cfg);
        assert!(
            problem && t.contains("did not answer. No certificate digest"),
            "{t}"
        );
        cfg.digest = "AB".into();
        let (t, _) = text(NetStatus::Failed("x".into()), &cfg);
        assert!(t.contains("new certificate: reload"), "{t}");
        cfg.transport = Transport::WebSocket;
        let (t, problem) = text(NetStatus::Disconnected("left".into()), &cfg);
        assert!(problem && t.ends_with("left. Reload to reconnect"), "{t}");
    }

    #[test]
    fn netcode_states_read_as_words() {
        let e = |s: &str| why(&DisconnectedReason::TransportError(s.into()));
        assert_eq!(e("ConnectionTimedOut"), "nothing from the server for 3 s");
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
        assert_eq!(why(&DisconnectedReason::UserRequested(None)), "left");
        let link = |r| why(&DisconnectedReason::LinkFailed(r));
        assert_eq!(
            link(UnlinkReason::TransportError("connection lost".into())),
            "connection lost"
        );
        assert_eq!(
            link(UnlinkReason::ByPeer("x".into())),
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
