#!/bin/bash
# Quick exit-code sweep on aarch64 under qemu, parallel.
cd /root/firn-regalloc
export FIRNLIB=$PWD/lib
F=$PWD/compiler/target/release/firnc
one() {
  t="$1"; b=$(basename "$t" .fi); hdr=$(head -1 "$t")
  case "$hdr" in *expect_exit:*) exp=${hdr#*expect_exit: };; *) return;; esac
  timeout 60 "$F" --target=aarch64-linux -o /tmp/sw_$b "$t" >/dev/null 2>&1 || { echo "CFAIL $b"; return; }
  timeout 30 qemu-aarch64 /tmp/sw_$b >/dev/null 2>&1; rc=$?
  rm -f /tmp/sw_$b
  if [ "$rc" = "$exp" ]; then echo "OK $b"; else echo "BAD $b got=$rc want=$exp"; fi
}
export -f one; export F
ls tests/*.fi | xargs -P 6 -I{} bash -c 'one "$@"' _ {} > /tmp/sweep.out 2>&1
echo "FINISHED" >> /tmp/sweep.out
