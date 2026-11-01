#!/usr/bin/env bash
set -euo pipefail
OUT=$1; DIM=$2; shift 2
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
TOOLS=${MC_TOOLS:-/root/.w5/sr}
WG=$(cd "$(dirname "$0")" && pwd)
MOJ="$JARS/server-1.20.1-mojang.jar"
mkdir -p "$TOOLS"
if [ ! -s "$MOJ" ]; then
    SS="$TOOLS/SpecialSource-1.11.4-shaded.jar"
    [ -s "$SS" ] || curl -sSfL -o "$SS" https://repo1.maven.org/maven2/net/md-5/SpecialSource/1.11.4/SpecialSource-1.11.4-shaded.jar
    T=$(mktemp -d)
    python3 -I "$WG/oracle/mk_srg.py" "$JARS/mappings-1.20.1.txt" "$T/m.srg"
    unzip -qo "$JARS/server-1.20.1.jar" 'META-INF/versions/*' -d "$T/j"
    "$JDK25/bin/java" -jar "$SS" -i "$(ls "$T"/j/META-INF/versions/*/server-1.20.1.jar)" -o "$MOJ.tmp" -m "$T/m.srg" >/dev/null 2>&1
    mv "$MOJ.tmp" "$MOJ"
    rm -rf "$T"
fi
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
unzip -qo "$JARS/server-1.20.1.jar" 'META-INF/libraries/*' -d "$W/j"
LIBS=$(find "$W/j/META-INF/libraries" -name '*.jar' | tr '\n' ':')
mkdir -p "$W/c"
"$JDK25/bin/javac" --release 17 -nowarn -cp "$MOJ:$LIBS" -d "$W/c" "$WG/oracle/WgSurfaceOracle.java" "$WG/oracle/WgCarve.java" "$WG/oracle/WgFeatures.java" 2>&1 | grep -v '^Note' || true
"$JDK25/bin/java" -XX:+UnlockDiagnosticVMOptions -XX:-UseLibmIntrinsic ${CARVE:+-Dwg.carve=1} ${FEATURES:+-Dwg.features=$FEATURES} ${CNTLOG:+-Dwg.cntlog=$CNTLOG} ${DRAWLOG:+-Dwg.drawlog=$DRAWLOG -Dwg.drawsel=$DRAWSEL ${COLPROBE:+-Dwg.colprobe=$COLPROBE} ${SETLOG:+-Dwg.setlog=1}} ${AQDBG:+-Dwg.aqdbg=$AQDBG -Dwg.aqout=${AQOUT:-/tmp/aqout.txt}} -Dwg.ftdir=$WG/../../mcgen/worldgen/1.20.1/placed_feature -Dwg.tags=$WG/../../mcgen/worldgen/1.20.1/tags/blocks -cp "$W/c:$MOJ:$LIBS" net.minecraft.world.level.levelgen.WgSurfaceOracle "$OUT" "$DIM" "$@" 2>&1 >/dev/null | grep -v WARN | head -80 || true
