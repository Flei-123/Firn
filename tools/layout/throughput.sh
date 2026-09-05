#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/layout/throughput.sh -- the cost of the LAYOUT in INSTRUCTIONS.
#
# NOT with the wall clock. On this machine the run time of one and the
# same binary scatters by more than ten per cent between runs; a number
# like that cannot carry a comparison between rounds.
# `valgrind --tool=callgrind` counts the instructions really executed and
# is reproducible down to the single instruction.
#
# Measured per page of testdata/realweb/, in two modes:
#     mode 0   HTML -> DOM -> cascade      -- the state after round 60
#     mode 1   the same plus box tree and layout
# The DIFFERENCE is the share of round 61, and it is reported per page,
# per element and per byte of the page.
#
# HONEST LIMIT: instructions are not run time. They say nothing about
# cache misses or branch prediction. They are the right metric for "how
# much work does this engine do per page", and they are used for that
# alone.
#
# Usage:  bash tools/layout/throughput.sh [binary]
set -uo pipefail
cd "$(dirname "$0")/../.."

BIN="${1:-.layout-work/layoutbench}"
WORK=.layout-work
CORPUS=testdata/realweb
UA=tools/layout/ua.css

mkdir -p "$WORK"
if [ ! -x "$BIN" ]; then
    echo "the binary $BIN is missing -- build it with tools/layout/run.sh"
    exit 1
fi
if ! command -v valgrind >/dev/null 2>&1; then
    echo "valgrind is missing -- no instruction measurement possible."
    exit 1
fi

job() {   # $1 = mode, $2 = page
    python3 - "$1" "$2" "$UA" <<'PY'
import os, struct, sys
sys.path.insert(0, os.path.join("tools", "layout"))
import realweb
mode, page, ua = sys.argv[1:4]
html = realweb.prepare(open(page, encoding="utf-8", errors="replace").read())
raw = html.encode("utf-8")
uab = open(ua, "rb").read()
out = struct.pack("<I", int(mode))
out += struct.pack("<II", 800, 600)
for part in (raw, uab, b""):
    out += struct.pack("<I", len(part)) + part
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

printf '%-26s %10s %10s %14s %14s %11s %9s\n' \
    PAGE BYTES ELEMENTS "CASCADE(Ir)" "FULL(Ir)" LAYOUT/ELEM LAYOUT/BYTE
echo "-------------------------------------------------------------------------------------------------"
total_layout=0
total_bytes=0
total_elements=0
total_boxes=0
for page in "$CORPUS"/*.html; do
    name=$(basename "$page")
    job 0 "$page" > "$WORK/job0.bin"
    job 1 "$page" > "$WORK/job1.bin"
    bytes=$(python3 -c "
import sys,struct
d=open('$WORK/job1.bin','rb').read()
print(struct.unpack('<I', d[12:16])[0])")
    base=$(count "$WORK/job0.bin" "base_$name")
    full=$(count "$WORK/job1.bin" "full_$name")
    elements=$(sed -n 's/.*elems=\([0-9]*\).*/\1/p' "$WORK/full_$name.out")
    boxes=$(sed -n 's/.*boxes=\([0-9]*\).*/\1/p' "$WORK/full_$name.out")
    if [ -z "$base" ] || [ -z "$full" ] || [ -z "$elements" ] || [ "$elements" = "0" ]; then
        printf '%-26s   MEASUREMENT FAILED\n' "$name"
        continue
    fi
    layout=$((full - base))
    printf '%-26s %10d %10d %14d %14d %11d %9d\n' \
        "$name" "$bytes" "$elements" "$base" "$full" \
        "$((layout / elements))" "$((layout / bytes))"
    total_layout=$((total_layout + layout))
    total_bytes=$((total_bytes + bytes))
    total_elements=$((total_elements + elements))
    total_boxes=$((total_boxes + boxes))
done
echo "-------------------------------------------------------------------------------------------------"
if [ "$total_elements" -gt 0 ]; then
    printf 'TOTAL LAYOUT share: %d instructions for %d elements (%d boxes) in %d bytes\n' \
        "$total_layout" "$total_elements" "$total_boxes" "$total_bytes"
    printf '                    %d instructions per element, %d per byte of the page\n' \
        "$((total_layout / total_elements))" "$((total_layout / total_bytes))"
    printf 'LAYOUT_PER_ELEMENT %d\n' "$((total_layout / total_elements))"
    printf 'LAYOUT_PER_BYTE %d\n' "$((total_layout / total_bytes))"
fi
