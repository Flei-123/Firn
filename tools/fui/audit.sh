#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# tools/fui/audit.sh -- THE ACCESSIBILITY AUDIT OF EVERY fUi PROGRAM (r98).
#
#   1. tools/fui/audit_main.fi: the audit itself has teeth (an unnamed
#      button, a duplicate key and a secret in the export are each found)
#   2. every fui.app program in examples/fui/*.fi is built and run with
#      FUI_AUDIT=1: `app.run` then opens no window, runs lib/fui/audit.fi on
#      the tree it has built and returns the result as the exit code
#      (0 = every operable node named, no key twice under one parent, no
#      secret in the export). A new program in that directory is picked up
#      by the glob -- nobody has to remember to add it.
#   3. tools/fui/audit_bad.fi, a program with a nameless text field, MUST
#      fail the same run (exit 1)
#   The programs with a main of their own call the same audit from their
#   checks: tools/fui/gallery9_main.fi (section 10, three requests) and
#   examples/codehub/main.fi (the line "audit:" of every --png run).
#
# Usage: tools/fui/audit.sh        (W = work directory)
set -u
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W="${W:-/tmp/fui-audit}"
mkdir -p "$W"
fail=0

echo "== the audit has teeth (tools/fui/audit_main.fi) =="
"$FIRNC" --opt-level=dev -o "$W/audit_main" tools/fui/audit_main.fi || exit 1
"$W/audit_main" | tail -1 | tee "$W/unit.log"
grep -q 'AUDIT PASSED' "$W/unit.log" || fail=1

echo "== every fui.app program in examples/fui/ =="
n=0
for f in examples/fui/*.fi; do
    name=$(basename "$f" .fi)
    "$FIRNC" --opt-level=dev -o "$W/$name" "$f" || { echo "  $name: does not build"; fail=1; continue; }
    out=$(FUI_AUDIT=1 "$W/$name")
    rc=$?
    n=$((n + 1))
    if [ $rc -eq 0 ]; then
        echo "  $name: $out"
    else
        echo "  $name: FAILED (exit $rc): $out"
        fail=1
    fi
done
[ $n -ge 3 ] || { echo "  FAILED: only $n programs found"; fail=1; }

echo "== a program with a nameless text field must fail =="
"$FIRNC" --opt-level=dev -o "$W/audit_bad" tools/fui/audit_bad.fi || exit 1
out=$(FUI_AUDIT=1 "$W/audit_bad")
rc=$?
echo "  audit_bad: exit $rc: $out"
[ $rc -eq 1 ] || { echo "  FAILED: the audit let it through"; fail=1; }

[ $fail -eq 0 ] && echo "AUDIT OF EVERY PROGRAM PASSED" || echo "AUDIT OF EVERY PROGRAM FAILED"
exit $fail
