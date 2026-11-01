#!/bin/bash
set -eu
SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DST="${1:?usage: port_to_fork.sh <firnc-gc worktree>}"
mkdir -p "$DST/compiler/src/own"
rm -f "$DST"/compiler/src/own/*.rs
cp -r "$SRC"/compiler/src/own/. "$DST/compiler/src/own/"
cp "$SRC/lib/gc/own.fi" "$DST/lib/gc/own.fi"
[ -e "$DST/compiler/src/fx.rs" ] || cp "$SRC/compiler/src/fx.rs" "$DST/compiler/src/fx.rs"
sed -i 's/let hi: u64 = (\*((st + S_THREAD_HI) as \*mut u64))/let hi: u64 = THREAD_MAX/' "$DST/lib/gc/own.fi"
python3 -I "$SRC/tools/ownership/fork_downgrade.py" "$DST/compiler/src/own"
if ! grep -q "inline_requested_tracked" "$DST/compiler/src/inline.rs"; then
  (cd "$DST" && patch -p1 < "$SRC/tools/ownership/fork_hooks.patch")
fi
if [ ! -e "$DST/compiler/src/parfn.rs" ]; then
  cp "$SRC/tools/ownership/fork_parfn.rs" "$DST/compiler/src/parfn.rs"
  sed -i '0,/^mod fx;/s//mod fx;\nmod parfn;/' "$DST/compiler/src/main.rs"
fi
if ! grep -q "fn for_each_use_mut" "$DST/compiler/src/fir.rs"; then
  python3 -I - "$SRC/tools/ownership/fork_use_mut.rs.inc" "$DST/compiler/src/fir.rs" <<'PY'
import sys
inc = open(sys.argv[1]).read()
text = open(sys.argv[2]).read()
marker = "    pub fn uses(&self, out: &mut Vec<Val>) {"
if text.count(marker) != 1:
    sys.exit("marker missing")
open(sys.argv[2], "w").write(text.replace(marker, inc + marker))
PY
fi
echo "ported $SRC -> $DST"
