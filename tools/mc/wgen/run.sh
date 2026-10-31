#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$ROOT/lib"
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
WG=tools/mc/wgen
DATA=tools/mcgen/worldgen
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
STRICT="-XX:+UnlockDiagnosticVMOptions -XX:-UseLibmIntrinsic"

compare() {
    if cmp -s "$2" "$3"; then
        echo "  ok    $1: $(wc -l < "$2") lines bit-identical"
    else
        echo "  FAIL  $1: $(diff "$2" "$3" | grep -c "^<") lines differ ($(diff "$2" "$3" | grep "^<" | grep -c final_density) final_density)"; diff "$2" "$3" | grep final_density | head -4 | cut -c1-200; ERR=1
    fi
}

"$FIRNC" -o "$W/probe" $WG/probe.fi 2>"$W/err" || { echo "  FAIL  compile probe"; head -8 "$W/err"; exit 1; }

version_jar_cp() {
    local ver=$1 dir=$2
    mkdir -p "$dir" && unzip -qo "$JARS/server-$ver.jar" 'META-INF/versions/*' 'META-INF/libraries/*' -d "$dir"
    CP=$(ls "$dir"/META-INF/versions/*/server-$ver.jar):$(find "$dir/META-INF/libraries" -name '*.jar' | tr '\n' ':')
}

run_oracle() {
    local ver=$1 dim=$2 out=$3
    if [ "$ver" = 1.20.1 ]; then
        local map="$JARS/mappings-1.20.1.txt"
        [ -s "$map" ] || { echo "  SKIP  $ver (mappings missing)"; return 1; }
        version_jar_cp 1.20.1 "$W/j120"
        cp "$(ls "$W"/j120/META-INF/versions/*/server-1.20.1.jar)" "$W/s120.jar"
        zip -q -d "$W/s120.jar" 'META-INF/*.SF' 'META-INF/*.RSA' 'META-INF/*.DSA' >/dev/null 2>&1
        CP="$W/s120.jar:$(find "$W/j120/META-INF/libraries" -name '*.jar' | tr '\n' ':')"
        mkdir -p "$W/g120" "$W/c120"
        python3 -I tools/mc/gen/oracle/mkapi.py $WG/oracle/v1_20_1/WApi.java.tmpl "$map" "$W/g120/WApi.java" || return 1
        "$JDK25/bin/javac" --release 17 -nowarn -cp "$CP" -d "$W/c120" $WG/oracle/WgOracle.java "$W/g120/WApi.java" 2>&1 | grep -v '^Note'
        "$JDK25/bin/java" $STRICT -cp "$W/c120:$CP" WgOracle "$out" "$dim" >/dev/null 2>"$W/jerr_$ver"
    else
        version_jar_cp "$ver" "$W/j$ver"
        mkdir -p "$W/c$ver"
        "$JDK25/bin/javac" -nowarn -cp "$CP" -d "$W/c$ver" $WG/oracle/WgOracle.java $WG/oracle/v$(echo "$ver" | tr . _)/WApi.java 2>&1 | grep -v '^Note'
        "$JDK25/bin/java" $STRICT -cp "$W/c$ver:$CP" WgOracle "$out" "$dim" >/dev/null 2>"$W/jerr_$ver"
    fi
    [ -s "$out" ] || { echo "  FAIL  oracle $ver produced nothing"; head -5 "$W/jerr_$ver"; return 1; }
}

SPECS=${WG_SPECS:-"1.20.1 overworld|1.20.1 amplified|1.20.1 large_biomes|26.1 overworld|1.20.1 nether|1.20.1 end"}
IFS="|" read -ra SPEC_LIST <<< "$SPECS"
for spec in "${SPEC_LIST[@]}"; do
    set -- $spec
    ver=$1; dim=$2
    [ -d "$DATA/$ver" ] || { echo "  SKIP  $ver (no data)"; continue; }
    run_oracle "$ver" "$dim" "$W/ref_${ver}_$dim.txt" || { ERR=1; continue; }
    "$W/probe" "$DATA/$ver" "$dim" > "$W/got_${ver}_$dim.txt" || { echo "  FAIL  run $ver $dim"; ERR=1; continue; }
    compare "$ver $dim density" "$W/ref_${ver}_$dim.txt" "$W/got_${ver}_$dim.txt"
done

if [ -z "${WG_NO_GEN:-}" ]; then
    bash tools/mc/wgen/gen_gate.sh || ERR=1
    bash tools/mc/wgen/gen_join.sh || ERR=1
fi

[ $ERR = 0 ] && echo "mc/wgen: all ok" || echo "mc/wgen: FAILED"
exit $ERR
