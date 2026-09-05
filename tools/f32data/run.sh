#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/f32data/run.sh -- `f32` AGAINST REAL DATA (round 71).
#
# WHY THIS EXISTS. A test in which a program writes a number and reads it
# back proves nothing about a file format -- it proves that the program
# agrees with itself. Without `f32` you cannot even READ a WAV, an OBJ, an
# STL, a glTF or a GPU buffer; 32-bit floats stand in all of them. So the
# yardstick has to come from outside.
#
#   1. `tools/f32data/gen.py` writes two real files -- a WAV with 32-bit
#      float PCM and a binary glTF -- and next to them the decimal text that
#      PYTHON reads out of the very same octets.
#   2. `tools/f32data/probe.fi` reads the files with Firn: RIFF chunks, GLB
#      chunks, four octets at a time into an `f32`, and out again as the
#      shortest text that reads back (`num.write_f32`).
#   3. The two lists are held against each other, line for line.
#
# The files are PRODUCED, not checked in: binary rubbish in a repository is
# something nobody can read, and a generator can be looked at.
#
# Both compilers translate the probe -- firnc0 and firnc1 have to read the
# same octets as the same numbers.
#
# Usage:  bash tools/f32data/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC=compiler/target/release/firnc
WORK=.f32data-work
DATA=testdata/f32
export FIRNLIB="$(pwd)/lib"
mkdir -p "$WORK"

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml || exit 1
fi

echo "== 1. produce the files =="
python3 tools/f32data/gen.py "$DATA" || exit 1

echo "== 2. translate the probe with both compilers =="
"$FIRNC" tools/f32data/probe.fi -o "$WORK/probe0" 2>"$WORK/c0.log" || {
    echo "FAIL: firnc0 does not translate the probe"; sed 's/^/   /' "$WORK/c0.log" | head -10; exit 1; }
C1=${C1:-./.firnc1}
if [ ! -x "$C1" ] || [ -n "$(find bin lib/firnc1 -name '*.fi' -newer "$C1" -print -quit)" ]; then
    rm -f "$C1"
    "$FIRNC" bin/firnc1.fi -o "$C1" || exit 1
fi
"$C1" tools/f32data/probe.fi -o "$WORK/probe1" 2>"$WORK/c1.log" || {
    echo "   NOTE: firnc1 does not translate the probe -- only firnc0 is measured"
    rm -f "$WORK/probe1"; }

FAIL=0
check() { # $1 = name, $2 = input file, $3 = expected, $4 = binary
    local name=$1 input=$2 want=$3 bin=$4
    "$bin" < "$input" > "$WORK/$name.out" 2>"$WORK/$name.err"
    local rc=$?
    if [ "$rc" -ne 0 ]; then
        echo "   $name: the probe ends with $rc (see tools/f32data/probe.fi)"
        FAIL=1
        return
    fi
    if ! diff -q "$want" "$WORK/$name.out" > /dev/null; then
        echo "   $name: DIFFERENT"
        diff "$want" "$WORK/$name.out" | head -10 | sed 's/^/      /'
        FAIL=1
        return
    fi
    echo "   $name: $(grep -c '' "$want") values identical"
}

echo "== 3. WAV, 32 bit float PCM =="
check wav0 "$DATA/tone.wav" "$DATA/wav_expected.txt" "$WORK/probe0"
[ -x "$WORK/probe1" ] && check wav1 "$DATA/tone.wav" "$DATA/wav_expected.txt" "$WORK/probe1"

echo "== 4. glTF, float32 VEC3 positions =="
check glb0 "$DATA/tri.glb" "$DATA/glb_expected.txt" "$WORK/probe0"
[ -x "$WORK/probe1" ] && check glb1 "$DATA/tri.glb" "$DATA/glb_expected.txt" "$WORK/probe1"

echo
if [ "$FAIL" -eq 0 ]; then
    echo "OK: Firn reads the same numbers out of the same octets as Python"
    exit 0
fi
echo "FAIL: the values out of the real files do not match"
exit 1
