#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/nbt/run.sh -- NBT against something that is not this repository
# (round 76).
#
# THREE MEASUREMENTS, and the first one is the one that counts:
#
#   1. OCTET FOR OCTET AGAINST NOTCH'S REFERENCE FILE. `bigtest.nbt` is the
#      example of the NBT specification; `testdata/nbt/bigtest.nbt.gz` holds
#      it exactly as it is published. `tools/nbt/bigtest.fi` rebuilds it out
#      of `lib/std/nbt.fi`, and the first 1,543 octets have to be IDENTICAL.
#      Not "parses the same" -- identical. That catches a wrong order, a
#      wrong length field, a missing TAG_End and every endianness mistake at
#      once, and it cannot be satisfied by a reader and a writer that are
#      wrong in the same way.
#   2. FIELD FOR FIELD AGAINST A SECOND PARSER. `tools/nbt/dump.fi` (Firn)
#      and `tools/nbt/check.py` (Python, not one shared line) turn the same
#      file into the same canonical text, and `diff` compares them.
#   3. THE OTHER DIRECTION. Python writes an NBT file with all thirteen tag
#      types -- the extremes of every width, an empty string, an empty
#      compound, an empty list, a list of lists, three levels of nesting --
#      and the Firn reader has to produce the same text over it.
#
# Plus the counter-checks: a truncated file, a file with a tag number that
# does not exist and one with a negative array length have to be REFUSED.
# Without them 1-3 would pass with a reader that accepts everything.
#
# All of it in THREE build stages, so that the optimiser cannot be the
# difference.
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=compiler/target/release/firnc
W=$(mktemp -d /tmp/firn-nbt.XXXXXX)
trap 'rm -rf "$W"' EXIT
ERRORS=0
report() { echo "  FAIL  $1"; ERRORS=$((ERRORS + 1)); }
export FIRNLIB="$(pwd)/lib"

gzip -dc testdata/nbt/bigtest.nbt.gz > "$W/reference.nbt" || {
    echo "FAIL: testdata/nbt/bigtest.nbt.gz cannot be unpacked"; exit 1; }
REFN=$(stat -c%s "$W/reference.nbt")
echo "  reference bigtest.nbt: $REFN octets"

for stage in "release-fast:" "no-opt:--no-opt" "dev-fast:--opt-level=dev-fast"; do
    name=${stage%%:*}
    opt=${stage#*:}

    if ! $FIRNC $opt -o "$W/bigtest.$name" tools/nbt/bigtest.fi 2>"$W/err"; then
        report "$name: tools/nbt/bigtest.fi does not compile"
        sed 's/^/        /' "$W/err" | head -6
        continue
    fi
    if ! $FIRNC $opt -o "$W/dump.$name" tools/nbt/dump.fi 2>"$W/err"; then
        report "$name: tools/nbt/dump.fi does not compile"
        sed 's/^/        /' "$W/err" | head -6
        continue
    fi

    # --- 1. octet for octet against the reference --------------------------
    if ! "$W/bigtest.$name" "$W/mine.$name.nbt"; then
        report "$name: bigtest.fi could not write the file"
        continue
    fi
    OUT=$(python3 tools/nbt/check.py bytes "$W/mine.$name.nbt" "$W/reference.nbt" \
        $((REFN - 1)) 2>&1)
    case "$OUT" in
        IDENTICAL*) echo "  $name: bigtest $OUT (of $REFN, the last is its TAG_End)" ;;
        *) report "$name: $OUT" ;;
    esac

    # --- 2. field for field against the Python parser ----------------------
    "$W/dump.$name" "$W/mine.$name.nbt" > "$W/firn.$name.txt" 2>"$W/err"
    rc=$?
    if [ $rc -ne 0 ]; then
        report "$name: the Firn dumper failed with $rc"
        sed 's/^/        /' "$W/err" | head -6
        continue
    fi
    python3 tools/nbt/check.py dump "$W/mine.$name.nbt" > "$W/py.$name.txt" 2>"$W/err" || {
        report "$name: the Python parser failed"
        sed 's/^/        /' "$W/err" | head -6
        continue
    }
    if diff -q "$W/firn.$name.txt" "$W/py.$name.txt" >/dev/null; then
        echo "  $name: dump of the Firn file identical, $(wc -l < "$W/firn.$name.txt") lines"
    else
        report "$name: Firn and Python read the same file differently"
        diff "$W/firn.$name.txt" "$W/py.$name.txt" | head -10 | sed 's/^/        /'
    fi

    # --- 3. the other direction --------------------------------------------
    python3 tools/nbt/check.py write "$W/frompy.nbt" 2>/dev/null
    "$W/dump.$name" "$W/frompy.nbt" > "$W/firn2.$name.txt" 2>"$W/err"
    rc=$?
    if [ $rc -ne 0 ]; then
        report "$name: the Firn reader failed on the Python file, $rc"
        sed 's/^/        /' "$W/err" | head -6
    else
        python3 tools/nbt/check.py dump "$W/frompy.nbt" > "$W/py2.$name.txt"
        if diff -q "$W/firn2.$name.txt" "$W/py2.$name.txt" >/dev/null; then
            echo "  $name: dump of the Python file identical, $(wc -l < "$W/firn2.$name.txt") lines"
        else
            report "$name: Firn reads the Python file differently"
            diff "$W/firn2.$name.txt" "$W/py2.$name.txt" | head -10 | sed 's/^/        /'
        fi
    fi

    # --- the anonymous root of 1.20.2 ---------------------------------------
    "$W/bigtest.$name" "$W/anon.nbt" anon
    if "$W/dump.$name" "$W/anon.nbt" > /dev/null 2>&1; then
        report "$name: the anonymous root was read as a NAMED one"
    fi
    "$W/dump.$name" "$W/anon.nbt" anon > "$W/anon.txt" 2>/dev/null || \
        report "$name: the anonymous root (1.20.2) was not read"
    python3 tools/nbt/check.py dump "$W/anon.nbt" anon > "$W/anonpy.txt" 2>/dev/null
    diff -q "$W/anon.txt" "$W/anonpy.txt" >/dev/null || \
        report "$name: the anonymous root differs between Firn and Python"

    # --- the counter-checks: bad input has to be REFUSED --------------------
    # a) truncated in the middle of a value
    head -c 40 "$W/mine.$name.nbt" > "$W/trunc.nbt"
    if "$W/dump.$name" "$W/trunc.nbt" >/dev/null 2>&1; then
        report "$name: a truncated file was accepted"
    fi
    # b) a tag number that does not exist (13)
    python3 - "$W/mine.$name.nbt" "$W/badtag.nbt" <<'PY'
