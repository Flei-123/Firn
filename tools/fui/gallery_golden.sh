#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
MAX="${GALLERY_GOLDEN_MAX:-0.5}"
PAGES="${GALLERY_PAGES:-16}"
MODE=compare
[ "$1" = "--update" ] && MODE=update
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/img"
"$FIRNC" --opt-level=dev -o "$T/gallery" examples/fui_apps/gallery/main.fi
p=0
while [ "$p" -lt "$PAGES" ]; do
    for theme in light dark; do
        "$T/gallery" --theme "$theme" --page "$p" --shot "$T/img/gallery-$(printf %02d "$p")-$theme.png" > /dev/null
    done
    p=$((p + 1))
done
python3 -I tools/fui/designlint.py examples/fui_apps/gallery
if [ "$MODE" = update ]; then
    python3 -I tools/fui/refdiff.py update "$T/img" tools/fui/golden_gallery
else
    python3 -I tools/fui/refdiff.py compare "$T/img" tools/fui/golden_gallery --max "$MAX"
fi
