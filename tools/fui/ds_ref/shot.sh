#!/usr/bin/env bash
set -euo pipefail
HTML=$1
PNG=$2
W=$3
H=$4
PROFILE=$(mktemp -d)
trap 'rm -rf "$PROFILE"' EXIT
chromium --headless=new --no-sandbox --disable-gpu --hide-scrollbars --allow-file-access-from-files \
    --user-data-dir="$PROFILE" --force-device-scale-factor=1 --window-size="$W,$H" \
    --virtual-time-budget=2000 --screenshot="$PNG" "file://$HTML" > /dev/null 2>&1
