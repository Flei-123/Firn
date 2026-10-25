#!/bin/bash
set -e
LAUNCHER=${LAUNCHER:-/root/FleiLauncher}
SRC=$LAUNCHER/src-tauri/src
HERE=$(cd "$(dirname "$0")" && pwd)
MODE=${1:-unit}
D=$(mktemp -d /tmp/flsp-scratch-XXXXXX)
H=$(mktemp -d /tmp/flsp-home-XXXXXX)
trap 'rm -rf "$D" "$H"' EXIT
mkdir -p "$D/src" "$D/tests" "$D/tauri"
cp -r "$HERE/tauri_stub/." "$D/tauri/"
cp "$HERE/Cargo.template.toml" "$D/Cargo.toml"
cp "$HERE/launch_stub.rs" "$D/src/launch.rs"
cp "$HERE/tests/"* "$D/tests/"
sed "s#@SRC@#$SRC#" "$HERE/lib_template.rs" > "$D/src/lib.rs"
grep -v "Keyring\|keyring" "$SRC/error.rs" > "$D/src/error.rs"
cd "$D"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$D/target}
if [ "$MODE" = unit ]; then
  /root/jarvis/bin/heavy cargo test --lib
else
  FLSP_HOME=$H FLSP_MC=${FLSP_MC:-1.20.1} /root/jarvis/bin/heavy cargo test --test real_flow -- --nocapture
fi
