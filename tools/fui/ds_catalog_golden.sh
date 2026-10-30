#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
set -e
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
MAX="${DS_GOLDEN_MAX:-0.5}"
MODE=compare
[ "$1" = "--update" ] && MODE=update
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/img"
"$FIRNC" --opt-level=dev -o "$T/shots" tools/fui/ds_catalog_shots_main.fi
"$T/shots" "$T/img"
if [ "$MODE" = update ]; then
    python3 -I tools/fui/refdiff.py update "$T/img" tools/fui/golden_ds
else
    python3 -I tools/fui/refdiff.py compare "$T/img" tools/fui/golden_ds --max "$MAX"
fi
