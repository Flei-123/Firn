#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/aarch64/machine.sh -- IS IT REALLY AARCH64, AND IS IT REALLY THE ABI?
#
# tools/aarch64/run.sh compares behaviour. That is the important half, but
# behaviour can be right for the wrong reason, so this script looks at the
# object file itself and asks a compiler nobody here wrote:
#
#   1. ELF: the object says EM_AARCH64, and the x86 build of the SAME source
#      still says EM_X86_64 (the counter-check -- a `--target` that changed
#      nothing would pass every behaviour test).
#   2. Relocations: the four kinds round 80 promised really appear --
#      R_AARCH64_CALL26 (bl), ADR_PREL_PG_HI21 + ADD_ABS_LO12_NC (adrp/add
#      of a .rodata label) and ABS64 (the entries of a jump table).
#   3. Disassembly (aarch64-linux-gnu-objdump -d): the frame really is
#      `stp x29, x30, [sp, #-16]!`, the system call really is `svc #0` with
#      the AARCH64 number in x8 (64 for write, not the x86 1), the dense
#      `match` really became an indirect `br`.
#   4. AAPCS64 against aarch64-linux-gnu-gcc, in BOTH directions and past
#      the register file: ten integer words (two on the stack) and nine
#      floating point words (one on the stack), each argument weighted by
#      its position (tools/aarch64/abi_probe.fi + abi_host.c).
#   5. `extern fn` of round 75 on this machine: the hand-written A64 strlen
#      (tools/aarch64/impl.s) and the C caller of tools/extfn/host.c.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="$ROOT/compiler/target/release/firnc"
QEMU=${QEMU:-qemu-aarch64}
WORK="$ROOT/.a64-machine"
GCC=aarch64-linux-gnu-gcc
OBJDUMP=aarch64-linux-gnu-objdump
READELF=aarch64-linux-gnu-readelf
OBJCOPY=aarch64-linux-gnu-objcopy
AS=aarch64-linux-gnu-as
LD=aarch64-linux-gnu-ld

rm -rf "$WORK"; mkdir -p "$WORK"
pass=0; fail=0
ok()  { echo "  ok   $1"; pass=$((pass + 1)); }
bad() { echo "  FAIL $1"; fail=$((fail + 1)); }

for t in "$QEMU" "$GCC" "$OBJDUMP" "$READELF" "$OBJCOPY" "$AS" "$LD"; do
    command -v "$t" >/dev/null 2>&1 || { echo "SKIP: $t is missing"; exit 0; }
done
[ -x "$FIRNC" ] || { echo "firnc is missing: $FIRNC"; exit 1; }

echo "== 1. the ELF header names the machine =="
"$FIRNC" --target=aarch64-linux -c -o "$WORK/table.a64.o" tools/aarch64/table.fi >"$WORK/c1.log" 2>&1 \
    || { bad "aarch64 compilation of tools/aarch64/table.fi"; cat "$WORK/c1.log"; }
"$FIRNC" --target=x86_64-linux  -c -o "$WORK/table.x86.o" tools/aarch64/table.fi >"$WORK/c2.log" 2>&1 \
    || bad "x86 compilation of tools/aarch64/table.fi"
if "$READELF" -h "$WORK/table.a64.o" | grep -q 'Machine:.*AArch64'; then
    ok "the aarch64 object file says EM_AARCH64"
else
    bad "the aarch64 object file does not say EM_AARCH64"
fi
if readelf -h "$WORK/table.x86.o" | grep -q 'Machine:.*X86-64'; then
    ok "counter-check: the same source for x86 still says EM_X86_64"
else
    bad "counter-check: the x86 object file does not say EM_X86_64"
fi

echo "== 2. the relocation types of round 80 =="
"$READELF" -r "$WORK/table.a64.o" > "$WORK/relocs.txt" 2>&1
for r in R_AARCH64_CALL26 R_AARCH64_ADR_PRE R_AARCH64_ADD_ABS R_AARCH64_ABS64; do
    if grep -q "$r" "$WORK/relocs.txt"; then
        ok "$r appears"
    else
        bad "$r is missing from the object file"
    fi
done

