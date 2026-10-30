#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
NR=${NR:-800}
GEN=tools/mc/gen
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
STRICT="-XX:+UnlockDiagnosticVMOptions -XX:-UseLibmIntrinsic"

mapfile_120() {
    local out=$JARS/mappings-1.20.1.txt
    [ -s "$out" ] && { echo "$out"; return; }
    local url
    url=$(curl -s -m 30 https://piston-meta.mojang.com/mc/game/version_manifest_v2.json | python3 -I -c "
import sys, json
for v in json.load(sys.stdin)['versions']:
    if v['id'] == '1.20.1': print(v['url'])")
    [ -n "$url" ] || return 1
    url=$(curl -s -m 30 "$url" | python3 -I -c "import sys, json; print(json.load(sys.stdin)['downloads']['server_mappings']['url'])")
    curl -s -m 120 -o "$out" "$url" && echo "$out"
}

build_probe() {
    local old=$1 stage=$2 tag=$3
    sed "s/SEED_COUNT: i32 = [0-9]*/SEED_COUNT: i32 = $NR/;s/OLD: bool = [a-z]*/OLD: bool = $old/" $GEN/probe.fi > "$W/probe_$tag.fi"
    $FIRNC $stage -o "$W/probe_$tag" "$W/probe_$tag.fi" 2>"$W/err" || { echo "  FAIL  compile probe ($tag)"; head -5 "$W/err"; return 1; }
}

compare() {
    local name=$1 ref=$2 got=$3
    if cmp -s "$ref" "$got"; then
        echo "  ok    $name: $(wc -l < "$ref") lines bit-identical"
    else
        echo "  FAIL  $name: $(diff "$ref" "$got" | grep -c '^<') lines differ"; diff "$ref" "$got" | head -4 | cut -c1-200; ERR=1
    fi
}

mkdir -p "$W/j26" && unzip -qo "$JARS/server-26.1.jar" 'META-INF/versions/*' 'META-INF/libraries/*' -d "$W/j26"
CP26=$(ls "$W"/j26/META-INF/versions/*/server-26.1.jar):$(find "$W/j26/META-INF/libraries" -name '*.jar' | tr '\n' ':')
mkdir -p "$W/c26" && "$JDK25/bin/javac" -nowarn -cp "$CP26" -d "$W/c26" $GEN/oracle/Oracle.java $GEN/oracle/v26_1/Api.java 2>&1 | grep -v '^Note'
"$JDK25/bin/java" $STRICT -cp "$W/c26:$CP26" Oracle "$NR" "$W/ref26.txt" >/dev/null 2>&1
[ -s "$W/ref26.txt" ] || { echo "  FAIL  26.1 oracle produced nothing"; exit 1; }
"$JDK25/bin/java" -cp "$W/c26:$CP26" Oracle "$NR" "$W/ref26_fast.txt" >/dev/null 2>&1

MAP=$(mapfile_120) || MAP=""
HAVE120=0
if [ -n "$MAP" ] && [ -f "$JARS/server-1.20.1.jar" ]; then
    mkdir -p "$W/j120" "$W/g120" "$W/c120"
    unzip -qo "$JARS/server-1.20.1.jar" 'META-INF/versions/*' 'META-INF/libraries/*' -d "$W/j120"
    cp "$W"/j120/META-INF/versions/1.20.1/server-1.20.1.jar "$W/server120.jar"
    zip -q -d "$W/server120.jar" 'META-INF/*.SF' 'META-INF/*.RSA' 'META-INF/*.DSA' >/dev/null 2>&1
    CP120="$W/server120.jar:$(find "$W/j120/META-INF/libraries" -name '*.jar' | tr '\n' ':')"
    python3 -I $GEN/oracle/mkapi.py $GEN/oracle/v1_20_1/Api.java.tmpl "$MAP" "$W/g120/Api.java" \
        && "$JDK25/bin/javac" --release 17 -nowarn -cp "$CP120" -d "$W/c120" $GEN/oracle/Oracle.java "$W/g120/Api.java" 2>&1 | grep -v '^Note'
    java $STRICT -cp "$W/c120:$CP120" Oracle "$NR" "$W/ref120.txt" old >/dev/null 2>&1
    [ -s "$W/ref120.txt" ] && HAVE120=1
fi
[ $HAVE120 = 1 ] || echo "  SKIP  1.20.1 oracle (mappings or jar missing)"

for stage in "" "--no-opt"; do
    name=${stage:-release}
    build_probe false "$stage" new || { ERR=1; continue; }
    "$W/probe_new" > "$W/got26.txt" || { echo "  FAIL  run ($name)"; ERR=1; continue; }
    compare "server-26.1 $name" "$W/ref26.txt" "$W/got26.txt"
    if [ $HAVE120 = 1 ]; then
        build_probe true "$stage" old || { ERR=1; continue; }
        "$W/probe_old" > "$W/got120.txt" || { echo "  FAIL  run old ($name)"; ERR=1; continue; }
        compare "server-1.20.1 $name" "$W/ref120.txt" "$W/got120.txt"
    fi
done

echo "  info  HotSpot Math.log intrinsic vs fdlibm: $(diff "$W/ref26_fast.txt" "$W/ref26.txt" | grep -c '^<') of $(wc -l < "$W/ref26.txt") lines differ in the Java oracle itself (Gaussian 1 ulp); Firn follows StrictMath"

mkdir -p "$W/badlib" && cp -r lib/. "$W/badlib/"
sed -i 's/0x6a09e667f3bcc909/0x6a09e667f3bcc90a/' "$W/badlib/noise/xoroshiro.fi"
sed "s/SEED_COUNT: i32 = [0-9]*/SEED_COUNT: i32 = 12/" $GEN/probe.fi > "$W/bad.fi"
FIRNLIB="$W/badlib" $FIRNC -o "$W/bad" "$W/bad.fi" 2>/dev/null && "$W/bad" > "$W/bad.txt"
head -c "$(stat -c %s "$W/bad.txt")" "$W/ref26.txt" | cmp -s - "$W/bad.txt" \
    && { echo "  FAIL  counter-check: a changed constant was NOT detected"; ERR=1; } \
    || echo "  ok    counter-check: a changed constant is detected"

[ $ERR = 0 ] && echo "mc/gen: all ok" || echo "mc/gen: FAILED"
exit $ERR
