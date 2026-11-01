#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/mc/play/run_join_fn.sh <version> <protocol> -- mcplay joins a Firn server (firnmc-server --protocol N) (Xvfb, CPU raster),
# needs chunks > 0, frames > 0, a non-trivial picture. Prints `RESULT mc-play-join <version>: ok`.
# Env: JAR (server jar), JAVA, FIRNC, MC_VIEW (4), MCPLAY_PNG_OUT (screenshot copy), MCPLAY_GFX (default cpu raster), RUN_S (12).
set -uo pipefail
cd "$(dirname "$0")/../../.."
VER=${1:?version}
PROTO=${2:?protocol}
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
VIEW=${MC_VIEW:-4}
W=$(mktemp -d); SRV_PID=""; XV_PID=""
cleanup() { exec 3>&- 2>/dev/null; [ -n "$XV_PID" ] && kill "$XV_PID" 2>/dev/null; [ -n "$SRV_PID" ] && kill "$SRV_PID" 2>/dev/null && sleep 1 && kill -9 "$SRV_PID" 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
fail() { echo "  FAIL  $1"; echo "RESULT mc-play-join $VER: FAIL"; exit 1; }
for t in Xvfb python3; do command -v $t >/dev/null || fail "missing tool: $t"; done
$FIRNC --opt-level=dev-fast -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "demos/mcplay does not compile"; }
$FIRNC --opt-level=dev-fast -o "$W/fmc" demos/firnmc-server/main.fi 2>"$W/err" || { head -10 "$W/err"; fail "firnmc-server does not compile"; }
mkdir -p "$W/srv"
PORT=$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')
mkfifo "$W/in"; exec 3<>"$W/in"
( cd "$W/srv" && exec "$W/fmc" nogui --protocol "$PROTO" --port "$PORT" < "$W/in" > "$W/server.log" 2>&1 ) &
SRV_PID=$!
for i in $(seq 1 60); do grep -q 'Done (' "$W/server.log" 2>/dev/null && break; kill -0 "$SRV_PID" 2>/dev/null || break; sleep 1; done
grep -q 'Done (' "$W/server.log" || { tail -10 "$W/server.log"; fail "server did not start"; }
DN=$(bash tools/mc/pick_display.sh 90 100)
Xvfb :$DN -screen 0 1024x600x24 >"$W/xvfb.log" 2>&1 & XV_PID=$!
sleep 2
export DISPLAY=:$DN
cat > "$W/pix.py" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
print(len(set(im.crop((0, 60, 640, 360)).getdata())))
PY
env MCPLAY_PROTOCOL=$PROTO MCPLAY_GL=0 MCPLAY_VSYNC=0 MCPLAY_AUDIO=0 MCPLAY_RUN_S=${RUN_S:-12} MCPLAY_VIEW=$VIEW MCPLAY_FPS=30 MCPLAY_W=640 MCPLAY_H=360 MCPLAY_SHOT="$W/shot.png" \
    timeout 240 "$W/mcplay" 127.0.0.1 "$PORT" Player > "$W/play.out" 2>&1
rc=$?
tail -8 "$W/play.out" | sed 's/^/  /'
grep -E "lost connection|Disconnect|Kick|rror|xception|Failed" "$W/server.log" | tail -4 | cut -c1-300 | sed 's/^/  srv: /'
ERR=0
[ $rc -eq 0 ] || { echo "  FAIL  mcplay exit code $rc"; ERR=1; }
grep -q "Player joined the game" "$W/server.log" || { echo "  FAIL  server never saw the join"; ERR=1; }
FR=$(grep -o 'frames=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2); CH=$(grep -o 'chunks=[0-9]*' "$W/play.out" | head -1 | cut -d= -f2)
[ "${FR:-0}" -gt 20 ] || { echo "  FAIL  too few frames (${FR:-0})"; ERR=1; }
[ "${CH:-0}" -gt 10 ] || { echo "  FAIL  too few chunks (${CH:-0})"; ERR=1; }
COLORS=$(python3 -I "$W/pix.py" "$W/shot.png" 2>/dev/null | tail -1)
[ "${COLORS:-0}" -ge 40 ] || { echo "  FAIL  picture has too few colors (${COLORS:-0})"; ERR=1; }
[ -n "${MCPLAY_PNG_OUT:-}" ] && cp "$W/shot.png" "$MCPLAY_PNG_OUT" 2>/dev/null
echo "  info  frames=${FR:-0} chunks=${CH:-0} colors=${COLORS:-0}"
if [ $ERR -eq 0 ]; then echo "RESULT mc-play-join $VER: ok"; else echo "RESULT mc-play-join $VER: FAIL"; exit 1; fi
