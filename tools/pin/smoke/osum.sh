#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
REPO=$1
VERSION=$2
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cd "$REPO" || exit 1
[ "$(cut -d" " -f2 firn.toolchain)" = "$VERSION" ] || { echo "osum smoke: firn.toolchain is not $VERSION"; exit 1; }
FIRN_VERSION=$VERSION bash tools/build-kernel.sh "$D/k.mb" > "$D/kernel.log" 2>&1 || { tail -15 "$D/kernel.log"; exit 1; }
[ -s "$D/k.mb" ] || { echo "osum smoke: no kernel image"; exit 1; }
