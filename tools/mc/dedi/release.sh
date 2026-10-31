#!/usr/bin/env bash
# tools/mc/dedi/release.sh -- builds firnmc-server (Linux + Windows, release-fast) and publishes to /srv/store/firnmc-server/<id>/ and latest/.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
[ -x "$FIRNC" ] || FIRNC=/root/firn/compiler/target/release/firnc
export FIRNLIB="$(pwd)/lib"
STORE=${STORE:-/srv/store/firnmc-server}
ID=$(date +%Y%m%d-%H%M)-$(git rev-parse --short HEAD)
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
$FIRNC --opt-level=release-fast -o "$W/firnmc-server-linux-x86_64" demos/firnmc-server/main.fi
$FIRNC --target=x86_64-windows --opt-level=release-fast -o "$W/firnmc-server-windows-x86_64.exe" demos/firnmc-server/main.fi
echo "$ID" > "$W/version.txt"
(cd "$W" && sha256sum firnmc-server-linux-x86_64 firnmc-server-windows-x86_64.exe > SHA256SUMS)
mkdir -p "$STORE/$ID" "$STORE/latest"
cp "$W"/* "$STORE/$ID/"
cp "$W"/* "$STORE/latest/"
echo "published $ID -> $STORE/latest"
