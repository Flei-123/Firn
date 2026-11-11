#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
set -u
R=$(cd "$(dirname "$0")/../.." && pwd)
FIRNC=${FIRNC:-$R/compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
for tool in Xvfb xdotool xwd xclip python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo "SKIP: $tool missing"; exit 0; }
done
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
FIRNLIB=$R/lib "$FIRNC" --opt-level=dev -o "$D/terminal" "$R/examples/fui_apps/terminal/main.fi" || exit 1
python3 -I "$R/tools/fui/term_x11.py" "$D/terminal" "${PROOF_DIR:-$D/proof}"
