#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/ton_bauen.sh -- baut die Tonwerkzeuge und laesst den Selbsttest laufen.
set -e
FIRNC=${FIRNC:-/root/firn/compiler/target/release/firnc}
HIER=$(cd "$(dirname "$0")/.." && pwd)
export FIRNLIB="$HIER/lib"
"$FIRNC" -o /tmp/mp3 "$HIER/lib/ton/mp3_main.fi"
"$FIRNC" -o /tmp/mp3pruef "$HIER/lib/ton/mp3_pruef_main.fi"
/tmp/mp3pruef "$HIER/testdata/ton/stereo44.mp3" "$HIER/testdata/ton/mono24.mp3" \
              "$HIER/testdata/ton/mono8.mp3" "$HIER/testdata/ton/kurzbloecke.mp3"
