#!/usr/bin/env bash
# tools/mc/play/run_deco.sh -- item decorations (durability bar, glint) and inventory item offsets against a real Vanilla 1.20.1 client (needs the /root/.w5/vanref setup)
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
$FIRNC -o "$W/mcplay" demos/mcplay/main.fi 2>"$W/err" || { head -10 "$W/err"; echo "RESULT mc-play-deco: FAIL"; exit 1; }
bash tools/mc/play/vanref_shot.sh "$W/out" "$W/mcplay" >"$W/shot.log" 2>&1 || { tail -5 "$W/shot.log"; echo "RESULT mc-play-deco: FAIL"; exit 1; }
if python3 -I tools/mc/play/deco_check.py "$W/out"; then echo "RESULT mc-play-deco: ok"; else echo "RESULT mc-play-deco: FAIL"; exit 1; fi
