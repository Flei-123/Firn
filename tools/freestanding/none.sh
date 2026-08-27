#!/usr/bin/env bash
# tools/freestanding/none.sh -- THE PROOF THAT `--target=*-none` MEANS SOMETHING.
#
# ROUND ARM-FREESTANDING. `tools/freestanding/run.sh` (round 52) measures the
# same idea from the SOURCE side: `profile kernel` in line 1 produces an
# object file with no libc in it. This script measures it from the COMMAND
# LINE side, and on BOTH machines:
#
#   1. `--target=x86_64-none` on a source that says `profile kernel` produces
#      exactly the OCTETS the plain build produces. That is the sharpest
#      statement this round can make about the x86 path: the new target is
#      the same switch under another name, not a second code path.
#   2. `--target=x86_64-none` and `--target=aarch64-none` refuse `syscall`
#      and refuse `profile app`, each with a message that names the target.
#   3. The A64 object file is an ELF `ET_REL` for `EM_AARCH64` with no
#      undefined name except the two the kernel author owes it (`osum_panic`
#      and the vector table of its own `start.s`), and no compiler-generated
#      `svc` in the machine code.
#   4. `eret`, `mrs`, `msr` and the interrupt register save are really in
#      there -- round 80 could produce none of them.
#   5. BOTH images BOOT and say something over the serial line:
#      `qemu-system-x86_64 -kernel` and `qemu-system-aarch64 -M virt`.
#      This is the measurement the round stands on. Everything above it can
#      be right while this is wrong; this cannot be right while anything
#      above it is wrong.
#
# A missing cross toolchain or a missing qemu is said out loud and skipped
# -- the same house rule tools/aarch64/run.sh follows.
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)
export FIRNLIB="$ROOT/lib"

FIRNC="$ROOT/compiler/target/release/firnc"
X86SRC=demos/freestanding/core.fi
A64SRC=demos/freestanding/a64/core.fi
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc is missing: $FIRNC"; exit 1; }

echo "== 1. the x86 path does not change: --target=x86_64-none == profile kernel =="
"$FIRNC" --emit=asm -o "$TMPD/plain.s" "$X86SRC" 2>"$TMPD/e1" \
    || { bad "plain build failed"; sed 's/^/        /' "$TMPD/e1" | head -3; }
"$FIRNC" --target=x86_64-none --emit=asm -o "$TMPD/none.s" "$X86SRC" 2>"$TMPD/e2" \
    || { bad "x86_64-none build failed"; sed 's/^/        /' "$TMPD/e2" | head -3; }
if [ -f "$TMPD/plain.s" ] && [ -f "$TMPD/none.s" ]; then
    if cmp -s "$TMPD/plain.s" "$TMPD/none.s"; then
        ok "the assembler text is IDENTICAL ($(wc -c < "$TMPD/plain.s") octets)"
    else
        bad "the two builds differ"
        diff "$TMPD/plain.s" "$TMPD/none.s" | head -6 | sed 's/^/        /'
    fi
fi
# The counter-check that gives the line above its meaning: a target that
# changed nothing at all would pass it just as well. The x86-64 build WITH an
# operating system under it has to be DIFFERENT (it has a `_start`).
"$FIRNC" --profile=app --emit=asm -o "$TMPD/app.s" tools/freestanding/volatile.fi 2>/dev/null
if grep -q '^_start:' "$TMPD/app.s" 2>/dev/null && ! grep -q '^_start:' "$TMPD/none.s"; then
    ok "counter-check: the app build has a _start, the freestanding one has none"
else
    bad "counter-check: _start is not the difference it should be"
fi

echo "== 2. what a target without an operating system refuses =="
probe() {   # $1 = target, $2 = source text, $3 = expected fragment
    printf '%s' "$2" > "$TMPD/p.fi"
    if "$FIRNC" --target="$1" -o "$TMPD/p.o" "$TMPD/p.fi" >"$TMPD/p.err" 2>&1; then
        bad "$1: '$3' was NOT refused"
        return
    fi
    if grep -qF "$3" "$TMPD/p.err"; then
        ok "$1: refused, and the message names the reason ('$3')"
    else
        bad "$1: refused with the wrong message"
        head -2 "$TMPD/p.err" | sed 's/^/        /'
    fi
}
probe x86_64-none  'fn main() -> i32 { syscall(60, 0)
 return 0 }
