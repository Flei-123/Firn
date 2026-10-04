#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/download/run.sh -- the download manager (lib/net/download.fi).
#
#   1. tools/download/dl_main.fi builds in three stages (opt, --no-opt, dev-fast)
#   2. tools/download/check.py: the manager against tools/download/fake_server.py
#      (resume, retry, backoff, ETag, cancel, rate limit, keep-alive, ...) and,
#      when there is a route to the internet, against the real Mojang CDN
#   3. the Windows build under Wine, the same checks (sections A, C, D, E, F, I)
#
# No route to the internet: section J skips, the hermetic ones run.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
export FIRNLIB="$(pwd)/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
rc=0
for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
    NAME=${STAGE%%:*}
    OPT=${STAGE#*:}
    if ! "$FIRNC" $OPT -o "$WORK/dl_$NAME" tools/download/dl_main.fi 2> "$WORK/b.log"; then
        echo "  FAIL dl_main does not compile ($NAME)"
        grep -v RWX "$WORK/b.log" | head -5
        rc=1
    fi
done
[ $rc -eq 0 ] || exit 1
echo "   dl_main built: opt, --no-opt, dev-fast"
python3 tools/download/check.py "$WORK/dl_opt" | grep -v "^  OK" || true
python3 tools/download/check.py "$WORK/dl_opt" > "$WORK/opt.log" 2>&1 || rc=1
tail -1 "$WORK/opt.log"
echo "== the same checks, --no-opt and dev-fast builds (hermetic sections) =="
for NAME in noopt dev; do
    python3 tools/download/check.py "$WORK/dl_$NAME" a c d e f g i > "$WORK/$NAME.log" 2>&1 || { rc=1; grep -v "^  OK" "$WORK/$NAME.log" | head -20; }
    echo "   $NAME: $(tail -1 "$WORK/$NAME.log")"
done

WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi
if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-ld >/dev/null 2>&1; then
    echo "== the Windows build (x86_64-windows) under Wine =="
    export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
    export WINEDEBUG=${WINEDEBUG:--all}
    if "$FIRNC" --target=x86_64-windows --opt-level=dev-fast -o "$WORK/dl_main.exe" tools/download/dl_main.fi 2> "$WORK/w.log"; then
        ROOT=$(pwd)
        (cd "$WORK" && RUNNER="$WINE" python3 "$ROOT/tools/download/check.py" "$WORK/dl_main.exe" a c d e f i > "$WORK/win.log" 2>&1) || { rc=1; grep -v "^  OK" "$WORK/win.log" | head -20; }
        echo "   windows: $(tail -1 "$WORK/win.log")"
    else
        echo "  FAIL dl_main does not build for Windows"
        grep -v RWX "$WORK/w.log" | head -5
        rc=1
    fi
else
    echo "   SKIP the Windows build: Wine or mingw is missing"
fi
exit $rc
