#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Builds the HTML5 tokenizer from lib/html/ (in Firn), drives it against the
# official html5lib test suite and prints the balance.
#
#   1. build the compiler (if needed)
#   2. compile lib/html/tokenize_main.fi (three build stages: opt/noopt/dev-fast
#      have to yield the same balance)
#   3. tools/tokenizer/harness.py: 6,810 cases, a balance per .test file
#      (the 4 xmlViolationTests run in XML mode, counter-check without it);
#      TWO quotas: only the token stream and additionally `--with-errors`, which
#      compares the 'errors' lists (WHATWG code name, line, column) exactly
#   4. throughput in MB/s on the test corpus; if bench/tokenizer/ is built,
#      html5ever next to it on THE SAME corpus
#
# Cases that are not supported count as a FAILURE. Nothing is filtered.
#
# Usage:  bash tools/tokenizer/run.sh [--fast]
set -euo pipefail

cd "$(dirname "$0")/../.."
FIRNC="compiler/target/release/firnc"
WORK=".tokenizer-work"
FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

mkdir -p "$WORK"
if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 0. test data unchanged? (sha256 against the upstream commit) =="
bash tools/tokenizer/verify_testdata.sh | sed 's/^/   /'
echo

echo "== 1. compile the tokenizer (Firn) =="
"$FIRNC" -o "$WORK/tokenize" lib/html/tokenize_main.fi
# Measuring version: it only counts tokens (a fair comparison with html5ever, which
# counts nothing else). See the head of tools/tokenizer/throughput.sh.
"$FIRNC" -o "$WORK/tokenize_bench" lib/html/tokenize_bench.fi
echo "   opt      : $WORK/tokenize"
if [ "$FAST" -eq 0 ]; then
    "$FIRNC" --no-opt -o "$WORK/tokenize.noopt" lib/html/tokenize_main.fi
    "$FIRNC" --opt-level=dev-fast -o "$WORK/tokenize.devfast" lib/html/tokenize_main.fi
    echo "   noopt    : $WORK/tokenize.noopt"
    echo "   dev-fast : $WORK/tokenize.devfast"
fi

echo
echo "== 1b. module proof for character references (lib/html/entities.fi) =="
"$FIRNC" -o "$WORK/entities_probe" lib/html/entities_probe.fi
python3 tools/tokenizer/check_entities.py "$WORK/entities_probe"

echo
echo "== 1c. name table: no fixed address, a failure is reported =="
"$FIRNC" -o "$WORK/entities_failure" lib/html/entities_failure.fi
"$WORK/entities_failure" > "$WORK/fallback1.txt"
"$WORK/entities_failure" > "$WORK/fallback2.txt"
sed 's/^/   /' "$WORK/fallback1.txt"
A1=$(head -1 "$WORK/fallback1.txt")
A2=$(head -1 "$WORK/fallback2.txt")
if [ "$A1" = "$A2" ]; then
    echo "   ERROR: two runs report the same table address ($A1) --"
    echo "           that points at a fixed address (MAP_FIXED)."
    exit 1
fi
echo "   second run: $A2 -- a different address, so chosen by the kernel (no MAP_FIXED)"

echo
echo "== 2. html5lib test suite (testdata/html5lib-tokenizer, 6,810 cases) =="
python3 tools/tokenizer/harness.py "$WORK/tokenize" \
        --json "$WORK/balance.json" --show 10 | tee "$WORK/balance.txt"

QUOTA=$(python3 -c "import json;d=json.load(open('$WORK/balance.json'));print(d['passed'])")
TOTAL=$(python3 -c "import json;d=json.load(open('$WORK/balance.json'));print(d['total'])")
QUOTA_ERRORS=$(python3 -c "import json;d=json.load(open('$WORK/balance.json'));print(d['passed_with_errors'])")

echo
echo "== 2a. second balance WITH a comparison of the parse error codes (--with-errors) =="
echo "   The 'errors' list of every case is compared in addition"
echo "   (WHATWG code name, line, column, in the order of the expectation)."
python3 tools/tokenizer/harness.py "$WORK/tokenize" --with-errors \
        --json "$WORK/balance.errors.json" --show 5 | tail -6

echo
echo "== 2b. counter-check without the XML adjustment (--no-xml-mode) =="
echo "   The 4 cases from xmlViolation.test expect the XML adjustment; without"
echo "   the job flag 3 of them have to fail."
python3 tools/tokenizer/harness.py "$WORK/tokenize" \
        --no-xml-mode --json "$WORK/balance.noxml.json" | tail -2

if [ "$FAST" -eq 0 ]; then
    echo
    echo "== 3. the same balance in all three build stages =="
    for m in noopt devfast; do
        python3 tools/tokenizer/harness.py "$WORK/tokenize.$m" --json "$WORK/balance.$m.json" >/dev/null
        Q=$(python3 -c "import json;print(json.load(open('$WORK/balance.$m.json'))['passed'])")
        QF=$(python3 -c "import json;print(json.load(open('$WORK/balance.$m.json'))['passed_with_errors'])")
        if [ "$Q" != "$QUOTA" ] || [ "$QF" != "$QUOTA_ERRORS" ]; then
            echo "   ERROR: $m yields $Q / $QF instead of $QUOTA / $QUOTA_ERRORS passed cases"
            exit 1
        fi
        echo "   $m: $Q without / $QF with error codes -- equal"
    done
fi

echo
echo "== 4. throughput =="
bash tools/tokenizer/throughput.sh "$WORK/tokenize" || true

echo
echo "== 5. regression limit =="
MIN=$(cat tools/tokenizer/minquota.txt)
MINF=$(cat tools/tokenizer/minquota_errors.txt)
echo "   without error codes: $QUOTA / $TOTAL   (limit: $MIN)"
echo "   with error codes:    $QUOTA_ERRORS / $TOTAL   (limit: $MINF)"
if [ "$QUOTA" -lt "$MIN" ] || [ "$QUOTA_ERRORS" -lt "$MINF" ]; then
    echo "   FAILED: a quota has fallen below the recorded limit."
    exit 1
fi
echo "OK: $QUOTA / $TOTAL without, $QUOTA_ERRORS / $TOTAL with error codes passed"
