#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/fui/xinput.sh -- the XInput2 touch handshake (tools/fui/xinput_main.fi)
# against a private Xvfb. No finger is played in (no touch device on the
# build machine); see the header of xinput_main.fi for what is checked.
#   bash tools/fui/xinput.sh        (FIRNC, FIRNLIB, W from the env)
set -uo pipefail
cd "$(dirname "$0")/../.."
FIRNC=${FIRNC:-compiler/target/release/firnc}
export FIRNLIB=${FIRNLIB:-$PWD/lib}
W=${W:-$(mktemp -d)}
D=${XINPUT_DISPLAY:-93}
"$FIRNC" --opt-level=dev -o "$W/xinput" tools/fui/xinput_main.fi || exit 1
Xvfb ":$D" -screen 0 640x480x24 >/dev/null 2>&1 &
XP=$!
sleep 1
DISPLAY=":$D" "$W/xinput"
rc=$?
kill $XP 2>/dev/null
exit $rc
