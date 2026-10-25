#!/bin/bash
# Build the screen apps (examples/fui_apps/*) for Windows into a folder: <app>.exe (Inter and mono face embedded) + data/ (sample data) + licenses/.
# usage: [OPT=release-fast|release-safe|dev] pack.sh <out dir> [app ...]     default apps: flei settings notes dash chat form
set -u
R=$(cd "$(dirname "$0")/../../.." && pwd)
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
OPT=${OPT:-release-fast}
OUT=${1:?out dir}
shift
APPS=${*:-flei settings notes dash chat form}
mkdir -p "$OUT/data" "$OUT/licenses"
cp "$R"/LICENSES/Bitstream-Vera.txt "$R"/LICENSES/Inter-OFL.txt "$OUT/licenses/"
cp "$R"/examples/fui_apps/data/* "$OUT/data/"
rc=0
for a in $APPS; do
    FIRNLIB=$R/lib $FIRNC --opt-level=$OPT --target=x86_64-windows -o "$OUT/$a.exe" "$R/examples/fui_apps/$a/main.fi" || rc=1
done
exit $rc
