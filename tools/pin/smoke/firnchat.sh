#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -uo pipefail
REPO=$1
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cd "$REPO" || exit 1
FIRNLIB=$(firnc --print-sysroot) env -u FIRNC bash build.sh > "$D/build.log" 2>&1 || { tail -15 "$D/build.log"; exit 1; }
[ -x bin/firnchat ] || { echo "firnchat smoke: no bin/firnchat"; exit 1; }
