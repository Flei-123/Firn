#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/gc_frag/run.sh -- ACCEPTANCE ITEM 2, the half that was open:
# FRAGMENTATION WITH CHANGING OBJECT SIZES (round 85).
#
# `tools/dom_soak/run.sh` builds the same set of objects over and over, so
# every allocation lands in the same size class. ACCEPTANCE.md has carried
# that as a known gap since round 3: "the soak test always uses the same set,
# which is the friendly case".
#
# This run does the unfriendly case: five phases that keep changing the
# object size (tiny / large / mixed / powers of two / medium), with a
# CONSTANT working set. If the collector -- which cannot compact, because the
# stack scan is conservative -- did not give whole chunks back, the heap would
# have to climb with every phase change.
#
# Environment:
#   FRAG_SEC     time budget per version in seconds (default 900 = 15 min)
#   FRAG_ROUNDS  upper bound on the rounds (default 100000000)
#   FRAG_SAMPLE  rounds per data line (default 500)
#   FRAG_LEAK_SEC time budget for the counter-check (default 60)
#   FRAG_LEAK_MB  hard memory brake for the counter-check in MiB (default 3072)
#
# Two versions run, and both are needed:
#   mode 0  replace -- the working set stays constant. The heap has to stay
#           bounded.
#   mode 1  keep -- nothing is ever released. The heap HAS to climb. If it
#           did not, the measuring method would be broken and the flat curve
#           of mode 0 would prove nothing.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
WORK=.frag-work
OUT=tools/gc_frag
SECS=${FRAG_SEC:-900}
ROUNDS=${FRAG_ROUNDS:-100000000}
SAMPLE=${FRAG_SAMPLE:-500}
LEAK_SECS=${FRAG_LEAK_SEC:-60}
# Below this many rounds mode 0 is still in its warm-up and the curve rises
# for that reason alone -- a verdict then would be a measuring artefact, not
# a leak. Measured in round 85: at 5 s the run reports GROWS, at 40 min it
# reports bounded. So the run refuses to judge instead of judging wrongly.
MIN_ROUNDS=${FRAG_MIN_ROUNDS:-1000000}
LEAK_MB=${FRAG_LEAK_MB:-3072}

export FIRNLIB="$(pwd)/lib"

if [ ! -x "$FIRNC" ]; then
    echo "ERROR: $FIRNC is missing -- run 'cargo build --release' in compiler/ first."
    exit 1
fi
[ -f tools/gc_frag/frag.fi ] || { echo "ERROR: tools/gc_frag/frag.fi is missing."; exit 1; }

rm -rf "$WORK"
mkdir -p "$WORK" "$OUT"

# $1 target  $2 budget ms  $3 rounds  $4 sample  $5 mode
adjust() {
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $2  // FRAG_BUDGET_MS|" \
        -e "s|^const ROUNDS_MAX: i64 = .*$|const ROUNDS_MAX: i64 = $3  // FRAG_ROUNDS_MAX|" \
        -e "s|^const SAMPLE: i64 = .*$|const SAMPLE: i64 = $4  // FRAG_SAMPLE|" \
        -e "s|^const MODE: i64 = .*$|const MODE: i64 = $5  // FRAG_MODE|" \
        tools/gc_frag/frag.fi > "$1"
    for pair in "BUDGET_MS: i64 = $2 " "ROUNDS_MAX: i64 = $3 " "SAMPLE: i64 = $4 " "MODE: i64 = $5 "; do
        grep -q "const $pair" "$1" || { echo "ERROR: '$pair' could not be replaced in $1."; exit 1; }
    done
}

echo "== GC fragmentation with changing object sizes (acceptance item 2) =="
echo "   budget: ${SECS}s (mode 0), ${LEAK_SECS}s (counter-check), sample every $SAMPLE rounds"

