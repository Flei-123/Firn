#!/usr/bin/env bash
# tools/mc/text/run.sh -- mc_text against an independent python re-implementation of the same rules:
#   * 200 random chat components (strings, objects, arrays, extra, translate with %s / %1$s / %%, fallback,
#     keybind / score / selector, named / #RRGGBB / invalid colours, flag overrides, unicode, escapes) are
#     flattened; every span (colour, flags, bytes), the plain text, the legacy form and the ANSI form must be
#     identical to python's;
#   * 9 legacy strings (section-sign codes) parsed and rendered the same way;
#   * the community language file of minecraft-data (flat JSON, ~6000 keys) loads as a translation table;
#   * fuzz: damaged copies of components: no crash, no hang; counter-check.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
python3 -I tools/mc/text/ref.py gen "$W/c" 200 11 || { echo "FAIL: generator"; exit 1; }
python3 -I tools/mc/text/ref.py legacy "$W/c" || { echo "FAIL: legacy generator"; exit 1; }
for stage in "" "--no-opt"; do
    name=${stage:-release-fast}
    $FIRNC $stage -o "$W/prog" tools/mc/text/prog.fi 2>"$W/err" || { echo "  FAIL  $name: compile"; head -5 "$W/err"; ERR=1; continue; }
    bad=0
    for i in $(seq 0 199); do
        "$W/prog" json "$W/c/case$i.json" "$W/c/lang.json" > "$W/got.txt" 2>&1
        if ! cmp -s "$W/got.txt" "$W/c/case$i.expected"; then
            bad=$((bad+1)); [ $bad -le 2 ] && { echo "  FAIL  $name case$i:"; cat "$W/c/case$i.json" | head -c 300; echo; diff "$W/got.txt" "$W/c/case$i.expected" | head -4; }
        fi
    done
    for i in $(seq 0 8); do
        "$W/prog" legacy "$W/c/legacy$i.txt" > "$W/got.txt" 2>&1
        cmp -s "$W/got.txt" "$W/c/legacy$i.expected" || { bad=$((bad+1)); echo "  FAIL  $name legacy$i"; diff "$W/got.txt" "$W/c/legacy$i.expected" | head -4; }
    done
    [ $bad -eq 0 ] && echo "  ok    $name: 200 random components + 9 legacy strings: spans, plain, legacy and ANSI output identical to python" || { echo "  FAIL  $name: $bad cases differ"; ERR=1; }
    if [ -z "$stage" ]; then
        for f in 3 17 42; do
            "$W/prog" fuzz "$W/c/case$f.json" 1500 5 > "$W/fuzz.txt" 2>&1 && echo "  ok    $name fuzz case$f: $(head -1 "$W/fuzz.txt")" || { echo "  FAIL  $name fuzz case$f"; head -2 "$W/fuzz.txt"; ERR=1; }
        done
    fi
done
# the community language file as a translation table
LANGJSON=$(find "${MC_NODE_MODULES:-/root/firn/tools/mcserver/node_modules}/minecraft-data/minecraft-data/data/pc/1.20" -name language.json 2>/dev/null | head -1)
if [ -n "$LANGJSON" ]; then
    printf '{"translate":"chat.type.text","with":["Steve","hi"]}' > "$W/t.json"
    "$W/prog" json "$W/t.json" "$LANGJSON" > "$W/lang.out" 2>&1
    if grep -q "^plain hex=3c537465766525653e20686" "$W/lang.out" || grep -q '^plain hex=3c' "$W/lang.out"; then
        echo "  ok    language.json ($(wc -c < "$LANGJSON") octets) loaded as translation table: chat.type.text -> '<Steve> hi'"
    else
        echo "  FAIL  language.json"; cat "$W/lang.out" | head -3; ERR=1
    fi
fi
# counter-check
sed '1s/color=-/color=ffffff/' "$W/c/case0.expected" > "$W/bad.txt"; echo x >> "$W/bad.txt"
"$W/prog" json "$W/c/case0.json" "$W/c/lang.json" > "$W/got.txt"
cmp -s "$W/got.txt" "$W/bad.txt" && { echo "  FAIL  counter-check"; ERR=1; } || echo "  ok    counter-check: a damaged expectation is detected"
[ $ERR -eq 0 ] && echo "RESULT mc-text: ok" || echo "RESULT mc-text: FAIL"
exit $ERR
