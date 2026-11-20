#!/usr/bin/env bash
set -uo pipefail
cd "$(dirname "$0")/../.."

FROZEN=tools/stability/api_std.txt
NOW=$(mktemp)
trap 'rm -f "$NOW"' EXIT

python3 -I tools/stability/api_extract.py lib tools/stability/core_modules.txt tools/stability/rt_stable.txt > "$NOW" || exit 1

if [ "${1:-}" = "--update" ]; then
    cp "$NOW" "$FROZEN"
    echo "api: frozen list updated ($(wc -l < "$FROZEN") entries)"
    exit 0
fi

removed=$(LC_ALL=C comm -23 <(LC_ALL=C sort "$FROZEN") <(LC_ALL=C sort "$NOW"))
added=$(LC_ALL=C comm -13 <(LC_ALL=C sort "$FROZEN") <(LC_ALL=C sort "$NOW"))

if [ -n "$added" ]; then
    echo "api: $(printf '%s\n' "$added" | wc -l) new entries (allowed; run api_check.sh --update to freeze them)"
    printf '%s\n' "$added" | head -20 | sed 's/^/  + /'
fi
if [ -n "$removed" ]; then
    echo "api: BREAKING CHANGE, frozen entries removed or changed:"
    printf '%s\n' "$removed" | sed 's/^/  - /'
    echo "api: a stable std item may only change with a new edition (docs/STABILITY.md)"
    exit 1
fi
echo "api: $(wc -l < "$FROZEN") frozen entries intact"
