#!/usr/bin/env bash
# tools/regex_look/run.sh -- lookaround in lib/regex (r320): the cross-check
# against Python's `re` and the MEASURED worst case.
#
#   1. check_look.py: REGEX_LOOK_CASES (default 6000) random patterns with at
#      least one lookaround, no backreferences, against Python's `re` (first
#      match, every group offset, the replace-all result) -- plus the two
#      OpenPlan patterns. The probe is built in all four stages and each stage
#      must agree with Python. Counter-check: with the lookahead questions
#      inverted on the Python side, the check must FAIL.
#   2. The existing cross-check (tools/libmvp/check_regex.py, 20,000 random
#      patterns without lookaround) must stay green with the new engine.
#   3. THE WORST CASE. worst.fi times one scan of "a" x n for the patterns whose
#      lookahead looks at the whole rest of the text from every position. n
#      doubles; the time per doubling is the exponent: x2 linear, x4 quadratic.
#      Asserted: no lookaround pattern grows faster than x6.5 per doubling (an
#      exponential one would show x100), nested lookarounds included (the memo),
#      and the plain pattern without lookaround stays linear. For contrast the
#      same kind of pattern is timed in Python's backtracking `re`.
# REGEX_LOOK_FAST=1: one stage only.
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W=$(mktemp -d /tmp/firn-relook.XXXXXX)
trap 'rm -rf "$W"' EXIT
CASES=${REGEX_LOOK_CASES:-6000}
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }

STAGES="release-fast:--opt-level=release-fast release-safe:--opt-level=release-safe dev-fast:--opt-level=dev-fast no-opt:--no-opt"
[ "${REGEX_LOOK_FAST:-0}" = "1" ] && STAGES="release-safe:--opt-level=release-safe"

for stage in $STAGES; do
    name=${stage%%:*}
    opt=${stage#*:}
    if ! "$FIRNC" $opt -o "$W/probe.$name" tools/libmvp/regex_probe.fi 2> "$W/err"; then
        report "$name: tools/libmvp/regex_probe.fi does not compile"; head -5 "$W/err" | sed 's/^/        /'; continue
    fi
    if python3 -I tools/regex_look/check_look.py "$W/probe.$name" "$CASES" > "$W/look.$name.out" 2>&1; then
        echo "  regex: $name: $(tail -1 "$W/look.$name.out")"
    else
        report "$name: lookaround differs from Python"; head -12 "$W/look.$name.out" | sed 's/^/        /'
    fi
done
first=$(ls "$W"/probe.* | head -1)
if LOOK_BREAK=1 python3 -I tools/regex_look/check_look.py "$first" 1000 > "$W/break.out" 2>&1; then
    report "counter-check: inverting the lookahead questions was not noticed"
else
    echo "  regex: counter-check: with every lookahead inverted on the Python side the check fails ($(tail -1 "$W/break.out" | sed 's/.*, //'))"
fi
if python3 -I tools/libmvp/check_regex.py "$first" 20000 > "$W/old.out" 2>&1; then
    echo "  regex: unchanged cross-check without lookaround: $(tail -1 "$W/old.out")"
else
    report "tools/libmvp/check_regex.py is not green any more"; tail -8 "$W/old.out" | sed 's/^/        /'
fi

# ---- the worst case, measured (release-fast build: numbers, not verdicts, so one stage)
if ! "$FIRNC" --opt-level=release-fast -o "$W/worst" tools/regex_look/worst.fi 2> "$W/err"; then
    report "tools/regex_look/worst.fi does not compile"; head -5 "$W/err" | sed 's/^/        /'
else
python3 -I - "$W/worst" <<'PY' || ERRORS=$((ERRORS + 1))
import subprocess, sys, time, re
worst = sys.argv[1]
cases = [
    ("a*b",                            "no lookaround (baseline)",            3.2),
    ("(?<=a{1,40})b",                  "lookbehind, width 40",                3.2),
    ("(?=.*z)a",                       "lookahead to the end, every position", 6.5),
    ("(?=(?:a+)+b)a",                   "ambiguous body (2^n for a backtracker)", 6.5),
    ("(?=(?=.*z).*y)a",                "nested, depth 2",                     6.5),
    ("(?=(?=(?=.*z).*y).*x)a",         "nested, depth 3",                     6.5),
    ("(?<=(?=.*z))a",                  "lookahead inside a lookbehind",       6.5),
]
ns = [500, 1000, 2000, 4000]
bad = 0
print("  regex: worst case, microseconds for one scan of \"a\" x n (best of 3), and the growth per doubling of n")
print("  %-28s %-38s %s   %s" % ("pattern", "what", "  ".join("%9d" % n for n in ns), "growth (limit)"))
for pat, what, limit in cases:
    t = [int(subprocess.run([worst, pat, str(n)], capture_output=True, text=True, check=True).stdout) for n in ns]
    # the growth of the LAST doubling (small n is dominated by the constant)
    g = [t[i + 1] / max(t[i], 1) for i in range(len(t) - 1)]
    last = g[-1]
    ok = last <= limit
    if not ok: bad += 1
    print("  %-28s %-38s %s   x%.1f x%.1f x%.1f (<= %.1f) %s" % (pat, what, "  ".join("%9d" % x for x in t), g[0], g[1], g[2], limit, "ok" if ok else "TOO FAST GROWING"))
# the contrast: Python's backtracking engine on a catastrophic pattern
tt = []
for n in (18, 20, 22):
    t0 = time.perf_counter(); re.search(r"(a+)+b", "a" * n); tt.append(time.perf_counter() - t0)
print("  regex: contrast, Python re.search('(a+)+b', 'a'*n): n=18 %.3f s, n=20 %.3f s, n=22 %.3f s (x%.1f per +2: exponential, 2^n); here (?=(?:a+)+b) over n=4000 is the row above" % (tt[0], tt[1], tt[2], tt[2] / max(tt[1], 1e-9)))
sys.exit(1 if bad else 0)
PY
fi

if [ $ERRORS -eq 0 ]; then
    echo "REGEX LOOK OK"
    exit 0
fi
echo "REGEX LOOK FAILED ($ERRORS)"
exit 1
