#!/bin/sh
set -e
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
cd "$ROOT"
FIRNLIB=$ROOT/lib ${FIRNC:-/root/firn/compiler/target/release/firnc} --opt-level=${OPT:-dev} -o "$D/app" "$1" 2>&1 | head -${LINES_MAX:-40}
shift
FIRNLIB=$ROOT/lib "$D/app" "$@"