import sys
d = bytearray(open(sys.argv[1], 'rb').read())
d[8] = 13           # the type octet of the first entry
open(sys.argv[2], 'wb').write(d)
PY
    if "$W/dump.$name" "$W/badtag.nbt" >/dev/null 2>&1; then
        report "$name: tag number 13 was accepted"
    fi
    # c) a negative array length
    python3 - "$W/mine.$name.nbt" "$W/badlen.nbt" <<'PY'
import struct, sys
d = bytearray(open(sys.argv[1], 'rb').read())
i = d.find(b'byteArrayTest')
n = i + len('byteArrayTest (the first 1000 values of (n*n*255+n*7)%100, '
            'starting with n=0 (0, 62, 34, 16, 8, ...))')
d[n:n + 4] = struct.pack('>i', -5)
open(sys.argv[2], 'wb').write(d)
PY
    if "$W/dump.$name" "$W/badlen.nbt" >/dev/null 2>&1; then
        report "$name: a negative array length was accepted"
    fi
    # d) THE DEPTH LIMIT. 255 levels have to go through, 256 and 100,000 have
    #    to be REFUSED -- and refused with an exit code, not with a
    #    segmentation fault. This is the counter-check that found the frame
    #    size of the optimised build (docs/ROUND76.md): before the limit
    #    existed, 400 levels killed the process here and 1,962 killed it
    #    without the optimiser.
    for depth in 255 256 100000; do
        python3 - "$W/deep.nbt" "$depth" <<'PY'
import sys
n = int(sys.argv[2])
d = bytes([10]) + b'\x00\x00'
d += (bytes([10]) + b'\x00\x00') * n
d += bytes([0]) * (n + 1)
open(sys.argv[1], 'wb').write(d)
PY
        "$W/dump.$name" "$W/deep.nbt" >/dev/null 2>&1
        rc=$?
        if [ $rc -eq 139 ] || [ $rc -eq 11 ]; then
            report "$name: $depth nested compounds crashed the reader (signal $rc)"
        elif [ "$depth" = "255" ] && [ $rc -ne 0 ]; then
            report "$name: 255 nested compounds were refused ($rc), the limit is 256"
        elif [ "$depth" != "255" ] && [ $rc -eq 0 ]; then
            report "$name: $depth nested compounds were ACCEPTED"
        fi
    done
    echo "  $name: counter-checks (truncated / tag 13 / negative length / 255 ok, 256 and 100000 refused)"
done

if [ "$ERRORS" -eq 0 ]; then
    echo "RESULT nbt: ok"
    exit 0
fi
echo "RESULT nbt: $ERRORS errors"
exit 1
