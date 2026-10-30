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
REPORTS=${MC_REPORTS_261:-/root/.w5/rep-26.1/out/reports}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
unzip -qo "$JARS/server-26.1.jar" 'META-INF/versions/*' 'META-INF/libraries/*' -d "$W/j"
CP=$(ls "$W"/j/META-INF/versions/*/server-26.1.jar):$(find "$W/j/META-INF/libraries" -name '*.jar' | tr '\n' ':')
mkdir -p "$W/oc"
"$JDK25/bin/javac" -nowarn -encoding UTF-8 -cp "$CP" -d "$W/oc" tools/mc/cmd/ArgOracle.java 2>&1 | grep -v '^Note'
python3 -I - "$REPORTS/registries.json" "$W/types.txt" <<'PYEOF'
import json, sys
e = json.load(open(sys.argv[1]))["minecraft:entity_type"]["entries"]
open(sys.argv[2], "w").write("\n".join(sorted(e)) + "\n")
PYEOF
python3 -I tools/mc/cmd/arg_corpus.py "$W/corpus.txt" "$SEED" 2>"$W/count"
echo "  info  corpus: $(cat "$W/count") inputs (seed $SEED)"
"$JDK25/bin/java" -Dfile.encoding=UTF-8 -cp "$W/oc:$CP" ArgOracle "$W/corpus.txt" "$W/ref.txt" >/dev/null 2>&1
[ -s "$W/ref.txt" ] || { echo "  FAIL  oracle produced nothing"; exit 1; }
for stage in "" "--no-opt"; do
    name=${stage:-release}
    if ! $FIRNC $stage -o "$W/prog" tools/mc/cmd/arg_prog.fi 2>"$W/cerr"; then
        echo "  FAIL  compile ($name)"; head -8 "$W/cerr"; ERR=1; continue
    fi
    "$W/prog" "$W/corpus.txt" "$W/types.txt" > "$W/got.txt" 2>"$W/rerr" || { echo "  FAIL  run ($name)"; head -3 "$W/rerr"; ERR=1; continue; }
    if cmp -s "$W/ref.txt" "$W/got.txt"; then
        echo "  ok    args $name: $(wc -l < "$W/ref.txt") lines bit-identical"
    else
        echo "  FAIL  args $name: $(diff "$W/ref.txt" "$W/got.txt" | grep -c '^<') lines differ"
        diff "$W/ref.txt" "$W/got.txt" | head -${DIFF_LINES:-8} | cut -c1-200
        ERR=1
    fi
done
exit $ERR
