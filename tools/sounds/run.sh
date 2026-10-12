#!/usr/bin/env bash
# tools/sounds/run.sh [--update] -- synthesised sound sets: generate (twice, hashes must be identical), check levels /
# clipping / length / DC / edges / reference hashes, the bank loads in audio.bank and every event plays in audio.engine.
# Usage: bash tools/sounds/run.sh        (--update rewrites tools/sounds/ref/*.sha256 after an intended sound change)
set -euo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
export FIRNLIB="$(pwd)/lib"
"$FIRNC" -o "$W/gen" tools/sounds/gen/main.fi
"$FIRNC" -o "$W/bankcheck" tools/sounds/bankcheck.fi
mkdir -p tools/sounds/ref
for set in orientos mc; do
    "$W/gen" tools/sounds/$set.json "$W/$set" --preview "${set}_preview.wav"
    "$W/gen" tools/sounds/$set.json "$W/$set.again"
    if ! diff <(sha256sum "$W/$set"/*.wav | grep -v preview | sed 's|/.*/||') <(sha256sum "$W/$set.again"/*.wav | sed 's|/.*/||') >/dev/null; then
        echo "FAIL $set: second generation differs (not deterministic)"; exit 1
    fi
    echo "ok   $set: second generation is bit identical"
    python3 -I tools/sounds/check.py tools/sounds/$set.json "$W/$set" tools/sounds/ref/$set.sha256 "$@"
    "$W/bankcheck" "$W/$set/bank.json" "$W/$set/manifest.json"
done
echo "RESULT sounds: ok"
