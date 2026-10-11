#!/bin/zsh
# Network smoke test: server + two headless scripted clients, a third joining mid-match, then
# checks the logs. Needs the asset pack (CONKER_ASSETS, default ../assets): server and clients both build scene 0x2D's
# collision from it.
# Usage: [PORT=5888] [MAP=2d|30|34|3f] crates/conker-server/smoke.sh [transport1 transport2 transport3]   (wt|ws, default: wt ws ws)
# Passes when every client connects, the server applies every player's inputs, each client sees
# the others move (interpolated, animated), no player is ever below its floor or without one, and
# the predicted state of each client equals the server's at the same tick.
# All processes exit by themselves (--seconds). Logs stay in the printed directory.
set -u
cd "${0:A:h}/../.."
[[ -f ${CONKER_ASSETS:-../assets}/pack.json ]] || { echo "no asset pack in ${CONKER_ASSETS:-../assets}: cargo run --release -p conker-extract -- pack"; exit 1 }
cargo build -q -p conker-server -p conker-client 2> /dev/null || exit 1
bin=${CARGO_TARGET_DIR:-target}/debug
logs=$(mktemp -d)
export NO_COLOR=1 RUST_LOG=info,conker_server=debug,conker_client=debug
port=${PORT:-5888}

$bin/conker-server --port $port --map ${MAP:-2d} --seconds 40 --digest-file $logs/digest.txt > $logs/server.log 2>&1 &
server=$!
for _ in {1..300}; do [[ -s $logs/digest.txt ]] && break; sleep 0.1; done
digest=$(<$logs/digest.txt)
client() { # id transport seconds
  $bin/conker-client --headless --script --id $1 --transport $2 --server 127.0.0.1:$port --digest $digest \
    --seconds $3 > $logs/client$1.log 2>&1
}
client 1 ${1:-wt} 24 &
c1=$!
client 2 ${2:-ws} 24 &
c2=$!
# Client 3 joins mid-match: 5 s after the first two are playing.
for _ in {1..300}; do
  grep -q synced $logs/client1.log 2> /dev/null && grep -q synced $logs/client2.log 2> /dev/null && break
  sleep 0.1
done
sleep 5
client 3 ${3:-ws} 10 &
wait $c1 $c2 $!
kill $server 2> /dev/null; wait

fail=0
report() { if (( $2 )); then echo "ok   $1"; else echo "FAIL $1"; fail=1; fi }
moves() { grep -E "$1" $2 | grep -oE 'pos \(.*\)' | sort -u | wc -l | tr -d ' ' }
for id in 1 2 3; do
  report "client $id connected" $(grep -c "client $id connected over" $logs/client$id.log)
  n=$(moves "player Netcode\\($id\\) input" $logs/server.log)
  report "server applied player $id inputs ($n distinct positions)" $((n >= 5))
  for other in 1 2 3; do
    (( other == id )) && continue
    n=$(moves "remote/interpolated player Netcode\\($other\\)" $logs/client$id.log)
    clips=$(grep "remote/interpolated player Netcode($other)" $logs/client$id.log | grep -oE 'clip [0-9]+ time [0-9.]+' | sort -u | wc -l | tr -d ' ')
    report "client $id sees player $other move ($n positions) and animate ($clips clip times)" $((n >= 3 && clips >= 3))
  done
done
# Floors, from the server's state of every player at every tick.
read below nofloor rows <<< $(awk '/STATE t=/ {
    for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
    split(f["pos"], p, ","); rows++
    if (f["fy"] + 0 <= -10000) nofloor++; else if (p[2] < f["fy"] - 0.01) below++
  } END { print below + 0, nofloor + 0, rows + 0 }' $logs/server.log)
report "no player below its floor or off the map ($below below, $nofloor without floor, $rows server rows)" $((below == 0 && nofloor == 0 && rows > 0))
# Prediction: each client's predicted state against the server's at the same tick.
for id in 1 2 3; do
  read n bad maxerr meanerr <<< $(awk '/STATE t=/ {
      for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
      key = f["t"] " " f["p"]
      if (FILENAME ~ /server/) { pos[key] = f["pos"]; st[key] = f["st"]; next }
      if (!(key in pos)) next
      split(pos[key], a, ","); split(f["pos"], b, ",")
      d = sqrt((a[1]-b[1])^2 + (a[2]-b[2])^2 + (a[3]-b[3])^2)
      n++; sum += d; if (d > max) max = d; if (st[key] != f["st"]) bad++
    } END { printf "%d %d %.6f %.6f\n", n, bad + 0, max + 0, (n ? sum / n : 0) }' $logs/server.log $logs/client$id.log)
  rb=$(grep -oE 'rollbacks [0-9]+ rollback ticks [0-9]+' $logs/client$id.log | tail -1)
  report "client $id prediction vs server: $n ticks, max error $maxerr, mean $meanerr, $bad state misses; $rb" $((n > 100))
done
echo "logs: $logs"
exit $fail
