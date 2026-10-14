#!/usr/bin/env bash
# tools/mc/play/run_wine.sh -- Windows build of demos/mcplay + demos/mcclient: cross-compile (--target=x86_64-windows),
# then run the .exe files under Wine against the official 1.20.1 server (offline mode, creative), Xvfb as display.
# NO real Windows and NO real GPU here: Wine proves link + start + network + window + software paths only.
# Env: FIRNC, DIST (copy target for the exes, default /root/projects/minecraft-neu/dist), MC_VIEW (4),
#      MCPLAY_GL=1 also runs mcplay.exe over Wine's wgl (Wine OpenGL on Xvfb = llvmpipe).
# Needs wine, Xvfb, java. Prints `RESULT mc-wine: ok`. Start via heavy (java server). Own WINEPREFIX (never ~/.wine-firn).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
DIST=${DIST:-/root/projects/minecraft-neu/dist}
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; WINEPREFIX="$W/wp" wineserver -k 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-wine: FAIL"; exit 1; }
for t in wine Xvfb java; do command -v $t >/dev/null || fail "missing tool: $t"; done
mkdir -p "$DIST"
for d in mcplay mcclient; do
    $FIRNC --target=x86_64-windows --opt-level=dev-fast -o "$W/$d.exe" demos/$d/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/$d does not cross-compile"; }
    cp "$W/$d.exe" "$DIST/$d.exe"
done
echo "  ok  cross-compile: mcplay.exe $(stat -c %s "$W/mcplay.exe") B, mcclient.exe $(stat -c %s "$W/mcclient.exe") B"
head -c 2 "$W/mcplay.exe" | grep -q MZ || fail "mcplay.exe is not a PE file"
SND=${MCPLAY_SOUNDS:-${XDG_CACHE_HOME:-$HOME/.cache}/mcplay/sounds}
[ -s "$SND/bank.json" ] && export MCPLAY_SOUNDS="$SND"
MCV=1.20.1; VIEW=${MC_VIEW:-4}
mkdir -p "$W/srv"
bash tools/mcclient/fetch_server.sh "$W/srv" "$MCV" || fail "server download"
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
( cd "$W/srv" && exec "${JAVA:-/usr/bin/java}" -Xmx1G -Xms256M -jar server.jar nogui < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 180); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
echo "time set day" >&3
DN=$((200 + RANDOM % 90))
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN WINEPREFIX="$W/wp" WINEDEBUG=-all WINEARCH=win64
wine wineboot -i >/dev/null 2>&1   # create the prefix once (first start is slow)
ERR=0
# 1) mcclient.exe: headless join + chunk dump
timeout -s KILL 120 wine "$W/mcclient.exe" 127.0.0.1 "$PORT" WinClient "$W/dump.bin" "$VIEW" 4000 > "$W/client.out" 2>&1; rc=$?
sed 's/^/  client: /' "$W/client.out" | tail -4
[ $rc -eq 0 ] || { echo "  FAIL  mcclient.exe exit code $rc"; ERR=1; }
grep -q "WinClient joined the game" "$W/server.log" || { echo "  FAIL  server never saw mcclient.exe"; ERR=1; }
[ -s "$W/dump.bin" ] || { echo "  FAIL  no dump written"; ERR=1; }
# 2) mcplay.exe: window + software render + audio to file, quits by MCPLAY_RUN_S
run_play() { # $1 = name, rest = env
    local nm=$1; shift
    env "$@" FIRN_AUDIO="file:$W/$nm.wav" MCPLAY_RUN_S=10 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/$nm.png" \
        timeout -s KILL 150 wine "$W/mcplay.exe" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1; rc=$?
    sed "s/^/  $nm: /" "$W/$nm.out" | tail -6
    [ $rc -eq 0 ] || { echo "  FAIL  mcplay.exe ($nm) exit code $rc"; ERR=1; }
    grep -q "$nm joined the game" "$W/server.log" || { echo "  FAIL  server never saw $nm"; ERR=1; }
    local fr ch
    fr=$(grep -o 'frames=[0-9]*' "$W/$nm.out" | head -1 | cut -d= -f2); ch=$(grep -o 'chunks=[0-9]*' "$W/$nm.out" | head -1 | cut -d= -f2)
    [ "${fr:-0}" -gt 10 ] || { echo "  FAIL  $nm: too few frames (${fr:-0})"; ERR=1; }
    [ "${ch:-0}" -gt 10 ] || { echo "  FAIL  $nm: too few chunks (${ch:-0})"; ERR=1; }
    [ -s "$W/$nm.png" ] && cp "$W/$nm.png" "${MCPLAY_WINE_PNG_OUT:-/tmp}/mcplay-wine-$nm.png" 2>/dev/null
}
run_play WinCpu MCPLAY_GL=0
[ "${MCPLAY_GL:-0}" = 1 ] && run_play WinGl MCPLAY_GL=1
[ $ERR -eq 0 ] && echo "RESULT mc-wine: ok" || echo "RESULT mc-wine: FAIL"
exit $ERR
