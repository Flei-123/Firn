#!/usr/bin/env bash
# package.sh -- every package of this program for every platform.
#
#   bash package.sh [options]        see tools/pack/all.sh of the Firn checkout:
#                                    --platforms linux,windows,mac,android,osum  --no-build  --sign-key F  ...
#
# Writes dist/<version>/: .deb, .tar.gz, AppImage / .run, the Windows exe with its icon, setup.exe,
# the portable zip, an NSIS script, an APK (--platforms android), manifest.json (SHA-256 + Ed25519
# signatures) and store-add.sh. Nothing is published: store-add.sh is for you to read and run.
set -euo pipefail
cd "$(dirname "$0")"
FIRN_ROOT=${FIRN_ROOT:-@FIRN_ROOT@}
exec bash "$FIRN_ROOT/tools/pack/all.sh" "$PWD" "$(tr -d ' \n' <VERSION)" "$@"
