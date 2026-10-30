#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
SEED=${SEED:-1}
DIR=tools/mc/brigadier
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0

compare() {
    local name=$1 ref=$2 got=$3
    if cmp -s "$ref" "$got"; then
        echo "  ok    $name: $(wc -l < "$ref") lines bit-identical"
    else
        echo "  FAIL  $name: $(diff "$ref" "$got" | grep -c '^<') lines differ"
        diff "$ref" "$got" | head -${DIFF_LINES:-6} | cut -c1-240
        ERR=1
    fi
}

mkdir -p "$W/j26"
unzip -qo "$JARS/server-26.1.jar" 'META-INF/libraries/*' -d "$W/j26"
BRIG=$(find "$W/j26/META-INF/libraries" -name 'brigadier-*.jar' | head -1)
[ -n "$BRIG" ] || { echo "  FAIL  brigadier jar not found in server-26.1.jar"; exit 1; }
mkdir -p "$W/oc"
"$JDK25/bin/javac" -nowarn -encoding UTF-8 -cp "$BRIG" -d "$W/oc" $DIR/Oracle.java 2>&1 | grep -v '^Note'
python3 -I $DIR/corpus.py "$W/corpus.txt" "$SEED" 2>"$W/count"
echo "  info  corpus: $(cat "$W/count") inputs (seed $SEED), oracle $(basename "$BRIG")"
"$JDK25/bin/java" -Dfile.encoding=UTF-8 -cp "$W/oc:$BRIG" Oracle $DIR/tree.txt "$W/corpus.txt" "$W/ref.txt" >"$W/oracle.log" 2>&1
[ -s "$W/ref.txt" ] || { echo "  FAIL  oracle produced nothing"; head -5 "$W/oracle.log"; exit 1; }

for stage in "" "--no-opt"; do
    name=${stage:-release}
    if ! $FIRNC $stage -o "$W/prog" $DIR/prog.fi 2>"$W/cerr"; then
        echo "  FAIL  compile prog ($name)"; head -8 "$W/cerr"; ERR=1; continue
    fi
    "$W/prog" $DIR/tree.txt "$W/corpus.txt" > "$W/got.txt" 2>"$W/rerr"
    [ $? -eq 0 ] || { echo "  FAIL  run ($name)"; head -3 "$W/rerr"; ERR=1; continue; }
    compare "brigadier $name" "$W/ref.txt" "$W/got.txt"
done

sed '0,/^lit hidden/s/^lit hidden req=3/lit hidden req=4/' $DIR/tree.txt > "$W/tree_alt.txt"
"$JDK25/bin/java" -Dfile.encoding=UTF-8 -cp "$W/oc:$BRIG" Oracle "$W/tree_alt.txt" "$W/corpus.txt" "$W/ref_alt.txt" >/dev/null 2>&1
if cmp -s "$W/ref.txt" "$W/ref_alt.txt"; then
    echo "  FAIL  counter-check: oracle ignores a changed tree"; ERR=1
else
    echo "  ok    counter-check: oracle reacts to a changed tree"
fi
exit $ERR
