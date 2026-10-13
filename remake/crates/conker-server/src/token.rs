//! Connect tokens. The server signs each client's netcode connect token with a private key that
//! never leaves this process, and hands it out over plain HTTP: `GET /token[?id=N]` answers the
//! token's 2048 bytes as hex (a random client id without `id=`). `GET /info` answers what a client
//! needs before it loads anything, and what a server list shows:
//! `{"map":"2d","protocol":33,"pack":"…","humans":1,"bots":3}` (the pack map, [`PROTOCOL_ID`], the
//! pack's fingerprint, conker-sim's `pack_hash`, and the players in the match now, [`Live`]).
//!
//! The endpoint lives in the game server because the server holds the key and its public
//! addresses; production binds it to loopback and the static server (Caddy, `deploy/`) proxies the
//! page's own `/token` to it, so HTTPS and same-origin come from there. In dev the page fetches it
//! from the game server's host, port + 1 (`--token-addr`).

use bevy::log::{info, warn};
use conker_sim::net::PROTOCOL_ID;
use lightyear::netcode::{ConnectToken, Key, generate_key};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Seconds a token may start a connection: the page fetches it when the ROM is picked and connects
/// once the level has loaded (2-3 s here, 7+ s in a debug build under load).
pub const TOKEN_SECS: i32 = 60;
/// Netcode's timeout both ways, carried in the token (conker-client's `TIMEOUT_SECS`).
pub const TIMEOUT_SECS: i32 = 3;

/// The private key: 64 hex digits from `--token-key FILE` or `CONKER_TOKEN_KEY` (the file wins);
/// without either, a random key for this run (dev: tokens from an earlier run are refused).
pub fn key(file: Option<&str>, env: Option<String>) -> Result<Key, String> {
    match (file, env) {
        (Some(path), _) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            parse_key(&text).map_err(|e| format!("{path}: {e}"))
        }
        (None, Some(hex)) => parse_key(&hex).map_err(|e| format!("CONKER_TOKEN_KEY: {e}")),
        (None, None) => Ok(generate_key()),
    }
}

/// 32 bytes as 64 hex digits (`openssl rand -hex 32`), not all zero (the old dev key).
pub fn parse_key(text: &str) -> Result<Key, String> {
    let hex = text.trim();
    let bad = || "the key must be 64 hex digits (openssl rand -hex 32)".to_string();
    if hex.len() != 64 || !hex.is_ascii() {
        return Err(bad());
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
    }
    if key == [0; 32] {
        return Err("an all-zero key is the public dev key: generate one".into());
    }
    Ok(key)
}

/// The humans and bots in the match now, which the server sets each tick and `/info` reads.
#[derive(Clone, Default)]
pub struct Live(pub Arc<Mutex<(usize, usize)>>);

/// Mints tokens for this server: its key, and the addresses the tokens name (the public ones);
/// and tells the map played, the pack's fingerprint and the players in.
pub struct Issuer {
    pub key: Key,
    pub addrs: Vec<SocketAddr>,
    pub map: String,
    pub pack: String,
    pub live: Live,
}

impl Issuer {
    pub fn token(&self, id: u64) -> Result<[u8; 2048], String> {
        ConnectToken::build(&self.addrs[..], PROTOCOL_ID, id, self.key)
            .timeout_seconds(TIMEOUT_SECS)
            .expire_seconds(TOKEN_SECS)
            .generate()
            .map_err(|e| e.to_string())?
            .try_into_bytes()
            .map_err(|e| e.to_string())
    }

