#!/bin/zsh
# Bots smoke test: the server with --bots (BOTS, default 2) and HUMANS headless clients (default 1;
# client 1 stands idle, client 2 runs the test pattern). Passes when every bot moves, fights (hits
# someone), dies and respawns, and every client sees every bot move. Needs the asset pack (CONKER_ASSETS, default assets).
# Usage: [PORT=5888] [MAP=2d|30|34|3f|2b|1a|24|33] [MODE=deathmatch|total-war|colors|beach|heist|tank|raptor|race] [SCORE=1-40|unlimited]
#   [SKILL=inbred|crap|normal|bastard|einstein] [TIME=3-10|unlimited] [SEED=n] [SIDE=squirrels|tediz|cavemen] [ROUTE=11|12] [LAPS=1-20]
#   [CHARACTER=0-21] [CHEATS=code,...|all] [TEAM=0-3] crates/conker-server/bots.sh [seconds=150]. SIDE, CHARACTER and TEAM: what
#   client 1 asks for. The bots are the slots the server's last ROSTER line before the clients left names (humans sit first).
# All processes exit by themselves (--seconds). MODE=colors (MAP=30) also counts the flags' takes, drops and captures;
# MODE=beach (MAP=2b) the escapes, detonations, strikes, scores and swims, and its villager bots (no attack) need only move;
# MODE=heist (MAP=1a) the bags made, the takes, drops, deliveries, voices and the bags per team;
# MODE=tank (MAP=24) the bombs taken and planted, the deaths by blast and by shell, the power-ups taken.
# MODE=raptor (MAP=3f) the eggs made, thrown and cooked, the nest drops, the raptor bots and the team scores.
# MODE=race (MAP=33): a bot need only move; a lap done or a power-up taken, and the race decided; the laps' frames, finishes, deaths and the end.
set -u
cd "${0:A:h}/../.."
[[ -f ${CONKER_ASSETS:-assets}/pack.json ]] || { echo "no asset pack in ${CONKER_ASSETS:-assets}: cargo run --release -p conker-extract -- pack"; exit 1 }
cargo build -q -p conker-server -p conker-client 2> /dev/null || exit 1
bin=${CARGO_TARGET_DIR:-target}/debug
logs=$(mktemp -d)
export NO_COLOR=1 RUST_LOG=info
secs=${1:-150} bots=${BOTS:-2} humans=${HUMANS:-1} port=${PORT:-5888}

mode=(); [[ -n ${MODE:-} ]] && mode=(--mode $MODE)
[[ -n ${SCORE:-} ]] && mode+=(--score $SCORE)
[[ -n ${SKILL:-} ]] && mode+=(--skill $SKILL)
[[ -n ${TIME:-} ]] && mode+=(--time $TIME)
[[ -n ${SEED:-} ]] && mode+=(--seed $SEED)
[[ -n ${ROUTE:-} ]] && mode+=(--route $ROUTE)
[[ -n ${LAPS:-} ]] && mode+=(--laps $LAPS)
[[ -n ${CHEATS:-} ]] && mode+=(--cheats $CHEATS)
$bin/conker-server --port $port --map ${MAP:-2d} $mode --bots $bots --seconds $((secs + 20)) --digest-file $logs/digest.txt \
  > $logs/server.log 2>&1 &
server=$!
for _ in {1..300}; do [[ -s $logs/digest.txt ]] && break; sleep 0.1; done
digest=$(<$logs/digest.txt)
clients=()
for id in $(seq 1 $humans); do
  script=()
  (( id == 2 )) && script=(--script)
  (( id == 1 )) && [[ -n ${SIDE:-} ]] && script+=(--side $SIDE)
  (( id == 1 )) && [[ -n ${CHARACTER:-} ]] && script+=(--character $CHARACTER)
  (( id == 1 )) && [[ -n ${TEAM:-} ]] && script+=(--team $TEAM)
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
grep -oE 'client Netcode\([0-9]+\) connected via .* as [A-Za-z]+ .*class 0x[0-9a-f]+ \(asked [^)]*\)' $logs/server.log | sed -E 's/ via .* as / as /'
grep -oE 'slot [0-9]+: pick .*' $logs/server.log
roster=$(grep -E 'ROSTER (start|join|switch|balance|match start)' $logs/server.log | tail -1 | grep -oE 'ROSTER.*')
echo $roster
botslots=($(echo $roster | grep -oE 'bots \[[0-9, ]*\]' | grep -oE '[0-9]+'))
teams=($(echo $roster | grep -oE 'teams \[[0-9, ]*\]' | grep -oE '[0-9]+'))
for b in $botslots; do
  n=$(moves "player Local\\($b\\) input None" $logs/server.log)
  hits=$(grep -c "EVENT .*Hit { victim: [0-9]*, attacker: $b," $logs/server.log)
  deaths=$(grep -c "EVENT .*Hit { victim: $b, attacker: [0-9]*, damage: [0-9]*, lethal: true" $logs/server.log)
  respawns=$(grep -c "EVENT .*Respawn { player: $b," $logs/server.log)
  report "bot $b moves ($n distinct positions a second)" $((n >= 10))
  if [[ ${MODE:-} == race ]]; then
    laps=$(grep -c "RACE t=[0-9]* slot $b lap" $logs/server.log)
    # Not a check: with an idle human of 3 lives the race ends at its elimination (oracle b1,
    # read 933), before most bots' first lap.
    echo "     bot $b: $laps laps, $deaths deaths, $(grep -oE "RACE t=[0-9]+ slot $b finishes [0-9]+" $logs/server.log | grep -oE '[0-9]+$' || echo no) finish"
  elif [[ ${MODE:-} == beach ]]; then
    if (( teams[b + 1] == 0 )); then
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
  echo "  swims (the bots keep out of the sea, V): $(grep -oE 'slot [0-9]+ swims' $logs/server.log | sort | uniq -c | tr '\n' ' ')"
