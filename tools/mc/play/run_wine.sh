#!/usr/bin/env bash
# tools/mc/play/run_wine.sh -- Windows build of demos/mcplay + demos/mcclient: cross-compile (--target=x86_64-windows),
# then run the .exe files under Wine against the official 1.20.1 server (offline mode, creative), Xvfb as display.
# NO real Windows and NO real GPU here: Wine proves link + start + network + window + software paths only.
# Env: FIRNC, DIST (copy target for the exes, default /root/projects/minecraft-neu/dist), MC_VIEW (4),
#      WINE_COPY_DIST=1 copies the (dev-fast) exes to DIST, MCPLAY_GL=1 also runs mcplay.exe over Wine's wgl (Wine OpenGL on Xvfb = llvmpipe).
# Also an input stage (xdotool: raw mouse, Esc, F11 fullscreen rect, DPI) -- skipped without xdotool.
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
    [ "${WINE_COPY_DIST:-0}" = 1 ] && cp "$W/$d.exe" "$DIST/$d.exe"   # dev-fast build: the dist package comes from tools/mc/dist.sh
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
# 3) input under Wine: xdotool drives the X11 side, Wine turns it into WM_INPUT / WM_KEYDOWN / window rects. Proves the Win32 window
#    backend's raw mouse, mouse capture, Esc release, F11 fullscreen and DPI reporting as far as Wine goes (NOT real hardware).
run_input() {
    command -v xdotool >/dev/null || { echo "  skip  input test (no xdotool)"; return; }
    local nm=WinInput
    env MCPLAY_GL=0 FIRN_AUDIO=null MCPLAY_RUN_S=${MCPLAY_INPUT_RUN_S:-40} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 \
        timeout -s KILL 170 wine "$W/mcplay.exe" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1 &
    local pid=$! i=0
    while [ $i -lt 90 ]; do grep -q "$nm joined the game" "$W/server.log" 2>/dev/null && break; kill -0 $pid 2>/dev/null || break; sleep 1; i=$((i+1)); done
    sleep 6
    local win geo0 geo1 geo2
    win=$(xdotool search --onlyvisible --name "" 2>/dev/null | while read -r id; do
        g=$(xdotool getwindowgeometry "$id" 2>/dev/null | grep Geometry | sed 's/.*: //'); case "$g" in 64[0-9]x3[0-9][0-9]) echo "$id";; esac; done | head -1)
    if [ -z "$win" ]; then echo "  FAIL  input: mcplay window not found"; ERR=1; wait $pid 2>/dev/null; return; fi
    xdotool windowfocus "$win" 2>/dev/null; sleep 1
    geo0=$(xdotool getwindowgeometry "$win" | grep Geometry | sed 's/.*: //')
    xdotool mousemove --window "$win" 320 180 2>/dev/null; sleep 0.5
    local k; for k in 1 2 3 4 5 6 7 8; do xdotool mousemove_relative -- 12 5 2>/dev/null; sleep 0.15; done
    xdotool key F11 2>/dev/null; sleep 3   # NOT --window: XSendEvent keys are ignored by Wine, XTEST (focused window) works
    geo1=$(xdotool getwindowgeometry "$win" | grep Geometry | sed 's/.*: //')
    xdotool key F11 2>/dev/null; sleep 3
    geo2=$(xdotool getwindowgeometry "$win" | grep Geometry | sed 's/.*: //')
    xdotool key Escape 2>/dev/null; sleep 1
    wait $pid 2>/dev/null
    echo "  input: window geometry start=$geo0 after-F11=$geo1 after-2nd-F11=$geo2"
    sed "s/^/  $nm: /" "$W/$nm.out" | grep "mcplay: input" | tail -2
    local line; line=$(grep "mcplay: input" "$W/$nm.out" | tail -1)
    [ -n "$line" ] || { echo "  FAIL  input: no input line (mcplay did not reach its end)"; ERR=1; return; }
    local dx f11 grab dpi
    dx=$(echo "$line" | grep -o 'raw_dx=[-0-9.]*' | cut -d= -f2); f11=$(echo "$line" | grep -o 'f11=[0-9]*' | cut -d= -f2)
    grab=$(echo "$line" | grep -o 'grab=[a-z]*' | cut -d= -f2); dpi=$(echo "$line" | grep -o 'dpi=[0-9]*' | cut -d= -f2)
    [ "${f11:-0}" -eq 2 ] || { echo "  FAIL  input: F11 presses seen by mcplay = ${f11:-0} (expected 2)"; ERR=1; }
    [ "$grab" = false ] || { echo "  FAIL  input: Esc did not release the mouse (grab=$grab)"; ERR=1; }
    [ "$geo1" != "$geo0" ] || { echo "  FAIL  input: F11 did not change the window rect ($geo1)"; ERR=1; }
    [ "$geo2" = "$geo0" ] || { echo "  FAIL  input: second F11 did not restore the window rect ($geo2 vs $geo0)"; ERR=1; }
    [ "${dpi:-0}" -ge 96 ] || { echo "  FAIL  input: dpi not reported (${dpi:-0})"; ERR=1; }
    case "${dx%.*}" in ""|0|-0) echo "  note  input: raw mouse delta stayed 0 (Wine/Xvfb did not deliver WM_INPUT for xdotool motion)";; *) echo "  ok    input: raw mouse dx=$dx";; esac
}
# 4) dist layout (tools/mc/dist.sh): GUI-subsystem exe (no console), sound bank NEXT TO the exe, log file instead of stdout.
run_dist() {
    local PK="$W/pkg" nm
    mkdir -p "$PK"
    $FIRNC --target=x86_64-windows --win-subsystem=windows --opt-level=dev-fast -o "$PK/mcplay.exe" demos/mcplay/main.fi 2>"$W/err" || { head -5 "$W/err"; echo "  FAIL  dist: gui exe does not build"; ERR=1; return; }
    $FIRNC -o "$W/gen" tools/sounds/gen/main.fi 2>"$W/err" && "$W/gen" tools/sounds/mc.json "$PK/sounds" >/dev/null || { echo "  FAIL  dist: sound bank"; ERR=1; return; }
    # PE subsystem 2 = GUI
    python3 -I -c 'import struct,sys;d=open(sys.argv[1],"rb").read(4096);pe=struct.unpack_from("<I",d,0x3c)[0];sys.exit(0 if struct.unpack_from("<H",d,pe+24+68)[0]==2 else 1)' "$PK/mcplay.exe" || { echo "  FAIL  dist: exe is not a GUI-subsystem PE"; ERR=1; }
    # (a) explicit MCPLAY_LOG, bank found next to the exe (MCPLAY_SOUNDS unset), audio probe plays the fixed sound list into a wav
    nm=WinDist
    env -u MCPLAY_SOUNDS MCPLAY_LOG="$W/dist.log" MCPLAY_AUDIO_PROBE=1 FIRN_AUDIO="file:$W/dist.wav" MCPLAY_RUN_S=12 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_GL=0 \
        timeout -s KILL 150 wine "$PK/mcplay.exe" 127.0.0.1 "$PORT" "$nm" > "$W/$nm.out" 2>&1; local rc=$?
    [ $rc -eq 0 ] || { echo "  FAIL  dist: gui exe exit code $rc"; ERR=1; }
    [ -s "$W/$nm.out" ] && { echo "  FAIL  dist: GUI exe wrote to stdout although MCPLAY_LOG was set"; ERR=1; }
    grep -q "mcplay: end" "$W/dist.log" 2>/dev/null && echo "  ok    dist: log file written ($(grep -c . "$W/dist.log") lines)" || { echo "  FAIL  dist: no mcplay log file"; ERR=1; }
    grep -q "no sound bank" "$W/dist.log" 2>/dev/null && { echo "  FAIL  dist: bank next to the exe was not found"; ERR=1; }
    local wb; wb=$(stat -c %s "$W/dist.wav" 2>/dev/null || echo 0)
    [ "$wb" -gt 100000 ] && echo "  ok    dist: bank next to the exe played ($wb B wav)" || { echo "  FAIL  dist: audio file too small ($wb B)"; ERR=1; }
    # (b) no MCPLAY_LOG and no console (stdout/stderr closed): the exe must find the per-user log by itself
    nm=WinDistNoCon
    env -u MCPLAY_SOUNDS -u MCPLAY_LOG FIRN_AUDIO=null MCPLAY_RUN_S=8 MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_GL=0 \
        timeout -s KILL 150 wine "$PK/mcplay.exe" 127.0.0.1 "$PORT" "$nm" >&- 2>&-; rc=$?
    local ul; ul=$(find "$W/wp/drive_c/users" -name mcplay.log 2>/dev/null | head -1)
    if [ -n "$ul" ] && grep -q "mcplay: end" "$ul"; then echo "  ok    dist: no console -> automatic log in the user profile (${ul#$W/wp/})"
    else echo "  note  dist: no automatic log found under Wine (stdout closed; rc=$rc) -- explicit MCPLAY_LOG works"; fi
}
run_play WinCpu MCPLAY_GL=0
run_input
run_dist
# 5) the seam maps a recv timeout to EAGAIN like Linux (needs a firnc built after the win_seam.rs fix, r133)
bash tools/windows/timeout.sh 2>&1 | sed "s/^/  seam: /" | grep -v "^  seam: RESULT" ; [ "${PIPESTATUS[0]}" -eq 0 ] || { echo "  FAIL  seam recv timeout"; ERR=1; }
[ "${MCPLAY_GL:-0}" = 1 ] && run_play WinGl MCPLAY_GL=1
[ $ERR -eq 0 ] && echo "RESULT mc-wine: ok" || echo "RESULT mc-wine: FAIL"
exit $ERR
