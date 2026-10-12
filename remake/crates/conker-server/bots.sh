#!/bin/zsh
# Bots smoke test: the server with --bots (BOTS, default 2) and HUMANS headless clients (default 1;
# client 1 stands idle, client 2 runs the test pattern). Passes when every bot moves, fights (hits
# someone), dies and respawns, and every client sees every bot move. Needs the asset pack (CONKER_ASSETS, default ../assets).
# Usage: [PORT=5888] [MAP=2d|30|34|3f|2b] [MODE=deathmatch|total-war|colors|beach] [SCORE=1-40|unlimited]
#   [SKILL=inbred|crap|normal|bastard|einstein] crates/conker-server/bots.sh [seconds=150].
# All processes exit by themselves (--seconds). MODE=colors (MAP=30) also counts the flags' takes, drops and captures;
# MODE=beach (MAP=2b) the escapes, detonations, strikes and scores, and its villager bots (no attack) need only move.
set -u
cd "${0:A:h}/../.."
[[ -f ${CONKER_ASSETS:-../assets}/pack.json ]] || { echo "no asset pack in ${CONKER_ASSETS:-../assets}: cargo run --release -p conker-extract -- pack"; exit 1 }
cargo build -q -p conker-server -p conker-client 2> /dev/null || exit 1
bin=${CARGO_TARGET_DIR:-target}/debug
logs=$(mktemp -d)
export NO_COLOR=1 RUST_LOG=info
secs=${1:-150} bots=${BOTS:-2} humans=${HUMANS:-1} port=${PORT:-5888}

mode=(); [[ -n ${MODE:-} ]] && mode=(--mode $MODE)
[[ -n ${SCORE:-} ]] && mode+=(--score $SCORE)
[[ -n ${SKILL:-} ]] && mode+=(--skill $SKILL)
$bin/conker-server --port $port --map ${MAP:-2d} $mode --bots $bots --seconds $((secs + 20)) --digest-file $logs/digest.txt \
  > $logs/server.log 2>&1 &
server=$!
for _ in {1..300}; do [[ -s $logs/digest.txt ]] && break; sleep 0.1; done
digest=$(<$logs/digest.txt)
clients=()
for id in $(seq 1 $humans); do
  script=()
  (( id == 2 )) && script=(--script)
  $bin/conker-client --headless --id $id --transport ws --server 127.0.0.1:$port --digest $digest --seconds $secs \
    $script > $logs/client$id.log 2>&1 &
  clients+=($!)
done
wait $clients
kill $server 2> /dev/null; wait

fail=0
report() { if (( $2 )); then echo "ok   $1"; else echo "FAIL $1"; fail=1; fi }
moves() { grep -E "$1" $2 | grep -oE 'pos \([^)]*\)' | sort -u | wc -l | tr -d ' ' }
for id in $(seq 1 $humans); do
  report "client $id connected" $(grep -c "client $id connected over" $logs/client$id.log)
done
for b in $(seq 0 $((bots - 1))); do
  n=$(moves "player Local\\($b\\) input None" $logs/server.log)
  hits=$(grep -c "EVENT .*Hit { victim: [0-9]*, attacker: $b," $logs/server.log)
  deaths=$(grep -c "EVENT .*Hit { victim: $b, attacker: [0-9]*, damage: [0-9]*, lethal: true" $logs/server.log)
  respawns=$(grep -c "EVENT .*Respawn { player: $b," $logs/server.log)
  report "bot $b moves ($n distinct positions a second)" $((n >= 10))
  if [[ ${MODE:-} == beach ]]; then
    side=$(grep -oE "bot in slot $b on record .*" $logs/server.log | grep -oE '[A-Za-z]+$')
    if [[ $side == Squirrels ]]; then
      echo "     bot $b villager: dies ($deaths), respawns ($respawns)"
    else
      report "bot $b Tedi fights ($hits hits), dies ($deaths), respawns ($respawns)" $((hits > 0))
    fi
  else
    report "bot $b fights ($hits hits), dies ($deaths) and respawns ($respawns)" $((hits > 0 && deaths > 0 && respawns > 0))
  fi
  for id in $(seq 1 $humans); do
    n=$(moves "remote/interpolated player Local\\($b\\)" $logs/client$id.log)
    report "client $id sees bot $b move ($n positions)" $((n >= 3))
  done
done
echo "kills: $(grep -oE 'player [A-Za-z]+\([0-9]+\) input .* kills [0-9]+' $logs/server.log | awk '{k[$2]=$NF} END {for (p in k) printf "%s %s  ", p, k[p]}')"
echo "lift rides: $(grep -c 'takes the lift' $logs/server.log)"
if [[ ${MODE:-} == colors ]]; then
  caps=$(grep -c 'COLORS t=[0-9]* Score' $logs/server.log)
  echo "colors: takes $(grep -c 'COLORS t=[0-9]* flag [01] taken' $logs/server.log)," \
    "drops $(grep -c 'COLORS t=[0-9]* LetGo' $logs/server.log), captures $caps" \
    "($(( caps * 60.0 / secs )) a minute) at t=$(grep -oE 'COLORS t=[0-9]+ Score' $logs/server.log | grep -oE '[0-9]+' | tr '\n' ' ')"
fi
if [[ ${MODE:-} == beach ]]; then
  at() { grep -oE "BEACH t=[0-9]+ $1" $logs/server.log | grep -oE 't=[0-9]+' | tr '\n' ' ' }
  echo "beach: escapes at $(at 'Respawn'); detonations at $(at 'Plunger\(true')"
  echo "  strikes at $(at 'Strike'); villager points at $(at 'villager down')"
  echo "  zone gives: $(grep -oE 'BEACH t=[0-9]+ Give\([0-9]+, (22|36)\)' $logs/server.log | wc -l | tr -d ' '), ejections: $(grep -c 'BEACH t=[0-9]* Place' $logs/server.log)," \
    "gun mounts: $(grep -oE 'GUN t=[0-9]+ gun 0 state 0x1 -> 0x2 slot Some\([0-9]+\)' $logs/server.log | grep -oE 'Some\([0-9]+' | tr -d 'Some(' | tr '\n' ' ')"
  echo "  $(grep -E 'match ends|villager down' $logs/server.log | tail -1)"
fi
report "no bot script faults or unknown natives" $(( ! $(grep -c 'bots: .* faults' $logs/server.log) ))
report "no panics" $(( ! $(cat $logs/*.log | grep -c 'panicked') ))
echo "logs: $logs"
exit $fail
