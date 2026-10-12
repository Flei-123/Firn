#!/usr/bin/env bash
# tools/audio/bank/run.sh -- audio.bank (JSON), stream voice, lock free mode + audio thread; --bench = mixer cost per block.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$W/main" tools/audio/bank/main.fi
"$W/main"
if [ "${1:-}" = "--bench" ]; then
  FIRNLIB="$(pwd)/lib" "$FIRNC" -o "$W/bench" tools/audio/bank/bench.fi
  "$W/bench"
fi
