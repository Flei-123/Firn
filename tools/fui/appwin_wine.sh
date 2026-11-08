#!/bin/bash
set -u
R=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
D=$(mktemp -d)
export WINEPREFIX=$D/wp WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml=d"
cleanup() {
  /usr/lib/wine/wine64 wineboot -k >/dev/null 2>&1
  wineserver -k >/dev/null 2>&1
  sleep 1
  rm -rf "$D"
}
trap cleanup EXIT
command -v xvfb-run >/dev/null 2>&1 || { echo "appwin_wine: xvfb-run missing"; exit 77; }
cp "$R/tools/fui/appwin_live_main.fi" "$D/live_main.fi"
FIRNLIB=$R/lib $FIRNC --opt-level=dev --target=x86_64-windows -o "$D/live.exe" "$D/live_main.fi" || exit 2
cat > "$D/go.sh" <<'INNER'
/usr/lib/wine/wine64 wineboot -i >/dev/null 2>&1
timeout 60 /usr/lib/wine/wine64 "$1" 2>&1
INNER
timeout 150 xvfb-run -a -s "-screen 0 1280x800x24" bash "$D/go.sh" "$D/live.exe" >"$D/out.txt"
cat "$D/out.txt"
grep -q "child opened" "$D/out.txt" && grep -q "main quit" "$D/out.txt" && grep -q "child closed" "$D/out.txt"
