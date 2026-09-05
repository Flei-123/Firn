#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# Throughput of the Firn tokenizer on TWO input corpora, after that -- if
# available -- html5ever (cargo --release) on THE SAME corpora.
#
#   Corpus A "html5lib": the inputs of the html5lib cases, concatenated
#       many times over. DELIBERATELY PATHOLOGICAL (almost only edge cases,
#       very many state changes per byte, hardly any long runs of text) -- a value
#       for the worst case. The reasoning is in tools/tokenizer/korpus.py.
#   Corpus B "realweb": eight stored real pages from testdata/realweb/
#       (Wikipedia, the WHATWG standard, W3C, rustdoc, Hacker News), ~4.6 MB,
#       unchanged as delivered. That is the everyday case.
#
# Both corpora are produced by tools/tokenizer/korpus.py; both sides (Firn and
# html5ever) get exactly the same bytes.
#
# It is measured THREE TIMES, what is reported is the best run per side (the
# scatter between runs is about 30 %). The factor is computed
# and printed, even when it misses the acceptance goal (<= 2x).
#
# FAIR COMPARISON (since 14.08.2026): what is measured is `tokenize_bench`, which only
# COUNTS tokens -- exactly like html5ever. The original driver additionally wrote
# html5lib JSON; measured with callgrind that was **14.7 % of all
# instructions** (`out_json_cp` 7.93 %, `out_json_cpbuf` 4.22 %, `out_word`
# 2.55 %). A factor that counts such work in does not measure the tokenizer.
# If `tokenize_bench` is not available, the script falls back to the JSON driver
# and says so.
#
# What is NOT taken out is the decoding to UTF-32 (`decode`, 28 % of the
# instructions), although html5ever does not need it: that is a real
# disadvantage of Firn's build and no unfairness of the measuring setup.
#
# Usage:  bash tools/tokenizer/throughput.sh [tokenizer-binary] [runs]
set -euo pipefail
cd "$(dirname "$0")/../.."
BIN="${1:-}"
if [ -z "$BIN" ]; then
    if [ -x .tokenizer-work/tokenize_bench ]; then
        BIN=.tokenizer-work/tokenize_bench
    else
        BIN=.tokenizer-work/tokenize
        echo "   NOTE: tokenize_bench is missing -- the JSON driver is measured."
        echo "         build it with: firnc -o .tokenizer-work/tokenize_bench lib/html/tokenize_bench.fi"
    fi
fi
RUNS="${2:-3}"
WORK=".tokenizer-work"
mkdir -p "$WORK"

# the best (smallest) time out of $RUNS runs
best_time() {
    local best=""
    local i a b t
    for ((i = 0; i < RUNS; i++)); do
        a=$(date +%s.%N)
        "$@" >/dev/null
        b=$(date +%s.%N)
        t=$(awk -v a="$a" -v b="$b" 'BEGIN{printf "%.6f", b-a}')
        best=$(awk -v x="$t" -v y="$best" 'BEGIN{if(y==""||x+0<y+0)print x;else print y}')
    done
    echo "$best"
}

measure_corpus() {
    local source="$1" description="$2"
    local html="$WORK/korpus.$source.html"
    local job="$WORK/korpus.$source.job"
    local out="$WORK/korpus.$source.out"

    echo "   -- corpus '$source' ($description)"
    if [ ! -f "$html" ] || [ ! -f "$job" ]; then
        python3 tools/tokenizer/korpus.py "$html" "$job" --source "$source"
    fi
    local size
    size=$(stat -c%s "$html")

    local tf
    tf=$(best_time sh -c "\"$BIN\" < \"$job\" > \"$out\"")
    if grep -q 'NICHT-UNTERSTUETZT' "$out"; then
        echo "      NOTE: the tokenizer did NOT process the corpus completely"
        echo "            (a state is not implemented) -- the MB/s are therefore no"
        echo "            comparable value and are shown for information only."
    fi
    awk -v t="$tf" -v n="$size" -v l="$RUNS" \
        'BEGIN{printf "      Firn      : %8.2f MB/s  (%.3f s for %.2f MB, best of %d)\n", n/t/1048576, t, n/1048576, l}'

    if [ -x bench/tokenizer/target/release/html5ever_bench ]; then
        local tr
        tr=$(best_time bench/tokenizer/target/release/html5ever_bench "$html")
        awk -v t="$tr" -v n="$size" -v l="$RUNS" \
            'BEGIN{printf "      html5ever : %8.2f MB/s  (%.3f s, best of %d)\n", n/t/1048576, t, l}'
        awk -v a="$tf" -v b="$tr" \
            'BEGIN{printf "      factor    : %.2fx slower than html5ever (acceptance goal <= 2.00x)\n", a/b}'
    else
        echo "      html5ever : not built -- build it with"
        echo "                  cargo build --release --manifest-path bench/tokenizer/Cargo.toml"
    fi
}

measure_corpus html5lib "edge cases of the test suite, deliberately pathological"
echo
measure_corpus realweb  "eight real pages from testdata/realweb/"