echo "== 3. the instructions are the ones we think they are =="
"$OBJDUMP" -d "$WORK/table.a64.o" > "$WORK/dis.txt" 2>&1
check_dis() { # $1 = pattern, $2 = label
    if grep -qE "$1" "$WORK/dis.txt"; then ok "$2"; else bad "$2 (pattern: $1)"; fi
}
check_dis 'stp\s+x29, x30, \[sp, #-16\]!' "the frame is built with stp x29, x30, [sp, #-16]!"
check_dis 'ldp\s+x29, x30, \[sp\], #16'   "and taken down again with ldp"
check_dis '\bbl\b'                        "the call is a bl"
check_dis '\badrp\b'                      "the table base is reached with adrp"
check_dis '\bbr\s+x10'                    "the dense match became an indirect br"
check_dis 'svc\s+#0x0'                    "the system call is svc #0"
# write(2) is 64 here and 1 on x86 -- the whole reason syscalls.rs exists.
if grep -qE 'mov(z)?\s+x8, #(0x40|64)' "$WORK/dis.txt"; then
    ok "write(2) travels as number 64 (x86: 1)"
else
    bad "the aarch64 number of write(2) does not stand in x8"
    grep -n 'x8' "$WORK/dis.txt" | head -5 | sed 's/^/       /'
fi

echo "== 4. AAPCS64 against aarch64-linux-gnu-gcc =="
if "$FIRNC" --target=aarch64-linux -c -o "$WORK/abi.o" tools/aarch64/abi_probe.fi >"$WORK/abi.build.log" 2>&1; then
    # Firn brings a `main` and a `_start` of its own; host.c brings the entry
    # point that is meant to run, so both are made local first.
    "$OBJCOPY" --localize-symbol=_start "$WORK/abi.o" 2>/dev/null
    "$OBJCOPY" --localize-symbol=main "$WORK/abi.o" 2>/dev/null
    if "$GCC" -static -o "$WORK/abi.bin" tools/aarch64/abi_host.c "$WORK/abi.o" 2>"$WORK/abi.link.log"; then
        "$QEMU" "$WORK/abi.bin"
        rc=$?
        if [ "$rc" -eq 0 ]; then ok "the calling convention agrees in both directions"
        else bad "$rc of the four AAPCS64 checks disagree"; fi
    else
        bad "linking against aarch64-linux-gnu-gcc failed"
        head -5 "$WORK/abi.link.log" | sed 's/^/       /'
    fi
else
    bad "aarch64 compilation of tools/aarch64/abi_probe.fi"
    head -5 "$WORK/abi.build.log" | sed 's/^/       /'
fi

echo "== 5. extern fn (round 75) on this machine =="
"$AS" -o "$WORK/impl.o" tools/aarch64/impl.s 2>"$WORK/impl.log" || bad "assembling tools/aarch64/impl.s"
# direction 1: Firn calls out. `ld` is expected to complain about the open
# symbol -- the object file is what matters, and it is linked by hand.
"$FIRNC" --target=aarch64-linux -c -o "$WORK/callout.o" tools/extfn/callout.fi >"$WORK/callout.log" 2>&1
if [ -f "$WORK/callout.o" ] && "$LD" -n -o "$WORK/callout.bin" "$WORK/callout.o" "$WORK/impl.o" 2>"$WORK/callout.link.log"; then
    "$QEMU" "$WORK/callout.bin"; rc=$?
    [ "$rc" -eq 5 ] && ok "Firn calls out: strlen(\"hello\") = 5" || bad "Firn calls out: exit $rc, expected 5"
else
    bad "direction 1 could not be linked"
fi
# direction 2: C calls in.
"$FIRNC" --target=aarch64-linux -c -o "$WORK/callback.o" tools/extfn/callback.fi >"$WORK/callback.log" 2>&1
if [ -f "$WORK/callback.o" ]; then
    "$OBJCOPY" --localize-symbol=_start "$WORK/callback.o" 2>/dev/null
    "$OBJCOPY" --localize-symbol=main "$WORK/callback.o" 2>/dev/null
    if "$GCC" -static -o "$WORK/callback.bin" tools/extfn/host.c "$WORK/callback.o" 2>"$WORK/callback.link.log"; then
        "$QEMU" "$WORK/callback.bin"; rc=$?
        [ "$rc" -eq 42 ] && ok "C calls in: add_one(41) = 42" || bad "C calls in: exit $rc, expected 42"
    else
        bad "direction 2 could not be linked"
        head -5 "$WORK/callback.link.log" | sed 's/^/       /'
    fi
else
    bad "direction 2 produced no object file"
fi

echo "--------------------------------------------------------------------"
echo "PASS: $pass   FAIL: $fail"
[ "$fail" -eq 0 ]
