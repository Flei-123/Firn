#!/usr/bin/env bash
# tools/firstrun/run.sh -- THE FIRST FIVE MINUTES WITH THE LANGUAGE (round 88).
#
# Every other tool in this directory proves that something DIFFICULT works:
# a tokenizer against 6,810 cases, a kernel in QEMU, sockets against `nc`.
# This one proves that the EASY thing works -- the programs a stranger writes
# before he has read anything:
#
#   01  join two pieces of text, ask for the length and the beginning
#   02  compare and join
#   03  take pieces out (trim, part, ab, to, find, contains)
#   04  put a number into a sentence
#   05  read a file by its name
#   06  forty thousand joins -- a real load on the collector
#   07  COUNTER-CHECK: the explicit `gc_init()` keeps working
#
# THE RULE FOR EVERY CASE 01..06: not one line about a collector, not one
# `gc_init()`, not one `[u8; N]`. It has to COMPILE, RUN and print exactly
# what stands next to it in the `.out` file. That is the whole measure of
# this round -- round 87 failed four of the seven for four different reasons
# (docs/ROUND88.md).
#
# Every case runs twice: with the optimizer (`--opt-level=release-fast`, the
# default) and without it (`--no-opt`). If `.firnc1` is there, everything
# runs a third time through the SELF HOSTED compiler -- a difference between
# the two compilers is a failure here as well.
#
# Three counter-checks at the end, because a check that only ever says yes
# proves nothing:
#
#   A  the sources of 01..06 really contain no `gc_init`
#   B  a program WITHOUT text gets NO setup in `_start` -- whoever does not
#      use the collector pays nothing for it
#   C  under `profile kernel` there is neither `_start` nor a setup
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
WORK=.firstrun-work
CASES=tools/firstrun/cases

rm -rf "$WORK"
mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

PASS=0
FAIL=0
ok()   { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad()  { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

# ---------------------------------------------------------------- the cases

# $1 label  $2 compiler  $3.. flags
run_all() {
    local label="$1"; shift
    local cc="$1"; shift
    local q n exe want got rc
    for q in "$CASES"/*.fi; do
        n=$(basename "$q" .fi)
        exe="$WORK/$n.$label"
        if ! "$cc" "$@" "$q" -o "$exe" > "$WORK/$n.$label.build" 2>&1; then
            bad "$n [$label]: does not compile"
            sed 's/^/       /' "$WORK/$n.$label.build" | head -8
            continue
        fi
        got=$("$exe" 2> "$WORK/$n.$label.err")
        rc=$?
        if [ "$rc" -ne 0 ]; then
            bad "$n [$label]: exit code $rc (expected 0)"
            sed 's/^/       /' "$WORK/$n.$label.err" | head -5
            continue
        fi
        want=$(cat "${q%.fi}.out")
        if [ "$got" != "$want" ]; then
            bad "$n [$label]: other output"
            diff <(printf '%s\n' "$want") <(printf '%s\n' "$got") \
                | sed 's/^/       /' | head -10
            continue
        fi
        ok "$n [$label]"
    done
}

echo "== 1. the seven programs, WITH the optimizer =="
run_all opt "$FIRNC"

echo
echo "== 2. the same seven WITHOUT the optimizer =="
run_all noopt "$FIRNC" --no-opt

echo
echo "== 3. the same seven through the self hosted compiler (firnc1) =="
if [ -x "$FC1" ]; then
    run_all fc1 "$FC1"
else
    echo "  SKIP $FC1 is not there (build it with: $FIRNC bin/firnc1.fi -o $FC1)"
fi

# ------------------------------------------------------------ counter-checks

echo
echo "== 4. counter-check A: no case mentions the collector =="
for q in "$CASES"/0[1-6]_*.fi; do
    n=$(basename "$q" .fi)
    if grep -q 'gc_init\|gc_set_\|#\[no_gc\]' "$q"; then
        bad "$n names the collector -- then it proves nothing"
    else
        ok "$n says nothing about a collector"
    fi
done
if grep -q 'gc_init()' "$CASES/07_gc_init_by_hand.fi"; then
    ok "07 really calls gc_init() by hand (the counter-check needs it)"
else
    bad "07 no longer calls gc_init() -- the counter-check is empty"
fi

echo
echo "== 5. counter-check B: a program without text gets NO setup =="
cat > "$WORK/plain.fi" <<'EOF'
fn main() -> i32 {
    var s: i32 = 0
    var i: i32 = 0
    while i < 10 {
        s = s + i
        i = i + 1
    }
    return s
}
EOF
"$FIRNC" --emit=asm "$WORK/plain.fi" -o "$WORK/plain.s" > "$WORK/plain.build" 2>&1
if grep -q 'gc_init' "$WORK/plain.s"; then
    bad "a program without text carries a gc_init in _start"
    grep -n 'gc_init' "$WORK/plain.s" | sed 's/^/       /' | head -3
else
    ok "no gc_init in the assembly -- nothing is paid for"
fi
# Only the ENTRY BLOCK counts. The runtime itself calls `gc_init` a second
# time (`thread_init`, lib/gc/gc.fi) -- that has been there since round 49
# and has nothing to do with this round.
"$FIRNC" --emit=asm "$CASES/01_join.fi" -o "$WORK/join.s" > /dev/null 2>&1
sed -n '/^_start:/,/^    hlt$/p' "$WORK/join.s" > "$WORK/join.start"
N=$(grep -c 'call .*gc_init' "$WORK/join.start")
if [ "$N" = "1" ]; then
    ok "the joining program carries the setup EXACTLY once, in _start"
else
    bad "the joining program carries the setup $N times in _start (expected 1)"
    sed 's/^/       /' "$WORK/join.start"
fi

echo
echo "== 6. counter-check C: profile kernel keeps its hands off =="
cat > "$WORK/kernel_probe.fi" <<'EOF'
profile kernel

fn kmain(p: *mut u32) -> u32 {
    var s: u32 = 0
    var i: u32 = 0
    while i < 4 {
        s = s + *((p as u64 + 4 * i as u64) as *mut u32)
        i = i + 1
    }
    return s
}
EOF
"$FIRNC" --emit=asm "$WORK/kernel_probe.fi" -o "$WORK/kernel_probe.s" > "$WORK/kernel_probe.build" 2>&1
if [ ! -s "$WORK/kernel_probe.s" ]; then
    bad "the kernel profile did not compile"
    sed 's/^/       /' "$WORK/kernel_probe.build" | head -5
elif grep -q '_start' "$WORK/kernel_probe.s"; then
    bad "the kernel profile has an entry point -- that is new and wrong"
elif grep -q 'gc_init' "$WORK/kernel_probe.s"; then
    bad "the kernel profile carries a gc_init"
else
    ok "no _start, no gc_init -- unchanged since round 52"
fi

echo
TOTAL=$((PASS + FAIL))
if [ "$FAIL" -eq 0 ]; then
    echo "PASS $PASS/$TOTAL first-run checks"
    exit 0
fi
echo "FAIL $FAIL of $TOTAL first-run checks"
exit 1
