#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
JDK25=${JDK25:-/root/tools/jdk25}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
"$FIRNC" ${WG_OPT:+--opt-level=$WG_OPT} -o "$W/jset_probe" tools/mc/wgen/jset_probe.fi 2>"$W/err" || { echo "  FAIL  compile jset_probe"; head -8 "$W/err"; exit 1; }
"$JDK25/bin/javac" --release 17 -nowarn -d "$W" tools/mc/wgen/oracle/HashSetOrder.java 2>&1 | grep -v '^Note'
"$JDK25/bin/java" -cp "$W" HashSetOrder 400 > "$W/ref.txt" || { echo "  FAIL  java reference"; exit 1; }
"$W/jset_probe" > "$W/got.txt" || { echo "  FAIL  run jset_probe"; exit 1; }
python3 -I tools/mc/wgen/jset_compare.py "$W/ref.txt" "$W/got.txt"
