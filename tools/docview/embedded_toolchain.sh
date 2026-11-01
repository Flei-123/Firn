#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT="$1"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
mkdir -p "$OUT/compiler/target/release"
cp -r lib "$OUT/lib"
rm -rf "$OUT/lib/@web/docview"
cp "$FIRNC" "$OUT/compiler/target/release/firnc"
