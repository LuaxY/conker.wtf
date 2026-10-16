# Deploying on a small Linux VPS

What runs on the host:

| Process | Listens on | Does |
|---|---|---|
| Caddy | TCP 80 (redirect), TCP 443 (+ UDP 443, HTTP/3) | serves `https://DOMAIN/`: the landing (`/`) and the game page (`/play.html`) from `deploy/build.sh`, and the asset pack at `/assets/` (brotli or gzip as the browser asks, cache headers), and proxies `/api/` to the web server |
| `conker-web` (systemd) | `127.0.0.1:8080` | the web server (`web/app/server.ts`, `web/README.md`, "Dev web server"): the landing's servers list, Create server and its unlinked `/#/admin` page's Kill; it starts a `conker-server` per match and stops one 5 min after its last human left (or its start, if none joined) |
| `conker-server` (one per match) | TCP + UDP N (5900-5949, even), and `127.0.0.1:N+1` | the match: WebTransport (UDP) and `wss://` (TCP) with the domain's certificate; connect tokens on loopback |
| certbot | TCP 80, only while renewing | obtains and renews the Let's Encrypt certificate, then `renew-hook.sh` hands it to Caddy and the game servers (they take it without a restart) |

How a player connects: the landing's Join game, then a server's lobby (or Create server), whose link opens the game
page, `/play.html?server=DOMAIN:N&token-url=/api/servers/ID/token&...`. The game page (over https) loads the client,
which asks `/api/servers/ID/info` which map the server plays, fetches that map and the rest of the asset pack from
`/assets/` and draws the level; then it fetches `/api/servers/ID/token`. Caddy passes both to the web server, which
passes them to that match's game server on loopback; the game server signs a netcode connect token with its private
key (`/etc/conker/token.key`, never sent anywhere) for the domain's addresses. The page then connects to `DOMAIN:N`
over WebTransport, or `wss://` with `?transport=ws`; the browser checks the certificate as for any site. The token is
valid for 60 s; the server refuses one that expired, was signed with another key, or names other addresses.

The 25 port pairs cap the matches at 25 at once; one full match takes about 15-20 % of a vCPU and 50 MB.

