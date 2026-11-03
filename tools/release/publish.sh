#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
OUT=/root/firn-publish/channels
CHANNEL=nightly
VERSION=
MIN_VERSION=
FIRNC=
LIB_DIRS=
STAMP=
FORCE=0
declare -A EXTRA_FIRNC=()
usage() { echo "usage: publish.sh [--out DIR] [--channel stable|nightly] [--version X.Y.Z] [--min-version X.Y.Z] [--firnc BIN] [--firnc-for PLATFORM=BIN] [--lib-dirs 'std ...'] [--gate-stamp FILE] [--force]" >&2; exit 2; }
while [ $# -gt 0 ]; do
    case "$1" in
        --out) OUT=$2; shift 2 ;;
        --channel) CHANNEL=$2; shift 2 ;;
        --version) VERSION=$2; shift 2 ;;
        --min-version) MIN_VERSION=$2; shift 2 ;;
        --firnc) FIRNC=$2; shift 2 ;;
        --firnc-for) EXTRA_FIRNC[${2%%=*}]=${2#*=}; shift 2 ;;
        --lib-dirs) LIB_DIRS=$2; shift 2 ;;
        --gate-stamp) STAMP=$2; shift 2 ;;
        --force) FORCE=1; shift ;;
        *) usage ;;
    esac
done
case "$CHANNEL" in stable|nightly) ;; *) echo "unknown channel $CHANNEL" >&2; exit 2 ;; esac
COMMIT=$(git rev-parse HEAD)
[ -n "$VERSION" ] || VERSION=0.1.$(git rev-list --count HEAD)
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "bad version $VERSION" >&2; exit 2; }
[ -n "$MIN_VERSION" ] || MIN_VERSION=0.1.0
if [ "$CHANNEL" = stable ]; then
    [ -n "$STAMP" ] && [ -f "$STAMP" ] || { echo "stable needs --gate-stamp (tools/release/gate.sh)" >&2; exit 1; }
    grep -qx "commit=$COMMIT" "$STAMP" && grep -qx "result=green" "$STAMP" || { echo "gate stamp is not green for $COMMIT" >&2; exit 1; }
fi
if [ -z "$FIRNC" ]; then
    /root/jarvis/bin/heavy cargo build --release --manifest-path compiler/Cargo.toml >&2
    FIRNC=$ROOT/compiler/target/release/firnc
fi
[ -x "$FIRNC" ] || { echo "no firnc at $FIRNC" >&2; exit 1; }
DEST=$OUT/$CHANNEL
mkdir -p "$DEST"
if [ -f "$DEST/manifest.json" ] && [ $FORCE = 0 ]; then
    python3 -I "$ROOT/tools/release/manifest.py" check-newer "$DEST/manifest.json" "$VERSION"
fi
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
mkdir -p "$W/lib"
if [ -n "$LIB_DIRS" ]; then
    git archive "$COMMIT" $(for d in $LIB_DIRS; do echo "lib/$d"; done) | tar -x -C "$W"
else
    git archive "$COMMIT" lib | tar -x -C "$W"
fi
FIRN_TOOL=$W/firn-tool
FIRNLIB=$ROOT/lib "$FIRNC" "$ROOT/tools/toolchain/firn_main.fi" -o "$FIRN_TOOL" >&2
ENTRIES=()
pack() {
    local plat=$1 bin=$2 ext=$3 tool=$4
    local name=firn-$VERSION-$plat
    local top=$W/stage-$plat/$name
    mkdir -p "$top/bin"
    cp -a "$W/lib" "$top/lib"
    cp "$bin" "$top/bin/firnc$ext"
    [ -n "$tool" ] && cp "$tool" "$top/bin/firn$ext"
    echo "$VERSION" > "$top/VERSION"
    echo "$plat" > "$top/PLATFORM"
    echo "$COMMIT" > "$top/COMMIT"
    local file
    if [ "$ext" = .exe ]; then
        file=$name.zip
        (cd "$W/stage-$plat" && python3 -I -m zipfile -c "$DEST/$file.tmp" "$name")
    else
        file=$name.tar.gz
        tar -C "$W/stage-$plat" -czf "$DEST/$file.tmp" "$name"
    fi
    mv "$DEST/$file.tmp" "$DEST/$file"
    ENTRIES+=("$plat=$file")
}
pack linux-x86_64 "$FIRNC" "" "$FIRN_TOOL"
for plat in "${!EXTRA_FIRNC[@]}"; do
    ext=""; [[ $plat == windows-* ]] && ext=.exe
    pack "$plat" "${EXTRA_FIRNC[$plat]}" "$ext" ""
done
python3 -I "$ROOT/tools/release/manifest.py" write "$DEST" "$CHANNEL" "$VERSION" "$MIN_VERSION" "$COMMIT" "${ENTRIES[@]}"
echo "published firn $VERSION to $DEST"
