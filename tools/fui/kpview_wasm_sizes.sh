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
E=$(mktemp -d)
trap 'rm -rf "$D" "$E"' EXIT
bash tools/docview/embedded_toolchain.sh "$E"
for opt in release-fast release-safe; do
    if "$FIRNC" --target=wasm32-browser --opt-level=$opt -o "$D/office.wasm" tools/fui/kpview_wasm_office.fi > "$D/office.log" 2>&1; then
        echo "$opt kpview_wasm_office $(stat -c %s "$D/office.wasm")"
    else
        echo "$opt kpview_wasm_office BUILD FAIL"; grep -v RWX "$D/office.log" | head -3
    fi
    if FIRNLIB="$E/lib" "$E/compiler/target/release/firnc" --target=wasm32-browser --opt-level=$opt -o "$D/officee.wasm" tools/fui/kpview_wasm_office.fi > "$D/officee.log" 2>&1; then
        echo "$opt kpview_wasm_office_embedded_fonts $(stat -c %s "$D/officee.wasm")"
    else
        echo "$opt kpview_wasm_office_embedded_fonts BUILD FAIL"; grep -v RWX "$D/officee.log" | head -3
    fi
done
