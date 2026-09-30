#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/phone_remote/win.sh -- examples/phone_remote as a WINDOWS program.
#
# The example is built unchanged for x86_64-windows (its input backend is
# then SendInput, lib/input/backend.windows.fi) and run under Wine on a
# virtual X display. A client pairs with the printed URL exactly like the
# phone page does (GET /?t=<token> -> cookie -> WebSocket /ws) and sends
# the page's messages; the X server itself (xdotool) has to see the
# pointer move. Counter-checks: no cookie -> no WebSocket, and nothing moves.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
for t in x86_64-w64-mingw32-as x86_64-w64-mingw32-ld wine Xvfb xdotool; do
    command -v "$t" >/dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done
python3 -c 'import websockets' 2>/dev/null || { echo "SKIP: python websockets missing"; exit 0; }
W=$(mktemp -d)
PORT=$((22000 + RANDOM % 900))
DNUM=$((140 + RANDOM % 50))
cleanup() { kill "$SRV" "$XV" 2>/dev/null; wait 2>/dev/null; rm -rf "$W"; }
trap cleanup EXIT
mkdir -p "$W/shadow"
cp examples/phone_remote/index.html "$W/shadow/"
sed "s/http.app(8080)/http.app($PORT)/" examples/phone_remote/main.fi > "$W/shadow/main.fi"
"$FIRNC" --target=x86_64-windows -o "$W/remote.exe" "$W/shadow/main.fi" 2> "$W/b.log" || { cat "$W/b.log"; exit 1; }
Xvfb ":$DNUM" -screen 0 1280x800x24 >/dev/null 2>&1 &
XV=$!
sleep 1
export DISPLAY=":$DNUM" WINEDEBUG=-all WINEPREFIX="${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}"
xdotool mousemove 400 300
wine "$W/remote.exe" > "$W/out.log" 2>&1 &
SRV=$!
python3 - "$W/out.log" "$PORT" <<'PY'
import asyncio, re, subprocess, sys, time, urllib.request, urllib.error
import websockets
log, port = sys.argv[1], sys.argv[2]
ok = total = 0
def check(name, cond, detail=""):
    global ok, total
    total += 1
    ok += bool(cond)
    print(("  OK    " if cond else "  FAIL  ") + name + ("" if cond else f"  {detail}"))
def where():
    o = subprocess.run(["xdotool", "getmouselocation"], capture_output=True, text=True).stdout
    m = re.search(r"x:(\d+) y:(\d+)", o)
    return (int(m.group(1)), int(m.group(2)))
line = ""
for _ in range(600):
    try:
        line = open(log).read().split("\n")[0]
    except OSError:
        pass
    if "\n" in open(log).read():
        break
    time.sleep(0.05)
check("the .exe prints the pairing URL", re.match(r"^Open on the phone: http://[\d.]+:\d+/\?t=[0-9a-f]{32}\s*$", line), line)
tok = re.search(r"t=([0-9a-f]{32})", line).group(1)
base = f"http://127.0.0.1:{port}"
try:
    urllib.request.urlopen(base + "/")
    check("REFUSED page without token", False)
except urllib.error.HTTPError as e:
    check("REFUSED page without token -> 403", e.code == 403, e.code)
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None
try:
    r = urllib.request.build_opener(NoRedirect).open(base + "/?t=" + tok)
    code, hdrs = r.status, r.headers
except urllib.error.HTTPError as e:
    code, hdrs = e.code, e.headers
cookie = (hdrs.get("Set-Cookie") or "").split(";")[0]
check("the paired URL answers with a cookie and a redirect", code in (302, 303) and cookie, (code, cookie))
r = urllib.request.urlopen(urllib.request.Request(base + "/", headers={"Cookie": cookie}))
body = r.read().decode()
check("with the cookie the page is served", r.status == 200 and "WebSocket" in body, r.status)
async def run():
    p0 = where()
    try:
        async with websockets.connect(f"ws://127.0.0.1:{port}/ws", open_timeout=5) as w:
            await w.send("move,50,50")
        stranger = True
    except Exception:
        stranger = False
    time.sleep(0.3)
    check("REFUSED WebSocket without the cookie", not stranger)
    check("nothing moved for the stranger", where() == p0, (p0, where()))
    async with websockets.connect(f"ws://127.0.0.1:{port}/ws", additional_headers={"Cookie": cookie}, open_timeout=5) as w:
        # Wine keeps its own idea of the pointer until the first motion it
        # injects; one warm-up move syncs both, then the measurement.
        await w.send("move,1,1")
        await asyncio.sleep(0.5)
        p0 = where()
        for _ in range(5):
            await w.send("move,10,6")
        await asyncio.sleep(0.5)
        p1 = where()
        check("five touch moves (+10,+6) move the X pointer right and down", p1[0] > p0[0] and p1[1] > p0[1], (p0, p1))
        await w.send("move,-30,-20")
        await asyncio.sleep(0.5)
        p2 = where()
        check("a move back (-30,-20) moves it left and up", p2[0] < p1[0] and p2[1] < p1[1], (p1, p2))
        for m in ("click", "rclick", "scroll,-1", "vol+", "vol-", "mute", "play"):
            await w.send(m)
        await asyncio.sleep(0.3)
        await w.send("move,1,0")
        await asyncio.sleep(0.3)
        check("the server is still alive after every page message", where()[0] >= p2[0], where())
asyncio.run(run())
print(f"  passed: {ok}   failed: {total - ok}")
sys.exit(0 if ok == total else 1)
PY
