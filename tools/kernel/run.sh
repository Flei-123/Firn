#!/usr/bin/env bash
# tools/kernel/run.sh -- THE PROOF THAT THE KERNEL DOES SOMETHING.
#
# Round 59. `tools/freestanding/run.sh` (round 52) proves what can be read
# off the OBJECT FILE: no undefined name, no libc, no syscall, and the
# boot in QEMU. This script proves what can be read off the RUNNING
# kernel -- and every case is a run of its own in QEMU, with a time limit,
# so that nothing can hang.
#
# The kernel under test is `demos/kernel/kmain.fi` with `boot.s` (long
# mode) and `isr.s` (interrupt entry points). It is built with BOTH
# compilers; a difference between the two would be a difference in the
# language, not in the kernel.
#
# The nine sections:
#
#   1. build with firnc0 and firnc1, link, multiboot image
#   2. freestanding evidence per object file: ELF REL, no undefined
#      symbol, no libc name, no collector, no `syscall` in the kernel's
#      own code
#   3. the full run: memory map, frame allocator, heap, timer, ring 3 --
#      every line has to be there, and the exit code has to say that the
#      kernel got to the end on its own
#   4. firnc0 against firnc1: the serial output has to be EQUAL apart
#      from addresses
#   5. the four exceptions: #DE, #PF, #GP, #DF -- each one reported with
#      error code and register set, and each one has to STOP the kernel
#   6. the double fault out of firnc1 as well (that is the path over IST1)
#   7. keyboard: keys through the QEMU monitor, characters over the serial
#      port -- with the counter-check that without keys nothing appears
#   8. timer counter-check: with IRQ0 masked the same loop counts zero
#      ticks
#   9. ring 3 counter-check: the user program executes `hlt`, and the #GP
#      that follows names cs=0x2b -- ring 3, not ring 0
#
# QEMU is started with `-display none -serial ... -no-reboot`, plus
# `-device isa-debug-exit`: the kernel ends the machine itself, and the
# exit code says whether it went cleanly (21) or over an exception (63).
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
SOURCE=demos/kernel/kmain.fi
LDSCRIPT=demos/kernel/kernel.ld
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "KERNEL: skipped, qemu-system-x86_64 is not available"
    exit 0
fi

