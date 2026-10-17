#!/usr/bin/env bash
# tools/textfmt_cross/run.sh -- lib/std/textfmt.fi against Python ('%.Nf', round) in all four build levels.
#   TEXTFMT_CROSS_N=20000 bash tools/textfmt_cross/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
N=${TEXTFMT_CROSS_N:-20000}
command -v python3 >/dev/null 2>&1 || { echo "  SKIP  python3 is not installed"; exit 0; }
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-textfmtcross.XXXXXX")
trap 'rm -rf "$W"' EXIT
python3 -I tools/textfmt_cross/cross.py gen "$N" 20261017 "$W/in.txt" || exit 1
ERRORS=0
for st in release-fast:--opt-level=release-fast no-opt:--no-opt dev-fast:--opt-level=dev-fast release-safe:--opt-level=release-safe; do
    name=${st%%:*}; flag=${st#*:}
    if ! $FIRNC $flag -o "$W/cli.$name" tools/textfmt_cross/textfmt_cli.fi > "$W/cc.$name" 2>&1; then
        echo "  FAIL  textfmt_cli does not compile [$name]"; head -5 "$W/cc.$name"; ERRORS=$((ERRORS + 1)); continue
    fi
    "$W/cli.$name" < "$W/in.txt" > "$W/out.$name" || { echo "  FAIL  textfmt_cli crashed [$name]"; ERRORS=$((ERRORS + 1)); continue; }
    python3 -I tools/textfmt_cross/cross.py check "$W/in.txt" "$W/out.$name" || ERRORS=$((ERRORS + 1))
done
[ "$ERRORS" -eq 0 ] && echo "textfmt_cross: ok" || echo "textfmt_cross: $ERRORS FAILED"
exit $((ERRORS > 0))
