#!/usr/bin/env bash
# build-windows.sh -- cross-build @NAME@ for Windows (x86_64) on Linux.
#
#   bash build-windows.sh    -> build/@ID@.exe (a window program: no console)
#
# Needs the mingw binutils (apt install binutils-mingw-w64-x86-64): the
# compiler writes the PE image itself and uses only `as` and `ld` of them.
# Testing under Wine works for the console parts; the window needs Windows.
set -euo pipefail
cd "$(dirname "$0")"
FIRN_ROOT=${FIRN_ROOT:-@FIRN_ROOT@}
FIRNC=${FIRNC:-$FIRN_ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || { echo "firnc not found at $FIRNC (set FIRN_ROOT or FIRNC)" >&2; exit 2; }
command -v x86_64-w64-mingw32-ld >/dev/null || { echo "x86_64-w64-mingw32-ld missing (binutils-mingw-w64-x86-64)" >&2; exit 2; }
export FIRNLIB=$FIRN_ROOT/lib
export FIRN_APP_VERSION=$(tr -d ' \n' <VERSION)
export FIRN_APP_CHANNEL=${CHANNEL:-stabil}
[ -n "${STORE:-}" ] && export FIRN_APP_STORE=$STORE
mkdir -p build
"$FIRNC" --target=x86_64-windows --win-subsystem=windows --opt-level=release-fast \
    -o build/@ID@.exe src/main.fi
echo "built build/@ID@.exe $(cat VERSION) ($(stat -c %s build/@ID@.exe) bytes)"
