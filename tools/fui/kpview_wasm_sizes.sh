#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
for opt in release-fast release-safe; do
    for p in kpview_wasm_probe kpview_wasm_wav kpview_wasm_docs kpview_wasm_kitdoc; do
        if "$FIRNC" --target=wasm32-browser --opt-level=$opt -o "$D/$p.wasm" "tools/fui/$p.fi" > "$D/$p.log" 2>&1; then
            echo "$opt $p $(stat -c %s "$D/$p.wasm")"
        else
            echo "$opt $p BUILD FAIL"; grep -v RWX "$D/$p.log" | head -3
        fi
    done
done
