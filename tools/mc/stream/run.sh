#!/usr/bin/env bash
# tools/mc/stream/run.sh -- cross-check of lib/mc/mc_stream.fi: tests/2773_mc_stream.fi in three build stages (the image of the stream
# -- inline, with 4 worker threads, asynchronous with an upload budget, neighbour wait, remesh, drop_far, cancel -- must equal the
# reference image built without the stream), then the benchmark smoke run (tools/mc/stream/bench.fi, radius 2) with its RESULT lines.
# `tools/mc/stream/run.sh --bench [radius] [nsec]` runs the full benchmark only (RESULT <key> <value> <unit> lines, see docs/mc/stream.md).
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
if [ "${1:-}" = "--bench" ]; then
    W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
    $FIRNC -o "$W/bench" tools/mc/stream/bench.fi || exit 1
    "$W/bench" "${2:-5}" "${3:-8}" "${4:-all}"
    exit 0
fi
if ! bash tools/mc/testmc.sh stream; then echo "RESULT mc-stream: FAIL"; exit 1; fi
W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
if ! $FIRNC -o "$W/bench" tools/mc/stream/bench.fi 2>"$W/err"; then echo "  FAIL  bench.fi does not compile"; head -5 "$W/err"; echo "RESULT mc-stream: FAIL"; exit 1; fi
if ! timeout 300 "$W/bench" 2 4 > "$W/out.txt" 2>&1; then echo "  FAIL  bench smoke run"; tail -3 "$W/out.txt"; echo "RESULT mc-stream: FAIL"; exit 1; fi
grep -q "RESULT cull_sections_cave" "$W/out.txt" && grep -q "RESULT load_w4_columns_per_s" "$W/out.txt" || { echo "  FAIL  bench output incomplete"; tail -3 "$W/out.txt"; echo "RESULT mc-stream: FAIL"; exit 1; }
echo "RESULT mc-stream: ok"
