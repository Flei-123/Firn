#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# tools/stdarchive/run.sh -- std.tar, std.extract, std.hashfile and std.secret
# held against implementations nobody here wrote: GNU tar, Python tarfile and
# zipfile, Info-ZIP, hashlib, and Python's `cryptography` (+ /usr/bin/argon2)
# for the encrypted vault. See check.py for what is compared.
set -uo pipefail
cd "$(dirname "$0")/../.."
export FIRNLIB="$(pwd)/lib"
FIRNC="${FIRNC:-$(pwd)/compiler/target/release/firnc}"
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
"$FIRNC" -o "$W/probe" tools/stdarchive/probe.fi > "$W/build.log" 2>&1 || { echo "  FAIL probe does not build"; grep -v RWX "$W/build.log" | head -8; exit 1; }
python3 tools/stdarchive/check.py "$W/probe" "$W/work"
