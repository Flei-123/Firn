#!/usr/bin/env bash
# tools/mc/nbt/run.sh -- mc_nbt against prismarine-nbt (an independent implementation):
#   * 60 random NBT trees (all 12 tag types, nested compounds / lists of lists / empty lists, unicode
#     names and strings, extreme numbers, arrays up to 40 elements) are WRITTEN by prismarine-nbt; Firn
#     reads them into a tree, the dump lines must equal the lines computed from the generated tree, and
#     writing the tree back must reproduce the input byte for byte -- for the named root and the nameless
#     root of 1.20.2+;
#   * fuzz: damaged / cut-off copies: no crash, no hang, whatever parses writes back to the octets consumed;
#   * counter-check: a damaged expectation is detected.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
NM=${MC_NODE_MODULES:-/root/firn/tools/mcserver/node_modules}
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
command -v node >/dev/null && [ -d "$NM/prismarine-nbt" ] || { echo "SKIP: node or prismarine-nbt missing"; exit 0; }
NODE_PATH=$NM node tools/mc/nbt/ref.cjs "$W/t" 60 7 >/dev/null || { echo "FAIL: reference generator"; exit 1; }
for stage in "" "--no-opt"; do
    name=${stage:-release-fast}
    $FIRNC $stage -o "$W/prog" tools/mc/nbt/prog.fi 2>"$W/err" || { echo "  FAIL  $name: compile"; head -5 "$W/err"; ERR=1; continue; }
    ok=0; bad=0
    for i in $(seq 0 59); do
        "$W/prog" dump "$W/t/t$i.nbt" 0 x > "$W/got.txt" 2>&1 && cmp -s "$W/got.txt" "$W/t/t$i.txt" || { bad=$((bad+1)); [ $bad -le 2 ] && { echo "  FAIL  $name t$i named"; diff "$W/got.txt" "$W/t/t$i.txt" | head -3; }; continue; }
        "$W/prog" dump "$W/t/t$i.anon.nbt" 1 x > "$W/got.txt" 2>&1 && { sed '1s/.*/root-name hex=/' "$W/t/t$i.txt" | cmp -s - "$W/got.txt"; } || { bad=$((bad+1)); [ $bad -le 2 ] && { echo "  FAIL  $name t$i anonymous"; diff "$W/got.txt" "$W/t/t$i.txt" | head -3; }; continue; }
        ok=$((ok+1))
    done
    [ $bad -eq 0 ] && echo "  ok    $name: $ok trees x (named + nameless root): dump == generated tree, write-back byte-identical ($(cat "$W"/t/t*.txt | wc -l) lines)" || { echo "  FAIL  $name: $bad trees differ"; ERR=1; }
    if [ -z "$stage" ]; then
        for f in t3 t11 t29; do
            "$W/prog" fuzz "$W/t/$f.nbt" 1500 5 x > "$W/fuzz.txt" 2>&1 && echo "  ok    $name fuzz $f: $(cat "$W/fuzz.txt")" || { echo "  FAIL  $name fuzz $f: $(cat "$W/fuzz.txt" | head -2)"; ERR=1; }
        done
    fi
done
# counter-check
sed '5s/ int / byte /' "$W/t/t0.txt" > "$W/bad.txt"; echo "x" >> "$W/bad.txt"
"$W/prog" dump "$W/t/t0.nbt" 0 x > "$W/got.txt"
cmp -s "$W/got.txt" "$W/bad.txt" && { echo "  FAIL  counter-check: a damaged expectation compared equal"; ERR=1; } || echo "  ok    counter-check: a damaged expectation is detected"
[ $ERR -eq 0 ] && echo "RESULT mc-nbt: ok" || echo "RESULT mc-nbt: FAIL"
exit $ERR
