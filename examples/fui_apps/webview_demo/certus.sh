#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# FUI_WEBVIEW_CERTUS=<Certus tree with .flib2 and lib/embed> bash certus.sh <out-binary>
# Builds the demo with the Certus Glass backend. Library roots: Certus' .flib2 with fui and plat replaced by this checkout.
set -eu
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
CERTUS="${FUI_WEBVIEW_CERTUS:?set FUI_WEBVIEW_CERTUS to a Certus tree}"
OUT="${1:?usage: certus.sh <out-binary>}"
FIRNC="${FIRNC:-$CERTUS/tools/firnc}"
[ -d "$CERTUS/.flib2" ] || { echo "no $CERTUS/.flib2 (run tools/work/mkflib.sh there)" >&2; exit 1; }
[ -e "$CERTUS/lib/embed/webview_fui.fi" ] || { echo "no lib/embed/webview_fui.fi in $CERTUS" >&2; exit 1; }
D="$(mktemp -d)"; trap 'rm -rf "$D"' EXIT
mkdir -p "$D/lib" "$D/app"
for l in "$CERTUS"/.flib2/*; do ln -s "$(readlink -f "$l")" "$D/lib/$(basename "$l")"; done
rm -f "$D/lib/fui" "$D/lib/plat"
ln -s "$REPO/lib/fui" "$D/lib/fui"
ln -s "$REPO/lib/plat" "$D/lib/plat"
[ -e "$D/lib/embed" ] || ln -s "$CERTUS/lib/embed" "$D/lib/embed"
cp "$HERE/main.fi" "$D/app/main.fi"
cp "$HERE/demoview_certus.fi" "$D/app/demoview.fi"
FIRNLIB="$D/lib" "$FIRNC" --opt-level="${LEVEL:-release-fast}" -o "$OUT" "$D/app/main.fi"
