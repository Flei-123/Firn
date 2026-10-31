#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/server/ent_run_v777.sh -- entities + dig of the Firn server for protocol 777 (26.3): mcplay A breaks the block under its feet in survival,
#   mcplay B only watches. Needs: A sees the item entity spawn + collect it (inventory), B sees player A + the item, bad=0 everywhere, the block change arrives.
# Env: FIRNC, MC_VIEW (3), PROTO (777). Prints `RESULT mc-server-ent-v777: ok`.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
PROTO=${PROTO:-777}
VIEW=${MC_VIEW:-3}
W=$(mktemp -d); SRV_PID=""; XA_PID=""; XB_PID=""; PA=""; PB=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "${ENT_KEEP:-}" ] && { mkdir -p "$ENT_KEEP"; cp "$W"/*.out "$W"/server.log "$ENT_KEEP"/ 2>/dev/null; }; for p in $PA $PB $XA_PID $XB_PID $SRV_PID; do kill "$p" 2>/dev/null; done; sleep 1; for p in $PA $PB $SRV_PID; do kill -9 "$p" 2>/dev/null; done; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-server-ent-v777: FAIL"; exit 1; }
for t in Xvfb xdotool python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "mcplay does not compile"; }
$FIRNC --opt-level=dev-fast -o "$W/fmc" demos/firnmc-server/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "firnmc-server does not compile"; }
mkdir -p "$W/srv"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
printf 'gamemode=survival\nforce-gamemode=true\nwhite-list=false\nview-distance=%s\n' "$VIEW" > "$W/srv/server.properties"
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$W/fmc" nogui --protocol "$PROTO" --port "$PORT" < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 60); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
DA=$(bash tools/mc/pick_display.sh 90 50)
Xvfb :$DA -screen 0 1024x600x24 >"$W/xa.log" 2>&1 & XA_PID=$!
DB=$(bash tools/mc/pick_display.sh 150 50)
Xvfb :$DB -screen 0 1024x600x24 >"$W/xb.log" 2>&1 & XB_PID=$!
sleep 2
COMMON="MCPLAY_PROTOCOL=$PROTO MCPLAY_GL=0 MCPLAY_VSYNC=0 MCPLAY_AUDIO=0 MCPLAY_PROGRESS=1 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_RUN_S=90"
prog() { grep -o "progress .*" "$W/$1.out" 2>/dev/null | tail -1 | grep -o " $2=[^ ]*" | cut -d= -f2; }
wait_ticks() { local t0 t; t0=$(prog "$1" ticks); t0=${t0:-0}; for i in $(seq 1 $(( ${3:-120} * 10 ))); do t=$(prog "$1" ticks); [ "${t:-0}" -ge $(( t0 + $2 )) ] && return 0; sleep 0.1; done; echo "  WARN  wait_ticks $2 timed out"; return 1; }
env DISPLAY=:$DB $COMMON MCPLAY_QUIT_FILE="$W/quit-B" timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Watcher > "$W/B.out" 2>&1 &
PB=$!
for i in $(seq 1 300); do grep -q "Watcher joined the game" "$W/server.log" && break; sleep 0.2; done
sleep 2
env DISPLAY=:$DA $COMMON MCPLAY_QUIT_FILE="$W/quit-A" timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" Digger > "$W/A.out" 2>&1 &
PA=$!
for i in $(seq 1 300); do grep -q "Digger joined the game" "$W/server.log" && break; sleep 0.2; done
for i in $(seq 1 600); do [ "$(prog A ground)" = 1 ] || [ "$(prog A ground)" = true ] && break; sleep 0.1; done
wait_ticks A 20
export DISPLAY=:$DA
xdotool mousemove 320 180 2>/dev/null
xdotool mousemove_relative -- 0 350; xdotool mousemove_relative -- 0 350; wait_ticks A 10
xdotool mousedown 1; wait_ticks A 100; xdotool mouseup 1
wait_ticks A 80
touch "$W/quit-A" "$W/quit-B"
wait $PA 2>/dev/null; PA=""; wait $PB 2>/dev/null; PB=""
ERR=0
num() { grep "mcplay: entities count" "$W/$1.out" | tail -1 | grep -o " $2=[0-9]*" | head -1 | cut -d= -f2; }
for who in A B; do
    echo "  $who: $(grep 'mcplay: entities count' "$W/$who.out" | tail -1 | cut -c1-200)"
    [ "$(num $who bad)" = 0 ] || { echo "  FAIL  $who: bad entity packets"; ERR=1; }
done
[ "${A_SPAWNS:-$(num A spawns)}" -ge 1 ] 2>/dev/null || { echo "  FAIL  A saw no spawn"; ERR=1; }
[ "$(num A destroys)" -ge 1 ] 2>/dev/null || { echo "  FAIL  A saw no destroy (item pickup)"; ERR=1; }
[ "$(num B players)" -ge 1 ] 2>/dev/null || { echo "  FAIL  B saw no player"; ERR=1; }
[ "$(num B spawns)" -ge 2 ] 2>/dev/null || { echo "  FAIL  B did not see player + item spawn"; ERR=1; }
grep -E "dig=|bchange" "$W/A.out" | tail -1 | cut -c1-200 | sed 's/^/  A: /'
grep -E "rror|xception|lost connection|Kick" "$W/server.log" | tail -3 | cut -c1-200 | sed 's/^/  srv: /'
if [ $ERR -eq 0 ]; then echo "RESULT mc-server-ent-v777: ok"; else echo "RESULT mc-server-ent-v777: FAIL"; exit 1; fi
