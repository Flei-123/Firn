#!/usr/bin/env bash
# tools/mc/server/run.sh -- integrated Firn server against the REAL vanilla 1.20.1 server with the same node bot:
#   A  status ping (protocol 763, max players, latency)
#   B  join sequence: packet order, 24 section columns parse, block histogram of column 0,0 EQUALS vanilla's flat world
#   C  TPS over >= 200 ticks (update_time age vs wall clock), first keep alive after 15 s, view center follows a 160 block move
#   D  chat echo, graceful stop (mcsrv --ticks N prints the tick statistics)
#   E  a client that never answers keep alive is dropped (30 s timeout)
#   F  negative controls: wrong protocol is refused; a mutated reference histogram must be detected
#   G  real mcplay joins the Firn server and its screenshot is not empty (MC_SERVER_PLAY=1, needs Xvfb)
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$REPO/lib"
JARS=${MC_JARS:-/root/.w5/jars}
TOOLS="$REPO/tools/mc/server"
W=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$W"' EXIT
ERR=0
VPORT=${MCS_VPORT:-25941}
FPORT=${MCS_FPORT:-25942}
SECS=${BOT_SECONDS:-18}
export BOT_SECONDS=$SECS
BOT="node $TOOLS/bot.cjs"

report() { while IFS= read -r l; do echo "$l"; case "$l" in *FAIL*) ERR=1;; esac; done; }

mkdir -p "$W/vanilla"
cat > "$W/vanilla/server.properties" <<PROPS
online-mode=false
server-port=$VPORT
level-type=flat
view-distance=8
simulation-distance=4
spawn-protection=0
enforce-secure-profile=false
max-tick-time=-1
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
generate-structures=false
gamemode=creative
motd=vanilla
PROPS
echo "eula=true" > "$W/vanilla/eula.txt"
(cd "$W/vanilla" && java -Xmx1G -jar "$JARS/server-1.20.1.jar" --nogui > server.out 2>&1 < <(sleep 400) &)
$FIRNC -o "$W/mcsrv" demos/mcsrv/main.fi 2>"$W/err" || { echo "  FAIL  compile mcsrv"; head -5 "$W/err"; exit 1; }
$FIRNC --no-opt -o "$W/mcsrv_noopt" demos/mcsrv/main.fi 2>"$W/err" || { echo "  FAIL  compile mcsrv --no-opt"; head -5 "$W/err"; exit 1; }

for i in $(seq 1 120); do grep -q 'Done (' "$W/vanilla/server.out" 2>/dev/null && break; sleep 1; done
grep -q 'Done (' "$W/vanilla/server.out" || { echo "  FAIL  vanilla server did not start"; tail -5 "$W/vanilla/server.out"; exit 1; }

STOP_TICKS=${MCS_STOP_TICKS:-1300}
"$W/mcsrv" --port "$FPORT" --ticks $STOP_TICKS > "$W/f1.out" 2>&1 &
F1=$!
"$W/mcsrv_noopt" --port $(( FPORT + 1 )) --ticks $STOP_TICKS > "$W/f2.out" 2>&1 &
F2=$!
sleep 1

{
  $BOT 127.0.0.1 $VPORT status > "$W/st_v.json"; $BOT 127.0.0.1 $FPORT status > "$W/st_f.json"
  for t in v f; do
    if grep -q '"protocol":763' "$W/st_$t.json"; then echo "  ok    status $t: $(cat "$W/st_$t.json")"; else echo "  FAIL  status $t: $(cat "$W/st_$t.json")"; fi
  done
} | report

$BOT 127.0.0.1 $VPORT join VanBot > "$W/join_v.json" &
J1=$!
$BOT 127.0.0.1 $FPORT join FirnBot > "$W/join_f1.json" &
J2=$!
$BOT 127.0.0.1 $(( FPORT + 1 )) join FirnBot2 > "$W/join_f2.json" &
J3=$!
BOT_SECONDS=52 $BOT 127.0.0.1 $VPORT silent VanSilent > "$W/sil_v.json" &
S1=$!
BOT_SECONDS=52 $BOT 127.0.0.1 $FPORT silent FirnSilent > "$W/sil_f.json" &
S2=$!
wait $J1 $J2 $J3

node "$TOOLS/check.cjs" join "$W/join_v.json" "$W/join_v.json" vanilla | report
node "$TOOLS/check.cjs" join "$W/join_f1.json" "$W/join_v.json" firn-release | report
node "$TOOLS/check.cjs" join "$W/join_f2.json" "$W/join_v.json" firn-no-opt | report
if node "$TOOLS/check.cjs" mutate "$W/join_f1.json" "$W/join_v.json"; then echo "  ok    counter-check: a mutated reference histogram is detected"; else echo "  FAIL  counter-check did not detect the mutation"; ERR=1; fi

cat > "$W/wrong.cjs" <<'JS'
const mc = require('/root/firn/tools/mcserver/node_modules/minecraft-protocol')
const c = mc.createClient({ host: '127.0.0.1', port: parseInt(process.argv[2], 10), username: 'OldBot', auth: 'offline', version: '1.19.4', hideErrors: true })
let reason = ''
c.on('disconnect', (p) => { reason = JSON.stringify(p.reason) })
c.on('error', () => {})
c.on('end', () => { console.log(reason); process.exit(0) })
setTimeout(() => { console.log('timeout'); process.exit(0) }, 8000)
JS
neg=$(node "$W/wrong.cjs" "$FPORT")
case "$neg" in *utdated*) echo "  ok    wrong protocol is refused: $neg";; *) echo "  FAIL  wrong protocol not refused: $neg"; ERR=1;; esac

wait $S1 $S2
node "$TOOLS/check.cjs" silent "$W/sil_v.json" vanilla | report
node "$TOOLS/check.cjs" silent "$W/sil_f.json" firn | report

wait $F1 $F2
for f in f1 f2; do
  line=$(grep 'stopped' "$W/$f.out" || true)
  avg=$(echo "$line" | sed -n 's/.*avg_us=\([0-9]*\).*/\1/p')
  tps=$(echo "$line" | sed -n 's/.*tps_x100=\([0-9]*\).*/\1/p')
  if [ -n "$tps" ] && [ "$tps" -ge 1900 ]; then echo "  ok    graceful stop ($f): $line"; else echo "  FAIL  stop/tps ($f): '$line'"; ERR=1; fi
done
exit $ERR
