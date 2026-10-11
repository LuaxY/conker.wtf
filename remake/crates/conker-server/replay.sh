#!/bin/zsh
# Replays an oracle input script through the networked stack (server + one headless client whose
# conker-camera supplies the yaw) and compares the server's player state, tick by tick, with the
# original game's trace.
# Usage: [PORT=5888] crates/conker-server/replay.sh <lab/oracle/scripts/NAME.txt> <lab/oracle/trajectories/NAME_a.csv> [seconds, default 30: the client's whole run, startup included]
# Needs the asset pack (CONKER_ASSETS, default ../assets). The server spawns player 1 where the original's stood (`--spawn`: record 1 plus a
# random jitter not reproduced); the offset printed is what remains between the two spawns.
# Script read r is client tick `since` + (r - 23); the server's state after tick T is trace row T - since + 24.
# Read 24 is the original's spawn frame (state 0, y_vel 0, gravity 6 for that one frame): one known state miss.
set -u
cd "${0:A:h}/../.."
[[ $# -ge 2 ]] || { echo "usage: $0 script.txt trace.csv [seconds]"; exit 1 }
[[ -f ${CONKER_ASSETS:-../assets}/pack.json ]] || { echo "no asset pack in ${CONKER_ASSETS:-../assets}: cargo run --release -p conker-extract -- pack"; exit 1 }
script=${1:A} trace=${2:A} seconds=${3:-30}
cargo build -q -p conker-server -p conker-client 2> /dev/null || exit 1
bin=${CARGO_TARGET_DIR:-target}/debug
logs=$(mktemp -d)
export NO_COLOR=1 RUST_LOG=info,conker_server=debug,conker_client=debug
port=${PORT:-5888}

$bin/conker-server --port $port --spawn -2267,-521,6016 --seconds $((seconds + 6)) --digest-file $logs/digest.txt > $logs/server.log 2>&1 &
for _ in {1..300}; do [[ -s $logs/digest.txt ]] && break; sleep 0.1; done
$bin/conker-client --headless --script $script --id 1 --transport ws \
  --server 127.0.0.1:$port --seconds $seconds > $logs/client.log 2>&1
wait

since=$(grep -oE 'start at tick [0-9]+' $logs/client.log | grep -oE '[0-9]+$')
awk '/STATE t=.* p=Netcode\(1\)/ {
    for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
    split(f["pos"], p, ","); print f["t"], p[1], p[2], p[3], f["st"]
  }' $logs/server.log > $logs/server_states.txt
awk -F, -v since=$since -v states=$logs/server_states.txt '
  BEGIN { while ((getline line < states) > 0) { split(line, v, " "); sx[v[1]] = v[2]; sy[v[1]] = v[3]; sz[v[1]] = v[4]; ss[v[1]] = v[5] } }
  NR == 1 { for (i = 1; i <= NF; i++) c[$i] = i; next }
  $c["player"] != "0" || $c["mp"] != "1" || $c["x"] == "" { next }
  {
    r = $c["read"]; t = r - 24 + since
    if (r == 24) { ox = sx[t] - $c["x"]; oy = sy[t] - $c["y"]; oz = sz[t] - $c["z"] }
    if (r < 24 || !(t in sx)) next
    d = sqrt((sx[t] - ox - $c["x"])^2 + (sy[t] - oy - $c["y"])^2 + (sz[t] - oz - $c["z"])^2)
    n++; sum += d; if (d > max) { max = d; at = r }
    if (ss[t] != $c["state"]) { bad++; if (!first) first = r }
  }
  END {
    printf "spawn offset (%g, %g, %g); %d rows compared (reads 24-%d): max error %.4f (read %d), mean %.4f, %d state misses%s\n",
      ox, oy, oz, n, r, max, at, (n ? sum / n : 0), bad, (first ? " (first at read " first ")" : "")
  }' $trace
echo "logs: $logs"
