#!/usr/bin/env bash
cd "$(dirname "$0")/../.." || exit 2
export FIRNLIB="$PWD/lib"
FIRNC="${FIRNC:-$PWD/compiler/target/release/firnc}"
D=$(mktemp -d)
trap 'rm -rf "$D"; [ -n "$XPID" ] && kill "$XPID" 2>/dev/null' EXIT
command -v Xvfb >/dev/null 2>&1 || { echo "SKIP: no Xvfb"; exit 0; }
"$FIRNC" --no-cache --opt-level=dev-fast -o "$D/wake" tools/window/wake_main.fi || exit 1
N=170
while [ -e "/tmp/.X$N-lock" ]; do N=$((N+1)); done
Xvfb ":$N" -screen 0 640x480x24 >/dev/null 2>&1 &
XPID=$!
sleep 1
DISPLAY=":$N" timeout 60 "$D/wake"
