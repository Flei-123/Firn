#!/usr/bin/env bash
# tools/vspy/check_sys.sh -- diff the Firn output of the vspy-sys tasks against the Python output.
# Usage: FIRNLIB=<wt>/lib bash tools/vspy/check_sys.sh   (needs firnc built; timestamps masked for 18)
set -u
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$PWD/lib}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
rc=0
M='s/^[0-9-]+ [0-9:,]+ /TS /'
for d in 03_find_rename 09_cli_args 14_timer_scheduler 15_parallel_map_files 18_logging; do
    D=examples/vs_python/$d
    "$FIRNC" -o "$T/$d" "$D/task.fi" || { echo "FAIL compile $d"; rc=1; continue; }
    "$T/$d" 2>/dev/null | sed -E "$M" > "$T/fi.txt"
    python3 "$D/task.py" 2>/dev/null | sed -E "$M" > "$T/py.txt"
    if cmp -s "$T/fi.txt" "$T/py.txt" && cmp -s "$T/fi.txt" "$D/expect.txt"; then echo "ok   $d"; else echo "FAIL $d"; rc=1; fi
done
exit $rc
