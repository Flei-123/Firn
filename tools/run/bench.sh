#!/usr/bin/env bash
# tools/run/bench.sh -- WHAT THE CACHE IS WORTH, IN MILLISECONDS (round 84).
#
# The question is not "is a cache faster" -- of course it is. The question is
# how much of the start is compilation, for a small, a middling and a large
# program, and where `firnc run` then sits next to `python3`, the thing
# people compare it with whether that is fair or not.
#
# Method: every figure is the MEDIAN of N runs (default 5), measured around
# the whole command with the shell's own clock. Cold means: the cache
# directory was emptied right before. Warm means: the same command a second
# time. The programs run to completion in both cases -- the runtime of the
# program is inside every figure, which is why the last column names it
# separately (the warm start IS the program plus the cache lookup).
#
# Usage:  bash tools/run/bench.sh [N]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
N=${1:-5}

export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
WORK=.run-work
mkdir -p "$WORK"
export FIRN_CACHE="$ROOT/$WORK/benchcache"

[ -x "$FIRNC" ] || cargo build --release --manifest-path compiler/Cargo.toml || exit 1

# milliseconds of one run, rounded
once() {
    local s e
    s=$(date +%s%N)
    "$@" > /dev/null 2>&1
    e=$(date +%s%N)
    echo $(( (e - s) / 1000000 ))
}

median() {
    local sorted
    sorted=$(printf '%s\n' "$@" | sort -n)
    echo "$sorted" | awk -v n="$#" 'NR==int((n+1)/2){print}'
}

# cold_median <file> [args...]
cold() {
    local out=()
    for _ in $(seq "$N"); do
        rm -rf "$FIRN_CACHE"
        out+=("$(once "$FIRNC" run "$@")")
    done
    median "${out[@]}"
}

warm() {
    rm -rf "$FIRN_CACHE"
    "$FIRNC" run "$@" > /dev/null 2>&1
    local out=()
    for _ in $(seq "$N"); do
        out+=("$(once "$FIRNC" run "$@")")
    done
    median "${out[@]}"
}

# the program on its own, once compiled by hand -- the floor under the warm
# start
bare() {
    local f=$1
    shift
    "$FIRNC" --opt-level=dev-fast -o "$WORK/bare" "$f" > /dev/null 2>&1 || { echo "-"; return; }
    local out=()
    for _ in $(seq "$N"); do
        out+=("$(once "$WORK/bare" "$@")")
    done
    median "${out[@]}"
}

mkdir -p "$WORK"
cat > "$WORK/bench.js" <<'EOF'
var n = 0;
for (var i = 0; i < 1000; i++) { n = n + i; }
EOF

echo "== firnc run: cold against warm (median of $N, milliseconds) =="
printf '%-34s %8s %8s %8s %8s\n' "program" "cold" "warm" "program" "saved"
row() {
    local name=$1 file=$2
    shift 2
    local c w b
    c=$(cold "$file" "$@")
    w=$(warm "$file" "$@")
    b=$(bare "$file" "$@")
    printf '%-34s %8s %8s %8s %8s\n' "$name" "$c" "$w" "$b" "$((c - w))"
}

row "hello.fi (14 lines)"            examples/hello.fi
row "number_check.fi (4 modules)"    demos/number_check.fi < /dev/null
row "run_main.fi (the JS engine)"    lib/js/run_main.fi "$WORK/bench.js"

echo
echo "== the yardstick next to it: python3 =="
# NOT the same thing -- python3 interprets, firnc compiles to machine code.
# The point of the line is to say what the number means: this is what the
# start of a small script costs in the language people compare Firn with.
cat > "$WORK/bench.py" <<'EOF'
print("hello")
EOF
py=()
for _ in $(seq "$N"); do
    py+=("$(once python3 "$WORK/bench.py")")
done
printf '%-34s %8s\n' "python3 bench.py" "$(median "${py[@]}")"
printf '%-34s %8s\n' "python3 -c pass" "$(
    out=()
    for _ in $(seq "$N"); do out+=("$(once python3 -c pass)"); done
    median "${out[@]}"
)"
