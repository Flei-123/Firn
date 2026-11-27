#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
OUT=${1:?usage: build_kit.sh OUT_DIR}
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
WIN_FIRNC=${WIN_FIRNC:-$ROOT/compiler/target/x86_64-pc-windows-gnu/release/firnc.exe}
MIRROR=https://mirror.msys2.org/mingw/mingw64
PACKAGES="binutils libwinpthread zlib zstd gettext-runtime libiconv"
KIT_NAME=firn-windows-x86_64
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
KIT="$STAGE/$KIT_NAME"
mkdir -p "$KIT/bin" "$KIT/licenses" "$OUT"

[ -f "$WIN_FIRNC" ] || { echo "missing $WIN_FIRNC (cargo build --release --target x86_64-pc-windows-gnu --manifest-path compiler/Cargo.toml)" >&2; exit 1; }
cp "$WIN_FIRNC" "$KIT/bin/firnc.exe"
FIRNLIB="$ROOT/lib" "$FIRNC" --target=x86_64-windows --opt-level=release-safe "$ROOT/tools/fmt/firnfmt.fi" -o "$KIT/bin/firnfmt.exe"
cp "$ROOT"/tools/windows/ide/{firn.cmd,firn.ps1} "$KIT/bin/"
cp "$ROOT"/tools/windows/ide/{INSTALL.cmd,install.ps1,README.txt} "$KIT/"
cp -r "$ROOT/lib" "$KIT/lib"
cp "$ROOT/LICENSE" "$KIT/licenses/FIRN-LICENSE.txt"

DL="$STAGE/dl"
mkdir -p "$DL"
curl -fsSL "$MIRROR/" > "$DL/index.html"
: > "$KIT/licenses/BINUTILS-SOURCES.txt"
for name in $PACKAGES; do
    file=$(grep -o "mingw-w64-x86_64-$name-[0-9][^\"]*\.pkg\.tar\.zst\"" "$DL/index.html" | tr -d '"' | sort -V | tail -1)
    curl -fsSL -o "$DL/$file" "$MIRROR/$file"
    echo "$file $(sha256sum "$DL/$file" | cut -d' ' -f1)" >> "$KIT/licenses/BINUTILS-SOURCES.txt"
    tar --zstd -xf "$DL/$file" -C "$DL"
done
for f in as.exe ld.exe libwinpthread-1.dll libz-1.dll libzstd.dll libintl-8.dll libiconv-2.dll; do
    cp "$DL/mingw64/bin/$f" "$KIT/bin/"
done
cp "$KIT/bin/as.exe" "$KIT/bin/x86_64-w64-mingw32-as.exe"
cp "$KIT/bin/ld.exe" "$KIT/bin/x86_64-w64-mingw32-ld.exe"
cp -r "$DL/mingw64/share/licenses/." "$KIT/licenses/"

"$ROOT/editors/vscode-firn/package.sh" "$KIT"
(cd "$STAGE" && zip -qr "$OUT/$KIT_NAME.zip" "$KIT_NAME")
ls -la "$OUT/$KIT_NAME.zip"