' "'syscall' does not exist on the target 'x86_64-none'"
probe aarch64-none 'fn main() -> i32 { syscall(60, 0)
 return 0 }
' "'syscall' does not exist on the target 'aarch64-none'"
probe aarch64-none 'profile app
fn main() -> i32 { return 0 }
' "profile 'app' cannot be built for the target 'aarch64-none'"
probe aarch64-none 'import std.io
fn main() -> i32 { return 0 }
' "is not available in profile 'kernel'"

# ---------------------------------------------------------------- aarch64
for t in aarch64-linux-gnu-as aarch64-linux-gnu-ld; do
    command -v "$t" >/dev/null 2>&1 || {
        echo "SKIP (rest): $t is missing (apt-get install binutils-aarch64-linux-gnu)"
        echo "--------------------------------------------------------------------"
        echo "FREESTANDING TARGETS: $pass passed, $fail failed"
        [ "$fail" -eq 0 ] && exit 0 || exit 1
    }
done

echo "== 3. the aarch64 object file is freestanding =="
if "$FIRNC" --target=aarch64-none -o "$TMPD/a64.o" "$A64SRC" 2>"$TMPD/a64.err"; then
    ok "$A64SRC -> ELF object (and the source never says 'profile kernel')"
else
    bad "aarch64-none does not compile it"
    sed 's/^/        /' "$TMPD/a64.err" | head -6
