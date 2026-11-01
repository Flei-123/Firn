#!/bin/bash
set -eu
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DST="${1:?usage: port_to_fork.sh <firnc-gc worktree>}"
mkdir -p "$DST/compiler/src/own"
rm -f "$DST"/compiler/src/own/*.rs
cp -r "$SRC"/compiler/src/own/. "$DST/compiler/src/own/"
cp "$SRC/lib/gc/own.fi" "$DST/lib/gc/own.fi"
[ -e "$DST/compiler/src/fx.rs" ] || cp "$SRC/compiler/src/fx.rs" "$DST/compiler/src/fx.rs"
python3 -I "$SRC/tools/ownership/fork_downgrade.py" "$DST/compiler/src/own"
if ! grep -q "inline_requested_tracked" "$DST/compiler/src/inline.rs"; then
  (cd "$DST" && patch -p1 < "$SRC/tools/ownership/fork_hooks.patch")
fi
echo "ported $SRC -> $DST"
