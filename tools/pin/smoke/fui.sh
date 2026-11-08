#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
REPO=$1
VERSION=$2
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cd "$REPO/examples/fui_apps" || exit 1
[ "$(firn version | cut -d" " -f2)" = "$VERSION" ] || { echo "fui smoke: firnc shim does not resolve $VERSION"; exit 1; }
for app in notes settings; do
    firnc --opt-level=release-safe -o "$D/$app" "$app/main.fi" > "$D/$app.log" 2>&1 || { echo "fui smoke: $app"; tail -10 "$D/$app.log"; exit 1; }
done
firnc --target=wasm32-browser -o "$D/counter.wasm" ../fui/counter.fi > "$D/wasm.log" 2>&1 || { echo "fui smoke: wasm counter"; tail -10 "$D/wasm.log"; exit 1; }
