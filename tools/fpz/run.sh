#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/fpz/run.sh -- ROUND B6, CHAPTER Z: the defence against
# fingerprinting, measured.
#
#   1. compile lib/browser/fpz_main.fi in THREE build stages
#      (opt / --no-opt / dev-fast) -- a defence that depends on the
#      optimiser is not one
#   2. the measurement itself (tools/fpz/fp_check.py): stability within one
#      session and one origin, separation between origins and between
#      sessions, the counter-check WITHOUT farbling, the largest deviation
#      of a colour channel, the `navigator` fields and the clock
#   3. the same run through the other two builds -- the numbers have to be
#      the SAME numbers
#   4. the regression limits from tools/fpz/minquota.txt
#
# WHY THE COUNTER-CHECK IS THE POINT. "500 origins give 500 different
# canvases" is also true of a program that returns pure noise, and that
# program would be useless: a script reads twice, sees a difference and
# knows. So the same path is walked once with the farbling taken out
# (`op 4`), and there the 500 origins have to give exactly ONE answer.
# Both numbers together say something; either alone says nothing.
set -uo pipefail
cd "$(dirname "$0")/../.."

FIRNC="compiler/target/release/firnc"
export FIRNLIB="$(pwd)/lib"
WORK=".fpz-work"
mkdir -p "$WORK"
ERRORS=0

if [ ! -x "$FIRNC" ]; then
    cargo build --release --manifest-path compiler/Cargo.toml
fi

echo "== 1. three build stages of lib/browser/fpz_main.fi =="
for STAGE in "opt:" "noopt:--no-opt" "dev:--opt-level=dev-fast"; do
    NAME=${STAGE%%:*}
    OPT=${STAGE#*:}
    if ! $FIRNC $OPT -o "$WORK/fpz_$NAME" lib/browser/fpz_main.fi \
        2>"$WORK/build_$NAME.log"; then
        echo "   FAILED: $NAME did not build"
        tail -5 "$WORK/build_$NAME.log"
        exit 1
    fi
done
echo "   opt, --no-opt and dev-fast built"

echo "== 2. the measurement (tools/fpz/fp_check.py) =="
python3 tools/fpz/fp_check.py "$WORK/fpz_opt" | tee "$WORK/opt.txt"
if ! grep -q '^FPZ OK' "$WORK/opt.txt"; then
    echo "   FAILED: the measurement did not pass"
    exit 1
fi

echo "== 3. the same measurement in the other two build stages =="
for NAME in noopt dev; do
    python3 tools/fpz/fp_check.py "$WORK/fpz_$NAME" > "$WORK/$NAME.txt" 2>&1
    if ! grep -q '^FPZ OK' "$WORK/$NAME.txt"; then
        echo "   FAILED: stage $NAME does not pass"
        tail -4 "$WORK/$NAME.txt"
        exit 1
    fi
    # The canvas lines have to be the SAME lines: a build stage that
    # farbled differently would mean the key stream depends on the
    # optimiser, and then it depends on the machine too.
    if ! diff <(grep -E 'distinct|deviation|touched' "$WORK/opt.txt") \
        <(grep -E 'distinct|deviation|touched' "$WORK/$NAME.txt") > /dev/null
    then
        echo "   FAILED: stage $NAME gives other numbers than the release build"
        exit 1
    fi
    echo "   $NAME: the same numbers"
done

echo "== 4. the regression limits (tools/fpz/minquota.txt) =="
CHECKS=$(grep -o '^FPZ OK: [0-9]*' "$WORK/opt.txt" | awk '{print $3}')
LIMIT=$(awk '/^fpz_checks /{print $2}' tools/fpz/minquota.txt)
echo "   checks: $CHECKS   (limit: $LIMIT)"
if [ "$CHECKS" -lt "$LIMIT" ]; then
    echo "   FAILED: fewer checks than before"
    exit 1
fi

grep -E '^FPZ OK' "$WORK/opt.txt"
