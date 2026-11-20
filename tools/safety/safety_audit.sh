#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
STEP=${STEP:-1}
JOBS=${JOBS:-4}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
cd "$ROOT"
{
    ls examples/*.fi bench/*.fi tools/*/*.fi bin/*.fi tests/stable/*.fi tests/opt/*.fi tests/diff/*.fi 2>/dev/null
    ls tests/*.fi | awk -v s="$STEP" 'NR % s == 0'
} > "$W/list"
export FIRNC W FIRNLIB="$ROOT/lib"
one() {
    n=$(echo "$1" | tr '/' '_')
    FIRN_REACH_CHECKS=0 timeout 120 "$FIRNC" --send-check=on --borrow=strict --emit=fir-raw -o "$W/$n.s" "$1" > "$W/$n.log" 2>&1
    rm -f "$W/$n.s"
}
export -f one
xargs -P "$JOBS" -I{} bash -c 'one {}' < "$W/list"
RULES="used after|also passed as 'inout'|points into|into the target of the reference parameter|escapes into|cannot be sent to another thread"
cat "$W"/*.log | awk '/^error/ {msg=$0} /^ *--> / {print msg " @ " $2}' | grep -E "$RULES" | sort -u > "$W/all"
echo "programs: $(wc -l < "$W/list")"
echo "findings: $(wc -l < "$W/all")"
echo "in lib/: $(grep -c '@ lib/' "$W/all")"
for rule in "used after" "also passed as 'inout'" "into the target of the reference parameter" "escapes into" "cannot be sent to another thread"; do
    echo "$rule: total $(grep -cF "$rule" "$W/all"), lib/ $(grep -F "$rule" "$W/all" | grep -c '@ lib/')"
done
head -${TOP:-25} "$W/all"
