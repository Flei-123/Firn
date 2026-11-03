#!/usr/bin/env bash
set -euo pipefail
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
HERE=$(cd "$(dirname "$0")" && pwd)
MOJ="$JARS/server-1.20.1-mojang.jar"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
unzip -qo "$JARS/server-1.20.1.jar" 'META-INF/libraries/*' -d "$W/j"
LIBS=$(find "$W/j/META-INF/libraries" -name '*.jar' | tr '\n' ':')
mkdir -p "$W/c"
"$JDK25/bin/javac" --release 17 -nowarn -cp "$MOJ:$LIBS" -d "$W/c" "$HERE/EquipDump.java" 2>&1 | grep -v '^Note' || true
"$JDK25/bin/java" -cp "$W/c:$MOJ:$LIBS" EquipDump 2>/dev/null
