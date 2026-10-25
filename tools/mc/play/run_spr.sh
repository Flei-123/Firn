#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
JAR=${SPR_JAR:-/root/.w5/jars/client-1.21.4.jar}
export FIRNLIB="$(pwd)/lib"
export MC_VANILLA_ASSETS=${MC_VANILLA_ASSETS:-/root/.w5/menu-assets} MC_VANILLA_INDEX=${MC_VANILLA_INDEX:-5}
export MC_VANILLA_JAR=$JAR SPR_NEW=1
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
export SPR_DUMP_DIR=$W
ERR=0
for t in tests/5370_mc_cont_spr.fi tests/5371_mc_creative_spr.fi tests/5431_mc_trade_strike.fi; do
  $FIRNC -o "$W/t" "$t" && "$W/t" || { echo "  FAIL  $t"; ERR=1; }
done
for k in enchant brewing anvil horse; do
  python3 -I tools/mc/play/hud_check.py spr "$W/$k.ppm" "$JAR" "$k" || ERR=1
done
python3 -I tools/mc/play/hud_check.py spr "$W/creative.ppm" "$JAR" creative || ERR=1
[ $ERR = 0 ] && echo "RESULT ok" || echo "RESULT FAIL"
exit $ERR
