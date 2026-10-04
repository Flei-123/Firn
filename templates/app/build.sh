#!/usr/bin/env bash
# build.sh -- build @NAME@ for this machine (Linux).
#
#   bash build.sh            optimised build -> build/@ID@
#   DEV=1 bash build.sh      fast compile (dev-fast), keeps debug info
#
# FIRN_ROOT is the Firn checkout (compiler + library); the generator wrote the
# path it was run from, override it on another machine.
set -euo pipefail
cd "$(dirname "$0")"
FIRN_ROOT=${FIRN_ROOT:-@FIRN_ROOT@}
FIRNC=${FIRNC:-$FIRN_ROOT/compiler/target/release/firnc}
[ -x "$FIRNC" ] || { echo "firnc not found at $FIRNC (set FIRN_ROOT or FIRNC)" >&2; exit 2; }
export FIRNLIB=$FIRN_ROOT/lib
export FIRN_APP_VERSION=$(tr -d ' \n' <VERSION)
export FIRN_APP_CHANNEL=${CHANNEL:-stabil}
[ -n "${STORE:-}" ] && export FIRN_APP_STORE=$STORE
OPT=release-fast
[ -n "${DEV:-}" ] && OPT=dev-fast
mkdir -p build
"$FIRNC" --opt-level=$OPT -o build/@ID@ src/main.fi
echo "built build/@ID@ $(cat VERSION) ($(stat -c %s build/@ID@) bytes)"
