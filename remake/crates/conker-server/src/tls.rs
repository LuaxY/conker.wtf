//! The server's certificate, for WebTransport and for WebSocket over TLS (`wss://`): from PEM files
//! (`--cert`, `--cert-key`: e.g. Let's Encrypt's `fullchain.pem` and `privkey.pem`), which browsers
//! check as any site's (no digest), or without them the dev certificate (self-signed, pinned by its
//! digest; WebSocket is then plain `ws://`).
//!
//! Every TLS handshake takes the [`Certificate`] in use, and [`watch`] replaces it when the files
//! change: a renewal reaches new connections while the matches go on (the sessions open keep the
//! keys they agreed). lightyear's `WebTransportServerIo` takes a fixed identity, so
//! [`open_webtransport`] opens the WebTransport server itself, as lightyear would.

use aeronet_webtransport::server::WebTransportServer;
use aeronet_webtransport::wtransport;
use bevy::log::{info, warn};
use bevy::prelude::*;
use core::time::Duration;
use lightyear::prelude::server::Server;
use lightyear::prelude::{LinkStart, Linked, Linking, LocalAddr};
use lightyear_aeronet::AeronetLinkOf;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use wtransport::tls::rustls;

/// Seconds between two looks at the certificate files.
pub const WATCH_SECS: u64 = 10;

/// The certificate in use: what each handshake gets, WebTransport's and `wss://`'s.
#[derive(Debug)]
pub struct Certificate(RwLock<Arc<CertifiedKey>>);

impl Certificate {
    pub fn new(key: Arc<CertifiedKey>) -> Arc<Self> {
        Arc::new(Self(RwLock::new(key)))
    }

    pub fn resolve_any(&self) -> Arc<CertifiedKey> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl ResolvesServerCert for Certificate {
    fn resolve(&self, _: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.resolve_any())
    }
}

/// Both transports sign with ring (rustls has aws-lc-rs too, so it has no default of its own).
fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The chain and the key in PEM files, checked against each other. Any PEM key format loads
/// (PKCS#8, SEC1 EC, PKCS#1 RSA).
pub fn load(cert: &str, key: &str) -> Result<Arc<CertifiedKey>, String> {
    let read = |path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"));
    let chain = CertificateDer::pem_slice_iter(&read(cert)?)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{cert}: {e}"))?;
    if chain.is_empty() {
        return Err(format!("{cert}: no certificate"));
    }
    let secret = PrivateKeyDer::from_pem_slice(&read(key)?).map_err(|e| format!("{key}: {e}"))?;
    CertifiedKey::from_der(chain, secret, &provider())
        .map(Arc::new)
        .map_err(|e| format!("{cert}, {key}: {e}"))
}

/// The dev certificate: self-signed ECDSA P-256, 14-day validity, which is what browsers accept
/// through WebTransport `serverCertificateHashes`. A new one at every start.
pub fn self_signed() -> Arc<CertifiedKey> {
    let identity = wtransport::Identity::self_signed(["localhost", "127.0.0.1", "::1"])
        .expect("self-signed cert");
    let chain = (identity.certificate_chain().as_slice().iter())
        .map(|c| CertificateDer::from(c.der().to_vec()))
        .collect();
    let secret = PrivateKeyDer::try_from(identity.private_key().secret_der().to_vec())
        .expect("self-signed key");
    Arc::new(CertifiedKey::from_der(chain, secret, &provider()).expect("self-signed pair"))
}

