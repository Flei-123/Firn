#!/usr/bin/env bash
# tools/mc/play/run_hash.sh [mc_version=1.21.5] -- hashed item stacks (1.21.5+) of the Click Container packet against an official server
# (mc_hash + mc_inv: a stack with six data components and a plain stack, hashed Click Container; docs/MC-VERSIONS.md "Hashed item stacks").
# Prints `RESULT mc-play-hash: ok|FAIL`. Env: RUN_INV_CMDS (extra server commands in the survival stage), MCPLAY_RAW_INV=1 (rawinv lines = real packet bodies for test fixtures), FIRNC, JAVA (1.21+ needs Java 21+; default /root/tools/jdk25/bin/java when present), MC_VIEW (3), MCPLAY_KEEP_DIR.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
MCV=${1:-1.21.5}; VIEW=${MC_VIEW:-3}
case "$MCV" in 1.20.1) PROTO=763;; 1.21.1) PROTO=767;; 1.21.4) PROTO=769;; 1.21.5) PROTO=770;; 1.21.8) PROTO=772;; 1.21.11) PROTO=774;; 26.1) PROTO=775;; *) PROTO=${MCPLAY_PROTOCOL:-0};; esac
[ "$PROTO" != 0 ] || { echo "unknown version $MCV (set MCPLAY_PROTOCOL)"; exit 2; }
export MCPLAY_PROTOCOL=$PROTO
JAVA=${JAVA:-/usr/bin/java}; [ -x /root/tools/jdk25/bin/java ] && case "$MCV" in 1.2[1-9]*|26*) JAVA=/root/tools/jdk25/bin/java ;; esac
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; if [ -n "${MCPLAY_KEEP_DIR:-}" ]; then mkdir -p "$MCPLAY_KEEP_DIR"; cp "$W"/*.out "$W"/server.log "$MCPLAY_KEEP_DIR"/ 2>/dev/null; fi; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null; sleep 1; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-hash: FAIL"; exit 1; }
for t in Xvfb xdotool; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
SND=${MCPLAY_SOUNDS:-${XDG_CACHE_HOME:-$HOME/.cache}/mcplay/sounds}
export MCPLAY_SOUNDS="$SND"
mkdir -p "$W/srv"
MCCLIENT_JAR_CACHE=${MCCLIENT_JAR_CACHE:-/root/.w5/jars} bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
echo "eula=true" > "$W/srv/eula.txt"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
cat > "$W/srv/server.properties" <<PROPS
server-ip=127.0.0.1
server-port=$PORT
online-mode=false
gamemode=creative
force-gamemode=true
level-seed=4242
view-distance=$VIEW
simulation-distance=$VIEW
spawn-protection=0
difficulty=peaceful
spawn-monsters=false
spawn-animals=false
spawn-npcs=false
enable-rcon=false
enable-query=false
PROPS
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$JAVA" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
sleep "${RUN_INV_WARMUP:-0}" # a cold 1.21.11 / 26.1 server answers the first login late
for gr in "doDaylightCycle false" "doMobSpawning false"; do echo "gamerule $gr" >&3; done
echo "time set day" >&3
DN=$((120 + RANDOM % 70))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
ERR=0
# ---- a pickaxe with six data components (damage, name, unbreakable, enchantments, rarity, glint) and a plain stack; mcplay picks each up and puts it
# back (Click Container with hashed stacks) and counts the packets the server answers with. A hash it does not recognise makes it re-send the slot:
# run 1 (hashes on) must see 0 re-sends, run 2 (MCPLAY_HASH_OFF=1, empty component hashes) must see some -> the check really measures something.
COMPS='damage=5,custom_name="Hi",enchantments={"minecraft:efficiency":5,"minecraft:unbreaking":2},rarity=epic,enchantment_glint_override=true,unbreakable={}'
run_hash() { # $1 = player name, $2 = HASH_OFF, $3 = output file
    MCPLAY_AUDIO=0 MCPLAY_CREATIVE=0 MCPLAY_HASH_TEST=1 MCPLAY_HASH_OFF=$2 MCPLAY_RUN_S=16 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
        timeout 150 "$W/mcplay" 127.0.0.1 "$PORT" "$1" > "$3" 2>&1 &
    PP=$!
    sleep 5
    echo "gamemode survival $1" >&3; echo "give $1 diamond_pickaxe[$COMPS] 1" >&3; echo "give $1 stone 32" >&3
    sleep 1
    echo "data get entity $1 Inventory" >&3
    wait $PP
}
run_hash HashOn 0 "$W/h_on.out"
run_hash HashOff 1 "$W/h_off.out"
L1=$(grep 'mcplay: hashtest' "$W/h_on.out" | head -1); L2=$(grep 'mcplay: hashtest' "$W/h_off.out" | head -1)
echo "  hashes on : $L1"; echo "  hashes off: $L2"
[ -n "$L1" ] || { echo "  FAIL  no hashtest line (hashes on)"; ERR=1; }
case "$L1" in *"comp_pick=0 comp_put=0 plain_pick=0 plain_put=0"*) ;; *) echo "  FAIL  hashed stacks: the server re-sent slots"; ERR=1;; esac
case "$L2" in *"comp_pick=0 comp_put=0 "*) echo "  FAIL  control: no re-send without hashes (the check measures nothing)"; ERR=1;; esac
BAD=$(grep -o ' bad=[0-9]*' "$W/h_on.out" | head -1 | cut -d= -f2)
[ "${BAD:-1}" -eq 0 ] || { echo "  FAIL  inventory bad=${BAD:-?}"; ERR=1; }
grep -qiE "invalid|disconnect|kicked" "$W/server.log" && grep -iE "invalid|disconnect|kicked" "$W/server.log" | head -3
echo stop >&3; sleep 1
[ $ERR -eq 0 ] && echo "RESULT mc-play-hash: ok" || echo "RESULT mc-play-hash: FAIL"
exit $ERR
