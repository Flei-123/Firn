#!/usr/bin/env bash
# tools/fsx_cross/run.sh -- std.fsx against Python and cp (docs/SKRIPT-LIBS.md, r314).
#
#   1. builds tools/fsx_cross/fsx_tool.fi in release-safe (checked arithmetic)
#   2. tools/fsx_cross/cross.py: 500 random trees against `cp -a` and shutil.copytree
#      (plain / ignore / dirs_exist_ok / follow symlinks), 150 single files against `cp -p`,
#      6000 random (path, base) pairs against os.path.normpath / abspath / relpath,
#      6000 random (pattern, name) pairs against fnmatch.fnmatchcase
#   3. no temporary copy name (`*.tmp-*`) is left behind
#   4. counter-check (cross.py selftest): six deliberate alterations of a copy (mode,
#      content, link target, extra file, missing empty directory, directory mode) are all
#      detected by the same comparison -- so a green run is not a blind one
#
# Usage:  bash tools/fsx_cross/run.sh [SEED] [TREES]
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
FIRNC=${FIRNC:-$ROOT/compiler/target/release/firnc}
export FIRNLIB="$ROOT/lib"
SEED=${1:-20261008}
TREES=${2:-500}

TMPD=$(mktemp -d)
trap 'chmod -R u+rwx "$TMPD" 2>/dev/null; rm -rf "$TMPD"' EXIT

echo "== fsx_cross: build =="
"$FIRNC" --opt-level=release-safe -o "$TMPD/fsx_tool" tools/fsx_cross/fsx_tool.fi || { echo "FAIL  fsx_tool does not build"; exit 1; }

echo "== fsx_cross: random trees, files, paths, patterns =="
python3 tools/fsx_cross/cross.py "$TMPD/fsx_tool" "$TMPD/work" "$SEED" "$TREES"
RC=$?
if [ "$RC" -ne 0 ]; then
    echo "FAIL  cross.py ended with $RC"
    exit 1
fi

echo "== fsx_cross: leftovers =="
# no temporary names of std.fsx in the work directory tree or /tmp from this run
if find "$TMPD" -name '*.tmp-*' | grep -q .; then
    echo "FAIL  temporary copy names left behind"
    exit 1
fi
echo "fsx_cross: ok"
