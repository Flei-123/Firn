#!/usr/bin/env bash
# tools/vorbis/run.sh -- ton.vorbis (+ floor 0 files from floor0_gen.py, ton.vorbis_stream chain/pipe/seek) against libsndfile (soundfile, f32; <= 1 LSB of 16 bit) + mutation fuzz.
# Needs python3 with numpy + soundfile. Usage: run.sh [fuzz_runs]   (default 600)
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
RUNS=${1:-600}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
export FIRNLIB="$(pwd)/lib"
"$FIRNC" --opt-level=release-safe -o "$W/vorbis" tools/vorbis/vorbis_main.fi
n=0
for f in tests/data/vorbis/*.ogg; do
  "$W/vorbis" "$f" "$W/out.f32" > "$W/info.txt"
  printf '%s: ' "$(basename "$f")"; python3 -I tools/vorbis/compare.py "$f" "$W/out.f32"
  n=$((n+1))
done
python3 -I tools/vorbis/fuzz.py "$W/vorbis" "$RUNS" 7 tests/data/vorbis/*.ogg
"$FIRNC" --opt-level=release-safe -o "$W/stream" tools/vorbis/stream_main.fi
python3 -I tools/vorbis/stream_check.py "$W/stream" "$W" tests/data/vorbis/*.ogg
echo "vorbis: $n files within 1 LSB (incl. synthetic floor 0), fuzz clean, stream/chain/seek ok"
