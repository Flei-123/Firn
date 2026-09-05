#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-only
# tools/windows/machine.sh -- the IMAGE, not the behaviour (round WINDOWS).
#
# tools/windows/run.sh asks what a program DOES. This asks what the file IS:
# is it really a PE image, is the import table really ours, does a big frame
# really probe the stack, does the thunk really speak Win64. Each of these
# can be wrong while every test still passes on Wine -- and each of them
# would then be a crash on a real Windows.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"
FIRNC="$ROOT/compiler/target/release/firnc"
WORK="$ROOT/.win-machine"
OBJDUMP=x86_64-w64-mingw32-objdump
export WINEPREFIX=${WINEPREFIX:-$HOME/.wine-firn}
export WINEDEBUG=${WINEDEBUG:--all}
WINE=${WINE:-/usr/lib/wine/wine64}

command -v "$OBJDUMP" >/dev/null 2>&1 || { echo "SKIP: $OBJDUMP is missing"; exit 0; }
rm -rf "$WORK"; mkdir -p "$WORK/win" "$WORK/lin"
pass=0; fail=0
ok()  { echo "  OK    $1"; pass=$((pass+1)); }
bad() { echo "  FAIL  $1"; fail=$((fail+1)); }
chk() { if [ "$1" = 0 ]; then ok "$2"; else bad "$2"; fi; }

echo "== 1. the file is a PE image and nothing foreign is in it =="
# `--keep-asm` writes the assembler next to the OUTPUT, so the two builds
# get a directory each -- otherwise the second overwrites the first's text
# and this tool would compare a file with itself.
EXE="$WORK/win/hello.exe"; ASM="$WORK/win/hello.s"
LIN="$WORK/lin/hello";     LASM="$WORK/lin/hello.s"
"$FIRNC" --target=x86_64-windows -o "$EXE" --keep-asm examples/hello.fi >/dev/null 2>&1
if [ -f "$EXE" ]; then ok "examples/hello.fi -> hello.exe"; else bad "no .exe came out"; fi
"$OBJDUMP" -f "$EXE" 2>/dev/null | grep -q 'pei-x86-64'
chk $? "the format is pei-x86-64 (PE32+)"
"$OBJDUMP" -h "$EXE" 2>/dev/null | grep -q '\.idata'
chk $? "there is an .idata section"
"$OBJDUMP" -p "$EXE" 2>/dev/null | grep -qi 'DLL Name: KERNEL32.dll'
chk $? "the image binds to KERNEL32.dll"
# The one that matters for the project rule: no C runtime, no import library
# object, no mingw start-up code.
# The claim is "no foreign OBJECT FILE in the image". `__CTOR_LIST__` and
# `__DTOR_LIST__` come from the LINKER SCRIPT (sixteen octets of empty list
# markers), the same way `ld` puts `_edata`/`_end` into a Linux image; they
# are named here rather than hidden. What must not be there is CODE: a C
# runtime start-up, a mingw helper, an import library stub.
if "$OBJDUMP" -t "$EXE" 2>/dev/null | grep -qiE '__mingw|mainCRTStartup|WinMainCRTStartup|_pei386_runtime_relocator|__gcc_|_Jv_'; then
    bad "a mingw or C runtime symbol is in the image"
else
    ok "no mingw and no C runtime symbol in the image"
fi
if "$OBJDUMP" -t "$EXE" 2>/dev/null | grep -q '__CTOR_LIST__'; then
    ok "the only foreign name is __CTOR_LIST__/__DTOR_LIST__, out of the linker script (data, not code)"
fi
grep -q '^_start:' "$ASM" 2>/dev/null
chk $? "the entry point is our own _start"

echo
echo "== 2. the calling convention at the boundary is Win64 =="
A="$ASM"
grep -q '_Fwin.WriteFile:' "$A"; chk $? "a thunk was emitted for WriteFile"
# The order of the moves IS the correctness (win.rs).
if awk '/_Fwin.WriteFile:/,/ret/' "$A" | grep -q 'mov qword ptr \[rsp+32\], r8'; then
    ok "argument five goes to the shadow area before r8 is overwritten"
else
    bad "argument five is lost"
