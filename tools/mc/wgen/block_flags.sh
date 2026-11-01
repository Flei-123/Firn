#!/usr/bin/env bash
set -euo pipefail
OUT=$1
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
WG=$(cd "$(dirname "$0")" && pwd)
MOJ="$JARS/server-1.20.1-mojang.jar"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
unzip -qo "$JARS/server-1.20.1.jar" 'META-INF/libraries/*' -d "$W/j"
LIBS=$(find "$W/j/META-INF/libraries" -name '*.jar' | tr '\n' ':')
mkdir -p "$W/c"
"$JDK25/bin/javac" --release 17 -nowarn -cp "$MOJ:$LIBS" -d "$W/c" "$WG/oracle/WgBlockFlags.java" 2>&1 | grep -v '^Note' || true
"$JDK25/bin/java" -cp "$W/c:$MOJ:$LIBS" net.minecraft.world.level.levelgen.WgBlockFlags "$OUT" 2>&1 | grep -v WARN | head -20 || true
