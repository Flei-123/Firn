#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/windows/timeout.sh -- recv() with SO_RCVTIMEO against a silent server: Linux and Windows (Wine) must both say `RECV -11` (EAGAIN).
# Env: FIRNC (default compiler/target/release/firnc), WINE.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB="$ROOT/lib"
WORK=$(mktemp -d)
WINE=${WINE:-}
if [ -z "$WINE" ]; then for c in wine64 wine /usr/lib/wine/wine64; do command -v "$c" >/dev/null 2>&1 && { WINE=$c; break; }; done; fi
export WINEPREFIX="$WORK/wp" WINEDEBUG=-all WINEARCH=win64
cat > "$WORK/server.py" <<'PY'
import socket, sys, time
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", int(sys.argv[1]))); srv.listen(8)
print("ready", flush=True)
conns = []
end = time.time() + 90
srv.settimeout(1)
while time.time() < end:
    try: conns.append(srv.accept()[0])
    except Exception: pass
PY
PORT=$(( 20000 + (RANDOM % 20000) ))
python3 "$WORK/server.py" "$PORT" > "$WORK/server.log" 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null; WINEPREFIX="$WORK/wp" wineserver -k 2>/dev/null; rm -rf "$WORK"' EXIT
for _ in $(seq 1 50); do grep -q ready "$WORK/server.log" 2>/dev/null && break; sleep 0.1; done
rc=0
"$FIRNC" --target=x86_64-linux -o "$WORK/t.lin" tools/windows/timeout.fi >"$WORK/l.log" 2>&1 || { head -5 "$WORK/l.log"; echo "FAIL linux build"; exit 1; }
LOUT=$(timeout 30 "$WORK/t.lin" "$PORT" 2>&1)
[ "$LOUT" = "RECV -11" ] && echo "  OK    linux: $LOUT" || { echo "  FAIL  linux: '$LOUT'"; rc=1; }
if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-as >/dev/null; then
    "$FIRNC" --target=x86_64-windows -o "$WORK/t.exe" tools/windows/timeout.fi >"$WORK/w.log" 2>&1 || { head -5 "$WORK/w.log"; echo "FAIL windows build"; exit 1; }
    WOUT=$(timeout 90 "$WINE" "$WORK/t.exe" "$PORT" 2>/dev/null | tr -d '\r')
    [ "$WOUT" = "RECV -11" ] && echo "  OK    windows under wine: $WOUT" || { echo "  FAIL  windows under wine: '$WOUT' (seam must map WSAETIMEDOUT of recv to EAGAIN)"; rc=1; }
else
    echo "  SKIP  windows (no wine or mingw)"
fi
[ $rc -eq 0 ] && echo "RESULT win-timeout: ok" || echo "RESULT win-timeout: FAIL"
exit $rc
