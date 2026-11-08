#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
REPO=$1
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cd "$REPO" || exit 1
bash vendor/firn/fetch-firnc.sh --force > "$D/fetch.log" 2>&1 || { tail -15 "$D/fetch.log"; exit 1; }
grep -q 'from installed toolchain' "$D/fetch.log" || { echo "osum smoke: pin is not served by an installed toolchain"; exit 1; }
bash tools/build-kernel.sh "$D/k.mb" > "$D/kernel.log" 2>&1 || { tail -15 "$D/kernel.log"; exit 1; }
[ -s "$D/k.mb" ] || { echo "osum smoke: no kernel image"; exit 1; }
