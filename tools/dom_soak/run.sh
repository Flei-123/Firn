#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/dom_soak/run.sh -- soak run of the DOM prototype (acceptance item 2).
#
# Checks the promise from FIRN-ANFORDERUNGEN.md 13: "DOM prototype with
# parent/child back references and listener cycles that does not leak in a soak run."
#
# What is measured is the REAL memory consumption of the process (RSS from
# /proc/self/statm), not the self-report of the runtime. In addition the
# deliberately leaking counter-check runs EVERY TIME (lib/dom/soak_leak.fi, the same
# set of cycles with reference counts). If it stays green, the measuring method
# is broken and this script aborts -- a measurement that cannot show anything
# would be worse than none.
#
# Environment:
#   SOAK_SEC         time budget per version in seconds (default 600)
#   SOAK_CYCLES      maximum number of cycle sets (default 100000000)
#   SOAK_SAMPLE      cycles per data line (default 1000)
#   SOAK_MIN_CYCLES  minimum number of cycles for a valid verdict (default 100000)
#   SOAK_LEAK_CYCLES upper limit for the LEAKING counter-check (default 600000;
#                    since round 53 one set leaks 13 objects of 128 bytes,
#                    that is about 1.0 GiB -- before it was 6 of 64 bytes)
#   SOAK_LEAK_MB     hard memory brake for the counter-check in MiB (default 3072)
#
# WHY THE COUNTER-CHECK IS CAPPED: by construction it leaks about 384 bytes per
# cycle (6 of 7 objects at 64 bytes). Without a cap it eats double-digit
# gigabytes at the full number of cycles and takes the machine down with it -- measured
# on the first long run: 7.0 GB after 18.8 million cycles. 2 million cycles (~770 MiB)
# show the same picture and are harmless. In addition `ulimit -v` limits
# the address space as a hard brake.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
WORK=.dom-soak-work
OUT=tools/dom_soak
SECS=${SOAK_SEC:-600}
CYCLES=${SOAK_CYCLES:-100000000}
SAMPLE=${SOAK_SAMPLE:-1000}
MINC=${SOAK_MIN_CYCLES:-100000}
LEAK_CYCLES=${SOAK_LEAK_CYCLES:-600000}
LEAK_MB=${SOAK_LEAK_MB:-3072}
BUDGET_MS=$((SECS * 1000))

if [ ! -x "$FIRNC" ]; then
    echo "ERROR: $FIRNC is missing -- run 'cargo build --release' in the folder compiler/ first."
    exit 1
fi
for f in lib/dom/dom.fi lib/dom/meas.fi lib/dom/soak_gc.fi lib/dom/soak_leak.fi; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f is missing -- the DOM prototype is not built."
        exit 1
    fi
done

rm -rf "$WORK"
mkdir -p "$WORK" "$OUT"
cp lib/dom/dom.fi lib/dom/meas.fi "$WORK/"

# Create a working copy with the constants changed over.
# $1 source  $2 target  $3 budget ms  $4 cycles  $5 sample
adjust() {
    sed -e "s|^const BUDGET_MS: i64 = .*$|const BUDGET_MS: i64 = $3  // SOAK_BUDGET_MS|" \
        -e "s|^const CYCLES_MAX: i64 = .*$|const CYCLES_MAX: i64 = $4  // SOAK_CYCLES_MAX|" \
        -e "s|^const SAMPLE: i64 = .*$|const SAMPLE: i64 = $5  // SOAK_SAMPLE|" \
        "$1" > "$2"
    # The three lines really have to have been replaced.
    if ! grep -q "const BUDGET_MS: i64 = $3 " "$2"; then
        echo "ERROR: BUDGET_MS in $1 cannot be replaced (line changed?)."
        exit 1
    fi
    if ! grep -q "const CYCLES_MAX: i64 = $4 " "$2"; then
        echo "ERROR: CYCLES_MAX in $1 cannot be replaced."
        exit 1
    fi
    if ! grep -q "const SAMPLE: i64 = $5 " "$2"; then
        echo "ERROR: SAMPLE in $1 cannot be replaced."
        exit 1
    fi
}

echo "== DOM soak run (acceptance item 2) =="
echo "   budget per version: ${SECS}s, at most $CYCLES cycles, a sample every $SAMPLE"

