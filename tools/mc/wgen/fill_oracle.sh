#!/usr/bin/env bash
set -euo pipefail
OUT=$1; DIM=$2; shift 2
JARS=${MC_JARS:-/root/.w5/jars}
JDK25=${JDK25:-/root/tools/jdk25}
WG=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/j" "$W/g" "$W/c"
unzip -qo "$JARS/server-1.20.1.jar" 'META-INF/versions/*' 'META-INF/libraries/*' -d "$W/j"
cp "$(ls "$W"/j/META-INF/versions/*/server-1.20.1.jar)" "$W/s.jar"
zip -q -d "$W/s.jar" 'META-INF/*.SF' 'META-INF/*.RSA' 'META-INF/*.DSA' >/dev/null 2>&1 || true
CP="$W/s.jar:$(find "$W/j/META-INF/libraries" -name '*.jar' | tr '\n' ':')"
python3 -I "$WG/../gen/oracle/mkapi.py" "$WG/oracle/v1_20_1/WApi.java.tmpl" "$JARS/mappings-1.20.1.txt" "$W/g/WApi.java"
"$JDK25/bin/javac" --release 17 -nowarn -cp "$CP" -d "$W/c" "$WG/oracle/WgFillOracle.java" "$W/g/WApi.java" 2>&1 | grep -v '^Note' || true
"$JDK25/bin/java" -XX:+UnlockDiagnosticVMOptions -XX:-UseLibmIntrinsic -cp "$W/c:$CP" WgFillOracle "$OUT" "$DIM" "$@" 2>&1 >/dev/null | head -20
