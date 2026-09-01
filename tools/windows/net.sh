#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/windows/net.sh -- the network proof of round WINDOWS.
#
# A server of six lines of Python answers with a FIXED reply, so that the
# comparison is a comparison and not a measurement of somebody's Date
# header. The same Firn client (tools/windows/net.fi) is built twice and
# both builds have to say the same sentence:
#
#   1032 FIRN-OK
#
# The Linux build reaches the server through `socket`/`connect`/`sendto`/
# `recvfrom`; the Windows build reaches it through `ws2_32.dll`, because the
# seam turned the very same numbers into those calls. Nothing in the SOURCE
# knows the difference -- that is the claim being tested.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.win-net"
export WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn}
export WINEDEBUG=${WINEDEBUG:--all}
WINE=${WINE:-/usr/lib/wine/wine64}

rm -rf "$WORK"; mkdir -p "$WORK"
pass=0; fail=0
ok()   { echo "  OK    $1"; pass=$((pass+1)); }
bad()  { echo "  FAIL  $1"; fail=$((fail+1)); }

# The reply: 8 octets of greeting plus 1024 octets of filling = 1032 octets,
# every time, whatever the day and whatever the Python version.
cat > "$WORK/server.py" <<'PY'
import socket, sys, threading
port = int(sys.argv[1])
body = b"FIRN-OK\n" + b"A" * 1024
srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", port))
srv.listen(8)
print("ready", flush=True)
def serve():
    while True:
        c, _ = srv.accept()
        c.recv(4096)
        c.sendall(body)
        c.close()
threading.Thread(target=serve, daemon=True).start()
import time
time.sleep(120)
PY

PORT=$(( 20000 + (RANDOM % 20000) ))
python3 "$WORK/server.py" "$PORT" > "$WORK/server.log" 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null' EXIT
for _ in $(seq 1 50); do
    grep -q ready "$WORK/server.log" 2>/dev/null && break
    sleep 0.1
done

EXPECT="1032 FIRN-OK"

# --- Linux ---------------------------------------------------------------
if "$FIRNC" --target=x86_64-linux -o "$WORK/net.lin" tools/windows/net.fi > "$WORK/lin.log" 2>&1; then
    LOUT=$("$WORK/net.lin" "$PORT" 2>&1); LRC=$?
    if [ "$LRC" -eq 0 ] && [ "$LOUT" = "$EXPECT" ]; then
        ok "linux: $LOUT"
    else
        bad "linux: exit $LRC, '$LOUT' (expected '$EXPECT')"
    fi
else
    bad "linux: build failed"; sed -n '1,5p' "$WORK/lin.log"
fi

# --- Windows -------------------------------------------------------------
if "$FIRNC" --target=x86_64-windows -o "$WORK/net.exe" tools/windows/net.fi > "$WORK/win.log" 2>&1; then
    ok "windows: the .exe was built"
    if x86_64-w64-mingw32-objdump -p "$WORK/net.exe" 2>/dev/null | grep -qi 'DLL Name: WS2_32.dll'; then
        ok "windows: the image really imports WS2_32.dll"
    else
        bad "windows: WS2_32.dll is not in the import table"
    fi
    WOUT=$(timeout 120 "$WINE" "$WORK/net.exe" "$PORT" 2>/dev/null); WRC=$?
    if [ "$WRC" -eq 0 ] && [ "$WOUT" = "$EXPECT" ]; then
        ok "windows under wine: $WOUT"
    else
        bad "windows under wine: exit $WRC, '$WOUT' (expected '$EXPECT')"
    fi
    if [ "$WOUT" = "$LOUT" ]; then
        ok "both operating systems say the same thing"
    else
        bad "linux='$LOUT' windows='$WOUT'"
    fi
else
    bad "windows: build failed"; sed -n '1,8p' "$WORK/win.log"
fi

echo
echo "  passed: $pass   failed: $fail"
[ "$fail" -eq 0 ] || exit 1
