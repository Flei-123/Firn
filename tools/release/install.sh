#!/usr/bin/env sh
# SPDX-License-Identifier: MPL-2.0
set -eu
URL=${1:?usage: install.sh CHANNEL-URL [stable|nightly]}
CHANNEL=${2:-stable}
CLI=firn
COMPILER=firnc
HOME_ENV=FIRN_HOME
HOME_DIR=.firn
eval "BASE=\${$HOME_ENV:-\$HOME/$HOME_DIR}"
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
TOOL=$(echo "$T"/x/*/bin/$CLI)
mkdir -p "$BASE/bin"
echo "$PLAT" > "$BASE/platform"
export "$HOME_ENV=$BASE"
"$TOOL" channel "$URL" "$CHANNEL"
"$TOOL" install --channel "$CHANNEL"
ln -sfn ../current/bin/$CLI "$BASE/bin/$CLI"
ln -sfn ../current/bin/$CLI "$BASE/bin/$COMPILER"
case ":$PATH:" in
    *":$BASE/bin:"*) ;;
    *) echo "add to your shell profile: export PATH=\"$BASE/bin:\$PATH\"" ;;
esac
echo "$CLI installed in $BASE"
