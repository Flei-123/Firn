#!/usr/bin/env bash
# tools/freestanding/run.sh -- THE PROOF THAT `profile kernel` MEANS SOMETHING.
#
# Round 52. What is checked is what can be READ OFF the produced file, not
# what the compiler claims about itself:
#
#   1. `demos/kernel/core.fi` compiles with BOTH compilers to an
#      ELF object file (`ET_REL`), not to an executable.
#   2. The object file has NO undefined name -- no libc, no
#      `_start`, no runtime. (`nm -u` yields nothing.)
#   3. It contains not a single `syscall` instruction (`objdump -d`).
#   4. It can be linked with `ld -T demos/kernel/linker.ld` into an image
#      and BOOTED in QEMU: the serial output of the kernel appears.
#   5. The inline assembly is really in there (`in`/`out`, `hlt`, `cli`),
#      the interrupt entry point ends with `iretq` and saves 14 registers.
#   6. The volatile promise holds in ALL THREE build stages: `asm` and MMIO
#      stay and are not merged (tests/850-854 check the same
#      at run time; here we count the instructions in the assembly).
#
# No comparison of the assembly texts between the stages: `firnc0` has a
# register allocation, `lib/firnc1/codegen.fi` does not. What is compared is what
# HAS to be equal -- the symbol table and the freestanding property.
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
SOURCE=demos/kernel/core.fi
LDSCRIPT=demos/kernel/linker.ld
# A temp directory of its own per run (several rounds run in parallel).
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
# Rebuild `.firnc1` when it is missing or a source is younger (the trap
# from round 35/45/46: an outdated binary measures yesterday's state).
neu=0
[ -x "$FC1" ] || neu=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && neu=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { neu=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
[ "$neu" -eq 1 ] && { rm -f "$FC1"; "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1; }

echo "== 1. compile (both compilers, the profile comes from the source) =="
"$FIRNC" -o "$TMPD/k0.o" "$SOURCE" 2>"$TMPD/e0" \
    && ok "firnc0: $SOURCE -> k0.o" || { bad "firnc0 does not compile it"; sed 's/^/        /' "$TMPD/e0"; }
"$FC1" "$SOURCE" -o "$TMPD/k1.o" >"$TMPD/e1" 2>&1 \
    && ok "firnc1: $SOURCE -> k1.o" || { bad "firnc1 does not compile it (rc=$?)"; sed 's/^/        /' "$TMPD/e1"; }

# Counter-check: the same source text with `--profile=app` MUST fail --
# `syscall` does exist there, but `#[interrupt]` does not. Without this probe
# a compiler that ignores the profile would pass here unnoticed.
if "$FIRNC" --profile=app -o "$TMPD/app.o" "$SOURCE" >"$TMPD/app.err" 2>&1; then
    bad "counter-check: --profile=app should have failed"
else
    grep -q "only in profile 'kernel'" "$TMPD/app.err" \
        && ok "counter-check: --profile=app is rejected (#[interrupt])" \
        || { bad "counter-check: wrong message"; sed 's/^/        /' "$TMPD/app.err" | head -4; }
fi

echo "== 2. it is an OBJECT file, and it is freestanding =="
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || { bad "firnc$s: keine Ausgabedatei"; continue; }
    kind=$(readelf -h "$f" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "REL" ] && ok "firnc$s: ELF-Typ REL (verschiebbare Objektdatei)" \
                       || bad "firnc$s: ELF kind '$kind', expected REL"
    undef=$(nm -u "$f" 2>/dev/null | sed '/^$/d')
    [ -z "$undef" ] && ok "firnc$s: NO undefined symbol" \
                    || { bad "firnc$s: undefinierte Symbole"; echo "$undef" | sed 's/^/        /'; }
    # Every defined symbol belongs to the program itself (prefix _F0./_F1.).
    fremd=$(nm --defined-only "$f" | awk '{print $3}' | grep -vE "^_F[01]\." || true)
    [ -z "$fremd" ] && ok "firnc$s: alle definierten Symbole sind eigene" \
                    || { bad "firnc$s: fremde Symbole"; echo "$fremd" | sed 's/^/        /'; }
    if objdump -d "$f" | grep -qE '^\s+[0-9a-f]+:.*\bsyscall\b'; then
        bad "firnc$s: the object file contains a syscall"
    else
        ok "firnc$s: no syscall in the machine code"
    fi
done

echo "== 3. link against the linker script (no libc, no crt files) =="
as --64 -o "$TMPD/start.o" demos/kernel/start.s 2>"$TMPD/as.err" \
    && ok "start.s assembles (multiboot header, long mode)" \
    || { bad "start.s"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || continue
    if ld -n -T "$LDSCRIPT" --defsym=KERN_START="_F$s.core_start" \
          -o "$TMPD/k$s.elf" "$TMPD/start.o" "$f" 2>"$TMPD/ld$s.err"; then
        ein=$(readelf -h "$TMPD/k$s.elf" | awk -F: '/Entry point/ {print $2}' | tr -d ' ')
        ok "firnc$s: gelinkt, Einsprung $ein"
        # The only code that does not come from Firn is `start.s`.
        undef=$(nm -u "$TMPD/k$s.elf" 2>/dev/null | sed '/^$/d')
        [ -z "$undef" ] && ok "firnc$s: the linked image has no open symbol" \
                        || { bad "firnc$s: offene Symbole im Abbild"; echo "$undef" | sed 's/^/        /'; }
    else
        bad "firnc$s: ld schlug fehl"; sed 's/^/        /' "$TMPD/ld$s.err" | head -5
    fi
done

echo "== 3b. boot in QEMU (the real proof) =="
if command -v qemu-system-x86_64 >/dev/null 2>&1; then
    for s in 0 1; do
        [ -f "$TMPD/k$s.elf" ] || continue
        # QEMU's multiboot loader only takes ELF32; all addresses lie below
        # 4 GiB, so a rewrite of the header is enough.
        objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
        timeout 20 qemu-system-x86_64 -kernel "$TMPD/k$s.mb" -serial stdio \
            -display none -no-reboot > "$TMPD/q$s.txt" 2>&1
        if grep -q "FIRN: profile kernel ist" "$TMPD/q$s.txt" \
           && grep -q "freestanding." "$TMPD/q$s.txt"; then
            ok "firnc$s: gebootet, serielle Ausgabe erschienen"
        else
            bad "firnc$s: keine serielle Ausgabe aus QEMU"
            sed 's/^/        /' "$TMPD/q$s.txt" | head -6
        fi
    done
else
    echo "  (skipped: qemu-system-x86_64 is not available)"
fi

echo "== 4. inline assembly and the interrupt ABI are really in the code =="
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || continue
    objdump -d "$f" > "$TMPD/d$s.txt"
    for instr in cli hlt "out    %al,(%dx)" "in     (%dx),%al" iretq; do
        if grep -qF "$instr" "$TMPD/d$s.txt"; then
            ok "firnc$s: '$instr' im Maschinencode"
        else
            bad "firnc$s: '$instr' is missing"
        fi
    done
    # `#[interrupt]`: 14 push + 14 pop in the entry point.
    n=$(awk '/<_F'"$s"'\.timer_ih>:/,/iretq/' "$TMPD/d$s.txt" | grep -cE '\bpush\b')
    [ "$n" -eq 15 ] && ok "firnc$s: timer_ih rettet 14 Register + rbp" \
                    || bad "firnc$s: timer_ih has $n push, expected 15"
done

echo "== 5. volatile haelt in allen drei Baustufen =="
# `cli` and `hlt` stand in `core_start`, which nobody calls -- so they cannot
# become more through inlining either. Exactly once, in every build stage.
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:---release-fast}
    "$FIRNC" $stage --emit=asm -o "$TMPD/k.s" "$SOURCE" 2>/dev/null || { bad "asm-Ausgabe $name"; continue; }
    c=$(grep -cE '^\s+cli$' "$TMPD/k.s")
    h=$(grep -cE '^\s+hlt$' "$TMPD/k.s")
    o=$(grep -cE '^\s+out dx, al$' "$TMPD/k.s")
    i=$(grep -cE '^\s+in al, dx$' "$TMPD/k.s")
    if [ "$c" = 1 ] && [ "$h" = 1 ] && [ "$o" -ge 1 ] && [ "$i" -ge 1 ]; then
        ok "$name: cli=1 hlt=1 (exakt), out=$o in=$i (>=1; --release-fast bettet out8/in8 ein)"
    else
        bad "$name: cli=$c hlt=$h out=$o in=$i"
    fi
done

# The SHARP count: tools/freestanding/volatile.fi has everything in ONE
# function and calls nothing -- inlining cannot shift the numbers.
# Counted in the FIR AFTER the optimiser: what it has left standing is
# written there. Three literally equal `asm("pause")` have to stay three
# (no CSE), two MMIO loads on the same address two (no
# merging), and the `rdtsc` with an unused result must not
# disappear (the trap from round 40).
VOL=tools/freestanding/volatile.fi
for stage in "" "--no-opt" "--opt-level=dev-fast"; do
    name=${stage:---release-fast}
    "$FIRNC" $stage --emit=fir "$VOL" > "$TMPD/v.fir" 2>/dev/null || { bad "volatile.fi $name"; continue; }
    a1=$(grep -cF 'asm.void "pause"' "$TMPD/v.fir")
    a2=$(grep -cF 'asm.u64 "rdtsc"' "$TMPD/v.fir")
    l=$(grep -cF 'mmio_load.u32' "$TMPD/v.fir")
    st=$(grep -cF 'mmio_store.u32' "$TMPD/v.fir")
    if [ "$a1" = 3 ] && [ "$a2" = 1 ] && [ "$l" = 2 ] && [ "$st" = 1 ]; then
        ok "volatile.fi $name: pause=3 rdtsc=1 mmio_load=2 mmio_store=1 (exakt)"
    else
        bad "volatile.fi $name: pause=$a1 (3) rdtsc=$a2 (1) load=$l (2) store=$st (1)"
    fi
    "$FIRNC" $stage -o "$TMPD/v0" "$VOL" 2>/dev/null && "$TMPD/v0" >/dev/null 2>&1
    [ "$?" = 6 ] && ok "volatile.fi $name: runs, yields 6" || bad "volatile.fi $name: wrong return value"
done
# And the same through the compiler in Firn: there is no optimiser there,
# so the assembly text counts -- and after that the behaviour.
rm -f "$TMPD/v1" "$TMPD/v1.s" "$TMPD/v1.o"
if "$FC1" "$VOL" -o "$TMPD/v1" >/dev/null 2>&1; then
    c=$(grep -cE '^\s+pause$' "$TMPD/v1.s")
    r=$(grep -cE '^\s+rdtsc$' "$TMPD/v1.s")
    [ "$c" = 3 ] && [ "$r" = 1 ] && ok "volatile.fi firnc1: pause=3 rdtsc=1 (exakt)" \
                                || bad "volatile.fi firnc1: pause=$c (3) rdtsc=$r (1)"
    "$TMPD/v1" >/dev/null 2>&1
    [ "$?" = 6 ] && ok "volatile.fi firnc1: runs, yields 6" || bad "volatile.fi firnc1: wrong return value"
else
    bad "volatile.fi: firnc1 does not compile it"
fi

echo
echo "FREESTANDING: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
exit 0
