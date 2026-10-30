#!/usr/bin/env bash
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
NAME=$1
THEME=$2
W=$3
H=$4
OUT=$5
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
python3 -I "$HERE/tpl_ref.py" "$NAME" "$THEME" "$W" "$H" "$D/p.html"
"$HERE/shot.sh" "$D/p.html" "$OUT" "$W" "$H"
