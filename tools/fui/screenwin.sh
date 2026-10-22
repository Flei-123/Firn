#!/bin/bash
# The screen apps on Windows: cross-built for x86_64-windows, run under Wine in --shot mode, and the PNG must be byte-identical to the Linux one.
# FIRNC defaults to the main compiler; the temp dir is removed on exit.
set -u
R=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
APPS=${APPS:-"settings form"}
OPT=dev bash "$R/tools/fui/screenwin/pack.sh" "$D/win" $APPS || exit 2
export WINEPREFIX=$D/wp WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=d" DISPLAY=
bad=0
for a in $APPS; do
    FIRNLIB=$R/lib $FIRNC --opt-level=dev -o "$D/$a" "$R/examples/fui_apps/$a/main.fi" || exit 2
    (cd "$D" && FIRNLIB=$R/lib ./$a --theme dark --shot "$D/$a.lin.png") || bad=1
    (cd "$D/win" && timeout 150 /usr/lib/wine/wine64 $a.exe --theme dark --shot "$D/$a.win.png") >/dev/null 2>&1 || bad=1
    if cmp -s "$D/$a.lin.png" "$D/$a.win.png"; then echo "SCREENWIN $a identical"; else echo "SCREENWIN $a DIFFERS"; bad=1; fi
done
echo "SCREENWIN bad=$bad"
[ "$bad" = 0 ]
