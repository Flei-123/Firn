#!/usr/bin/env bash
# tools/soup_cross/run.sh -- lib/html/soup.fi against bs4 (tree from html5lib, selectors by soupsieve)
# in all four build levels, plus a garbage/fuzz run that must not crash.
#   SOUP_CROSS_N=2000 bash tools/soup_cross/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
N=${SOUP_CROSS_N:-300}
FZ=${SOUP_FUZZ_N:-600}
python3 -c "import bs4, html5lib, soupsieve" >/dev/null 2>&1 || { echo "  SKIP  python3 bs4/html5lib/soupsieve missing"; exit 0; }
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-soupcross.XXXXXX")
trap 'rm -rf "$W"' EXIT
python3 -I tools/soup_cross/cross.py gen "$N" 3 "$W/in.bin" || exit 1
python3 -I tools/soup_cross/cross.py fuzz "$FZ" 7 "$W/fuzz.bin" || exit 1
ERRORS=0
for st in release-fast:--opt-level=release-fast no-opt:--no-opt dev-fast:--opt-level=dev-fast release-safe:--opt-level=release-safe; do
    name=${st%%:*}; flag=${st#*:}
    if ! $FIRNC $flag -o "$W/cli.$name" tools/soup_cross/soup_cli.fi > "$W/cc.$name" 2>&1; then
        echo "  FAIL  soup_cli does not compile [$name]"; head -5 "$W/cc.$name"; ERRORS=$((ERRORS + 1)); continue
    fi
    "$W/cli.$name" < "$W/in.bin" > "$W/out.$name" || { echo "  FAIL  soup_cli crashed [$name]"; ERRORS=$((ERRORS + 1)); continue; }
    python3 -I tools/soup_cross/cross.py check "$W/in.bin" "$W/out.$name" || ERRORS=$((ERRORS + 1))
    "$W/cli.$name" < "$W/fuzz.bin" > "$W/fz.$name" || { echo "  FAIL  soup_cli crashed on fuzz input [$name]"; ERRORS=$((ERRORS + 1)); }
done
[ "$ERRORS" -eq 0 ] && echo "soup_cross: ok" || echo "soup_cross: $ERRORS FAILED"
exit $((ERRORS > 0))