fi
if [[ ${MODE:-} == heist ]]; then
  at() { grep -oE "HEIST t=[0-9]+ $1" $logs/server.log | grep -oE 't=[0-9]+' | tr '\n' ' ' }
  bags=$(grep -c 'HEIST t=[0-9]* Score' $logs/server.log)
  echo "heist: bags made at $(at 'Cash\(true')"
  echo "  takes $(grep -c 'HEIST t=[0-9]* bag taken' $logs/server.log), let-gos $(grep -c 'HEIST t=[0-9]* LetGo' $logs/server.log)" \
    "(deliveries included), voices $(grep -c 'HEIST t=[0-9]* Voice' $logs/server.log)"
  echo "  deliveries $bags: $(grep -oE 'HEIST t=[0-9]+ Score\([0-9]+' $logs/server.log | sed 's/HEIST //; s/Score(/team /' | tr '\n' ' ')"
  echo "  $(grep -E 'match ends' $logs/server.log | tail -1)"
fi
if [[ ${MODE:-} == tank ]]; then
  blasts=$(grep -c 'TANK t=[0-9]* blast kills' $logs/server.log)
  deaths=$(grep -c 'EVENT .*Hit { victim: [0-9]*, attacker: [0-9]*, damage: [0-9]*, lethal: true' $logs/server.log)
  echo "tank: bombs taken $(grep -c 'TANK t=[0-9]* bomb taken' $logs/server.log), planted $(grep -c 'TANK t=[0-9]* bomb planted' $logs/server.log)" \
    "at $(grep -oE 'TANK t=[0-9]+ bomb planted' $logs/server.log | grep -oE '[0-9]+' | tr '\n' ' ')"
  echo "  blast deaths $blasts, shell deaths $((deaths - blasts)), power-ups taken $(grep -c 'TANK t=[0-9]* slot [0-9]* takes power-up' $logs/server.log)"
  echo "  $(grep -E 'match ends' $logs/server.log | tail -1)"
fi
if [[ ${MODE:-} == raptor ]]; then
  echo "raptor: eggs made $(grep -c 'RAPTOR t=[0-9]* Egg([0-9], true)' $logs/server.log)," \
    "throws $(grep -c 'RAPTOR t=[0-9]* Throw' $logs/server.log), cooked $(grep -c 'RAPTOR t=[0-9]* Cooked' $logs/server.log)," \
    "nest drops $(grep -c 'RAPTOR t=[0-9]* Nest' $logs/server.log)," \
    "raptors (slots) $(grep -oE 'bot in slot [0-9]+ on record .*Tediz' $logs/server.log | grep -oE 'slot [0-9]+' | tr '\n' ' ')"
  echo "  scores: $(grep -oE 'RAPTOR t=[0-9]+ scores \[[0-9]+, [0-9]+\]' $logs/server.log | sed 's/RAPTOR //' | tail -4 | tr '\n' ' ')"
  echo "  $(grep -E 'match ends' $logs/server.log | tail -1)"
fi
if [[ ${MODE:-} == race ]]; then
  echo "race: laps (t, then the frames each took) per slot:"
  for slot in $(grep -oE 'RACE t=[0-9]+ slot [0-9]+ lap' $logs/server.log | grep -oE 'slot [0-9]+' | sort -u | grep -oE '[0-9]+' | sort -n); do
    ts=($(grep -oE "RACE t=[0-9]+ slot $slot lap" $logs/server.log | grep -oE 't=[0-9]+' | tr -d 't='))
    gaps=(); for k in $(seq 2 ${#ts}); do gaps+=($((ts[k] - ts[k - 1]))); done
    echo "  slot $slot: ${ts[*]} ($gaps)"
  done
  echo "  finishes: $(grep -oE 'RACE t=[0-9]+ slot [0-9]+ finishes [0-9]+' $logs/server.log | sed 's/RACE //' | tr '\n' ' ')"
  echo "  outs: $(grep -oE 'RACE t=[0-9]+ slot [0-9]+ out' $logs/server.log | sed 's/RACE //' | tr '\n' ' ')"
  echo "  boosts $(grep -c 'RACE t=[0-9]* slot [0-9]* boosts' $logs/server.log), missiles $(grep -c 'RACE t=[0-9]* slot [0-9]* fires' $logs/server.log)," \
    "knocks $(grep -c 'RACE t=[0-9]* slot [0-9]* knocked' $logs/server.log), deaths $(grep -c 'RACE t=[0-9]* slot [0-9]* dies' $logs/server.log)"
  echo "  $(grep -E 'RACE t=[0-9]+ the race is decided|RACE t=[0-9]+ results' $logs/server.log | tr '\n' ' ')"
  echo "  $(grep -E 'match ends' $logs/server.log | tail -1)"
  # Route 12's first lap outlasts the idle human's 3 lives (oracle L7f: 1088+): power-ups count too.
  report "the bots race (laps done or power-ups taken)" \
    $(grep -cE 'RACE t=[0-9]+ slot [0-9]+ (lap|takes)' $logs/server.log)
  report "the race is decided" $(grep -c 'RACE t=[0-9]* the race is decided' $logs/server.log)
fi
report "no bot script faults or unknown natives" $(( ! $(grep -c 'bots: .* faults' $logs/server.log) ))
report "no panics" $(( ! $(cat $logs/*.log | grep -c 'panicked') ))
echo "logs: $logs"
exit $fail
