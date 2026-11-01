#!/bin/ash
set -e
INSTALL_DIR="${INSTALL_DIR:-/mnt/server}"
BINARY_FILE="${BINARY_FILE:-firnmc-server}"
ASSET="firnmc-server-linux-x86_64"
BASE="${FIRN_RELEASE_URL%/}"
if [ -z "$BASE" ]; then
    echo "FIRN_RELEASE_URL is empty"
    exit 1
fi
mkdir -p "$INSTALL_DIR/logs"
cd "$INSTALL_DIR"
echo "Downloading $BASE/$ASSET"
curl -fSL --retry 3 --connect-timeout 20 -o "$BINARY_FILE.download" "$BASE/$ASSET"
if curl -fsSL --retry 2 --connect-timeout 20 -o SHA256SUMS.download "$BASE/SHA256SUMS"; then
    expected=$(grep " $ASSET\$" SHA256SUMS.download | cut -d' ' -f1)
    actual=$(sha256sum "$BINARY_FILE.download" | cut -d' ' -f1)
    rm -f SHA256SUMS.download
    if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
        echo "Checksum mismatch for $ASSET"
        rm -f "$BINARY_FILE.download"
        exit 1
    fi
    echo "Checksum ok"
else
    echo "No SHA256SUMS found, skipping the checksum"
fi
mv -f "$BINARY_FILE.download" "$BINARY_FILE"
chmod +x "$BINARY_FILE"
echo "Installed $BINARY_FILE"
