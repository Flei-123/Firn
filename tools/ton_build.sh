#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/ton_build.sh -- builds the audio tools and runs the self-test.
set -e
HERE=$(cd "$(dirname "$0")/.." && pwd)
FIRNC=${FIRNC:-$HERE/compiler/target/release/firnc}
export FIRNLIB="$HERE/lib"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
"$FIRNC" -o "$WORK/mp3" "$HERE/lib/ton/mp3_main.fi"
"$FIRNC" -o "$WORK/mp3check" "$HERE/lib/ton/mp3_check_main.fi"
"$WORK/mp3check" "$HERE/testdata/ton/stereo44.mp3" "$HERE/testdata/ton/mono24.mp3" \
                 "$HERE/testdata/ton/mono8.mp3" "$HERE/testdata/ton/shortblocks.mp3"