/// The certificate's SHA-256 in hex: the digest a dev client pins.
pub fn digest(key: &CertifiedKey) -> String {
    let cert = wtransport::tls::Certificate::from_der(key.cert[0].to_vec()).expect("a certificate");
    cert.hash()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

/// The TLS settings of WebTransport (TLS 1.3 and ALPN h3, as wtransport's own) and of `wss://`,
/// both handing out `current`.
pub fn configs(current: Arc<Certificate>) -> (rustls::ServerConfig, rustls::ServerConfig) {
    let mut wt = rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3")
        .with_no_client_auth()
        .with_cert_resolver(current.clone());
    wt.alpn_protocols = vec![wtransport::tls::WEBTRANSPORT_ALPN.to_vec()];
    let ws = rustls::ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .expect("TLS 1.2 and 1.3")
        .with_no_client_auth()
        .with_cert_resolver(current);
    (wt, ws)
}

/// Looks at the files every [`WATCH_SECS`] on a thread of its own; once either changed, loads them
/// into `current`. A pair that does not load yet (the renewal writes one file, then the other)
/// keeps the one in use, and is tried again at the next look.
pub fn watch(cert: String, key: String, current: Arc<Certificate>) {
    let stamp = move |path: &str| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    std::thread::spawn(move || {
        let mut seen: [Option<SystemTime>; 2] = [stamp(&cert), stamp(&key)];
        loop {
            std::thread::sleep(Duration::from_secs(WATCH_SECS));
            let now = [stamp(&cert), stamp(&key)];
            if now == seen {
                continue;
            }
            match load(&cert, &key) {
                Ok(new) => {
                    *current.0.write().unwrap_or_else(|e| e.into_inner()) = new;
                    seen = now;
                    info!(
                        "certificate reloaded from {cert}: digest {}",
                        digest(&current.resolve_any())
                    );
                }
                Err(e) => warn!("certificate not reloaded yet ({e}): the one in use stays"),
            }
        }
    });
}

/// The WebTransport server's TLS settings ([`configs`]), on the lightyear `Server` entity.
#[derive(Component)]
#[require(Server)]
pub struct WebTransportTls(pub rustls::ServerConfig);

/// Opens the WebTransport server on its [`LocalAddr`], as lightyear's `WebTransportServerIo` does
/// (its other observers accept the sessions and give each its link).
#[allow(clippy::type_complexity)]
pub fn open_webtransport(
    trigger: On<LinkStart>,
    servers: Query<(&WebTransportTls, &LocalAddr), (Without<Linking>, Without<Linked>)>,
    mut commands: Commands,
) {
    let Ok((tls, addr)) = servers.get(trigger.entity) else {
        return;
    };
    let config = wtransport::ServerConfig::builder()
        .with_bind_address(addr.0)
        .with_custom_tls(tls.0.clone())
        .keep_alive_interval(Some(Duration::from_secs(1)))
        .max_idle_timeout(Some(Duration::from_secs(5)))
        .expect("a valid idle timeout")
        .build();
    let server = trigger.entity;
    commands.queue(move |world: &mut World| {
        let child = world.spawn((AeronetLinkOf(server), Name::new("WebTransportServer")));
        WebTransportServer::open(config).apply(child);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pair that does not match is refused, so a half-written renewal never replaces the one in
    /// use; the resolver hands out whichever is current.
    #[test]
    fn pairs_load_checked_and_swap() {
        let dir = std::env::temp_dir().join(format!("conker-tls-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pem = |id: &wtransport::Identity, name: &str| {
            let (cert, key) = (
                dir.join(format!("{name}.pem")),
                dir.join(format!("{name}-key.pem")),
            );
            let chain = id.certificate_chain().as_slice()[0].to_pem();
            std::fs::write(&cert, chain).unwrap();
            std::fs::write(&key, id.private_key().to_secret_pem()).unwrap();
            (cert.display().to_string(), key.display().to_string())
        };
        let a = wtransport::Identity::self_signed(["localhost"]).unwrap();
        let b = wtransport::Identity::self_signed(["localhost"]).unwrap();
        let (cert_a, key_a) = pem(&a, "a");
        let (cert_b, key_b) = pem(&b, "b");
        let first = load(&cert_a, &key_a).unwrap();
        assert!(load(&cert_a, &key_b).unwrap_err().contains("KeyMismatch"));
        let current = Certificate::new(first.clone());
        assert_eq!(digest(&current.resolve_any()), digest(&first));
        *current.0.write().unwrap() = load(&cert_b, &key_b).unwrap();
        assert_ne!(digest(&current.resolve_any()), digest(&first));
        assert_eq!(digest(&self_signed()).len(), 64);
        let _ = configs(current);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
