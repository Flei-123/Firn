#!/usr/bin/env bash
# tools/mc/dedi/run.sh -- dedicated server (firnmc-server) end to end:
#   A  first start: server.properties generated, Done line in vanilla format, logs/latest.log written
#   B  custom server.properties: unknown key kept, missing keys added, motd/max-players/version in the status ping
#   C  console over stdin: list say op deop whitelist kick time tps version and an unknown command
#   D  join/leave log lines, whitelist refusal, ops.json / whitelist.json in vanilla format
#   E  `stop` saves and exits with code 0; SIGTERM does the same
#   F  saved vanilla world copy: served, level.dat clock and playerdata written on stop
#   G  port in use fails with exit code 1
# Env: FIRNC, MC_WORLD (vanilla world dir, default /root/.w5/savetest/v1201/world), DEDI_KEEP=<dir> keeps logs.
set -uo pipefail
cd "$(dirname "$0")/../../.."
REPO=$(pwd)
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$REPO/lib"
PROBE="node $REPO/tools/mc/dedi/probe.cjs"
WORLD_SRC=${MC_WORLD:-/root/.w5/savetest/v1201/world}
W=$(mktemp -d)
HOLD=""
cleanup() {
    [ -n "$HOLD" ] && kill $HOLD 2>/dev/null
    for p in $(pgrep -f "$W/firnmc-server" 2>/dev/null); do kill -9 "$p" 2>/dev/null; done
    if [ -n "${DEDI_KEEP:-}" ]; then mkdir -p "$DEDI_KEEP"; cp -r "$W"/r* "$DEDI_KEEP"/ 2>/dev/null; fi
    rm -rf "$W"
}
trap cleanup EXIT
ERR=0
ok() { echo "  ok    $1"; }
bad() { echo "  FAIL  $1"; ERR=$((ERR+1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }
free_port() { python3 -I -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
wait_for() { local f=$1 pat=$2 n=${3:-100}; for _ in $(seq 1 $n); do grep -q -- "$pat" "$f" 2>/dev/null && return 0; sleep 0.1; done; return 1; }

$FIRNC -o "$W/firnmc-server" demos/firnmc-server/main.fi 2>"$W/err" || { echo "  FAIL  compile"; head -5 "$W/err"; exit 1; }
ok "compile firnmc-server"

start() {
    local dir=$1; shift
    mkdir -p "$dir"; rm -f "$dir/in.fifo"; mkfifo "$dir/in.fifo"
    (setsid nohup sleep 600 > "$dir/in.fifo" 2>/dev/null & echo $! > "$dir/hold.pid")
    HOLD="$HOLD $(cat "$dir/hold.pid")"
    (cd "$dir" && setsid nohup bash -c "$W/firnmc-server nogui $*; echo \$? > exit.code" < in.fifo > out.log 2>&1 &)
}
cmd() { echo "$2" > "$1/in.fifo"; sleep 0.4; }
stop_and_wait() {
    cmd "$1" stop
    for _ in $(seq 1 60); do [ -f "$1/exit.code" ] && return 0; sleep 0.1; done
    return 1
}

echo "== A first start"
P1=$(free_port); R1=$W/r1
start "$R1" --port "$P1"
check "Done line" "wait_for $R1/out.log 'Done (' 100"
check "Done line format" "grep -Eq '^\[[0-9]{2}:[0-9]{2}:[0-9]{2} INFO\]: Done \([0-9]+\.[0-9]{3}s\)! For help, type \"help\"\$' $R1/out.log"
check "server.properties generated" "grep -q '^server-port=25565' $R1/server.properties && grep -q '^motd=' $R1/server.properties"
check "logs/latest.log written" "grep -q 'Done (' $R1/logs/latest.log"
check "stop exits 0" "stop_and_wait $R1 && [ \"\$(cat $R1/exit.code)\" = 0 ]"
check "stop logged" "grep -q 'Stopping server' $R1/out.log"

echo "== B custom properties + status ping"
P2=$(free_port); R2=$W/r2; mkdir -p "$R2"
printf 'motd=Hello Firn\nmax-players=7\nserver-port=%s\ngamemode=creative\nmy-custom-key=keepme\n#a comment\n' "$P2" > "$R2/server.properties"
start "$R2"
wait_for "$R2/out.log" 'Done (' 100
PING=$($PROBE ping 127.0.0.1 $P2)
check "ping motd" "echo '$PING' | grep -q '\"motd\":\"Hello Firn\"'"
check "ping max players" "echo '$PING' | grep -q '\"max\":7'"
check "ping version" "echo '$PING' | grep -q '\"version\":\"1.20.1\"' && echo '$PING' | grep -q '\"protocol\":763'"
check "unknown key preserved" "grep -q '^my-custom-key=keepme' $R2/server.properties && grep -q '^#a comment' $R2/server.properties"
check "missing keys added" "grep -q '^view-distance=' $R2/server.properties && grep -q '^white-list=false' $R2/server.properties"

echo "== C console"
cmd "$R2" "list"
cmd "$R2" "say hello world"
cmd "$R2" "tps"
cmd "$R2" "version"
cmd "$R2" "time set noon"
cmd "$R2" "nonsense"
cmd "$R2" "help"
check "list" "grep -q 'There are 0 of a max of 7 players online' $R2/out.log"
check "say" "grep -q '\[Server\] hello world' $R2/out.log"
check "tps" "grep -q 'TPS: ' $R2/out.log"
check "version" "grep -q 'protocol 763' $R2/out.log"
check "time set" "grep -q 'Set the time to 6000' $R2/out.log"
check "unknown command" "grep -q 'Unknown or incomplete command' $R2/out.log"
check "help" "grep -q 'Available commands' $R2/out.log"

echo "== D join, whitelist, ops"
(HOLD_MS=2500 $PROBE join 127.0.0.1 $P2 Bot1 >/dev/null &)
sleep 1.2
cmd "$R2" "list"
check "join logged" "grep -q 'Bot1 joined the game' $R2/out.log"
check "leave logged" "wait_for $R2/out.log 'Bot1 left the game' 30"
check "list shows player" "grep -q 'players online: Bot1' $R2/out.log"
cmd "$R2" "whitelist on"
J2=$($PROBE join 127.0.0.1 $P2 Bot2)
check "non-whitelisted refused" "echo '$J2' | grep -q 'white-listed'"
cmd "$R2" "whitelist add Bot2"
J3=$($PROBE join 127.0.0.1 $P2 Bot2)
check "whitelisted joins" "echo '$J3' | grep -q '\"joined\":true'"
cmd "$R2" "whitelist list"
check "whitelist list" "grep -q 'whitelisted players: Bot2' $R2/out.log"
check "whitelist.json format" "grep -q '\"uuid\":' $R2/whitelist.json && grep -q '\"name\": \"Bot2\"' $R2/whitelist.json"
cmd "$R2" "whitelist remove Bot2"
J4=$($PROBE join 127.0.0.1 $P2 Bot2)
check "removed refused" "echo '$J4' | grep -q 'white-listed'"
cmd "$R2" "op Bot1"
check "ops.json" "grep -q '\"name\": \"Bot1\"' $R2/ops.json && grep -q '\"level\": 4' $R2/ops.json"
cmd "$R2" "deop Bot1"
check "deop" "grep -q 'no longer a server operator' $R2/out.log && ! grep -q Bot1 $R2/ops.json"
cmd "$R2" "whitelist off"
(HOLD_MS=3000 $PROBE join 127.0.0.1 $P2 Kicked >/dev/null &)
sleep 1.2
cmd "$R2" "kick Kicked bad manners"
check "kick" "wait_for $R2/out.log 'Kicked Kicked: bad manners' 20"
check "stop 0" "stop_and_wait $R2 && [ \"\$(cat $R2/exit.code)\" = 0 ]"

echo "== E SIGTERM"
P3=$(free_port); R3=$W/r3
start "$R3" --port "$P3"
wait_for "$R3/out.log" 'Done (' 100
kill -TERM $(pgrep -f "$W/firnmc-server nogui --port $P3" | tail -1)
for _ in $(seq 1 60); do [ -f "$R3/exit.code" ] && break; sleep 0.1; done
check "SIGTERM exits 0" "[ \"\$(cat $R3/exit.code 2>/dev/null)\" = 0 ] && grep -q 'Saved and stopped' $R3/out.log"

echo "== F saved world"
if [ -f "$WORLD_SRC/level.dat" ]; then
    P4=$(free_port); R4=$W/r4; mkdir -p "$R4"
    cp -r "$WORLD_SRC" "$R4/world"
    LV0=$(stat -c %Y "$R4/world/level.dat")
    sleep 1.1
    printf 'server-port=%s\nlevel-name=world\n' "$P4" > "$R4/server.properties"
    start "$R4"
    check "world loaded" "wait_for $R4/out.log 'Done (' 100 && ! grep -q 'non-persistent' $R4/out.log"
    J5=$(HOLD_MS=1500 $PROBE join 127.0.0.1 $P4 Saver)
    check "join saved world" "echo '$J5' | grep -q '\"joined\":true'"
    check "stop 0" "stop_and_wait $R4 && [ \"\$(cat $R4/exit.code)\" = 0 ]"
    check "level.dat rewritten" "[ \$(stat -c %Y $R4/world/level.dat) -gt $LV0 ]"
    check "playerdata written" "ls $R4/world/playerdata/*.dat >/dev/null 2>&1"
else
    echo "  skip  no world at $WORLD_SRC"
fi

echo "== G port in use"
P5=$(free_port); R5=$W/r5; R6=$W/r6
start "$R5" --port "$P5"
wait_for "$R5/out.log" 'Done (' 100
start "$R6" --port "$P5"
for _ in $(seq 1 60); do [ -f "$R6/exit.code" ] && break; sleep 0.1; done
check "port in use exits 1" "[ \"\$(cat $R6/exit.code 2>/dev/null)\" = 1 ] && grep -q 'ERROR' $R6/out.log"
stop_and_wait "$R5" >/dev/null

echo
if [ $ERR -eq 0 ]; then echo "dedi: all ok"; else echo "dedi: $ERR FAILED"; fi
exit $ERR
