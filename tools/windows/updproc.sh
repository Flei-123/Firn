#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/windows/updproc.sh -- window.update_run/update_poll/restart_self (lib/window/win32_proc.fi) under Wine: the probe starts "<name>-update.exe" next to itself and reads its exit code.
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
if [ -z "$WINE" ] || ! command -v x86_64-w64-mingw32-as >/dev/null; then
    echo "  SKIP  no wine or mingw"
    exit 0
fi
mkdir -p "$WORK/bin"
"$FIRNC" --no-cache --target=x86_64-windows -o "$WORK/bin/probe.exe" tools/windows/updproc/probe.fi >"$WORK/p.log" 2>&1 || { head -8 "$WORK/p.log"; echo "FAIL probe build"; exit 1; }
"$FIRNC" --no-cache --target=x86_64-windows -o "$WORK/bin/probe-update.exe" tools/windows/updproc/updater.fi >"$WORK/u.log" 2>&1 || { head -8 "$WORK/u.log"; echo "FAIL updater build"; exit 1; }
timeout 120 "$WINE" "$WORK/bin/probe.exe" >/dev/null 2>&1
rc=$?
if [ $rc -eq 0 ]; then
    echo "  OK    update_run/update_poll under wine (exit codes 10 and 10, busy and idle states)"
else
    echo "  FAIL  probe exit code $rc"
fi
printf 'fn main() -> i32 {\n    return 10\n}\n' > "$WORK/plain.fi"
"$FIRNC" --no-cache --target=x86_64-windows -o "$WORK/bin/probe-update.exe" "$WORK/plain.fi" >"$WORK/n.log" 2>&1 || { head -8 "$WORK/n.log"; echo "FAIL plain updater build"; exit 1; }
timeout 120 "$WINE" "$WORK/bin/probe.exe" >/dev/null 2>&1
rc2=$?
if [ $rc2 -eq 6 ]; then
    echo "  OK    an updater without the protocol mark is refused for fetch (exit 6)"
else
    echo "  FAIL  counter-probe exit code $rc2 (expected 6)"
    rc=1
fi
[ $rc -eq 0 ] && echo "RESULT win-updproc: ok" || echo "RESULT win-updproc: FAIL"
exit $rc
