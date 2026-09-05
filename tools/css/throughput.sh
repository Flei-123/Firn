#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/css/throughput.sh -- the throughput of the CSS path in INSTRUCTIONS.
#
# NOT with the wall clock. On this machine the run time of the very same
# binary scatters by more than 10 % between runs; a number like that cannot
# carry a comparison between rounds. `valgrind --tool=callgrind` counts the
# instructions really executed and is reproducible down to the single
# instruction (the same reasoning as in bench/instr.sh).
#
# Measured per page of testdata/realweb/, in two modes:
#     mode 0   HTML -> DOM only              -- the baseline
#     mode 1   the whole path                -- DOM + CSS + selectors + cascade
# The difference is the share of the CSS engine; it is reported per page,
# per element and per byte of the page.
#
# HONEST LIMIT: instructions are not run time. They say nothing about cache
# misses or branch prediction. They are the right metric for "how much work
# does this engine do per page", and they are used here for that alone.
#
# Usage:  bash tools/css/throughput.sh [binary] [selectors-file]
set -uo pipefail
cd "$(dirname "$0")/../.."

BIN="${1:-.css-work/cssbench}"
# By default the measurement runs WITHOUT extra selectors: measured is the
# path a page really walks -- parse the stylesheet, match its rules,
# compute the style per node. The 60 selectors of the cross-check are a
# matter for the runner, not for the engine.
SELECTORS="${2:-/dev/null}"
WORK=.css-work
CORPUS=testdata/realweb
UA=tools/css/ua.css
AUTHOR=tools/css/bench.css

mkdir -p "$WORK"
if [ ! -x "$BIN" ]; then
    echo "the binary $BIN is missing -- build it with tools/css/run.sh"
    exit 1
fi
if ! command -v valgrind >/dev/null 2>&1; then
    echo "valgrind is missing -- no instruction measurement possible."
    exit 1
fi

job() {   # $1 = mode, $2 = page
    python3 - "$1" "$2" "$UA" "$AUTHOR" "$SELECTORS" <<'PY'
import struct, sys
mode, page, ua, author, selectors = sys.argv[1:6]
html = open(page, 'rb').read()
ua = open(ua, 'rb').read()
author = open(author, 'rb').read()
sel = b"".join(l.encode() + b"\n" for l in open(selectors, encoding='utf-8').read().split("\n")
               if l.strip() and not l.startswith("#"))
out = struct.pack('<I', int(mode))
for part in (html, ua, b"", author, sel):
    out += struct.pack('<I', len(part)) + part
sys.stdout.buffer.write(out)
PY
}

count() {   # $1 = job file, $2 = tag ; prints the instruction count
    if ! valgrind --tool=callgrind --callgrind-out-file="$WORK/$2.cg" \
            "$BIN" < "$1" > "$WORK/$2.out" 2> "$WORK/$2.log"; then
        echo ""
        return
    fi
    grep -oP 'I\s+refs:\s+\K[0-9,]+' "$WORK/$2.log" | tr -d ','
}

printf '%-26s %10s %14s %14s %14s %9s %8s\n' \
    PAGE BYTES ELEMENTS "HTML(Ir)" "FULL(Ir)" CSS/ELEM CSS/BYTE
echo "---------------------------------------------------------------------------------------------"
total_css=0
total_bytes=0
total_elements=0
for page in "$CORPUS"/*.html; do
    name=$(basename "$page")
    bytes=$(stat -c%s "$page")
    job 0 "$page" > "$WORK/job0.bin"
    job 1 "$page" > "$WORK/job1.bin"
    base=$(count "$WORK/job0.bin" "base_$name")
    full=$(count "$WORK/job1.bin" "full_$name")
    elements=$(awk '{print $1}' "$WORK/full_$name.out" 2>/dev/null)
    if [ -z "$base" ] || [ -z "$full" ] || [ -z "$elements" ]; then
        printf '%-26s   MEASUREMENT FAILED\n' "$name"
        continue
    fi
    css=$((full - base))
    per_elem=$((css / elements))
    per_byte=$((css / bytes))
    printf '%-26s %10d %14d %14d %14d %9d %8d\n' \
        "$name" "$bytes" "$elements" "$base" "$full" "$per_elem" "$per_byte"
    total_css=$((total_css + css))
    total_bytes=$((total_bytes + bytes))
    total_elements=$((total_elements + elements))
done
echo "---------------------------------------------------------------------------------------------"
if [ "$total_elements" -gt 0 ]; then
    printf 'TOTAL CSS share: %d instructions for %d elements in %d bytes\n' \
        "$total_css" "$total_elements" "$total_bytes"
    printf '                 %d instructions per element, %d per byte of the page\n' \
        "$((total_css / total_elements))" "$((total_css / total_bytes))"
fi
