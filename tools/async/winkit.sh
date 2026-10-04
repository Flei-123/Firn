#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/async/winkit.sh -- a KIT that runs the async IO tests on ANOTHER machine.
#
#   bash tools/async/winkit.sh [windows|linux] [out.zip]
#
# This server cannot put binaries on a real Windows PC (FLEI-ONE is not
# reachable from here), so the kit is a zip that needs ONLY Python 3 there:
#   t2120..t2125  the in-process tests (loop, streams, TLS, HTTP, WebSocket, posting)
#   conn          the connection test program (scaled down by run.py)
#   wss           the WebSocket client against real wss:// echo services
#   run.py        runs all of it and says what passed, failed and was skipped
# Unzip, `py run.py` (Windows) or `python3 run.py` (Linux).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
TARGET=${1:-windows}
OUT=${2:-$ROOT/build/async-$TARGET-kit.zip}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB=$ROOT/lib
case "$TARGET" in
    windows) EXT=.exe; TF="--target=x86_64-windows" ;;
    linux) EXT=""; TF="" ;;
    *) echo "target: windows or linux"; exit 2 ;;
esac
W=$(mktemp -d "${TMPDIR:-/tmp}/async-winkit.XXXXXX")
trap 'rm -rf "$W"' EXIT
K=$W/async-kit
mkdir -p "$K" "$(dirname "$OUT")"
for t in 2120_async_loop 2121_async_stream 2122_async_tls 2123_async_http 2124_async_ws 2125_async_post; do
    "$FIRNC" $TF --opt-level=dev-fast -o "$K/t$t$EXT" "tests/$t.fi" 2>"$W/b.err" || { echo "build failed: $t"; grep -v RWX "$W/b.err" | head -5; exit 1; }
done
"$FIRNC" $TF --opt-level=dev-fast -o "$K/conn$EXT" tools/async/conn_main.fi 2>"$W/b.err" || { echo "build failed: conn"; exit 1; }
"$FIRNC" $TF --opt-level=dev-fast -o "$K/wss$EXT" tools/async/wss_main.fi 2>"$W/b.err" || { echo "build failed: wss"; exit 1; }
cp tools/async/winkit/run.py "$K/run.py"
cat > "$K/README.txt" <<'EOT'
Async IO kit (Firn lib/async). Needs only Python 3.

    Windows:  py run.py
    Linux:    python3 run.py

It starts programs from this folder; they listen on 127.0.0.1 only and
connect to two public WebSocket echo services (echo.websocket.org,
ws.ifelse.io) to check TLS with this machine's own certificate roots.
"unreachable" (no network, a firewall) is reported as a skip.
The output is the answer; please send it back.
EOT
rm -f "$OUT"
( cd "$W" && python3 -c "
import sys, zipfile, os
z = zipfile.ZipFile(sys.argv[1], 'w', zipfile.ZIP_DEFLATED)
for root, dirs, files in os.walk('async-kit'):
    for f in files:
        z.write(os.path.join(root, f))
z.close()" "$OUT" )
ls -l "$OUT"
