#!/usr/bin/env bash
# tools/mc/biomezoom/run.sh -- reference values for tests 4092 / 4093 / 4094 from the REAL 1.20.1 classes (obfuscated server jar:
# aow = LinearCongruentialGenerator, cnm = BiomeManager, dwj = PerlinSimplexNoise, dia = LegacyRandomSource). Prints LCG / FID / BIOME / BSUM /
# SGRID / SWAMP lines; the tests hold these numbers. Needs java 17 and the 1.20.1 server jar (default /root/.w5/jars/server-1.20.1.jar).
set -euo pipefail
JAR=${1:-/root/.w5/jars/server-1.20.1.jar}
HERE=$(cd "$(dirname "$0")" && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
mkdir -p "$D/o" "$D/x"
unzip -q "$JAR" 'META-INF/versions/*' 'META-INF/libraries/*' -d "$D/o"
unzip -q "$D"/o/META-INF/versions/*/server-*.jar -d "$D/x"
CP="$D/x:$(find "$D/o/META-INF/libraries" -name '*.jar' | tr '\n' ':')"
mkdir -p "$D/h"
javac -cp "$CP" -d "$D/h" "$HERE/Ref.java"
java -cp "$D/h:$CP" Ref
