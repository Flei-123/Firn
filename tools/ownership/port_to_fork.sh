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
elif ! grep -q "fn map_weighted" "$DST/compiler/src/parfn.rs"; then
  sed -n "/^pub(crate) fn map_weighted/,\$p" "$SRC/tools/ownership/fork_parfn.rs" > "$DST/compiler/src/parfn.tmp"
  (echo; cat "$DST/compiler/src/parfn.tmp") >> "$DST/compiler/src/parfn.rs"
  rm "$DST/compiler/src/parfn.tmp"
  sed -i '1i use crate::fir::Func;' "$DST/compiler/src/parfn.rs"
  sed -i 's/^fn threads()/pub(crate) fn threads()/' "$DST/compiler/src/parfn.rs"
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
if grep -q "s.push_str(crate::own::RUNTIME);" "$DST/compiler/src/gc.rs"; then
  sed -i 's/^        s.push_str(crate::own::RUNTIME);$/        crate::own::patch_gc_runtime(\&mut s);\n        s.push_str(\&crate::own::runtime_text());/' "$DST/compiler/src/gc.rs"
fi
python3 -I - "$DST/compiler/src/opt.rs" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
old = """    let mut st = optimize_with_base(m, cfg);
    if own {
        let changed = crate::own::fir::run(m);
        if !changed.is_empty() {
            let mut clk = PassClock {
                on: std::env::var_os("FIRN_PASS_TIMINGS").is_some(),
                ..Default::default()
            };
            for &i in changed.iter() {
                optimize_func(&mut m.funcs[i], &mut st, cfg, &mut clk);
            }
        }
    }
"""
new = """    let st = optimize_with_base(m, cfg);
    if own {
        crate::own::fir::run(m);
    }
"""
if old in text:
    open(path, "w").write(text.replace(old, new))
PY
echo "ported $SRC -> $DST"
