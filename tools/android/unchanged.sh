#!/usr/bin/env bash
# tools/android/unchanged.sh -- THE COUNTER-CHECK OF ROUND ANDROID.
#
# The round added a third target. The auflage was: nothing that is green
# today may change. For a code generator "did not change" has a sharper
# meaning than "still passes" -- the EMITTED TEXT may not have moved a
# character, on either of the two old targets.
#
# So this script compiles every case of the corpus with the compiler from
# BEFORE the round and with the one from after, for `x86_64-linux` and for
# `aarch64-linux`, and compares the assembly octet for octet. A single
# differing character is a failure, and the file is named.
#
#   BASELINE_FIRNC=<path to the old firnc> tools/android/unchanged.sh
#
# Without `BASELINE_FIRNC` the script says what it needs and skips -- it is
# a comparison against a predecessor and cannot invent one.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

NEW="$ROOT/compiler/target/release/firnc"
OLD=${BASELINE_FIRNC:-}
WORK="$ROOT/.unchanged-work"

if [ ! -x "$NEW" ]; then
    echo "firnc is missing: $NEW"
    exit 1
fi
if [ -z "$OLD" ] || [ ! -x "$OLD" ]; then
    echo "SKIP: no baseline compiler."
    echo "      Build the commit before this round somewhere and point"
    echo "      BASELINE_FIRNC=<path>/compiler/target/release/firnc at it."
    exit 0
fi

rm -rf "$WORK"
mkdir -p "$WORK"

SAME=0
DIFFTEXT=0
DIFFRC=0
for f in tests/*.fi; do
    b=$(basename "$f" .fi)
    for t in x86_64-linux aarch64-linux; do
        "$OLD" --target=$t --emit=asm -o "$WORK/$b.$t.old" "$f" >/dev/null 2>&1
        o=$?
        "$NEW" --target=$t --emit=asm -o "$WORK/$b.$t.new" "$f" >/dev/null 2>&1
        n=$?
        if [ $o -ne $n ]; then
            echo "  ACCEPTANCE CHANGED  $b $t (old exit $o, new exit $n)"
            DIFFRC=$((DIFFRC + 1))
            continue
        fi
        [ $o -ne 0 ] && continue
        if cmp -s "$WORK/$b.$t.old" "$WORK/$b.$t.new"; then
            SAME=$((SAME + 1))
        else
            echo "  TEXT CHANGED        $b $t"
            DIFFTEXT=$((DIFFTEXT + 1))
        fi
    done
done

echo
echo "== the two old targets, before and after round ANDROID =="
echo "  identical assembly   $SAME"
echo "  changed assembly     $DIFFTEXT"
echo "  changed acceptance   $DIFFRC"
rm -rf "$WORK"
[ "$DIFFTEXT" -eq 0 ] && [ "$DIFFRC" -eq 0 ] && exit 0
exit 1
