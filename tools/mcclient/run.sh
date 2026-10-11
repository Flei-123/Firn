#!/usr/bin/env bash
# run.sh [--no-compression] [--mock] [firn_client_binary]
#
# End-to-end test of the M0 protocol prototype against a REAL official Minecraft 1.20.1 server
# (offline mode, fixed seed, small view distance):
#     A  = reference client (tools/mcclient/ref_dump.cjs, node-minecraft-protocol)
#     B  = Firn client (binary from argument or env MCCLIENT_BIN; skipped with SKIPPED if absent)
#     A2 = reference client again
# Pass = A == A2 (reference stable) and B == A (byte identical dump, same checksum).
#
# EULA: eula.txt is written with eula=true. Justin / the lead explicitly ordered running this
# test server (Minecraft EULA accepted for this throw-away local test instance, bound to 127.0.0.1).
#
# Heavy: start it through the server wrapper:  /root/jarvis/bin/heavy bash tools/mcclient/run.sh
# Self cleaning: everything lives in one mktemp dir that is removed on exit; the server is stopped.
#
# --mock: EMERGENCY ONLY. Uses tools/mcclient/mock_server.cjs ('MOCK - not a real server') instead of the
# official server. Everything printed is marked MOCK; the result proves nothing about the real game.
#
# Env: MCCLIENT_BIN       Firn client binary (CLI contract in demos/mcclient/DESIGN.md)
#      MCCLIENT_JAR_CACHE optional dir with a verified server jar (see fetch_server.sh)
#      MC_COMPRESSION     network-compression-threshold (default 256; -1 = off, or --no-compression)
#      MC_SEED            level seed (default 4242)
#      MC_VIEW            view distance for server and clients (default 4)
#      MC_IDLE_MS         client idle time after the last chunk (default 2500)
#      MC_SETTLE_MS       settle run (player online, dump discarded) before A so fluids settle (default 45000; 0 = off)
#      REF_EXTRA          number of additional reference runs (A3, A4, ...) for stability checks
#      ALLOW_SKIP=1       exit 0 when B is skipped and A == A2 (default: exit 3)
#      JAVA               java binary (default /usr/bin/java)
#      MC_VERSION         release to test (default 1.20.1; also 1.12.2, 1.20.2 ...). The protocol number and the
#                         node-minecraft-protocol version come from tools/mcgen/versions.json; the same value is
#                         exported to ref_dump.cjs (MC_NMP, MC_PROTOCOL) and passed to the Firn client (arg 7).
# Exit: 0 all equal, 1 mismatch, 2 setup/server failure, 3 reference stable but B skipped.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export NODE_PATH="${NODE_PATH:-/root/firn/tools/mcserver/node_modules}"
JAVA="${JAVA:-/usr/bin/java}"
SEED="${MC_SEED:-4242}"
VIEW="${MC_VIEW:-4}"
IDLE="${MC_IDLE_MS:-2500}"
COMP="${MC_COMPRESSION:-256}"
MOCK=0
NAME="Dumper"
SETTLE="${MC_SETTLE_MS:-45000}"
BIN="${MCCLIENT_BIN:-}"
for a in "$@"; do
  case "$a" in
    --no-compression) COMP=-1 ;;
    --mock) MOCK=1 ;;
    -*) echo "run.sh: unknown option $a" >&2; exit 2 ;;
    *) BIN="$a" ;;
  esac
done

MCV="${MC_VERSION:-1.20.1}"
read -r PROTO NMPV < <(node -e '
  const v = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).versions[process.argv[2]];
  if (!v) { console.error("unknown version " + process.argv[2]); process.exit(1); }
  console.log(v.protocol + " " + (v.nmp || process.argv[2]));
' "$HERE/../mcgen/versions.json" "$MCV") || { echo "run.sh: version $MCV not in versions.json" >&2; exit 2; }
export MC_NMP="$NMPV" MC_PROTOCOL="$PROTO"
LEGACY=0; DIFF=peaceful; case "$MCV" in 1.8*|1.9*|1.10*|1.11*|1.12*) LEGACY=1; DIFF=0 ;; esac
W=$(mktemp -d)
SRV_PID=""
cleanup() {
  exec 3>&- 2>/dev/null || true
  if [ -n "$SRV_PID" ] && kill -0 "$SRV_PID" 2>/dev/null; then
    kill "$SRV_PID" 2>/dev/null; sleep 1; kill -9 "$SRV_PID" 2>/dev/null
  fi
  rm -rf "$W"
}
trap cleanup EXIT