# Rebuild `.firnc1` when it is missing or a source is younger (the trap
# from round 35/45/46: an outdated binary measures yesterday's state).
fresh=0
[ -x "$FC1" ] || fresh=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && fresh=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { fresh=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
[ "$fresh" -eq 1 ] && { rm -f "$FC1"; "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1; }

# One run: $1 = image, $2 = command line, $3 = output file.
# Returns the exit code of QEMU (21 = the kernel shut down cleanly,
# 63 = it stopped at an exception, 124 = the time limit struck).
run_kernel() {
    timeout 60 qemu-system-x86_64 -kernel "$1" -m 128 -append "$2" \
        -serial "file:$3" -display none -no-reboot \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

# Addresses differ between the compilers (different code, different
# lengths); everything else must not.
normalise() {
    sed -E 's/0x[0-9a-f]+/0xX/g; s/k[01]\.mb/kernel.mb/g; s/[0-9]+/N/g' "$1"
}

echo "== 1. build the kernel with both compilers =="
as --64 -o "$TMPD/boot.o" demos/kernel/boot.s 2>"$TMPD/as1.err" \
    && ok "boot.s assembles (multiboot, long mode, GDT, TSS)" \
    || { bad "boot.s"; sed 's/^/        /' "$TMPD/as1.err" | head -5; }
as --64 -o "$TMPD/isr.o" demos/kernel/isr.s 2>"$TMPD/as2.err" \
    && ok "isr.s assembles (48 vectors, syscall, ring 3)" \
    || { bad "isr.s"; sed 's/^/        /' "$TMPD/as2.err" | head -5; }

"$FIRNC" -o "$TMPD/k0.o" "$SOURCE" 2>"$TMPD/e0" \
    && ok "firnc0: $SOURCE -> k0.o" \
    || { bad "firnc0 does not compile the kernel"; sed 's/^/        /' "$TMPD/e0" | head -10; }
"$FC1" "$SOURCE" -o "$TMPD/k1.o" >"$TMPD/e1" 2>&1 \
    && ok "firnc1: $SOURCE -> k1.o" \
    || { bad "firnc1 does not compile the kernel"; sed 's/^/        /' "$TMPD/e1" | head -10; }

for s in 0 1; do
    [ -f "$TMPD/k$s.o" ] || continue
    if ld -n -T "$LDSCRIPT" \
          --defsym=KERNEL_MAIN="_F$s.kernel_main" \
          --defsym=KERNEL_TRAP="_F$s.trap__entry" \
          --defsym=KERNEL_SYSCALL="_F$s.user__handle" \
          -o "$TMPD/k$s.elf" "$TMPD/boot.o" "$TMPD/isr.o" "$TMPD/k$s.o" \
          2>"$TMPD/ld$s.err"; then
        objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
        ok "firnc$s: linked and turned into a multiboot image"
    else
        bad "firnc$s: ld failed"; sed 's/^/        /' "$TMPD/ld$s.err" | head -5
    fi
done

echo "== 2. freestanding: what the object file says about itself =="
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || continue
    kind=$(readelf -h "$f" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "REL" ] && ok "firnc$s: ELF type REL (object file, not an executable)" \
                        || bad "firnc$s: ELF kind '$kind', expected REL"
    undef=$(nm -u "$f" 2>/dev/null | sed '/^$/d')
    [ -z "$undef" ] && ok "firnc$s: no undefined symbol (no libc, no runtime)" \
                    || { bad "firnc$s: undefined symbols"; echo "$undef" | sed 's/^/        /'; }
    foreign=$(nm --defined-only "$f" | awk '{print $3}' | grep -vE "^_F[01]\." || true)
    [ -z "$foreign" ] && ok "firnc$s: every defined symbol is its own" \
                      || { bad "firnc$s: foreign symbols"; echo "$foreign" | sed 's/^/        /'; }
    if nm "$f" | grep -qiE '(malloc|printf|memcpy|__gc_|gc_alloc|mmap)'; then
        bad "firnc$s: a libc or collector name is in the symbol table"
    else
        ok "firnc$s: no libc name, no collector in the symbol table"
    fi
    if objdump -d "$f" | grep -qE '^\s+[0-9a-f]+:.*\bsyscall\b'; then
        bad "firnc$s: the kernel's own code contains a syscall"
    else
        ok "firnc$s: no syscall in the kernel's own code"
    fi
done
# The `syscall` instruction exists only in isr.s: the entry point and the
# two calls of the user program. Round 52 forbids it in Firn code; a
# kernel needs it as a target.
if [ -f "$TMPD/k0.elf" ]; then
    n=$(objdump -d "$TMPD/k0.elf" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    [ "$n" -eq 2 ] && ok "image: exactly 2 syscall instructions, both in the user program of isr.s" \
                   || bad "image: $n syscall instructions, expected 2"
fi

echo "== 3. the full run (both compilers) =="
expected=(
    "firn kernel r59, profile kernel"
    "idt: 48 gates, vector 8 IST"
    "pic: master 0x20, slave 0x28"
    "pit: 100 Hz, divisor 11931"
    "frame test: ok"
    "heap test: ok"
    "ring3: syscall 1 arg 59"
    "ring3: back in ring 0"
    "kernel: done"
)
for s in 0 1; do
    [ -f "$TMPD/k$s.mb" ] || continue
    run_kernel "$TMPD/k$s.mb" "nokbd" "$TMPD/full$s.txt"; rc=$?
    [ "$rc" -eq 21 ] && ok "firnc$s: the kernel shut down on its own (exit 21)" \
                     || bad "firnc$s: QEMU exit code $rc, expected 21"
    miss=0
    for line in "${expected[@]}"; do
        grep -qF "$line" "$TMPD/full$s.txt" || { bad "firnc$s: the line '$line' is missing"; miss=1; }
    done
    [ "$miss" -eq 0 ] && ok "firnc$s: all ${#expected[@]} expected lines are there"
    # The numbers, not just the words.
    ticks=$(grep -oE '^ticks: [0-9]+' "$TMPD/full$s.txt" | head -1 | awk '{print $2}')
    if [ -n "$ticks" ] && [ "$ticks" -ge 20 ]; then
        ok "firnc$s: $ticks timer interrupts counted (at least 20)"
    else
        bad "firnc$s: tick counter '$ticks', expected at least 20"
    fi
    spin=$(grep -oE '^spin: ticks=\+[0-9]+' "$TMPD/full$s.txt" | head -1 | grep -oE '[0-9]+$')
    if [ -n "$spin" ] && [ "$spin" -ge 1 ]; then
        ok "firnc$s: $spin ticks arrived during the spin loop"
    else
        bad "firnc$s: spin loop counted '$spin' ticks, expected at least 1"
    fi
    frames=$(grep -oE '^frames: [0-9]+ covered, [0-9]+ free' "$TMPD/full$s.txt" | head -1)
    [ -n "$frames" ] && ok "firnc$s: memory map read ($frames)" \
                     || bad "firnc$s: no frame line"
    if grep -q '^heap test: ok' "$TMPD/full$s.txt" && grep -q '1M refused=1' "$TMPD/full$s.txt"; then
        ok "firnc$s: the heap refuses a request bigger than itself"
    else
        bad "firnc$s: the counter-check of the heap is missing"
    fi
    # No two different frames may carry the same address.
    line=$(grep -m1 '^frame test: a=' "$TMPD/full$s.txt")
    a=$(echo "$line" | grep -oE ' a=0x[0-9a-f]+' | cut -d= -f2)
    b=$(echo "$line" | grep -oE '  b=0x[0-9a-f]+' | cut -d= -f2)
    c=$(echo "$line" | grep -oE '  c=0x[0-9a-f]+' | cut -d= -f2)
    again=$(echo "$line" | grep -oE 'again b=0x[0-9a-f]+' | cut -d= -f2)
    if [ -n "$a" ] && [ "$a" != "$b" ] && [ "$b" != "$c" ] && [ "$a" != "$c" ] && [ "$again" = "$b" ]; then
        ok "firnc$s: three frames $a $b $c, and the freed one comes back"
    else
        bad "firnc$s: frames a=$a b=$b c=$c again=$again"
    fi
done

echo "== 4. firnc0 against firnc1: the same kernel says the same thing =="
if [ -f "$TMPD/full0.txt" ] && [ -f "$TMPD/full1.txt" ]; then
    normalise "$TMPD/full0.txt" > "$TMPD/n0.txt"
    normalise "$TMPD/full1.txt" > "$TMPD/n1.txt"
    if diff -q "$TMPD/n0.txt" "$TMPD/n1.txt" >/dev/null; then
        ok "the serial output of both compilers is equal apart from addresses"
    else
        bad "the two kernels behave differently"
        diff "$TMPD/n0.txt" "$TMPD/n1.txt" | head -10 | sed 's/^/        /'
    fi
fi

echo "== 5. the four exceptions =="
check_trap() {
    local stage=$1 kind=$2 vector=$3 name=$4 extra=$5
    run_kernel "$TMPD/k$stage.mb" "trap=$kind nokbd noring3" "$TMPD/t$kind$stage.txt"
    local rc=$?
    local f="$TMPD/t$kind$stage.txt"
    [ "$rc" -eq 63 ] || { bad "firnc$stage/$kind: QEMU exit code $rc, expected 63"; return; }
    grep -qE "^\*\*\* EXCEPTION $vector $name" "$f" \
        || { bad "firnc$stage/$kind: no report '*** EXCEPTION $vector $name'"; tail -4 "$f" | sed 's/^/        /'; return; }
    grep -qE '^  rip=0x[0-9a-f]+  cs=0x[0-9a-f]+  rflags=0x[0-9a-f]+' "$f" \
        || { bad "firnc$stage/$kind: the frame line is missing"; return; }
    grep -qE '^  rax=0x[0-9a-f]{16}' "$f" \
        || { bad "firnc$stage/$kind: the register set is missing"; return; }
    if [ -n "$extra" ]; then
        grep -qF "$extra" "$f" || { bad "firnc$stage/$kind: '$extra' is missing"; return; }
    fi
    # A kernel that keeps going after an exception is lying.
    grep -q "kernel: done" "$f" && { bad "firnc$stage/$kind: the kernel kept going after the exception"; return; }
    ok "firnc$stage: $name reported (vector $vector), the kernel stops"
}
check_trap 0 de 0 "#DE" ""
check_trap 0 pf 14 "#PF" "cr2=0x40000000"
check_trap 0 gp 13 "#GP" ""
check_trap 0 df 8 "#DF" ""

echo "== 6. the double fault out of the compiler in Firn as well =="
[ -f "$TMPD/k1.mb" ] && check_trap 1 df 8 "#DF" ""

echo "== 7. the keyboard (keys through the QEMU monitor) =="
if [ -f "$TMPD/k0.mb" ]; then
    rm -f "$TMPD/mon.sock" "$TMPD/kbd.txt" "$TMPD/kbd.rc"
    ( timeout 60 qemu-system-x86_64 -kernel "$TMPD/k0.mb" -m 128 -append "noring3" \
        -serial "file:$TMPD/kbd.txt" -display none -no-reboot \
        -monitor "unix:$TMPD/mon.sock,server,nowait" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
      echo $? > "$TMPD/kbd.rc" ) &
    qemu_pid=$!
    # Wait for the kernel to say it is ready -- and not longer.
    for _ in $(seq 1 100); do
        [ -f "$TMPD/kbd.txt" ] && grep -q "kbd: ready" "$TMPD/kbd.txt" && break
        sleep 0.2
    done
    if [ -S "$TMPD/mon.sock" ] && grep -q "kbd: ready" "$TMPD/kbd.txt" 2>/dev/null; then
        python3 - "$TMPD/mon.sock" <<'PY'
import socket, sys, time
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
time.sleep(0.3)
for key in ["f", "i", "r", "n"]:
    s.sendall(("sendkey %s\n" % key).encode())
    time.sleep(0.25)
s.close()
PY
        wait $qemu_pid 2>/dev/null
        rc=$(cat "$TMPD/kbd.rc" 2>/dev/null || echo 99)
        [ "$rc" -eq 21 ] && ok "keyboard run: the kernel shut down on its own (exit 21)" \
                         || bad "keyboard run: exit code $rc, expected 21"
        grep -q "^kbd: firn$" "$TMPD/kbd.txt" \
            && ok "IRQ1: the four keys arrived as 'firn' over the serial port" \
            || { bad "IRQ1: no 'kbd: firn'"; grep '^kbd' "$TMPD/kbd.txt" | sed 's/^/        /'; }
        n=$(grep -c '^key: ' "$TMPD/kbd.txt")
        [ "$n" -eq 4 ] && ok "IRQ1: exactly 4 key events (no repeats, no release codes)" \
                       || bad "IRQ1: $n key events, expected 4"
    else
        kill $qemu_pid 2>/dev/null
        bad "keyboard run: the kernel never said 'kbd: ready'"
    fi
    # Counter-check: without keys nothing may appear. Otherwise the line
    # above would prove nothing.
    run_kernel "$TMPD/k0.mb" "noring3" "$TMPD/nokey.txt"
    grep -q "^kbd: (none)$" "$TMPD/nokey.txt" \
        && ok "counter-check: without keys the kernel reports '(none)'" \
        || { bad "counter-check keyboard"; grep '^kbd' "$TMPD/nokey.txt" | sed 's/^/        /'; }
fi

echo "== 8. counter-check for the timer: masked IRQ0 counts nothing =="
if [ -f "$TMPD/k0.mb" ]; then
    run_kernel "$TMPD/k0.mb" "notimer nokbd noring3" "$TMPD/notimer.txt"; rc=$?
    [ "$rc" -eq 21 ] && ok "masked run: the kernel gets to the end (exit 21)" \
                     || bad "masked run: exit code $rc, expected 21"
    grep -q "^spin: ticks=+0$" "$TMPD/notimer.txt" \
        && ok "counter-check: with IRQ0 masked the spin loop counts 0 ticks" \
        || { bad "counter-check timer"; grep '^spin' "$TMPD/notimer.txt" | sed 's/^/        /'; }
fi

echo "== 9. counter-check for ring 3: hlt is forbidden down there =="
if [ -f "$TMPD/k0.mb" ]; then
    run_kernel "$TMPD/k0.mb" "ring3fault nokbd" "$TMPD/ring3.txt"; rc=$?
    [ "$rc" -eq 63 ] && ok "ring 3 counter-check: the kernel stops at the exception (exit 63)" \
                     || bad "ring 3 counter-check: exit code $rc, expected 63"
    grep -qE '^\*\*\* EXCEPTION 13 #GP' "$TMPD/ring3.txt" \
        && ok "ring 3 counter-check: 'hlt' in the user program yields #GP" \
        || { bad "ring 3 counter-check: no #GP"; tail -4 "$TMPD/ring3.txt" | sed 's/^/        /'; }
    grep -qE '^  rip=0x[0-9a-f]+  cs=0x2b' "$TMPD/ring3.txt" \
        && ok "ring 3 counter-check: cs=0x2b -- the processor really was in ring 3" \
        || { bad "ring 3 counter-check: the fault did not come out of ring 3"; grep '  rip=' "$TMPD/ring3.txt" | sed 's/^/        /'; }
fi

echo
echo "KERNEL: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
exit 0
