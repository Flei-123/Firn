#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/windows/wincred.sh -- std.secret on the Windows target (x86_64-windows),
# under Wine: the Credential Manager roundtrip of tools/windows/wincred.fi.
# Wine implements CredWriteW/CredReadW/CredDeleteW in advapi32; a real Windows
# run is the same program (see wincred.fi).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="${FIRNC:-$ROOT/compiler/target/release/firnc}"
export LC_ALL=${LC_ALL:-C.UTF-8}
export WINEPREFIX=${WINEPREFIX:-${HOME:-$(getent passwd "$(id -u)" | cut -d: -f6)}/.wine-firn}
export WINEDEBUG=${WINEDEBUG:--all}
WINE=${WINE:-}
if [ -z "$WINE" ]; then
    for c in wine64 wine /usr/lib/wine/wine64; do
        if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then WINE=$c; break; fi
    done
fi
for t in x86_64-w64-mingw32-as x86_64-w64-mingw32-ld; do
    command -v "$t" >/dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done
[ -n "$WINE" ] || { echo "SKIP: wine is missing"; exit 0; }
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
"$FIRNC" --target=x86_64-windows -o "$W/wincred.exe" tools/windows/wincred.fi > "$W/build.log" 2>&1 \
    || { echo "FAIL wincred.exe does not build"; grep -v RWX "$W/build.log" | head -8; exit 1; }
out=$(timeout 120 "$WINE" "$W/wincred.exe" 2>&1); rc=$?
echo "$out" | tail -8
[ $rc -eq 0 ] && echo "$out" | grep -q ' 0 failed' || { echo "FAIL wincred (exit $rc)"; exit 1; }
echo "wincred ok"