# ---------------------------------------------------------- 1. build stages
# A memory model that only holds with the optimiser switched on is worthless.
echo
echo "-- 1. build in three build stages, short run compared --"
STAGES=("release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast")
errs=0
for mode in 0 1; do
    adjust "$WORK/short_$mode.fi" 2000 4000 1000 "$mode"
    expected=""
    for st in "${STAGES[@]}"; do
        name=${st%%:*}
        opt=${st#*:}
        # shellcheck disable=SC2086
        if ! "$FIRNC" "$WORK/short_$mode.fi" -o "$WORK/short_${mode}_$name" $opt 2> "$WORK/build_${mode}_$name.err"; then
            echo "   ERROR: build mode $mode / $name failed:"
            head -5 "$WORK/build_${mode}_$name.err"
            errs=1
            continue
        fi
        ( ulimit -v $((LEAK_MB * 1024)); exec "$WORK/short_${mode}_$name" ) > "$WORK/short_${mode}_$name.tsv" 2>&1
        rc=$?
        rounds=$(grep -o 'rounds=[0-9]*' "$WORK/short_${mode}_$name.tsv" | head -1)
        if [ $rc -ne 0 ] || grep -q '^# error' "$WORK/short_${mode}_$name.tsv"; then
            echo "   ERROR: short run mode $mode / $name ended with $rc:"
            grep '^# error' "$WORK/short_${mode}_$name.tsv" | head -2
            errs=1
            continue
        fi
        if [ -z "$expected" ]; then
            expected="$rounds"
        elif [ "$rounds" != "$expected" ]; then
            echo "   ERROR: mode $mode / $name yields $rounds, expected $expected"
            errs=1
        fi
        printf '   mode %s  %-12s %s, %s data lines\n' "$mode" "$name" "$rounds" \
            "$(grep -c '^[0-9]' "$WORK/short_${mode}_$name.tsv")"
    done
done
[ $errs -eq 0 ] || { echo "ABORT: the build stages do not agree."; exit 1; }

# ------------------------------------------------------------ 2. the runs
echo
echo "-- 2. the measuring runs --"
for mode in 0 1; do
    budget=$((SECS * 1000))
    [ "$mode" = 1 ] && budget=$((LEAK_SECS * 1000))
    adjust "$WORK/frag_$mode.fi" "$budget" "$ROUNDS" "$SAMPLE" "$mode"
    if ! "$FIRNC" "$WORK/frag_$mode.fi" -o "$WORK/frag_$mode" 2> "$WORK/build_$mode.err"; then
        echo "   ERROR: build of the measuring version $mode failed:"
        head -5 "$WORK/build_$mode.err"
        exit 1
    fi
    start=$(date +%s)
    # The hard brake: the counter-check is meant to grow, not to take the
    # machine with it.
    ( ulimit -v $((LEAK_MB * 1024)); exec "$WORK/frag_$mode" ) > "$OUT/measurement-$mode.tsv"
    rc=$?
    seconds=$(( $(date +%s) - start ))
    if [ $rc -ne 0 ] && [ "$mode" = 0 ]; then
        echo "   ERROR: run $mode ended with $rc:"
        grep '^# error' "$OUT/measurement-$mode.tsv" | head -2
        exit 1
    fi
    echo "   mode $mode: $(grep '^# done' "$OUT/measurement-$mode.tsv") (${seconds}s wall clock, exit $rc)"
done

# ------------------------------------------------------------ 3. evaluation
echo
echo "-- 3. evaluation --"
python3 - "$OUT/measurement-0.tsv" "$OUT/measurement-1.tsv" "$MIN_ROUNDS" <<'PYEOF'
import sys


def read(path):
    rows, done = [], None
    for line in open(path, encoding='utf-8'):
        line = line.strip()
        if not line:
            continue
        if line.startswith('#'):
            if line.startswith('# done'):
                done = line
            continue
        f = line.split('\t')
        if len(f) != 9:
            raise SystemExit('%s: a line with %d fields instead of 9' % (path, len(f)))
        rows.append([int(x) for x in f])
    return rows, done


def median(xs):
    s = sorted(xs)
    n = len(s)
    if not n:
        return 0
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2


T, ROUNDS, RSS, LIVE, HEAP, LIVEB, RUNS, PAUSE, ASKED = range(9)


def look(path):
    rows, done = read(path)
    if len(rows) < 8:
        raise SystemExit('%s: only %d samples' % (path, len(rows)))
    q = len(rows) // 4
    warm = rows[q:]                       # after the warm-up quarter
    second = rows[q:2 * q] or rows[q:q + 1]
    last = rows[3 * q:] or rows[-1:]
    rss_after = [r[RSS] for r in warm]
    monotone = all(b >= a for a, b in zip(rss_after, rss_after[1:])) and rss_after[-1] > rss_after[0]
    rss2, rssl = median([r[RSS] for r in second]), median([r[RSS] for r in last])
    heap2, heapl = median([r[HEAP] for r in second]), median([r[HEAP] for r in last])
    ratios = [r[HEAP] / r[LIVEB] for r in warm if r[LIVEB] > 0]
    return {
        'rows': rows, 'done': done, 'n': len(rows),
        'rounds': rows[-1][ROUNDS], 'asked': rows[-1][ASKED],
        'rss2': rss2, 'rssl': rssl, 'rss_max': max(r[RSS] for r in rows),
        'heap2': heap2, 'heapl': heapl, 'heap_max': max(r[HEAP] for r in rows),
        'live_max': max(r[LIVEB] for r in rows),
        'ratio_med': median(ratios) if ratios else 0,
        'ratio_max': max(ratios) if ratios else 0,
        'runs': rows[-1][RUNS], 'pause': max(r[PAUSE] for r in rows),
        'grew': rssl > rss2 * 1.05 or heapl > heap2 * 1.05,
        'monotone': monotone,
    }


a = look(sys.argv[1])
b = look(sys.argv[2])
min_rounds = int(sys.argv[3])
if a['rounds'] < min_rounds:
    raise SystemExit('   ABORT: mode 0 got only %d rounds, %d are needed for a verdict.\n'
                     '          The warm-up alone makes the curve rise -- raise FRAG_SEC\n'
                     '          (or lower FRAG_MIN_ROUNDS if you really want to judge this).'
                     % (a['rounds'], min_rounds))


def show(u, name):
    print('   %s:' % name)
    print('     rounds                  %d  (%d samples)' % (u['rounds'], u['n']))
    print('     octets asked for        %d  (%.1f MiB through the allocator)'
          % (u['asked'], u['asked'] / 1048576))
    print('     RSS median 2nd quarter  %d KiB' % u['rss2'])
    print('     RSS median last quarter %d KiB' % u['rssl'])
    print('     RSS maximum             %d KiB' % u['rss_max'])
    print('     heap median 2nd/last    %d / %d octets' % (u['heap2'], u['heapl']))
    print('     heap maximum            %d octets' % u['heap_max'])
    print('     live maximum            %d octets' % u['live_max'])
    print('     heap/live median        %.2fx   maximum %.2fx'
          % (u['ratio_med'], u['ratio_max']))
    print('     collections             %d, longest pause %d ns' % (u['runs'], u['pause']))
    print('     verdict                 %s (growth %s, monotone %s)'
          % ('GROWS' if (u['grew'] or u['monotone']) else 'bounded',
             'yes' if u['grew'] else 'no', 'yes' if u['monotone'] else 'no'))


show(a, 'mode 0  replace (the working set stays constant)')
show(b, 'mode 1  keep    (counter-check, MUST grow)')

print()
errs = 0
if a['grew'] or a['monotone']:
    print('   FAILED: with changing object sizes the heap grows without bound.')
    errs = 1
else:
    print('   PASSED: with changing object sizes the heap stays bounded '
          '(RSS %d -> %d KiB, heap %d -> %d octets).'
          % (a['rss2'], a['rssl'], a['heap2'], a['heapl']))
    print('   Overhead above the live set: %.2fx in the median, %.2fx at worst.'
          % (a['ratio_med'], a['ratio_max']))
if not (b['grew'] or b['monotone']):
    print('   FAILED: the counter-check does NOT grow -- the measurement is worthless.')
    errs = 1
else:
    print('   Counter-check strikes: RSS %d -> %d KiB, heap up to %d octets.'
          % (b['rss2'], b['rssl'], b['heap_max']))
sys.exit(errs)
PYEOF
rc=$?

echo
if [ $rc -eq 0 ]; then
    echo "OK: fragmentation with changing object sizes -- passed"
    echo "    (series in $OUT/measurement-0.tsv and $OUT/measurement-1.tsv)."
else
    echo "ERROR: fragmentation run NOT passed."
fi
exit $rc
