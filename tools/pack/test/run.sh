#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/pack/test/run.sh -- the packaging tests.
#
#   bash tools/pack/test/run.sh            checks.py (every writer against an independent reader)
#   PACK_ANDROID=1 ...                    ... and android.sh (an APK with launcher icons; needs the SDK/NDK)
#   PACK_WINE=1 bash tools/pack/test/run.sh    ... and windows.sh (installer, shortcuts, registry,
#                                                uninstaller, the window -- under Wine on Xvfb)
#
# Builds what it needs into a temp directory (removed at the end). Heavy only because of the
# compiles: run it through /root/jarvis/bin/heavy on a loaded machine.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB=$ROOT/lib
W=$(mktemp -d "${TMPDIR:-/tmp}/pack-test.XXXXXX")
trap 'rm -rf "$W"' EXIT
echo "== building the test programs and the stubs"
"$FIRNC" -o "$W/hello" "$ROOT/tools/pack/test/hello.fi" >/dev/null 2>"$W/b.log" || { cat "$W/b.log"; exit 1; }
"$FIRNC" --target=x86_64-windows -o "$W/hello.exe" "$ROOT/tools/pack/test/hello.fi" >/dev/null 2>"$W/b.log" || { cat "$W/b.log"; exit 1; }
PACK_BUILD=$W/build bash "$ROOT/tools/pack/pack.sh" stubs >/dev/null || exit 1
export PACK_HELLO=$W/hello PACK_HELLO_EXE=$W/hello.exe PACK_ICONS=$W/build/icons PACK_SELFX=$W/build/selfx
[ -f "$W/build/setup-stub.exe" ] && export PACK_SETUP_STUB=$W/build/setup-stub.exe
echo "== checks.py"
python3 "$ROOT/tools/pack/test/checks.py" --tmp "$W/checks" || RC=1
if [ "${PACK_WINE:-0}" = "1" ]; then
    echo "== windows.sh (Wine)"
    PACK_W=$W bash "$ROOT/tools/pack/test/windows.sh" || RC=1
fi
if [ "${PACK_ANDROID:-0}" = "1" ]; then
    echo "== android.sh"
    bash "$ROOT/tools/pack/test/android.sh" || RC=1
fi
exit ${RC:-0}