    /// The answer to an HTTP request line: status and body.
    fn answer(&self, request: &str) -> (&'static str, String) {
        let mut words = request.split_whitespace();
        let (Some("GET"), Some(target)) = (words.next(), words.next()) else {
            return (
                "405 Method Not Allowed",
                "GET /token[?id=N] or /info\n".into(),
            );
        };
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if path == "/info" {
            let (humans, bots) = *self.live.0.lock().unwrap_or_else(|e| e.into_inner());
            let info = format!(
                "{{\"map\":\"{}\",\"protocol\":{PROTOCOL_ID},\"pack\":\"{}\",\"humans\":{humans},\"bots\":{bots}}}",
                self.map, self.pack
            );
            return ("200 OK", info);
        }
        if path != "/token" {
            return ("404 Not Found", "GET /token[?id=N] or /info\n".into());
        }
        let id = match query.split('&').find_map(|kv| kv.strip_prefix("id=")) {
            Some(id) => match id.parse() {
                Ok(id) => id,
                Err(_) => return ("400 Bad Request", "id must be a number\n".into()),
            },
            None => u64::from_le_bytes(generate_key()[..8].try_into().unwrap()).max(1),
        };
        match self.token(id) {
            Ok(token) => {
                info!("token issued to client {id}");
                ("200 OK", token.iter().map(|b| format!("{b:02x}")).collect())
            }
            Err(e) => ("500 Internal Server Error", format!("{e}\n")),
        }
    }
}

/// Answers token requests on `listener`, one at a time, on a thread of its own.
pub fn serve(listener: TcpListener, issuer: Issuer) {
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Err(e) = reply(stream, &issuer) {
                warn!("token request: {e}");
            }
        }
    });
}

fn reply(mut stream: TcpStream, issuer: &Issuer) -> std::io::Result<()> {
    // ponytail: one request at a time with a 2 s read limit; a thread per request if pages wait.
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut head = BufReader::new((&stream).take(8192));
    let mut request = String::new();
    head.read_line(&mut request)?;
    let mut line = String::new();
    while head.read_line(&mut line)? > 2 {
        line.clear();
    }
    let (status, body) = issuer.answer(&request);
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_64_hex_digits_and_never_zero() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899AABBCCDDEEFF\n";
        let k = parse_key(hex).unwrap();
        assert_eq!((k[0], k[1], k[31]), (0x00, 0x11, 0xff));
        assert!(parse_key(&hex[..62]).is_err());
        assert!(parse_key(&"zz".repeat(32)).is_err());
        assert!(parse_key(&"0".repeat(64)).unwrap_err().contains("zero"));
        assert_eq!(key(None, Some(hex.into())).unwrap()[1], 0x11);
        assert_ne!(key(None, None).unwrap(), key(None, None).unwrap());
    }

    #[test]
    fn the_endpoint_answers_get_token_and_info_only() {
        let issuer = Issuer {
            key: [7; 32],
            addrs: vec!["[::1]:5888".parse().unwrap()],
            map: "30".into(),
            pack: "00ff".into(),
            live: Live::default(),
        };
        let (status, body) = issuer.answer("GET /token?id=42 HTTP/1.1\r\n");
        assert_eq!((status, body.len()), ("200 OK", 4096));
        let bytes: Vec<u8> = (0..2048)
            .map(|i| u8::from_str_radix(&body[2 * i..2 * i + 2], 16).unwrap())
            .collect();
        assert!(ConnectToken::try_from_bytes(&bytes).is_ok());
        assert_eq!(issuer.answer("GET /token HTTP/1.0").0, "200 OK");
        assert_eq!(
            issuer.answer("GET /token?id=x HTTP/1.1").0,
            "400 Bad Request"
        );
        assert_eq!(issuer.answer("GET / HTTP/1.1").0, "404 Not Found");
        *issuer.live.0.lock().unwrap() = (1, 3);
        let info = format!(
            "{{\"map\":\"30\",\"protocol\":{PROTOCOL_ID},\"pack\":\"00ff\",\"humans\":1,\"bots\":3}}"
        );
        assert_eq!(issuer.answer("GET /info HTTP/1.1"), ("200 OK", info));
        assert_eq!(
            issuer.answer("POST /token HTTP/1.1").0,
            "405 Method Not Allowed"
        );
        assert_eq!(issuer.answer("").0, "405 Method Not Allowed");
    }

    /// Over a real socket: the headers a page needs (cross-origin in dev, never cached).
    #[test]
    fn the_endpoint_serves_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        serve(
            listener,
            Issuer {
                key: [7; 32],
                addrs: vec![addr],
                map: "2d".into(),
                pack: String::new(),
                live: Live::default(),
            },
        );
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(stream, "GET /token?id=3 HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");
        assert!(head.contains("Access-Control-Allow-Origin: *"), "{head}");
        assert!(head.contains("Cache-Control: no-store"), "{head}");
        assert_eq!(body.len(), 4096);
    }
}