fi
if [ -f "$TMPD/a64.o" ]; then
    kind=$(readelf -h "$TMPD/a64.o" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    mach=$(readelf -h "$TMPD/a64.o" | awk -F: '/^  Machine:/ {print $2}' | sed 's/^ *//')
    [ "$kind" = "REL" ] && ok "ELF type REL (relocatable object file)" \
                        || bad "ELF kind '$kind', expected REL"
    [ "$mach" = "AArch64" ] && ok "ELF machine AArch64" || bad "ELF machine '$mach'"
    # The two names the kernel author owes the object: the panic hand-off and
    # the vector table out of his own start.s. Anything else undefined would
    # mean a runtime crept in.
    undef=$(aarch64-linux-gnu-nm -u "$TMPD/a64.o" | awk '{print $NF}' | sed '/^$/d' \
            | grep -vxF osum_panic | grep -vxF VECTORS)
    [ -z "$undef" ] && ok "no undefined name except osum_panic and VECTORS" \
                    || { bad "undefined symbols"; echo "$undef" | sed 's/^/        /'; }
    foreign=$(aarch64-linux-gnu-nm --defined-only "$TMPD/a64.o" | awk '{print $3}' | grep -vE '^_F0\.' || true)
    [ -z "$foreign" ] && ok "every defined symbol is its own (_F0.)" \
                      || { bad "foreign symbols"; echo "$foreign" | sed 's/^/        /'; }
    [ -z "$(aarch64-linux-gnu-nm "$TMPD/a64.o" | grep -w _start || true)" ] \
        && ok "no _start (the entry point belongs to the kernel)" \
        || bad "the object file has a _start"
    aarch64-linux-gnu-objdump -d "$TMPD/a64.o" > "$TMPD/a64.dis"
    # ONE `svc`, and it is the one the SOURCE wrote in an asm template
    # (`core_start`'s deliberate exception). A compiler generated system call
    # would be a second one -- and would be a defect, not a style question.
    n=$(grep -cE '\bsvc\b' "$TMPD/a64.dis")
    [ "$n" -eq 1 ] && ok "exactly one svc, and the source wrote it itself" \
                   || bad "$n svc instructions in the object file, expected 1"
fi

echo "== 4. what round 80 could not produce is really in there =="
if [ -f "$TMPD/a64.dis" ]; then
    for want in eret 'mrs' 'msr' 'wfe'; do
        grep -qE "\b$want\b" "$TMPD/a64.dis" \
            && ok "'$want' in the machine code" || bad "'$want' is missing"
    done
    # The interrupt entry saves the whole corruptible set: ten `stp` pairs
    # BEFORE the frame is built (`stp x29, x30, [sp, #-16]!` is the marker
    # for the end of that sequence) -- codegen_a64.rs, INT_SAVE_A64.
    n=$(awk '/<_F0\.sync_handler>:/{f=1} f{print; if ($0 ~ /stp.*x29, x30, \[sp, #-16\]!/) exit}' \
        "$TMPD/a64.dis" | grep -cE '\bstp\b')
    [ "$n" -eq 11 ] && ok "sync_handler saves 10 register pairs + x29/x30" \
                    || bad "sync_handler has $n stp, expected 11"
    # And the checked arithmetic really reaches the external hand-off.
    grep -q 'osum_panic' "$TMPD/a64.dis" \
        && ok "the checked arithmetic hands off to osum_panic" \
        || bad "no osum_panic call in the object file"
fi

echo "== 5. both machines BOOT and say something (the measurement) =="
# --- x86-64
if command -v qemu-system-x86_64 >/dev/null 2>&1; then
    "$FIRNC" --target=x86_64-none -o "$TMPD/x86.o" "$X86SRC" 2>/dev/null
    as --64 -o "$TMPD/x86start.o" demos/freestanding/start.s 2>/dev/null
    if ld -n -T demos/freestanding/linker.ld --defsym=KERN_START=_F0.core_start \
          -o "$TMPD/x86.elf" "$TMPD/x86start.o" "$TMPD/x86.o" 2> >(grep -vE \
          'GNU-stack|deprecated|LOAD segment with RWX' >&2); then
        objcopy -O elf32-i386 "$TMPD/x86.elf" "$TMPD/x86.mb" 2>/dev/null
        timeout 30 qemu-system-x86_64 -kernel "$TMPD/x86.mb" -serial stdio \
            -display none -no-reboot > "$TMPD/qx86.txt" 2>&1
        if grep -q "FIRN: profile kernel ist" "$TMPD/qx86.txt" \
           && grep -q "freestanding." "$TMPD/qx86.txt"; then
            ok "x86_64-none: booted in qemu-system-x86_64, serial output appeared"
        else
            bad "x86_64-none: no serial output out of QEMU"
            sed 's/^/        /' "$TMPD/qx86.txt" | head -6
        fi
    else
        bad "x86_64-none: ld failed"
    fi
else
    echo "  (skipped: qemu-system-x86_64 is not available)"
fi
# --- aarch64
if command -v qemu-system-aarch64 >/dev/null 2>&1 && [ -f "$TMPD/a64.o" ]; then
    aarch64-linux-gnu-as -o "$TMPD/a64start.o" demos/freestanding/a64/start.s 2>"$TMPD/as.err" \
        && ok "start.s assembles (entry, vector table, osum_panic)" \
        || { bad "start.s"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
    if aarch64-linux-gnu-ld -T demos/freestanding/a64/linker.ld \
          --defsym=KERN_START=_F0.core_start --defsym=KERN_TRAP=_F0.sync_handler \
          -o "$TMPD/a64.elf" "$TMPD/a64start.o" "$TMPD/a64.o" 2> >(grep -vE \
          'GNU-stack|deprecated|LOAD segment with RWX' >&2); then
        left=$(aarch64-linux-gnu-nm -u "$TMPD/a64.elf" 2>/dev/null | sed '/^$/d')
        [ -z "$left" ] && ok "linked, the image has no open symbol" \
                       || { bad "open symbols in the image"; echo "$left" | sed 's/^/        /'; }
        timeout 40 qemu-system-aarch64 -M virt -cpu cortex-a57 -nographic \
            -kernel "$TMPD/a64.elf" > "$TMPD/qa64.txt" 2>&1
        # Four separate statements, and each one is a different claim:
        #   greeting  -> MMIO and the frame really work
        #   EL=1      -> `mrs` read a system register
        #   !8        -> the svc reached the #[interrupt] function, whose
        #                CHECKED arithmetic computed 8 without panicking
        #   back...   -> `eret` returned to the instruction after the svc
        for want in "FIRN: freestanding aarch64 is" "alive." "EL=1" "trap: !8" "back from eret."; do
            grep -qF "$want" "$TMPD/qa64.txt" \
                && ok "aarch64-none in qemu-system-aarch64 -M virt: '$want'" \
                || { bad "aarch64-none: '$want' did not appear"; }
        done
        if [ "$fail" -gt 0 ]; then
            sed 's/^/        /' "$TMPD/qa64.txt" | head -10
        fi
    else
        bad "aarch64-none: ld failed"
    fi
else
    echo "  (skipped: qemu-system-aarch64 is not available)"
fi

echo "--------------------------------------------------------------------"
echo "FREESTANDING TARGETS: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
