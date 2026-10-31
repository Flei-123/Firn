#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# build_v777.sh -- regenerate tools/mcgen/data_extra (26.3) from the server jars and the generated lib/mc/v777 tables.
# Needs JDK 25, server-26.1.jar (base) and server-26.3.jar in /root/.w5/jars.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
JARS=${MC_JARS:-/root/.w5/jars}
JDK=${JDK25:-/root/tools/jdk25}
BASE=${MC_BASE_DATA:-/root/firn/tools/mcserver/node_modules/minecraft-data/minecraft-data/data}
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
for v in 26.1 26.3; do
    mkdir -p "$D/j$v" "$D/x$v" "$D/o$v" "$D/jd$v"
    (cd "$D/x$v" && unzip -q -o "$JARS/server-$v.jar" 'META-INF/versions/*')
    (cd "$D/j$v" && "$JDK/bin/java" -DbundlerMainClass=net.minecraft.data.Main -jar "$JARS/server-$v.jar" --reports --output "$D/o$v" >"$D/dg$v.log" 2>&1)
    CP="$D/x$v/META-INF/versions/$v/server-$v.jar:$(find "$D/j$v/libraries" -name '*.jar' | tr '\n' ':')"
    "$JDK/bin/javac" -cp "$CP" -d "$D/jd$v" "$ROOT/tools/mcgen/jar/JarDump.java"
    (cd "$D" && "$JDK/bin/java" -cp "$D/jd$v:$CP" JarDump "$D/dump$v.json" >/dev/null 2>&1)
done
rm -rf "$ROOT/tools/mcgen/data_extra"
python3 "$ROOT/tools/mcgen/jar_report_to_mcdata.py" --reports "$D/o26.3/reports" --dump "$D/dump26.3.json" \
    --base-data "$BASE" --base-reports "$D/o26.1/reports" --version 26.3 --protocol 777 \
    --delta "$ROOT/tools/mcgen/deltas/26.3.json" --out "$ROOT/tools/mcgen/data_extra"
python3 "$ROOT/tools/mcgen/overlay_data.py" "$BASE" "$ROOT/tools/mcgen/data_extra" "$D/data"
python3 "$ROOT/tools/mcgen/gen_protocol.py" --all "$D/data" "$D/gen" all >/dev/null
mkdir -p "$ROOT/lib/mc/v777"
cp "$D/gen/v777/"*.fi "$ROOT/lib/mc/v777/"
cp "$D/gen/mc_versions.fi" "$D/gen/mc_sem.fi" "$ROOT/lib/mc/"