fi
awk '/_Fwin.WriteFile:/,/ret/' "$A" | grep -q 'mov rcx, rdi'; chk $? "System V rdi becomes Win64 rcx"
awk '/_Fwin.WriteFile:/,/ret/' "$A" | grep -q 'mov r9, rcx';  chk $? "System V rcx becomes Win64 r9"
awk '/_Fwin.WriteFile:/,/ret/' "$A" | grep -qE 'sub rsp, (32|48|64)'; chk $? "32 octets of shadow space are allocated"
awk '/_Fwin.WriteFile:/,/ret/' "$A" | grep -q 'call \[rip + __imp_WriteFile\]'; chk $? "the call goes through the import address table"
# Counter-check: the Linux build has none of this.
"$FIRNC" -o "$LIN" --keep-asm examples/hello.fi >/dev/null 2>&1
if grep -q '_Fwin\.' "$LASM" 2>/dev/null; then
    bad "the Linux build carries Windows thunks"
else
    ok "counter-check: the Linux build carries none of it"
fi
grep -q '^    syscall$' "$LASM" && ok "counter-check: the Linux build uses the syscall instruction"
if grep -qE '^    syscall$' "$A"; then
    bad "the Windows build contains a syscall instruction"
else
    ok "the Windows build contains no syscall instruction at all"
fi

echo
echo "== 3. the stack probe =="
cat > "$WORK/big.fi" <<'EOF'
// expect_exit: 7
// A frame far bigger than one page. Without the probe this writes past the
// guard page of the Windows stack and the process dies.
fn deep(n: i64) -> i64 {
    var buf: [u8; 60000] = [0; 60000]
    buf[0] = 1
    buf[59999] = 6
    buf[30000] = n as u8
    return (buf[0] as i64) + (buf[59999] as i64) + (buf[30000] as i64)
}
fn main() -> i32 {
    return deep(0) as i32
}
EOF
"$FIRNC" --target=x86_64-windows -o "$WORK/win/big.exe" --keep-asm "$WORK/big.fi" >/dev/null 2>&1
grep -q '_Fwin.chkstk' "$WORK/win/big.s" 2>/dev/null; chk $? "a frame of 60000 octets calls the probe"
grep -q 'sub rcx, 4096' "$WORK/win/big.s" 2>/dev/null; chk $? "the probe walks page by page"
timeout 60 "$WINE" "$WORK/win/big.exe" >/dev/null 2>&1; rc=$?
[ "$rc" = 7 ]; chk $? "the program with the big frame runs (exit $rc, expected 7)"
# Counter-check: a small frame must NOT probe.
if awk '/^_F0.main:/,/ret/' "$ASM" | grep -q '_Fwin.chkstk'; then
    bad "a small frame probes for nothing"
else
    ok "counter-check: a small frame does not probe"
fi

echo
echo "== 3b. no program of the corpus keeps a syscall instruction =="
# The sharpest form of the claim. A `syscall` instruction in a PE image is
# not a wrong answer, it is a dead process -- and it can hide in hand
# written runtime text (panic_rt.rs) that no ordinary test touches until
# something goes wrong. So the whole corpus is SCANNED rather than sampled.
left=0; built=0
for f in tests/*.fi examples/*.fi; do
    b=$(basename "$f" .fi)
    "$FIRNC" --target=x86_64-windows -o "$WORK/win/scan.exe" --keep-asm "$f" >/dev/null 2>&1 || continue
    built=$((built + 1))
    if grep -qE '^    syscall$' "$WORK/win/scan.s" 2>/dev/null; then
        left=$((left + 1)); echo "      $b"
    fi
done
[ "$left" -eq 0 ]; chk $? "$built Windows builds, $left of them with a syscall instruction"

echo
echo "== 4. the sections =="
"$OBJDUMP" -h "$EXE" 2>/dev/null | grep -A1 '\.bss' | grep -q 'ALLOC'
chk $? ".bss is allocated"
if "$OBJDUMP" -h "$EXE" 2>/dev/null | grep -A1 '\.bss' | grep -q 'CONTENTS'; then
    bad ".bss takes up room in the file"
else
    ok ".bss takes up no room in the file (COFF 'b' flag)"
fi

echo
echo "== 5. what the target refuses =="
# `pipefail` is on, so the message is captured first and matched after --
# otherwise the compiler's exit code 2 would be the pipeline's.
MSG=$("$FIRNC" --target=x86_64-sunos -o "$WORK/x" examples/hello.fi 2>&1)
case "$MSG" in *"unknown target 'x86_64-sunos'"*) ok "an unknown target name is refused by name" ;;
               *) bad "no clear message for an unknown target: $MSG" ;; esac
case "$MSG" in *x86_64-windows*) ok "the message lists x86_64-windows among the targets" ;;
               *) bad "x86_64-windows is not in the list of targets" ;; esac

echo
echo "  passed: $pass   failed: $fail"
[ "$fail" -eq 0 ] || exit 1
