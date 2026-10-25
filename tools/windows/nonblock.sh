#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/windows/nonblock.sh -- F_GETFL shows O_NONBLOCK for socket(SOCK_NONBLOCK) and accept4(SOCK_NONBLOCK) only: Linux and Windows (Wine) must both say `NB 2048 0 2048 0`.
# Env: FIRNC (default compiler/target/release/firnc), WINE.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
WORK=$(mktemp -d)
WINE=${WINE:-}
if [ -z "$WINE" ]; then for c in wine64 wine /usr/lib/wine/wine64; do command -v "$c" >/dev/null 2>&1 && { WINE=$c; break; }; done; fi
export WINEPREFIX="$WORK/wp" WINEDEBUG=-all WINEARCH=win64
trap 'WINEPREFIX="$WORK/wp" wineserver -k 2>/dev/null; rm -rf "$WORK"' EXIT
rc=0
"$FIRNC" --target=x86_64-linux -o "$WORK/t.lin" tools/windows/nonblock.fi >"$WORK/l.log" 2>&1 || { head -5 "$WORK/l.log"; echo "FAIL linux build"; exit 1; }
LOUT=$(timeout 30 "$WORK/t.lin"  2>&1)
[ "$LOUT" = "NB 2048 0 2048 0" ] && echo "  OK    linux: $LOUT" || { echo "  FAIL  linux: '$LOUT'"; rc=1; }
if [ -n "$WINE" ] && command -v x86_64-w64-mingw32-as >/dev/null; then
    "$FIRNC" --target=x86_64-windows -o "$WORK/t.exe" tools/windows/nonblock.fi >"$WORK/w.log" 2>&1 || { head -5 "$WORK/w.log"; echo "FAIL windows build"; exit 1; }
    WOUT=$(timeout 90 "$WINE" "$WORK/t.exe"  2>/dev/null | tr -d '\r')
    [ "$WOUT" = "NB 2048 0 2048 0" ] && echo "  OK    windows under wine: $WOUT" || { echo "  FAIL  windows under wine: '$WOUT'"; rc=1; }
else
    echo "  SKIP  windows (no wine or mingw)"
fi
[ $rc -eq 0 ] && echo "RESULT win-nonblock: ok" || echo "RESULT win-nonblock: FAIL"
exit $rc
