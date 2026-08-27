#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/thread/stress.sh -- soak run with several threads and a running collector.
#
# What is measured is the REAL memory consumption of the process (RSS from
# /proc/self/statm), not the self-report of the runtime. What is judged:
#
#   * no crash and no deadlock over the whole run time
#   * error word 0 -- NO thread found a chain from which the
#     collector had removed a link
#   * the mutex counter and the atomic counter match the number of rounds EXACTLY
#   * the counter WITHOUT a lock has lost (otherwise the threads did not
#     really run at the same time and the run proves nothing)
#   * the RSS does not drift: the last sample must not exceed the smallest one
#     by more than STRESS_DRIFT_KIB
#
# Environment:
#   STRESS_SEK     run time in seconds (default 130)
#   STRESS_THREADS number of threads (default 4)
#   STRESS_LOCAL   1 = free lists per thread (variant B), 0 = GC lock (A)
#   STRESS_DRIFT_KIB  allowed RSS increase (default 1024)
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
SECS=${STRESS_SEK:-130}
THREADS=${STRESS_THREADS:-4}
LOCAL_LISTS=${STRESS_LOCAL:-0}
DRIFT=${STRESS_DRIFT_KIB:-1024}
WORK=$(mktemp -d /tmp/firn-faden-stress.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

if [ ! -x "$FIRNC" ]; then
    echo "ERROR: $FIRNC is missing"
    exit 1
fi

cp lib/dom/meas.fi "$WORK/"
sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $((SECS * 1000))  // STRESS_BUDGET_MS|" \
    -e "s|^const THREADS: u64 = .*$|const THREADS: u64 = $THREADS  // STRESS_THREADS|" \
    -e "s|^const LOCAL: u64 = .*$|const LOCAL: u64 = $LOCAL_LISTS  // STRESS_LOCAL|" \
    tools/thread/stress.fi > "$WORK/stress.fi"

export FIRNLIB="$(pwd)/lib"
if ! "$FIRNC" -o "$WORK/stress" "$WORK/stress.fi" 2>"$WORK/err"; then
    echo "ERROR: build failed"
    head -10 "$WORK/err"
    exit 1
fi

echo "== soak run: $THREADS threads, ${SECS}s, LOCAL=$LOCAL_LISTS =="
set +e
timeout $((SECS + 120)) "$WORK/stress" > "$WORK/aus.tsv" 2>"$WORK/err2"
rc=$?
set -e
if [ "$rc" -ne 0 ]; then
    echo "FAILED: return value $rc"
    echo "  1 = a thread found a destroyed chain (the collector collected something live)"
    echo "  2/3/4 = a counter is wrong * 5 = a thread was left over * 124 = timeout"
    tail -5 "$WORK/aus.tsv"
    head -5 "$WORK/err2"
    exit 1
fi

sed 's/^/  /' "$WORK/aus.tsv"

get() { awk -v c="$1" '$1=="Q" && $2==c {print $3}' "$WORK/aus.tsv"; }
errword=$(get 0)
rounds=$(get 1)
with_lock=$(get 2)
atom=$(get 3)
without_lock=$(get 4)
rss_end=$(get 5)
runs=$(get 6)
stw=$(get 7)

rss_min=$(awk '$1=="S" {print $3}' "$WORK/aus.tsv" | sort -n | head -1)
rss_max=$(awk '$1=="S" {print $3}' "$WORK/aus.tsv" | sort -n | tail -1)
samples=$(grep -c '^S' "$WORK/aus.tsv" || true)

echo
echo "  rounds           $rounds"
echo "  collections      $runs"
echo "  Stop-the-World   $stw"
echo "  RSS min/max/end  $rss_min / $rss_max / $rss_end KiB ($samples samples)"

bad=0
[ "$errword" = "0" ] || { echo "ERROR: error word $errword -- the collector collected something live"; bad=1; }
[ "$with_lock" = "$rounds" ] || { echo "ERROR: mutex counter $with_lock != rounds $rounds"; bad=1; }
[ "$atom" = "$rounds" ] || { echo "ERROR: atomic counter $atom != rounds $rounds"; bad=1; }
if [ "$without_lock" -ge "$rounds" ]; then
    echo "ERROR: the counter WITHOUT a lock lost nothing ($without_lock of $rounds) -- the threads did not run at the same time"
    bad=1
fi
[ "$runs" -gt 0 ] || { echo "ERROR: not a single collection"; bad=1; }
[ "$stw" -gt 0 ] || { echo "ERROR: the world was never stopped"; bad=1; }
[ "$samples" -ge 3 ] || { echo "ERROR: too few samples"; bad=1; }
if [ $((rss_end - rss_min)) -gt "$DRIFT" ]; then
    echo "ERROR: RSS drift $((rss_end - rss_min)) KiB > $DRIFT KiB"
    bad=1
fi

if [ "$bad" -ne 0 ]; then
    echo "STRESS: FAILED"
    exit 1
fi
echo "STRESS: passed -- $rounds rounds, $runs collections, $stw stops, RSS drift $((rss_end - rss_min)) KiB"
exit 0
