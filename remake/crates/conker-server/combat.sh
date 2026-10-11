#!/bin/zsh
# Replays an oracle combat encounter through the networked stack: the server (`--wait 2`, both
# players placed where the original's stood at read 745, the weapon handed over at 745 as the
# oracle's lldb poke did) and two headless clients fed the encounter's two scripts, then compares
# the server's hits, deaths, respawns, animations, weapons and kills with the original's events
# (`lab/oracle/trajectories/<name>_events.csv`), each client's prediction with the server (ticks off
# by more than 0.001), and, per shooter, the targets its bullets were tested against with where its
# own client showed them when it took the input (the server's lag compensation).
# Usage: crates/conker-server/combat.sh [name, default combat_e_w09] [weapon id hex, default 09; - for none]
#        [ms added to player 1's packets, both ways (`lag=`)] [ms for player 2]
# Server log row read R (the state after frame R) is the trace's row R + 1. Needs the asset pack (CONKER_ASSETS, default ../assets).
# Optional environment: P1=script P2=script (other scripts than the encounter's), SERVER_FLAGS
# (e.g. --no-lagcomp), THIRD=1 (a third client joins 4 s into the match), PORT (default 5888).
set -u
cd "${0:A:h}/../.."
[[ -f ${CONKER_ASSETS:-../assets}/pack.json ]] || { echo "no asset pack in ${CONKER_ASSETS:-../assets}: cargo run --release -p conker-extract -- pack"; exit 1 }
name=${1:-combat_e_w09} weapon=${2:-09} lag1=${3:-} lag2=${4:-}
oracle=${0:A:h}/../../../lab/oracle
trace=$oracle/trajectories/$name.csv events=$oracle/trajectories/${name}_events.csv
cargo build -q -p conker-server -p conker-client 2> /dev/null || exit 1
bin=${CARGO_TARGET_DIR:-target}/debug
logs=$(mktemp -d)
export NO_COLOR=1 RUST_LOG=info,conker_server=debug,conker_client=debug
port=${PORT:-5888}

at745() { awk -F, -v p=$1 '$1 == 745 && $15 == p { print $16 "," $17 "," $18 "," $24 }' $trace }
give=(); [[ $weapon != - ]] && give=(--give 0:745:$weapon)
$bin/conker-server --port $port --wait 2 --first-read 745 --spawn $(at745 0) --spawn $(at745 1) \
  $give ${=SERVER_FLAGS:-} --seconds 50 --digest-file $logs/digest.txt > $logs/server.log 2>&1 &
for _ in {1..300}; do [[ -s $logs/digest.txt ]] && break; sleep 0.1; done
client() { # id script lag (both Tediz, as every player of the original's Deathmatch)
  $bin/conker-client --headless --script $2 --id $1 --transport ws --server 127.0.0.1:$port --seconds 44 \
    --side tediz ${3:+--lag} ${3:-} > $logs/client$1.log 2>&1
}
client 1 ${P1:-$oracle/scripts/$name.txt} "$lag1" &
for _ in {1..300}; do grep -q "slot 0" $logs/server.log 2> /dev/null && break; sleep 0.1; done
client 2 ${P2:-$oracle/scripts/${name}_p2.txt} "$lag2" &
if [[ -n ${THIRD:-} ]]; then
  for _ in {1..600}; do grep -q "match started" $logs/server.log 2> /dev/null && break; sleep 0.1; done
  sleep 4
  $bin/conker-client --headless --script --id 3 --transport wt --server 127.0.0.1:$port \
    --digest $(<$logs/digest.txt) --seconds 20 > $logs/client3.log 2>&1 &
fi
wait

# Events per (row, player): hp, anim, weapon, kills from the server's COMBAT lines (changes), the
# respawn records from its EVENT lines; the oracle's from its table.
awk '/COMBAT t=.* read=[0-9]/ {
    for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
    k = f["slot"]; row = f["read"] + 1
    if (!(k in hp)) { hp[k] = f["hp"]; anim[k] = f["anim"]; w[k] = "0x00"; kills[k] = f["kills"] }
    {
      # The respawn restores health inside the oracle respawn event: not an hp event.
      if (f["hp"] < hp[k]) print row, k, "hp", f["hp"]
      if (f["anim"] != anim[k]) print row, k, "anim", f["anim"]
      if (f["w"] != w[k] && f["w"] != "0x00") print row, k, "weapon", f["w"]
      if (f["kills"] != kills[k]) print row, k, "kills", f["kills"]
    }
    hp[k] = f["hp"]; anim[k] = f["anim"]; w[k] = f["w"]; kills[k] = f["kills"]; last = row
  }
  /EVENT .*read=[0-9].*Respawn/ {
    match($0, /read=[0-9]+/); r = substr($0, RSTART + 5, RLENGTH - 5) + 1
    match($0, /player: [0-9]+/); p = substr($0, RSTART + 8, RLENGTH - 8)
    match($0, /index: [0-9]+/); print r, p, "respawn", substr($0, RSTART + 7, RLENGTH - 7)
  }
  END { print last, "-", "last", "-" > "/dev/stderr" }' $logs/server.log > $logs/server_events.txt 2> $logs/last.txt
# Up to the server's last row and the trace's.
last=$(awk '{ print $1 }' $logs/last.txt)
tlast=$(awk -F, 'NR > 1 && $1 > m { m = $1 } END { print m }' $trace)
(( tlast < last )) && last=$tlast
awk -v last=$last '$1 <= last' $logs/server_events.txt > $logs/x && mv $logs/x $logs/server_events.txt
kills=$(grep -h "write 0x800E0B3[68]:2" $oracle/trajectories/${name}_lldb*.txt 2> /dev/null |
  awk '{ p = ($4 == "0x800E0B38:2"); print $1 + 1, p, "kills", $7 }')
