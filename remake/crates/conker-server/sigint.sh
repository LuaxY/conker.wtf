#!/bin/zsh
# Leaving by signal: a server and four headless scripted clients; client 1 (WebTransport) gets
# SIGINT (Ctrl-C), client 2 (WebTransport) SIGTERM, then the server SIGTERM (systemd's stop) with
# clients 3 (WebTransport) and 4 (WebSocket) still in. WebTransport is the case that matters: a
# killed process sends nothing, so without the disconnect the other side waits for its 3 s timeout
# (a WebSocket's TCP close shows at once anyway).
# Passes when the server logs each client's leave, and each remaining client the server's, within
# 1 s of the signal, and every process exits within 2 s of it.
# Usage: [PORT=5974] crates/conker-server/sigint.sh   (also takes PORT+1 for tokens)
set -u
cd "${0:A:h}/../.."
[[ -f ${CONKER_ASSETS:-../assets}/pack.json ]] || { echo "no asset pack in ${CONKER_ASSETS:-../assets}"; exit 1 }
cargo build -q -p conker-server -p conker-client 2> /dev/null || exit 1
bin=${CARGO_TARGET_DIR:-target}/debug
logs=$(mktemp -d)
export NO_COLOR=1 RUST_LOG=info
port=${PORT:-5974}
zmodload zsh/datetime

$bin/conker-server --port $port --seconds 90 --digest-file $logs/digest.txt > $logs/server.log 2>&1 &
server=$!
for _ in {1..300}; do [[ -s $logs/digest.txt ]] && break; sleep 0.1; done
digest=$(<$logs/digest.txt)
pids=()
for id t in 1 wt 2 wt 3 wt 4 ws; do
  $bin/conker-client --headless --script --id $id --transport $t --server 127.0.0.1:$port \
    --digest $digest --seconds 80 > $logs/client$id.log 2>&1 &
  pids+=($!)
done
for id in 1 2 3 4; do
  for _ in {1..600}; do grep -q synced $logs/client$id.log 2> /dev/null && break; sleep 0.1; done
done
sleep 2

fail=0
report() { if (( $2 )); then echo "ok   $1"; else echo "FAIL $1"; fail=1; fi }
# Seconds from now until `pattern` shows in `file` (polled every 20 ms, up to 5 s), else 99.
until_logged() { # file pattern
  local start=$EPOCHREALTIME
  for _ in {1..250}; do
    grep -q "$2" $1 && { printf '%.2f' $((EPOCHREALTIME - start)); return }
    sleep 0.02
  done
  echo 99
}
# Seconds until `pid` exits (up to 5 s), else 99.
until_gone() { # pid
  local start=$EPOCHREALTIME
  for _ in {1..250}; do
    kill -0 $1 2> /dev/null || { printf '%.2f' $((EPOCHREALTIME - start)); return }
    sleep 0.02
  done
  echo 99
}
for id sig in 1 INT 2 TERM; do
  kill -$sig $pids[$id]
  dt=$(until_logged $logs/server.log "client Netcode($id) left")
  gone=$(until_gone $pids[$id])
  why=$(grep -m1 -oE "client Netcode\($id\) left: .*" $logs/server.log)
  report "client $id (WebTransport) on SIG$sig: the server saw it leave in $dt s ($why); it exited in $gone s" \
    $((dt < 1 && gone < 2))
done
kill -TERM $server
for id in 3 4; do
  dt=$(until_logged $logs/client$id.log "disconnected from")
  why=$(grep -m1 -oE "disconnected from [^ ]*: [^.]*" $logs/client$id.log)
  report "client $id saw the server stop on SIGTERM in $dt s ($why)" $((dt < 1))
done
gone=$(until_gone $server)
report "the server exited in $gone s" $((gone < 2))
kill $pids[3] $pids[4] 2> /dev/null
wait
echo "logs: $logs"
exit $fail
