#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
HERE=tools/teststages

if [ "${1:-}" = "--list" ]; then
    awk -v list=1 -f "$HERE/select.awk" "$HERE/stages.txt" test.sh
    exit 0
fi

if [ "${1:-}" = "--count" ]; then
    awk -v list=1 -f "$HERE/select.awk" "$HERE/stages.txt" test.sh | tail -n 1 | cut -d: -f1
    exit 0
fi

stage="${1:?usage: select.sh <stage>|--list|--count}"
last=$(bash "$HERE/select.sh" --count)
case "$stage" in
    ''|*[!0-9]*) echo "stage must be a number 1..$last" >&2; exit 2 ;;
esac
if [ "$stage" -lt 1 ] || [ "$stage" -gt "$last" ]; then
    echo "stage must be 1..$last" >&2
    exit 2
fi
awk -v stage="$stage" -f "$HERE/select.awk" "$HERE/stages.txt" test.sh | sed "s#\.test-work/#.test-work-stage${stage}/#g"