die() { echo "run.sh: $*" >&2; exit 2; }

if [ "$MOCK" -eq 1 ]; then
  echo "== MOCK - not a real server (emergency substitute, proves nothing about the real game) =="
  mkdir -p "$W/out"
  SETTLE=0
  (exec node "$HERE/mock_server.cjs" 0 "$VIEW" > "$W/server.log" 2>&1) &
  SRV_PID=$!
  for i in $(seq 1 20); do grep -q 'listening on' "$W/server.log" 2>/dev/null && break; sleep 0.5; done
  PORT=$(grep -o 'on 127.0.0.1:[0-9]*' "$W/server.log" | head -1 | cut -d: -f2)
  [ -n "$PORT" ] || { cat "$W/server.log" >&2; die "mock server did not start"; }
  LEAVE_PATTERN='MOCK - not a real server: .* left'
else
echo "== setup (version $MCV, protocol $PROTO, compression threshold $COMP, seed $SEED, view distance $VIEW) =="
  mkdir -p "$W/srv" "$W/out"
  bash "$HERE/fetch_server.sh" "$W/srv" "$MCV" || die "server download failed"
  
  echo "eula=true" > "$W/srv/eula.txt"
  PORT=$(node -e 'const s=require("net").createServer();s.listen(0,"127.0.0.1",()=>{console.log(s.address().port);s.close()})')
  cat > "$W/srv/server.properties" <<EOF
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
level-seed=$SEED
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
max-players=4
enable-command-block=false
difficulty=$DIFF
network-compression-threshold=$COMP
enable-rcon=false
enable-query=false
player-idle-timeout=0
motd=mcclient test server
EOF
  
  # 1.8.x: the Netty epoll transport of the old server fails on Java 9+ ("Unable to access address of buffer"); use NIO
  case "$MCV" in 1.8*) echo "use-native-transport=false" >> "$W/srv/server.properties" ;; esac
  mkfifo "$W/in"
  exec 3<>"$W/in"
  ( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
  SRV_PID=$!
  
  echo "== waiting for server (port $PORT) =="
  for i in $(seq 1 180); do
    grep -q 'Done (' "$W/server.log" 2>/dev/null && break
    kill -0 "$SRV_PID" 2>/dev/null || { tail -20 "$W/server.log" >&2; die "server died during start"; }
    sleep 1
  done
  grep -q 'Done (' "$W/server.log" || { tail -20 "$W/server.log" >&2; die "server start timeout (180 s)"; }
  grep -m1 -i 'Starting minecraft server version' "$W/server.log" | sed 's/^/server log: /'
  grep -m1 'Done (' "$W/server.log" | sed 's/^/server log: /'
  
  # freeze world dynamics as far as the game allows: no random ticks, mobs, fire, weather, day cycle
  for gr in "randomTickSpeed 0" "doFireTick false" "doMobSpawning false" "doWeatherCycle false" "doDaylightCycle false" "doTraderSpawning false" "doPatrolSpawning false" "doInsomnia false"; do
    echo "gamerule $gr" >&3
  done
  if [ "$LEGACY" -eq 1 ]; then sleep 3; else
    for i in $(seq 1 20); do grep -q 'Gamerule doInsomnia is now set' "$W/server.log" && break; sleep 0.5; done
  fi
  
fi
LEAVE_PATTERN="${LEAVE_PATTERN:-left the game}"

# wait until the Nth leave line appears so the player position is saved
LEFT=0
wait_left() {
  LEFT=$((LEFT + 1))
  for i in $(seq 1 60); do
    [ "$(grep -c "$LEAVE_PATTERN" "$W/server.log")" -ge "$LEFT" ] && { sleep 1; return 0; }
    sleep 0.5
  done
  echo "run.sh: warning: player did not leave in time" >&2
}

run_ref() { # label [idle_ms]
  local label="$1" idle="${2:-$IDLE}"
  echo "== reference client $label =="
  timeout 200 node "$HERE/ref_dump.cjs" 127.0.0.1 "$PORT" "$NAME" "$W/out/$label.dump" "$VIEW" "$idle" > "$W/out/$label.metrics" 2> "$W/out/$label.err"
  local rc=$?
  [ -s "$W/out/$label.err" ] && { echo "  stderr of $label:"; head -3 "$W/out/$label.err" | sed 's/^/    /'; }
  [ $rc -eq 0 ] || { echo "run.sh: reference client $label failed (rc $rc)" >&2; return 1; }
  wait_left
}

run_firn() { # label
  local label="$1"
  echo "== Firn client $label ($BIN) =="
  timeout 150 "$BIN" 127.0.0.1 "$PORT" "$NAME" "$W/out/$label.dump" "$VIEW" "$IDLE" "$PROTO" > "$W/out/$label.metrics" 2> "$W/out/$label.err"
  local rc=$?
  [ -s "$W/out/$label.err" ] && { echo "  stderr of $label:"; head -3 "$W/out/$label.err" | sed 's/^/    /'; }
  [ $rc -eq 0 ] || { echo "run.sh: Firn client $label failed (rc $rc)" >&2; return 1; }
  wait_left
}

OK=1
if [ "$SETTLE" -gt 0 ]; then
  echo "(settle run: dump discarded)"
  run_ref W0 "$SETTLE" || OK=0
fi
run_ref A || OK=0
B_STATE=SKIPPED
if [ -n "$BIN" ] && [ -x "$BIN" ]; then
  if run_firn B; then B_STATE=RAN; else B_STATE=FAILED; fi
else
  echo "== Firn client B: SKIPPED (no binary; pass it as argument or MCCLIENT_BIN) =="
fi
run_ref A2 || OK=0
EXTRA_LABELS=""
for n in $(seq 1 "${REF_EXTRA:-0}"); do
  run_ref "A$((n + 2))" && EXTRA_LABELS="$EXTRA_LABELS A$((n + 2))" || OK=0
done

echo "== stopping server =="
[ "$MOCK" -eq 1 ] || echo stop >&3
[ "$MOCK" -eq 1 ] && kill "$SRV_PID" 2>/dev/null
for i in $(seq 1 40); do kill -0 "$SRV_PID" 2>/dev/null || break; sleep 0.5; done
kill -0 "$SRV_PID" 2>/dev/null && { echo "server did not stop, killing"; kill "$SRV_PID"; sleep 2; kill -9 "$SRV_PID" 2>/dev/null; }
SRV_PID=""

[ "$MOCK" -eq 1 ] && echo "NOTE: MOCK - not a real server; this comparison does not prove real-server compatibility"
echo "== comparison =="
RESULT=0
[ "$OK" -eq 1 ] || RESULT=2
if [ -f "$W/out/A.dump" ] && [ -f "$W/out/A2.dump" ]; then
  bash "$HERE/compare.sh" "$W/out/A.dump" "$W/out/A2.dump" A A2 || RESULT=1
  for l in $EXTRA_LABELS; do
    bash "$HERE/compare.sh" "$W/out/A.dump" "$W/out/$l.dump" A "$l" || RESULT=1
  done
fi
if [ "$B_STATE" = RAN ]; then
  bash "$HERE/compare.sh" "$W/out/A.dump" "$W/out/B.dump" A B || RESULT=1
elif [ "$B_STATE" = FAILED ]; then
  echo "Firn client B: FAILED"; RESULT=1
else
  echo "Firn client B: SKIPPED (not compared)"
fi

echo "== metrics =="
metric() { grep -h "^metric $2=" "$W/out/$1.metrics" 2>/dev/null | head -1 | cut -d= -f2-; }
KEYS="connect_ms join_ms first_chunk_ms total_ms packets bytes_in bytes_decompressed packets_per_s chunks walk_ok walk_fail bulk_packets bulk_columns"
COLS="A B A2$EXTRA_LABELS"
printf '%-20s' key; for c in $COLS; do printf '%-14s' "$c"; done; echo
for k in $KEYS; do
  printf '%-20s' "$k"
  for c in $COLS; do
    v=$(metric "$c" "$k"); [ -z "$v" ] && v="-"
    [ "$c" = B ] && [ "$B_STATE" = SKIPPED ] && v="SKIPPED"
    printf '%-14s' "$v"
  done
  echo
done
printf '%-20s' checksum; for c in $COLS; do v=$(metric "$c" checksum); printf '%-14s' "${v:0:12}"; done; echo

if [ "$RESULT" -eq 0 ] && [ "$B_STATE" = SKIPPED ]; then
  echo "RESULT: reference stable (A == A2), B SKIPPED"
  [ "${ALLOW_SKIP:-0}" = 1 ] || RESULT=3
elif [ "$RESULT" -eq 0 ]; then
  echo "RESULT: PASS (A == A2, B == A)$([ "$MOCK" -eq 1 ] && echo ' [MOCK - not a real server]')"
else
  echo "RESULT: FAIL (code $RESULT)"
fi
exit "$RESULT"