**The asset pack.** Nobody picks or uploads a ROM. The operator exports the asset pack once from
**their own ROM** on their machine (`conker-extract pack`: JSON, glTF, PNG and WAV files, about 40 MB) and
uploads it to the host (step 6). The game server reads it at start (the level's collision, spawns,
pickups, the bots' data), and Caddy serves the same folder at `https://DOMAIN/assets/`, where each
page's client fetches it. It is Rare's data, served to anyone who reaches the page: for a public
deployment turn on the Caddyfile's password (step 8). Neither the ROM nor the pack goes in the site
folder or in this source tree.

Files here: `build.sh` (the pages), `pack.sh` (the pack's upload), `Caddyfile`, `conker-web.service`,
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
sudo ufw allow 5900:5949/tcp    # the games: WebSocket (wss://)
sudo ufw allow 5900:5949/udp    # the games: WebTransport
sudo ufw enable
```

The token ports (N + 1) are open in that range but bound to loopback, as is the web server's 8080. If your provider has its own firewall
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

Build it on the VPS (or on another Linux machine of the same architecture) from this repository, checked out in
`/opt/conker/src` (the web server runs from there too), and install [Bun](https://bun.sh) as `/usr/local/bin/bun`:

```sh
cd /opt/conker/src
cargo build --release -p conker-server          # Bevy + lightyear: ~15-30 min the first time
sudo install -m 0755 target/release/conker-server /opt/conker/conker-server
```

Linking needs more memory than 2 GB: add swap first if the build is killed
(`sudo fallocate -l 4G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile`).

## 6. The asset pack

On your machine (`pack.sh` needs `brotli` and `rsync`):

```sh
CONKER_ROM=roms/game.z64 cargo run --release -p conker-extract -- pack
#   once: writes the pack to assets, gitignored (it refuses to overwrite one: your edits)
ssh vps sudo install -d -o $USER /srv/conker/assets
sh deploy/pack.sh vps:/srv/conker/assets      # a copy with .br and .gz files, rsynced with --delete
```

`pack.sh` compresses a copy in a temporary folder (outside this tree) for Caddy's `precompressed` (JSON,
glTF, `.bin` and the sound's WAVs; the background WAV goes as is, 4.2 MB brotli), and uploads it: about 400
files and their compressed copies. The pack is yours to edit
(`assets/README.md`): edit it on your machine, run `pack.sh` again, then restart the game server
(`sudo systemctl restart conker-server`; it reads the pack at start) and reload the pages. Never edit it
on the host: Caddy serves a file's `.br` or `.gz` copy as long as it exists, even when the file is newer.
The server and every page must read the same pack: the clients predict on the same collision, in the
same order.

## 7. The page

On your machine (the build needs the wasm toolchain of `web/README.md`, `wasm-opt`, `brotli` and `bun`, which builds
the landing from `web/app`):

```sh
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

Install the web server's unit and edit its `--public` addresses to your A and AAAA records (one
`--public` per address; leave out the IPv6 one if the host has none):

```sh
sudo cp deploy/conker-web.service /etc/systemd/system/
sudo systemctl edit --full conker-web               # set the two --public addresses
sudo systemctl enable conker-web                    # started by the hook below
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

The browser asks once and sends it with every request to the site: the page, the pack and `/api/` (the
servers, Create server, Kill and the tokens). The game ports need no password: nobody joins a match without a
token. Without the password anyone can start and kill matches (up to the 25).

Then the certificate. certbot answers Let's Encrypt's challenge itself on port 80 (`--standalone`), so
Caddy stops for those few seconds (`--pre-hook`, `--post-hook`); the deploy hook copies the certificate
to `/etc/conker/tls`, restarts Caddy and starts the web server (the first time; after that it is running
already). certbot saves the hooks and renews on its own (its systemd timer, twice a day; a renewal happens 30
days before expiry):

```sh
sudo certbot certonly --standalone -d game.example.org -m you@example.org --agree-tos \
  --pre-hook "systemctl stop caddy" --post-hook "systemctl start caddy" \
  --deploy-hook /etc/conker/renew-hook.sh
```

The game servers are not restarted on a renewal: each looks at `/etc/conker/tls`'s two files every 10 s and gives
the new pair to new connections, so matches in progress go on (its log: `certificate reloaded from ...: digest
...`). While the hook is between the two files, it keeps the one in use (`certificate
not reloaded yet`) and tries again 10 s later.

## 9. Check

On the host:

```sh
systemctl status conker-web caddy
journalctl -u conker-web -n 20          # "game server 1 ...: pid ..., port 5900, /tmp/conker-server-..." per match
sysctl net.ipv6.bindv6only              # 0: the [::] sockets accept IPv4 too (Linux's default)
ss -tuln | grep ':59'                   # once a match runs: tcp and udp *:5900 (dual-stack; [::]:5900 would
                                        # mean IPv6 only), tcp 127.0.0.1:5901
```

A game server's log is in the web server's private `/tmp`: `ls /tmp/systemd-private-*-conker-web.service-*/tmp/`.

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
curl -s -X POST -H 'content-type: application/json' https://game.example.org/api/servers -d '{"name":"test",
  "map":"2d","mode":"deathmatch","settings":{"bots":3,"skill":"normal","time":5,"lives":3,"kills":0,"score":20,
  "radar":true,"laps":5,"route":11,"cheats":[]}}'                                  # 201 and the record: id, port
curl -s 'https://game.example.org/api/servers/ID/token?id=1' | wc -c              # 4096 (hex)
#   Each address may take 10 tokens per 10 s, then 429 (behind Caddy: the X-Forwarded-For address)
#   With the password on: 401 without it; add -u player:PASSWORD to each curl
nc -4 -vz game.example.org 5900 && nc -6 -vz game.example.org 5900                # TCP over IPv4 and IPv6
curl -s -X DELETE https://game.example.org/api/servers/ID                          # 204: the match ends
```

Then open `https://game.example.org/` in Chrome: Join game, Create server, then the lobby's link opens the game
page, which loads the pack and starts by itself; the line at the bottom left reads
`connected to game.example.org:5900 over WebTransport`. The same lobby with WebSocket picked, in a second window:
`... over WebSocket` (that is `wss://`). For UDP over IPv4 (WebTransport into the `[::]`
socket), load the page once from an IPv4-only network (many mobile hotspots).

Renewal: `sudo certbot renew --dry-run` checks the challenge against Let's Encrypt's staging service
(deploy hooks don't run in a dry run); `sudo RENEWED_LINEAGE=/etc/letsencrypt/live/game.example.org
/etc/conker/renew-hook.sh` runs the hook by hand: within 10 s each game server's log has `certificate
reloaded`, and the players in a match stay in.

## Updating

- Page: `deploy/build.sh`, then `rsync` (step 7). Open pages keep their client until reloaded.
- Asset pack: edit `assets`, `deploy/pack.sh` (step 6), restart the server.
- Server: `git pull` in `/opt/conker/src`, rebuild, `sudo install` the binary (step 5), `sudo systemctl restart
  conker-web` (it runs `server.ts` from the checkout). A stop or restart (SIGTERM) stops every game server, which
  first sends its players the disconnect, so pages show it at once; matches in progress end.

## Tested locally (no host, no domain)

`lab/deploy-test/` (outside this tree) holds a test CA and a `localhost` certificate, never added to
any trust store, and the scripts that check this kit on one machine: the server with the certificate
(SEC1 and PKCS#8 keys), the token key file, `--public` and the endpoint on loopback; native clients over
WebTransport and `wss://` that check the certificate against the test CA (`conker-client --ca`, dev/test
only); clients refused for another CA, plain `ws://`, zero-key, foreign-key, tampered, misaddressed and
expired tokens; a renewal while a client plays (`test-reload.sh`: half-written, then whole; the new CA's
clients connect, the old one's are refused, the player stays); and this Caddyfile serving `deploy/build.sh`'s
site and `pack.sh`'s pack with the test certificate, with and without the password. What only a
real host can show: Chrome trusting the certificate (WebTransport without a digest, `wss://` from an https
page), Let's Encrypt issuing and renewing, the systemd units, and IPv4 into `[::]` on Linux.
