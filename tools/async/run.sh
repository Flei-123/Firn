#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/async/run.sh -- the event loop, held against the outside world.
#
#   1. a thousand connections on one loop (tools/async/check_conn.py): the
#      Firn client against Python asyncio, Python asyncio against the Firn
#      server, both in Firn on epoll and on poll
#   2. a window and the loop in one thread (tools/async/check_ui.py, Xvfb +
#      python-xlib): network, worker-thread and timer results reach the UI
#      thread with no window event; an idle second costs no CPU; a window
#      event is seen at once
#   3. the WebSocket client against REAL wss:// echo services (system roots,
#      DNS by name, TLS 1.3): echo of text and fragmented binary, ping/pong,
#      the closing handshake. Skipped (said so) when there is no route.
#   4. the Windows build of the same, under Wine: the in-process tests, and
#      the thousand-connection run scaled down to what the seam's select()
#      allows. Skipped when Wine or mingw is missing.
#
# The in-process half is tests/2120 .. tests/2125 (test.sh runs them).
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0

build() {   # build <out> <src> [flags]
    local out=$1 src=$2; shift 2
    if ! "$FIRNC" "$@" -o "$out" "$src" > "$WORK/b.log" 2>&1; then
        echo "  FAIL $src does not build"; grep -v RWX "$WORK/b.log" | head -5; rc=1; return 1
    fi
}

build "$WORK/conn" tools/async/conn_main.fi --opt-level=release-fast || exit 1
build "$WORK/ui_probe" tools/async/ui_probe.fi --opt-level=dev-fast || exit 1
build "$WORK/wss" tools/async/wss_main.fi --opt-level=release-fast || exit 1
echo "   conn_main, ui_probe, wss_main built"

echo "== 1. a thousand connections =="
python3 tools/async/check_conn.py "$WORK/conn" 1000 || rc=1

echo "== 2. a window and the loop in one thread (Xvfb) =="
python3 tools/async/check_ui.py "$WORK/ui_probe" || rc=1

echo "== 3. real wss:// echo services =="
reached=0
for u in wss://echo.websocket.org/ wss://ws.ifelse.io/; do
    out=$(timeout 60 "$WORK/wss" "$u" 2>&1); code=$?
    if [ $code -eq 0 ]; then
        echo "  ok   $u : $(echo "$out" | tail -1)"; reached=$((reached + 1))
    elif [ $code -eq 3 ]; then
        echo "  skip $u : not reachable from here"
    else
        echo "  FAIL $u (exit $code)"; echo "$out" | sed 's/^/        /'; rc=1
    fi
done
[ $reached -gt 0 ] || echo "  (no wss service reached: nothing was checked here)"

echo "== 4. Windows build under Wine =="
WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi
if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
    export WINEDEBUG=${WINEDEBUG:--all}
    WB=1
    for t in 2120_async_loop 2121_async_stream 2122_async_tls 2123_async_http 2124_async_ws; do
        if "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$WORK/$t.exe" "tests/$t.fi" > "$WORK/w.log" 2>&1; then
            if timeout 300 "$WINE" "$WORK/$t.exe" > "$WORK/w.out" 2>&1; then echo "  ok   tests/$t.fi (Windows, Wine)"
            else echo "  FAIL tests/$t.fi (Windows, Wine)"; head -5 "$WORK/w.out"; rc=1; fi
        else
            echo "  FAIL tests/$t.fi does not build for Windows"; grep -v RWX "$WORK/w.log" | head -3; rc=1
        fi
    done
    if "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$WORK/conn.exe" tools/async/conn_main.fi > "$WORK/w.log" 2>&1; then
        out=$(timeout 120 "$WINE" "$WORK/conn.exe" inproc 28 2>&1 | tr -d '\r' | head -1)
        case "$out" in
            "poll OK 28 28"*) echo "  ok   28 connections + 28 server sockets in one loop (select backend): $out" ;;
            *) echo "  FAIL conn_main inproc 28 under Wine: $out"; rc=1 ;;
        esac
    else
        echo "  FAIL conn_main does not build for Windows"; rc=1
    fi
else
    echo "   SKIP: Wine or mingw is missing"
fi
[ $rc -eq 0 ] && echo "ASYNC PASSED" || echo "ASYNC FAILED"
exit $rc
