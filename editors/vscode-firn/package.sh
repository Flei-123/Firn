#!/usr/bin/env bash
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${1:-$PWD}
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
cp -r "$HERE"/. "$STAGE"/
rm -rf "$STAGE/test" "$STAGE/node_modules" "$STAGE/.gitignore" "$STAGE/package.sh"
cd "$STAGE"
npm install --omit=dev --no-audit --no-fund >/dev/null
POLYFILL="$STAGE/../firn-vsce-file-polyfill.js"
echo "globalThis.File = globalThis.File || require('buffer').File;" > "$POLYFILL"
NODE_OPTIONS="--require $POLYFILL" npx --yes @vscode/vsce@2.32.0 package --allow-missing-repository --skip-license -o "$OUT/"
rm -f "$POLYFILL"
