#!/usr/bin/env sh
# SPDX-License-Identifier: MPL-2.0
set -eu
URL=${1:?usage: install.sh CHANNEL-URL [stable|nightly]}
CHANNEL=${2:-stable}
FIRN_HOME=${FIRN_HOME:-$HOME/.firn}
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) PLAT=linux-x86_64 ;;
    Linux-aarch64|Linux-arm64) PLAT=linux-aarch64 ;;
    *) echo "unsupported system $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
get() {
    case "$1" in
        file://*) cp "${1#file://}" "$2" ;;
        http://*|https://*) curl -fsSL -o "$2" "$1" ;;
        *) cp "$1" "$2" ;;
    esac
}
get "$URL/$CHANNEL/manifest.json" "$T/manifest.json"
LINE=$(grep "\"$PLAT\"" "$T/manifest.json") || { echo "no build for $PLAT" >&2; exit 1; }
FILE=$(echo "$LINE" | sed 's/.*"file": "\([^"]*\)".*/\1/')
SHA=$(echo "$LINE" | sed 's/.*"sha256": "\([0-9a-f]*\)".*/\1/')
get "$URL/$CHANNEL/$FILE" "$T/$FILE"
echo "$SHA  $T/$FILE" | sha256sum -c - >/dev/null || { echo "SHA-256 mismatch for $FILE" >&2; exit 1; }
mkdir -p "$T/x"
tar -xzf "$T/$FILE" -C "$T/x"
TOOL=$(echo "$T"/x/*/bin/firn)
mkdir -p "$FIRN_HOME/bin"
echo "$PLAT" > "$FIRN_HOME/platform"
export FIRN_HOME
"$TOOL" channel "$URL" "$CHANNEL"
"$TOOL" install --channel "$CHANNEL"
ln -sfn ../current/bin/firn "$FIRN_HOME/bin/firn"
ln -sfn ../current/bin/firn "$FIRN_HOME/bin/firnc"
case ":$PATH:" in
    *":$FIRN_HOME/bin:"*) ;;
    *) echo "add to your shell profile: export PATH=\"$FIRN_HOME/bin:\$PATH\"" ;;
esac
echo "firn installed in $FIRN_HOME"