awk -F, -v last=$last '{ sub(/\r$/, "") } NR > 1 && $1 > 745 && $1 <= last {
    split($4, v, "->"); split(v[2], n, " ")
    if ($3 == "hp" || $3 == "death") print $1, $2, "hp", n[1]
    else if ($3 == "state") print $1, $2, "anim", n[1]
    else if ($3 == "weapon") { match($4, /id 0x[0-9a-f]+/); print $1, $2, "weapon", substr($4, RSTART + 3, RLENGTH - 3) }
    else if ($3 == "respawn") { match($0, /record [0-9]+/); print $1, $2, "respawn", substr($0, RSTART + 7, RLENGTH - 7) }
  }' $events > $logs/oracle_events.txt
print -r -- "$kills" | awk -v last=$last 'NF && $1 <= last' >> $logs/oracle_events.txt
sort -u $logs/oracle_events.txt -o $logs/oracle_events.txt
sort -u $logs/server_events.txt -o $logs/server_events.txt
matched=$(comm -12 $logs/oracle_events.txt $logs/server_events.txt | wc -l | tr -d ' ')
echo "$name through the server, rows 746-$last: $(wc -l < $logs/oracle_events.txt | tr -d ' ') oracle events, $matched matched"
echo "only in the oracle:"; comm -23 $logs/oracle_events.txt $logs/server_events.txt | sed 's/^/  /'
echo "only on the server:"; comm -13 $logs/oracle_events.txt $logs/server_events.txt | sed 's/^/  /'
# Clock: the server's timer at read R against the trace's at row R + 1.
awk -F, 'NR == FNR { if ($0 ~ /COMBAT t=.* read=[0-9].*slot=0/) { match($0, /read=[0-9]+/); r = substr($0, RSTART + 5, RLENGTH - 5) + 1
    match($0, /timer=[0-9]+/); t[r] = substr($0, RSTART + 6, RLENGTH - 6) } next }
  FNR > 1 && $15 == 0 && ($1 in t) { n++; if (t[$1] != $8) bad++ }
  END { printf "clock: %d rows compared, %d differ\n", n, bad + 0 }' $logs/server.log $trace
for id in 1 2; do
  read n bad maxerr off <<< $(awk '/STATE t=/ {
      for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
      key = f["t"] " " f["p"]
      if (FILENAME ~ /server/) { pos[key] = f["pos"]; st[key] = f["st"]; next }
      if (!(key in pos)) next
      split(pos[key], a, ","); split(f["pos"], b, ",")
      d = sqrt((a[1]-b[1])^2 + (a[2]-b[2])^2 + (a[3]-b[3])^2)
      n++; if (d > max) max = d; if (st[key] != f["st"]) bad++; if (d > 0.001) off++
    } END { printf "%d %d %.6f %d\n", n, bad + 0, max + 0, off + 0 }' $logs/server.log $logs/client$id.log)
  rb=$(grep -oE 'rollbacks [0-9]+ rollback ticks [0-9]+' $logs/client$id.log | tail -1)
  echo "client $id prediction vs server: $n ticks, $off over 0.001, max error $maxerr, $bad state misses; $rb"
done
grep -h "HUD" $logs/client2.log | awk '{ $1 = $2 = $3 = ""; print }' | uniq | grep -E "DEAD|kills [1-9]|HP [0-5]/" | head -3
# Lag compensation, per shooter: the bullets tested (each `LAGCOMP` line is one bullet in one
# tick), how many views came from the client's own report (Saw) rather than lightyear's estimate,
# those whose hit the rewind changed, and how far the rewound target stood from where the shooter's
# own client showed it when it took that tick's input (its `SEEN` line).
for k in 0 1; do
  awk -v k=$k 'NR == FNR { if ($0 ~ /SEEN t=/ && $0 ~ ("p=Netcode\\(" (2 - k) "\\)")) { for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] } seen[f["t"]] = f["pos"] } next }
    $0 ~ ("LAGCOMP t=.*shooter=" k " ") {
      for (i = 1; i <= NF; i++) { split($i, kv, "="); f[kv[1]] = kv[2] }
      n++; if (f["differs"] == "true") d++; if (f["saw"] == "true") told++
      if (f["rewound"] ~ /Some/) hits++
      split(f["seen"], s, ";"); o = 2 - k
      if (s[o] != "-" && (f["t"] in seen)) {
        split(s[o], a, ","); split(seen[f["t"]], b, ",")
        e = sqrt((a[1]-b[1])^2 + (a[2]-b[2])^2 + (a[3]-b[3])^2); m++; sum += e; if (e > max) max = e
      }
    }
    END { printf "lag compensation, shooter %d: %d bullet tests (%d with the client'"'"'s Saw), %d on the rewound target, %d changed by the rewind; rewound target vs the shooter'"'"'s screen: %d tests, mean %.6f, max %.6f\n", k, n, told + 0, hits + 0, d + 0, m, (m ? sum / m : 0), max + 0 }' \
    $logs/client$((k + 1)).log $logs/server.log
done
if [[ -n ${THIRD:-} ]]; then
  grep -E "slot 2" $logs/server.log | sed 's/.*conker_server: //'
  grep -h "HUD" $logs/client3.log | tail -1 | sed 's/.*HUD/client 3 HUD/'
fi
echo "logs: $logs"
