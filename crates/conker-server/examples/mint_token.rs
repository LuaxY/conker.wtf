//! Test tool for the token checks: mints a connect token as the server's endpoint does, but with
//! any key, named address and lifetime, so a client can try a forged (another or the all-zero key),
//! misaddressed or expired token. Prints it as hex, for conker-client's `--token`.
//! Usage: cargo run -p conker-server --example mint_token -- KEY_HEX ADDR ID EXPIRE_SECS

use lightyear::netcode::ConnectToken;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [key, addr, id, secs] = &args[..] else {
        panic!("usage: mint_token KEY_HEX ADDR ID EXPIRE_SECS");
    };
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&key[2 * i..2 * i + 2], 16).expect("64 hex digits");
    }
    let addr: std::net::SocketAddr = addr.parse().expect("ADDR");
    let token = ConnectToken::build(
        addr,
        conker_sim::net::PROTOCOL_ID,
        id.parse().expect("ID"),
        k,
    )
    .timeout_seconds(3)
    .expire_seconds(secs.parse().expect("EXPIRE_SECS"))
    .generate()
    .expect("token")
    .try_into_bytes()
    .expect("2048 bytes");
    println!(
        "{}",
        token.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
}
