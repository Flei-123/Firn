#!/bin/bash
# ROUND REGALLOC-A64 -- does the x86-64 side still emit exactly what it did?
# Every program in tests/ compiled to assembly with the base compiler (branch
# phi, the point this round started from) and with this round's, byte compared.
cd /root/firn-regalloc
export FIRNLIB=$PWD/lib
REF=/root/firn-phi/compiler/target/release/firnc
NEW=/tmp/firnc_final
one() {
  b=$(basename "$1" .fi)
  "$REF" --emit=asm -o /tmp/xs_$b.r.s "$1" >/dev/null 2>&1 || { echo "SKIP $b"; rm -f /tmp/xs_$b.*.s; return; }
  "$NEW" --emit=asm -o /tmp/xs_$b.n.s "$1" >/dev/null 2>&1 || { echo "NEWFAIL $b"; rm -f /tmp/xs_$b.*.s; return; }
  if cmp -s /tmp/xs_$b.r.s /tmp/xs_$b.n.s; then echo "SAME $b"; else echo "DIFF $b"; fi
  rm -f /tmp/xs_$b.r.s /tmp/xs_$b.n.s
}
export -f one; export REF NEW
ls tests/*.fi | xargs -P 8 -I{} bash -c 'one "$@"' _ {} > /tmp/x86same.out 2>&1
echo FINISHED >> /tmp/x86same.out
