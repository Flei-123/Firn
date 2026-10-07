#!/usr/bin/env bash
# no_german_ci.sh -- CI entry of the shared "no German in code" guard (tools/english/no_german.py).
#
#   bash <firn>/tools/english/no_german_ci.sh [repo-root]     (default: the current directory)
#
# Uses <repo-root>/no-german.json (allow list) and <repo-root>/no-german.baseline.json (frozen counts,
# the ratchet: German may only go down). Exit 0 = ok. Prints one OK/FAIL line the section runners can grep.
# Repos find this script through $FIRN (default /root/firn).
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "${1:-.}" && pwd)"
if [ ! -f "$root/no-german.baseline.json" ]; then
    echo "  FAIL  no-german.baseline.json missing in $root (create: python3 $here/no_german.py --root $root --update-baseline)"
    exit 1
fi
out="$(python3 "$here/no_german.py" --root "$root" 2>&1)"; rc=$?
if [ $rc -eq 0 ]; then
    echo "  OK    no German in code beyond the baseline ($(echo "$out" | grep -o '{[^}]*}' | head -1))"
else
    echo "$out" | tail -25
    echo "  FAIL  more German in code than the baseline allows (guard: $here/no_german.py; a wanted line: 'english: ok')"
fi
exit $rc
