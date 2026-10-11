#!/usr/bin/env bash
# tools/mc/noise/run.sh -- mc_noise against independent references:
#   * java.util.Random: the REAL class (javac/java), for the Java compatible LCG;
#   * python (ref.py): splitmix64, xoshiro256**, improved noise, simplex 2D/3D, octaves, seeded
#     permutation -- bit for bit (same float64 operation order);
#   * published vectors: splitmix64(seed 0) = e220a8397b1dcdaf 6e789e6aa1b965f4 06c45d188009454f,
#     improved noise (3.14, 42, 7) = 0.13691995878400012 (Ken Perlin's reference implementation).
# Counter-check: a changed constant must be detected.
set -uo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
ERR=0
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:-release-fast}
    $FIRNC $stage -o "$W/dump" tools/mc/noise/dump.fi 2>"$W/err" || { echo "  FAIL  $name: compile"; head -5 "$W/err"; ERR=1; continue; }
    "$W/dump" > "$W/firn.txt" || { echo "  FAIL  $name: run"; ERR=1; continue; }
    python3 -I tools/mc/noise/ref.py > "$W/py.txt" || { echo "  FAIL  python reference"; ERR=1; continue; }
    if command -v javac >/dev/null; then
        [ -f "$W/Ref.class" ] || javac -cp "$W" -d "$W" tools/mc/noise/Ref.java 2>"$W/jerr" || { echo "  FAIL  javac"; head -3 "$W/jerr"; ERR=1; }
        java -cp "$W" Ref > "$W/java.txt"
    else
        echo "  SKIP  java missing: java.util.Random lines not compared"; : > "$W/java.txt"
    fi
    grep -v '^jr ' "$W/firn.txt" > "$W/firn_py.txt"
    grep '^jr ' "$W/firn.txt" > "$W/firn_java.txt"
    if cmp -s "$W/firn_py.txt" "$W/py.txt"; then
        echo "  ok    $name: $(wc -l < "$W/py.txt") lines (splitmix64, xoshiro256**, improved noise, simplex 2D/3D, octaves, seeded permutation) bit-identical to python"
    else
        echo "  FAIL  $name: python reference differs"; diff "$W/firn_py.txt" "$W/py.txt" | head -6; ERR=1
    fi
    if [ -s "$W/java.txt" ]; then
        if cmp -s "$W/firn_java.txt" "$W/java.txt"; then
            echo "  ok    $name: java.util.Random: $(wc -l < "$W/java.txt") seeds x (6 nextInt, 8 nextInt(bound), nextLong, nextDouble, nextBoolean) identical to the real class"
        else
            echo "  FAIL  $name: java.util.Random differs"; diff "$W/firn_java.txt" "$W/java.txt" | head -6; ERR=1
        fi
    fi
done
# published vectors
grep -q '^sm0 e220a8397b1dcdaf' "$W/firn.txt" && grep -q '^sm0 6e789e6aa1b965f4' "$W/firn.txt" && grep -q '^sm0 06c45d188009454f' "$W/firn.txt" \
    && echo "  ok    published splitmix64 vector (seed 0)" || { echo "  FAIL  splitmix64 vector"; ERR=1; }
python3 -I - "$W/firn.txt" <<'PY' || ERR=1
import struct, sys
for l in open(sys.argv[1]):
    if l.startswith("pr 3.14"):
        v = struct.unpack(">d", bytes.fromhex(l.split()[-1]))[0]
        if abs(v - 0.13691995878400012) < 1e-15:
            print("  ok    published improved-noise vector noise(3.14, 42, 7) = %r" % v)
        else:
            print("  FAIL  improved-noise vector: %r" % v); sys.exit(1)
PY
# counter-check
cp tools/mc/noise/dump.fi "$W/dump_bad.fi"
mkdir -p "$W/badlib/mc" && cp -r lib/. "$W/badlib/" && sed -i 's/11400714819323198485/11400714819323198486/' "$W/badlib/mc/mc_noise.fi"
FIRNLIB="$W/badlib" $FIRNC -o "$W/dump_bad" "$W/dump_bad.fi" 2>/dev/null && "$W/dump_bad" | grep -v '^jr ' > "$W/bad.txt"
cmp -s "$W/bad.txt" "$W/py.txt" && { echo "  FAIL  counter-check: a changed constant was NOT detected"; ERR=1; } || echo "  ok    counter-check: a changed constant is detected"
[ $ERR -eq 0 ] && echo "RESULT mc-noise: ok" || echo "RESULT mc-noise: FAIL"
exit $ERR
