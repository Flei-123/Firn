#!/bin/bash
# Build tools/fui/uiatextcom_main.fi for Windows and run it under Wine.
# FIRNC defaults to the main compiler; the temp dir is removed on exit.
set -u
R=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$R/lib $FIRNC --opt-level=dev --target=x86_64-windows -o "$D/uiatextcom.exe" "$R/tools/fui/uiatextcom_main.fi" || exit 2
export WINEPREFIX=$D/wp WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=d"
timeout 120 /usr/lib/wine/wine64 "$D/uiatextcom.exe" 2>&1 | tee "$D/out.txt"
grep -q "UIATEXTCOM bad=0" "$D/out.txt"
