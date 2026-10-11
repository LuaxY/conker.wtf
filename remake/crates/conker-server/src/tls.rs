//! The server's certificate from PEM files (`--cert`, `--cert-key`: e.g. Let's Encrypt's
//! `fullchain.pem` and `privkey.pem`), for WebTransport and for WebSocket over TLS (`wss://`).
//! Browsers check it as any site's (no digest). Without them the server makes its dev
//! certificate (self-signed, pinned by digest) and serves plain `ws://`.

use lightyear::prelude::Identity;
use lightyear::websocket::prelude::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use lightyear::websocket::server::Identity as WsIdentity;

/// The certificate chain and key, for each transport. Any PEM key format loads (PKCS#8, SEC1 EC,
/// PKCS#1 RSA).
pub fn load(cert: &str, key: &str) -> Result<(Identity, WsIdentity), String> {
    // wtransport reads the files with tokio::fs, which needs a tokio runtime.
    let identity = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| e.to_string())?
        .block_on(Identity::load_pemfiles(cert, key))
        .map_err(|e| format!("{cert}, {key}: {e}"))?;
    let chain: Vec<CertificateDer<'static>> = identity
        .certificate_chain()
        .as_slice()
        .iter()
        .map(|c| CertificateDer::from(c.der().to_vec()))
        .collect();
    if chain.is_empty() {
        return Err(format!("{cert}: no certificate"));
    }
    let secret = PrivateKeyDer::try_from(identity.private_key().secret_der().to_vec())
        .map_err(|e| format!("{key}: {e}"))?;
    Ok((identity, WsIdentity::new(chain, secret)))
}
