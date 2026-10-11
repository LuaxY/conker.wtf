# Deploying on a small Linux VPS

What runs on the host:

| Process | Listens on | Does |
|---|---|---|
| Caddy | TCP 80 (redirect), TCP 443 (+ UDP 443, HTTP/3) | serves `https://DOMAIN/`: the landing (`/`) and the game page (`/play.html`) from `deploy/build.sh`, and the asset pack at `/assets/` (brotli or gzip as the browser asks, cache headers), and proxies `/token` and `/info` to the game server |
| `conker-server` (systemd) | TCP + UDP 5888, and `127.0.0.1:5889` | the match: WebTransport (UDP) and `wss://` (TCP) with the domain's certificate; connect tokens on loopback |
| certbot | TCP 80, only while renewing | obtains and renews the Let's Encrypt certificate, then `renew-hook.sh` hands it to both |

How a player connects: the landing (`/`) lists the servers and opens a lobby, whose "Join game" opens the game
page, `/play.html?server=DOMAIN:5888&transport=wt&side=...` (`web/README.md`, "Landing"; the server list and lobbies
are mock for now, "This machine" is this host's server). The game page (over https) loads the client, which asks `/info` which map the server plays
(`conker-server --map 2d|30|34`), fetches that map and the rest of the asset pack from `/assets/` and
draws the level; then it fetches `/token` from its own origin. Caddy passes it to the
game server, which signs a netcode connect token with its private key (`/etc/conker/token.key`, never
sent anywhere) for the domain's addresses. The page then connects to `DOMAIN:5888` over WebTransport, or
`wss://` with `?transport=ws`; the browser checks the certificate as for any site. The token is valid for
60 s; the server refuses one that expired, was signed with another key, or names other addresses.

**The asset pack.** Nobody picks or uploads a ROM. The operator exports the asset pack once from
**their own ROM** on their machine (`conker-extract pack`: JSON, glTF, PNG and WAV files, about 40 MB) and
uploads it to the host (step 6). The game server reads it at start (the level's collision, spawns,
pickups, the bots' data), and Caddy serves the same folder at `https://DOMAIN/assets/`, where each
page's client fetches it. It is Rare's data, served to anyone who reaches the page: for a public
deployment turn on the Caddyfile's password (step 8). Neither the ROM nor the pack goes in the site
folder or in this source tree.

Files here: `build.sh` (the pages), `pack.sh` (the pack's upload), `Caddyfile`, `conker-server.service`,
`renew-hook.sh`. The examples use
`game.example.org`, `203.0.113.7` and `2001:db8::7`: replace them with yours.

## 1. The host and the domain

- A VPS with Debian 12 or Ubuntu 24.04, x86-64 or arm64. Running the server takes little (2 vCPU, 2 GB).
  Building it takes more (step 5).
- DNS at your registrar, for the game's name:
  - `A     game.example.org  203.0.113.7` (the VPS's IPv4);
  - `AAAA  game.example.org  2001:db8::7` (its IPv6, if it has one);
  - a low TTL (300) while you set up.

  Check from your machine: `dig +short A game.example.org` and `dig +short AAAA game.example.org`.

## 2. Packages

```sh
sudo apt update
sudo apt install -y caddy certbot ufw build-essential curl rsync
```

Debian 12's own `caddy` package is old; for a current one use Caddy's repository
(<https://caddyserver.com/docs/install#debian-ubuntu-raspbian>). Then, for building the server:
`curl https://sh.rustup.rs -sSf | sh` (the repository's `rust-toolchain.toml` picks the version).

## 3. Firewall

```sh
sudo ufw allow 22/tcp      # ssh
sudo ufw allow 80/tcp      # http -> https redirect, and certbot's challenge
sudo ufw allow 443/tcp     # the page
sudo ufw allow 443/udp     # the page over HTTP/3 (optional)
sudo ufw allow 5888/tcp    # the game: WebSocket (wss://)
sudo ufw allow 5888/udp    # the game: WebTransport
sudo ufw enable
```

Port 5889 (tokens) stays closed: the server binds it to loopback. If your provider has its own firewall
(security groups), open the same ports there.

## 4. A user, folders and the token key

```sh
sudo useradd --system --home /opt/conker --shell /usr/sbin/nologin conker
sudo install -d -o root -g conker -m 0750 /opt/conker /etc/conker
sudo usermod -aG conker caddy                       # Caddy reads the certificate copy in /etc/conker/tls
openssl rand -hex 32 | sudo install -o root -g conker -m 0640 /dev/stdin /etc/conker/token.key
sudo install -o root -g root -m 0755 deploy/renew-hook.sh /etc/conker/renew-hook.sh
```

Keep `token.key` secret: anyone with it can mint tokens for your server. A new key only invalidates
tokens not yet used.

## 5. The server binary

Build it on the VPS (or on another Linux machine of the same architecture) from this repository:

```sh
cd remake
cargo build --release -p conker-server          # Bevy + lightyear: ~15-30 min the first time
sudo install -m 0755 target/release/conker-server /opt/conker/conker-server
```

Linking needs more memory than 2 GB: add swap first if the build is killed
(`sudo fallocate -l 4G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile`).

## 6. The asset pack

On your machine (`pack.sh` needs `brotli` and `rsync`):

```sh
cd remake
CONKER_ROM="/path/to/Conker's Bad Fur Day (USA).z64" cargo run --release -p conker-extract -- pack
#   once: writes the pack to ../assets, next to this tree (it refuses to overwrite one: your edits)
ssh vps sudo install -d -o $USER /srv/conker/assets
sh deploy/pack.sh vps:/srv/conker/assets      # a copy with .br and .gz files, rsynced with --delete
```

`pack.sh` compresses a copy in a temporary folder (outside this tree) for Caddy's `precompressed` (JSON,
glTF, `.bin` and the sound's WAVs; the background WAV goes as is, 4.2 MB brotli), and uploads it: about 400
files and their compressed copies. The pack is yours to edit
(`../assets/README.md`): edit it on your machine, run `pack.sh` again, then restart the game server
(`sudo systemctl restart conker-server`; it reads the pack at start) and reload the pages. Never edit it
on the host: Caddy serves a file's `.br` or `.gz` copy as long as it exists, even when the file is newer.
The server and every page must read the same pack: the clients predict on the same collision, in the
same order.

## 7. The page

On your machine (the build needs the wasm toolchain of `web/README.md`, `wasm-opt`, `brotli` and `bun`, which builds
the landing from `web/app`):

```sh
cd remake
WASM_OPT=/path/to/wasm-opt sh deploy/build.sh            # -> target-net/site (about 10 minutes)
ssh vps sudo install -d -o $USER /srv/conker/site
rsync -a target-net/site/ vps:/srv/conker/site/
```

The site holds `index.html` and `landing/` (the landing: its scripts, styles and fonts, named by their
content), `play.html`, `play.js` and `v/<hash>/` (the client, named by its content), the text files with
`.br` and `.gz` copies. Pages revalidate `index.html`, `play.html` and `play.js` on every load; `landing/`
and the client folder are cached for a year, and each build makes new names. Old `landing/` files and `v/`
folders can be deleted once no page has them open (`rsync --delete` does it at the next upload). Deploy the server and the page together: they must
have the same protocol version (`PROTOCOL_ID`), or the server refuses the page's token.

## 8. The certificate, the server and Caddy

Install the game server's unit and edit its `--public` addresses to your A and AAAA records (one
`--public` per address; leave out the IPv6 one if the host has none):

```sh
sudo cp deploy/conker-server.service /etc/systemd/system/
sudo systemctl edit --full conker-server            # set the two --public addresses
sudo systemctl enable conker-server                 # started by the hook below
```

Caddy: the Caddyfile and its settings.

```sh
sudo cp deploy/Caddyfile /etc/caddy/Caddyfile
sudo systemctl edit caddy        # add the two lines below, save
#   [Service]
#   Environment=DOMAIN=game.example.org SITE=/srv/conker/site PACK=/srv/conker/assets TLS_DIR=/etc/conker/tls
```

**A password** (for a public deployment: `/assets/` serves Rare's data to anyone who reaches the page).
The Caddyfile has a `basic_auth` block for the whole site, commented out:

```sh
caddy hash-password                  # type the password: it prints a hash
sudo nano /etc/caddy/Caddyfile       # uncomment the basic_auth block and put the hash in place of HASH
sudo systemctl reload caddy          # (once Caddy runs, after the certificate below)
```

The browser asks once and sends it with every request to the site: the page, the pack and `/token`. The
game port needs no password: nobody joins a match without a token from `/token`.

Then the certificate. certbot answers Let's Encrypt's challenge itself on port 80 (`--standalone`), so
Caddy stops for those few seconds (`--pre-hook`, `--post-hook`); the deploy hook copies the certificate
to `/etc/conker/tls` and restarts Caddy and the game server. certbot saves the hooks and renews on its
own (its systemd timer, twice a day; a renewal happens 30 days before expiry):

```sh
sudo certbot certonly --standalone -d game.example.org -m you@example.org --agree-tos \
  --pre-hook "systemctl stop caddy" --post-hook "systemctl start caddy" \
  --deploy-hook /etc/conker/renew-hook.sh
```

The server reads its certificate at start, so each renewal restarts it: matches in progress end and the
pages say `Reload to reconnect`. To keep that out of busy hours, move certbot's timer
(`sudo systemctl edit certbot.timer`).

## 9. Check

On the host:

```sh
systemctl status conker-server caddy
journalctl -u conker-server -n 20       # "Listening on [::]:5888: ... (TCP, wss://) ... connect tokens on
                                        #  http://127.0.0.1:5889/token, for [203.0.113.7:5888, [2001:db8::7]:5888]"
sysctl net.ipv6.bindv6only              # 0: the [::] sockets accept IPv4 too (Linux's default)
ss -tuln | grep ':588[89]'              # tcp and udp *:5888 (dual-stack; [::]:5888 would mean IPv6 only),
                                        # tcp 127.0.0.1:5889
```

If `net.ipv6.bindv6only` is 1, IPv4 players can't connect: set it to 0
(`echo net.ipv6.bindv6only=0 | sudo tee /etc/sysctl.d/90-dual-stack.conf && sudo sysctl --system`) and
restart the server.

From your machine:

```sh
curl -sI https://game.example.org/ | grep -iE '^(HTTP|cache-control)'          # 200, no-cache
curl -s https://game.example.org/play.js | grep -o 'v/[0-9a-f]*/client.js'     # the client's folder
curl -s https://game.example.org/ | grep -o 'landing/index-[^"]*\.js' \
  | xargs -I{} curl -sI https://game.example.org/{} | grep -i '^cache-control'    # public, max-age=31536000, immutable
curl -s -D - -o /dev/null -H 'Accept-Encoding: br' https://game.example.org/v/HASH/client_bg.wasm \
  | grep -iE '^(content-(type|encoding|length)|cache-control|vary)'
#   application/wasm, br, about 5 MB, public, max-age=31536000, immutable, Vary: Accept-Encoding
curl -s -D - -o /dev/null -H 'Accept-Encoding: br' https://game.example.org/assets/maps/2d/level.gltf \
  | grep -iE '^(HTTP|content-encoding|cache-control|etag)'
#   200, br, no-cache, an ETag (the pack is editable: pages revalidate it on each load)
curl -s https://game.example.org/token | wc -c                                    # 4096 (hex)
#   With the password on: 401 without it; add -u player:PASSWORD to each curl
nc -4 -vz game.example.org 5888 && nc -6 -vz game.example.org 5888                # TCP over IPv4 and IPv6
```

Then open `https://game.example.org/` in Chrome: Join game, This machine, Join game opens the game page,
which loads the pack and starts by itself; the line at the bottom left reads
`connected to game.example.org:5888 over WebTransport`. Open `https://game.example.org/play.html?transport=ws` in a
second window: `... over WebSocket` (that is `wss://`). For UDP over IPv4 (WebTransport into the `[::]`
socket), load the page once from an IPv4-only network (many mobile hotspots).

Renewal: `sudo certbot renew --dry-run` checks the challenge against Let's Encrypt's staging service
(deploy hooks don't run in a dry run); `sudo RENEWED_LINEAGE=/etc/letsencrypt/live/game.example.org
/etc/conker/renew-hook.sh` runs the hook by hand.

## Updating

- Page: `deploy/build.sh`, then `rsync` (step 7). Open pages keep their client until reloaded.
- Asset pack: edit `../assets`, `deploy/pack.sh` (step 6), restart the server.
- Server: rebuild, `sudo install` the binary (step 5), `sudo systemctl restart conker-server`.

## Tested locally (no host, no domain)

`../lab/deploy-test/` (outside this tree) holds a test CA and a `localhost` certificate, never added to
any trust store, and the scripts that check this kit on one machine: the server with the certificate
(SEC1 and PKCS#8 keys), the token key file, `--public` and the endpoint on loopback; native clients over
WebTransport and `wss://` that check the certificate against the test CA (`conker-client --ca`, dev/test
only); clients refused for another CA, plain `ws://`, zero-key, foreign-key, tampered, misaddressed and
expired tokens; and this Caddyfile serving `deploy/build.sh`'s site and `pack.sh`'s pack with the test
certificate, with and without the password. What only a
real host can show: Chrome trusting the certificate (WebTransport without a digest, `wss://` from an https
page), Let's Encrypt issuing and renewing, the systemd units, and IPv4 into `[::]` on Linux.
