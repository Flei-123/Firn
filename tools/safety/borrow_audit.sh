#!/usr/bin/env bash
set -u
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
STEP=${STEP:-8}
JOBS=${JOBS:-3}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
cd "$ROOT"
{
    ls examples/*.fi bench/*.fi tools/*/*.fi 2>/dev/null
    ls tests/*.fi | awk -v s="$STEP" 'NR % s == 0'
} > "$W/list"
export FIRNC W FIRNLIB="$ROOT/lib"
one() {
    n=$(echo "$1" | tr '/' '_')
    FIRN_REACH_CHECKS=0 timeout 120 "$FIRNC" --borrow=strict --emit=asm -o "$W/$n.s" "$1" > "$W/$n.log" 2>&1
    rm -f "$W/$n.s"
}
export -f one
xargs -P "$JOBS" -I{} bash -c 'one {}' < "$W/list"
cat "$W"/*.log | awk '/^error/ {msg=$0} /^ *--> / {print msg " @ " $2}' | sort -u > "$W/all"
echo "programs: $(wc -l < "$W/list")"
echo "distinct findings: $(wc -l < "$W/all")"
echo "in lib/: $(grep -c '@ lib/' "$W/all")"
for rule in "used after" "which is also passed as 'inout'" "into the target of the reference parameter" "whose body is not known"; do
    echo "$rule: total $(grep -cF "$rule" "$W/all"), lib/ $(grep -F "$rule" "$W/all" | grep -c '@ lib/')"
done
grep -F "@ lib/" "$W/all" | sed 's/.*@ //; s/:[0-9]*:[0-9]*$//' | sort | uniq -c | sort -rn | head -${TOP:-25}
