#!/bin/bash
set -u
BIN=$1; shift
SYM=$(nm "$BIN" | awk '$3 ~ /\.gc_init$/ {print $3; exit}')
[ -n "$SYM" ] || { echo "gc_runs=0 gc_pause_max_ns=0 gc_pause_total_ns=0 gc_alloc_bytes=0"; exit 0; }
ADDR=$(objdump -d --no-show-raw-insn --disassemble="$SYM" "$BIN" | grep -E 'lea +0x[0-9a-f]+\(%rip\)' | head -1 | sed -E 's/.*# +([0-9a-f]+).*/\1/')
OUT=$(gdb -batch -q -ex 'set pagination off' -ex 'catch syscall exit exit_group' -ex "run $* > /dev/null" \
  -ex "x/1gu 0x$ADDR+32" -ex "x/1gu 0x$ADDR+72" -ex "x/1gu 0x$ADDR+80" -ex "x/1gu 0x$ADDR+88" -ex "x/1gu 0x$ADDR+296" "$BIN" 2>&1 | grep -E '^0x' | sed -E 's/^[^:]*:\s*//')
set -- $OUT
echo "gc_runs=${1:-0} gc_pause_max_ns=${2:-0} gc_pause_total_ns=${3:-0} gc_alloc_bytes=$(( ${4:-0} + ${5:-0} ))"
