#!/bin/bash
# ROUND REGALLOC-A64 -- THE MEASUREMENT.
#
# Executed instructions (not seconds, not static size) for the TEMPO-3
# kernels, on both machines, with the round 80 aarch64 path and with this
# round's.
#
#   x86 : valgrind --tool=callgrind
#   a64 : tools/regalloc_a64/icount_a64.py -- this qemu has no instruction plugin, so the
#         count is (block size from -d in_asm) x (block entries from
#         -d exec,nochain), which is exact for what really ran.
#
# `misch` is measured at the default level (dev-fast, checked arithmetic --
# that is the level Certus ships and the one TEMPO-3 measured). `gc_alloc`
# mixes a hash into a u32 on purpose, which the checked levels rightly
# refuse, so it is measured at --opt-level=release-fast; there the kernel is
# inlined into main, so the whole program is the honest number.
cd /root/firn-regalloc
export FIRNLIB=$PWD/lib
F=$PWD/compiler/target/release/firnc
W=$PWD/.ra/m
mkdir -p "$W"

x86_fn() {  # cg-file, binary, symbol substring ("" = whole program)
  if [ -z "$3" ]; then
    grep -m1 '^summary:' "$1" | awk '{print $2}'
    return
  fi
  python3 - "$1" "$2" "$3" <<'EOF'
import re, subprocess, sys
cg, binary, want = sys.argv[1], sys.argv[2], sys.argv[3]
syms = []
for ln in subprocess.run(["nm","-n","--defined-only",binary],
                         capture_output=True,text=True).stdout.splitlines():
    p = ln.split()
    if len(p) >= 3 and p[1].lower() in ("t","w"):
        syms.append((int(p[0],16), p[2]))
syms.sort()
rng = [(a, syms[i+1][0] if i+1 < len(syms) else a+(1<<20), nm)
       for i,(a,nm) in enumerate(syms)]
tot = {}
ann = subprocess.run(["callgrind_annotate",cg],capture_output=True,text=True).stdout
for ln in ann.splitlines():
    m = re.match(r'^\s*([\d,]+)\s*\([^)]*\)\s+\S*?0x([0-9a-f]+)', ln)
    if not m: continue
    n, pc = int(m.group(1).replace(",","")), int(m.group(2),16)
    for a,b,nm in rng:
        if a <= pc < b:
            tot[nm] = tot.get(nm,0)+n; break
for nm,n in sorted(tot.items(), key=lambda kv:-kv[1]):
    if want in nm:
        print(n); break
EOF
}
a64_fn() {  # binary, symbol substring ("" = whole program)
  if [ -z "$2" ]; then
    python3 tools/regalloc_a64/icount_a64.py "$1" 2>/dev/null | awk '/^total/{print $2}'
  else
    python3 tools/regalloc_a64/icount_a64.py "$1" --fn "$2" 2>/dev/null | awk -v f="$2" '$1 ~ f {print $2}'
  fi
}

printf '%-26s %13s %14s %13s %9s %8s\n' "what" "x86" "a64 round80" "a64 now" "now/x86" "gain"
for spec in "misch|misch_zeile|" "gc_alloc||--opt-level=release-fast"; do
  IFS='|' read -r src FN LVL <<<"$spec"
  label="$src"
  [ -n "$FN" ] && label="$src :: $FN" || label="$src (whole program)"

  $F $LVL -o "$W/$src.x86" tools/regalloc_a64/$src.fi 2>/dev/null
  valgrind --tool=callgrind --callgrind-out-file="$W/$src.cg" "$W/$src.x86" >/dev/null 2>&1
  XF=$(x86_fn "$W/$src.cg" "$W/$src.x86" "$FN")

  FIRN_NO_RA_A64=1 $F $LVL --target=aarch64-linux -o "$W/$src.a64.old" tools/regalloc_a64/$src.fi 2>/dev/null
  OF=$(a64_fn "$W/$src.a64.old" "$FN")

  $F $LVL --target=aarch64-linux -o "$W/$src.a64.new" tools/regalloc_a64/$src.fi 2>/dev/null
  NF=$(a64_fn "$W/$src.a64.new" "$FN")

  python3 - "$label" "${XF:-0}" "${OF:-0}" "${NF:-0}" <<'EOF'
import sys
lab, x, o, n = sys.argv[1], int(sys.argv[2] or 0), int(sys.argv[3] or 0), int(sys.argv[4] or 0)
r = f"{n/x:.2f}x" if x and n else "-"
g = f"{o/n:.2f}x" if n else "-"
print(f"{lab:<26} {x:13,} {o:14,} {n:13,} {r:>9} {g:>8}")
EOF
done
