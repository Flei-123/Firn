#!/bin/bash
OUT=${OUT:?set OUT dir}
B=$(cd "$(dirname "$0")/.." && pwd)
F=/root/firn/compiler/target/release/firnc
mkdir -p $OUT; cd $B
for f in firn/*.fi; do n=$(basename $f .fi); [ -f c/$n.c ] || continue
 $F --opt-level=release-fast --target=aarch64-linux firn/$n.fi -o $OUT/$n.firn || echo FAIL firn $n
 aarch64-linux-gnu-gcc -static -O2 -fno-strict-aliasing -ffp-contract=off c/$n.c -o $OUT/$n.c -lm || echo FAIL c $n
done; echo BUILD_DONE
