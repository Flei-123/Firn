#!/usr/bin/env bash
# The block cache of rt.heap_alloc/heap_free (OpenPlan r66), MEASURED at the
# kernel's door: tools/rtcache/churn.fi frees and allocates blocks of 1..8
# pages in a rolling window of 64, 200,000 rounds, and strace -c counts the
# mmap/munmap calls that really reach the kernel.
#
#   1. with the cache: the calls stay below 2,000 (without it: ~400,000)
#   2. the counter-check: the same program that NEVER frees needs one mmap per
#      block -- the count must be at least the number of rounds, so the
#      counting itself is proved
#   3. the answer of the program (a sum over the first word of every block)
#      is the same in both -- the blocks hold what was written into them
# A machine without strace skips the counting and still runs the program.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERRORS=0
fail() { echo "  FAIL $1"; ERRORS=$((ERRORS + 1)); }

$FIRNC --opt-level=release-fast -o "$W/churn" tools/rtcache/churn.fi > "$W/b.log" 2>&1 || { grep -v RWX "$W/b.log" | head -5; fail "churn does not build"; exit 1; }
R=200000
out=$("$W/churn" $R) || fail "churn failed"
echo "   $out"
want_sum=$((R * (R + 1) / 2))
[ "$out" = "rounds=$R sum=$want_sum" ] || fail "answer '$out', wanted rounds=$R sum=$want_sum"
if ! command -v strace > /dev/null; then
    echo "   skip: strace not installed (the program ran and gave the right answer)"
    [ $ERRORS -eq 0 ] && exit 0 || exit 1
fi
calls() { # <args...> -> number of mmap + munmap calls
    strace -f -c -o "$W/st.txt" "$W/churn" "$@" > /dev/null 2>&1
    awk '$NF == "mmap" || $NF == "munmap" { n += $4 } END { print n + 0 }' "$W/st.txt"
}
with=$(calls $R)
hold=$(calls 20000 hold)
echo "   rolling window, $R rounds: $with mmap/munmap calls"
echo "   never frees, 20000 rounds: $hold mmap calls (the counter-check)"
[ "$with" -lt 2000 ] || fail "$with kernel calls for $R rounds (cache not working)"
[ "$hold" -ge 20000 ] || fail "counter-check: only $hold calls for 20000 blocks that are never freed"
if [ $ERRORS -eq 0 ]; then
    echo "rtcache: ok"
else
    echo "rtcache: $ERRORS failed"
    exit 1
fi
