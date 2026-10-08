#!/usr/bin/env bash
# tools/jsonsort_cross/run.sh -- json_write_opts against Python's json.dumps (SKRIPT-LIBS r315).
#
# 1. N random documents (default 3300; JSONSORT_CROSS_N) -- nested objects and arrays,
#    quotes, backslashes, control characters, DEL, accents, CJK, characters above the BMP,
#    U+2028, keys that are prefixes of each other and keys whose code point order differs
#    from their UTF-16 order, integers up to the i64 limits, floats from random bit patterns,
#    subnormals, -0.0, documents nested 30-120 deep, and a non-finite family -- written by
#    json.dumps with random ensure_ascii/indent/separators/sort_keys, parsed by Firn, and
#    written again in 11 variants (sort_keys, indent=N / "\t", ensure_ascii, separators)
#    that are compared with json.dumps(json.loads(text), **kwargs) BYTE FOR BYTE.
# 2. The writer fuzz run (JSONSORT_FUZZ_N, default 20000): mutated and junk documents go
#    through parse -> write -> parse -> write; the second text must equal the first, the
#    sorted compact text of the original must equal that of the re-parsed one; no crash.
# 3. The other three build levels answer the same input byte for byte like the first.
#
#   JSONSORT_CROSS_FAST=1 -> release-fast only
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
export FIRNLIB="$(pwd)/lib"
N=${JSONSORT_CROSS_N:-3300}
FN=${JSONSORT_FUZZ_N:-20000}
SEED=${JSONSORT_CROSS_SEED:-20261008}
if ! command -v python3 >/dev/null 2>&1; then
    echo "  SKIP  python3 is not installed"
    exit 0
fi
W=$(mktemp -d "${TMPDIR:-/tmp}/firn-jsonsort.XXXXXX")
trap 'rm -rf "$W"' EXIT
python3 -I tools/jsonsort_cross/cross.py gen "$N" "$SEED" "$W/in.bin" "$W/meta.json" || exit 1
python3 -I tools/jsonsort_cross/cross.py fuzz "$FN" "$SEED" "$W/fuzz.bin" || exit 1
ERRORS=0
FIRST=""
STAGES="release-fast:--opt-level=release-fast no-opt:--no-opt dev-fast:--opt-level=dev-fast release-safe:--opt-level=release-safe"
[ "${JSONSORT_CROSS_FAST:-0}" = "1" ] && STAGES="release-fast:--opt-level=release-fast"
for st in $STAGES; do
    name=${st%%:*}; flag=${st#*:}
    if ! $FIRNC $flag -o "$W/cli.$name" tools/jsonsort_cross/jsonsort_cli.fi > "$W/cc.$name" 2>&1; then
        echo "  FAIL  jsonsort_cli does not compile [$name]"; head -5 "$W/cc.$name"; ERRORS=$((ERRORS + 1)); continue
    fi
    if ! "$W/cli.$name" dumps "$W/in.bin" "$W/out.$name"; then
        echo "  FAIL  jsonsort_cli dumps crashed [$name]"; ERRORS=$((ERRORS + 1)); continue
    fi
    if "$W/cli.$name" fuzz "$W/fuzz.bin" > "$W/fuzz.$name" 2>&1; then
        echo "  writer fuzz [$name]: $(cat "$W/fuzz.$name")"
    else
        echo "  FAIL  writer fuzz [$name]: $(head -c 300 "$W/fuzz.$name")"; ERRORS=$((ERRORS + 1))
    fi
    if [ -z "$FIRST" ]; then
        FIRST=$name
        if python3 -I tools/jsonsort_cross/cross.py check "$W/in.bin" "$W/out.$name" "$W/meta.json" > "$W/report" 2>&1; then
            echo "  json.dumps cross-check ok [$name]"
        else
            echo "  FAIL  json.dumps cross-check [$name]"; ERRORS=$((ERRORS + 1))
        fi
        cat "$W/report" | head -40
    elif cmp -s "$W/out.$FIRST" "$W/out.$name"; then
        echo "  [$name] identical to [$FIRST]"
    else
        echo "  FAIL  [$name] answers differ from [$FIRST]"; ERRORS=$((ERRORS + 1))
    fi
done
[ "$ERRORS" -eq 0 ] && echo "jsonsort_cross: ok" || echo "jsonsort_cross: $ERRORS FAILED"
exit $((ERRORS > 0))