# The markers '# fertig', '# fehler' and 'zyklen=' are written by
# lib/dom/soak_*.fi (round for lib/) -- do not rename them here.
# ---------------------------------------------------------- 1. build stages
# Build both programs in ALL THREE build stages and compare a short run.
# A memory model that only holds with the optimiser switched on is worthless.
echo
echo "-- 1. build in three build stages and compare a short run --"
STUFEN=("release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast")
errs=0
for variant in gc leak; do
    adjust "lib/dom/soak_$variant.fi" "$WORK/kurz_$variant.fi" 3000 5000 1000
    expected=""
    for st in "${STUFEN[@]}"; do
        name=${st%%:*}
        opt=${st#*:}
        # shellcheck disable=SC2086
        if ! "$FIRNC" "$WORK/kurz_$variant.fi" -o "$WORK/kurz_${variant}_$name" $opt 2>"$WORK/bau_${variant}_$name.err"; then
            echo "   ERROR: build $variant/$name failed:"
            head -5 "$WORK/bau_${variant}_$name.err"
            errs=1
            continue
        fi
        "$WORK/kurz_${variant}_$name" > "$WORK/kurz_${variant}_$name.tsv" 2>&1
        rc=$?
        dl=$(grep -c '^[0-9]' "$WORK/kurz_${variant}_$name.tsv")
        done=$(grep -o 'zyklen=[0-9]*' "$WORK/kurz_${variant}_$name.tsv" | head -1)
        if [ $rc -ne 0 ] || grep -q '^# fehler' "$WORK/kurz_${variant}_$name.tsv"; then
            echo "   ERROR: short run $variant/$name ended with $rc:"
            grep '^# fehler' "$WORK/kurz_${variant}_$name.tsv" | head -2
            errs=1
            continue
        fi
        if [ -z "$expected" ]; then
            expected="$done"
        elif [ "$done" != "$expected" ]; then
            echo "   ERROR: $variant/$name yields $done, expected $expected"
            errs=1
        fi
        printf '   %-6s %-12s %s, %s data lines\n' "$variant" "$name" "$done" "$dl"
    done
done
if [ $errs -ne 0 ]; then
    echo "ABORT: the build stages do not yield the same result."
    exit 1
fi

# ------------------------------------------------------------ 2. measuring run
echo
echo "-- 2. soak run --"
for variant in gc leak; do
    limit=$CYCLES
    if [ "$variant" = leak ]; then
        # Capped: see the head of the file. This version leaks on purpose.
        limit=$LEAK_CYCLES
    fi
    adjust "lib/dom/soak_$variant.fi" "$WORK/soak_$variant.fi" "$BUDGET_MS" "$limit" "$SAMPLE"
    if ! "$FIRNC" "$WORK/soak_$variant.fi" -o "$WORK/soak_$variant" 2>"$WORK/bau_$variant.err"; then
        echo "   ERROR: build of the measuring version $variant failed:"
        head -5 "$WORK/bau_$variant.err"
        exit 1
    fi
    start=$(date +%s)
    if [ "$variant" = leak ]; then
        # Hard brake: the address space is limited so that a bug in the
        # counter-check never takes the machine with it.
        ( ulimit -v $((LEAK_MB * 1024)); exec "$WORK/soak_$variant" ) > "$OUT/measurement-$variant.tsv"
    else
        "$WORK/soak_$variant" > "$OUT/measurement-$variant.tsv"
    fi
    rc=$?
    duration=$(( $(date +%s) - start ))
    if [ $rc -ne 0 ]; then
        echo "   ERROR: run $variant ended with $rc:"
        grep '^# fehler' "$OUT/measurement-$variant.tsv" | head -2
        exit 1
    fi
    echo "   $variant: $(grep '^# fertig' "$OUT/measurement-$variant.tsv") (${duration}s wall clock)"
done

# ---------------------------------------------------------------- 3. evaluation
echo
echo "-- 3. evaluation --"
LEAK_MINC=$((LEAK_CYCLES / 4))
if [ "$LEAK_MINC" -gt "$MINC" ]; then LEAK_MINC=$MINC; fi
python3 - "$OUT/measurement-gc.tsv" "$OUT/measurement-leak.tsv" "$MINC" "$LEAK_MINC" <<'PYEOF'
import sys

def read_tsv(path):
    lines = []
    done = None
    with open(path, encoding='utf-8') as f:
        for z in f:
            z = z.strip()
            if not z:
                continue
            if z.startswith('#'):
                if z.startswith('# fertig'):
                    done = z
                continue
            t = z.split('\t')
            if len(t) != 7:
                raise SystemExit(f'ERROR: {path}: a line with {len(t)} fields instead of 7: {z}')
            lines.append([int(x) for x in t])
    return lines, done

def median(xs):
    s = sorted(xs)
    n = len(s)
    if n == 0:
        return 0
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2

def verdict(path, minc):
    z, done = read_tsv(path)
    if len(z) < 8:
        raise SystemExit(f'ERROR: {path}: only {len(z)} samples -- too few for a verdict.')
    cycles = z[-1][1]
    if cycles < minc:
        raise SystemExit(f'ERROR: {path}: only {cycles} cycles, {minc} are required.')
    n = len(z)
    v = n // 4                      # warm-up phase: the first quarter
    second = z[v:2 * v] or z[v:v + 1]
    last = z[3 * v:] or z[-1:]
    rss2, rssl = median([r[2] for r in second]), median([r[2] for r in last])
    live2, livel = median([r[3] for r in second]), median([r[3] for r in last])
    # Monotonicity: does the RSS rise continuously after the warm-up phase?
    after = [r[2] for r in z[v:]]
    monotone = all(b >= a for a, b in zip(after, after[1:])) and after[-1] > after[0]
    # Threshold: a 5 % increase of the median counts as a leak.
    grew = rssl > rss2 * 1.05
    return {
        'path': path, 'cycles': cycles, 'samples': n, 'done': done,
        'rss2': rss2, 'rssl': rssl, 'live2': live2, 'livel': livel,
        'monotone': monotone, 'grew': grew,
        'leak': grew or monotone,
        'rss_max': max(r[2] for r in z), 'pause_max': max(r[6] for r in z),
        'runs': z[-1][4], 'heap': z[-1][5],
    }

gc = verdict(sys.argv[1], int(sys.argv[3]))
leak = verdict(sys.argv[2], int(sys.argv[4]))

def show(u, name):
    print(f'   {name}:')
    print(f'     cycles                {u["cycles"]}  ({u["samples"]} samples)')
    print(f'     RSS median 2nd quarter {u["rss2"]} KiB')
    print(f'     RSS median last quarter {u["rssl"]} KiB')
    print(f'     RSS maximum           {u["rss_max"]} KiB')
    print(f'     live objects          {u["live2"]} -> {u["livel"]}')
    print(f'     collections           {u["runs"]}, heap size {u["heap"]} B')
    print(f'     longest pause         {u["pause_max"]} ns')
    print(f'     verdict               {"LEAK" if u["leak"] else "no leak"}'
          f' (growth {"yes" if u["grew"] else "no"}, monotone {"yes" if u["monotone"] else "no"})')

show(gc, 'GC version   (lib/dom/soak_gc.fi)')
show(leak, 'reference count (lib/dom/soak_leak.fi, MUST leak)')

print()
errs = 0
if gc['leak']:
    print('   FAILED: the GC version leaks.')
    errs = 1
else:
    factor = leak['rssl'] / max(gc['rssl'], 1)
    print(f'   PASSED: the GC version keeps the consumption flat '
          f'({gc["rss2"]} -> {gc["rssl"]} KiB).')
    print(f'   The counter-check needs {factor:.1f} times as much at the end.')
if not leak['leak']:
    print('   FAILED: the counter-check does NOT leak -- the measurement is worthless.')
    errs = 1
else:
    print(f'   counter-check strikes: {leak["rss2"]} -> {leak["rssl"]} KiB, '
          f'{leak["livel"]} live objects.')
sys.exit(errs)
PYEOF
rc=$?

echo
if [ $rc -eq 0 ]; then
    echo "OK: DOM soak run passed (measurement series in $OUT/measurement-gc.tsv and $OUT/measurement-leak.tsv)."
else
    echo "ERROR: DOM soak run NOT passed."
fi
exit $rc
