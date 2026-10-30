#!/usr/bin/env bash
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
GOLD=$HERE/../golden/ds_tpl
mkdir -p "$GOLD"
while read -r name theme w h tag min app; do
    "$HERE/run_ref.sh" "$name" "$theme" "$w" "$h" "$GOLD/$name-$theme.png"
done < "$HERE/cases.txt"
