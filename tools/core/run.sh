#!/usr/bin/env bash
# tools/core/run.sh -- THE PROOF THAT A KERNEL MAY USE THE LIBRARY
# (round 73, docs/ROUND73.md).
#
# Round 52 forbade `import std.*` under `profile kernel` wholesale. Round 73
# makes the ban precise: the half of the library that needs neither an
# allocator nor a system call moved into `lib/std/core.fi`, and that module
# is admitted. What is proven here is what can be READ OFF the produced
# file and off a machine that really boots -- not what the compiler claims
# about itself.
#
#   1. `demos/kernel/kcore.fi` compiles with BOTH compilers in the kernel
#      profile to an ELF OBJECT FILE, although it says `import std.core`.
#   2. The object file has NO undefined name and contains NOT ONE `syscall`
#      instruction. That is the hard form of "this library asks nobody for
#      anything".
#   3. It boots in QEMU, and over the serial line it says what it computed:
#      a kernel command line searched, trimmed and split, the number behind
#      `root=` read, a UTF-8 character decoded, integer mathematics, and an
#      `Arena` over physical memory that hands out, refuses, grows, frees
#      and resets -- all of it through the `Allocator` interface.
#   4. THE COUNTER-CHECKS. `import std.io` and `import std.str` in the
#      kernel profile HAVE to be refused, with the known message, in BOTH
#      compilers. And a module that CLAIMS `profile kernel` but makes a
#      `syscall` has to be refused as well -- that is what makes the
#      declaration a checked property instead of a promise.
#   5. THE MEMORY PROOF. `tools/core/soak.fi` runs hundreds of thousands of
#      requests through the arena; RSS has to stay flat and the number of
#      system calls for memory has to stay at ONE. The counter-check is the
#      same program taking a fresh page from the operating system every
#      round and never giving it back -- its RSS HAS to climb.
#
# Environment: CORE_ROUNDS (default 40000), CORE_LEAK_ROUNDS (20000),
# CORE_SLACK (pages the soak run may drift, default 64 = 256 KiB).
set -uo pipefail
cd "$(dirname "$0")/../.."
ROOT=$(pwd)

export FIRNLIB="$ROOT/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
SOURCE=demos/kernel/kcore.fi
LDSCRIPT=demos/kernel/linker.ld
ROUNDS=${CORE_ROUNDS:-40000}
LEAK_ROUNDS=${CORE_LEAK_ROUNDS:-20000}
SLACK=${CORE_SLACK:-64}

TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }

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

echo "== 1. a kernel that says 'import std.core' compiles (both compilers) =="
"$FIRNC" -o "$TMPD/k0.o" "$SOURCE" 2>"$TMPD/e0" \
    && ok "firnc0: $SOURCE -> k0.o" \
    || { bad "firnc0 does not compile it"; sed 's/^/        /' "$TMPD/e0" | head -8; }
"$FC1" "$SOURCE" -o "$TMPD/k1.o" >"$TMPD/e1" 2>&1 \
    && ok "firnc1: $SOURCE -> k1.o" \
    || { bad "firnc1 does not compile it"; sed 's/^/        /' "$TMPD/e1" | head -8; }

echo "== 2. freestanding all the same: object file, no open name, no syscall =="
for s in 0 1; do
    f="$TMPD/k$s.o"
    [ -f "$f" ] || { bad "firnc$s: no output file"; continue; }
    kind=$(readelf -h "$f" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "REL" ] && ok "firnc$s: ELF type REL (relocatable object file)" \
                        || bad "firnc$s: ELF kind '$kind', expected REL"
    undef=$(nm -u "$f" 2>/dev/null | sed '/^$/d')
    [ -z "$undef" ] && ok "firnc$s: NO undefined symbol" \
                    || { bad "firnc$s: undefined symbols"; echo "$undef" | sed 's/^/        /'; }
    # NOTE: no `... | grep -q` here. `grep -q` leaves the pipe as soon as it
    # has its hit, the producer dies of SIGPIPE, and with `set -o pipefail`
    # the pipeline then reports a FAILURE -- nondeterministically, because
    # it is a race. That cost this script two runs; the output goes into a
    # file and grep reads the file.
    objdump -d "$f" > "$TMPD/d$s.txt"
    if grep -qE '^\s+[0-9a-f]+:.*\bsyscall\b' "$TMPD/d$s.txt"; then
        bad "firnc$s: the object file contains a syscall -- std.core allocates after all"
    else
        ok "firnc$s: not one syscall instruction in the machine code"
    fi
    # The library really is IN there: the search, the number reader and the
    # allocator have to appear as symbols of their own, otherwise the test
    # proves nothing about std.core.
    nm --defined-only "$f" > "$TMPD/n$s.txt"
    for sym in core__find_ab core__read_i64 core__utf8_read core__Arena__raw_alloc; do
        if grep -q "_F$s\.$sym\$" "$TMPD/n$s.txt"; then
            ok "firnc$s: $sym is in the image"
        else
            bad "firnc$s: $sym is missing -- the library was not linked in"
        fi
    done
done

echo "== 3. it boots, and it says what it computed =="
as --64 -o "$TMPD/start.o" demos/kernel/start.s 2>"$TMPD/as.err" \
    && ok "start.s assembles" \
    || { bad "start.s"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "  (QEMU is not available -- the boot proof is skipped)"
else
    for s in 0 1; do
        [ -f "$TMPD/k$s.o" ] || continue
        if ! ld -n -T "$LDSCRIPT" --defsym=KERN_START="_F$s.kcore_start" \
                -o "$TMPD/k$s.elf" "$TMPD/start.o" "$TMPD/k$s.o" 2>"$TMPD/ld$s.err"; then
            bad "firnc$s: ld failed"; sed 's/^/        /' "$TMPD/ld$s.err" | head -5
            continue
        fi
        objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
        timeout 30 qemu-system-x86_64 -kernel "$TMPD/k$s.mb" -serial stdio \
            -display none -no-reboot > "$TMPD/q$s.txt" 2>&1
        for want in \
            'FIRN kernel + std.core' \
            'text: len=19 at=6 parts=3 root=17 trim=[vga=1 root=17 quiet]' \
            'utf8: bytes=4 chars=3 cp=955' \
            'math: isqrt=12 gcd=6 ilog2=10' \
            'arena: used=12288 high=12288 after=4096 given=2 refused=1 grown=8192' \
            'core.ok'
        do
            grep -qF "$want" "$TMPD/q$s.txt" \
                && ok "firnc$s: '$want'" \
                || { bad "firnc$s: the line '$want' is missing"; sed 's/^/        /' "$TMPD/q$s.txt" | head -8; }
        done
        grep -qF 'core.bad' "$TMPD/q$s.txt" \
            && bad "firnc$s: the kernel reported a wrong result (core.bad)" \
            || ok "firnc$s: no wrong result in the kernel"
    done
fi

echo "== 4. the counter-checks: what stays forbidden =="
MSG="belongs to the standard library and is not available in profile 'kernel'"
for m in io str vec rc; do
    printf 'profile kernel\nimport std.%s\nfn kmain() -> i32 { return 0 }\n' "$m" \
        > "$TMPD/no_$m.fi"
    if "$FIRNC" -o "$TMPD/x.o" "$TMPD/no_$m.fi" >"$TMPD/n0_$m.txt" 2>&1; then
        bad "firnc0: 'import std.$m' was ACCEPTED in the kernel profile"
    else
        grep -qF "$MSG" "$TMPD/n0_$m.txt" \
            && ok "firnc0: 'import std.$m' refused, message unchanged" \
            || { bad "firnc0: 'import std.$m' refused with the WRONG message"; sed 's/^/        /' "$TMPD/n0_$m.txt" | head -3; }
    fi
    if "$FC1" "$TMPD/no_$m.fi" -o "$TMPD/x1.o" >"$TMPD/n1_$m.txt" 2>&1; then
        bad "firnc1: 'import std.$m' was ACCEPTED in the kernel profile"
    else
        grep -qF "$MSG" "$TMPD/n1_$m.txt" \
            && ok "firnc1: 'import std.$m' refused, message unchanged" \
            || { bad "firnc1: 'import std.$m' refused with the WRONG message"; sed 's/^/        /' "$TMPD/n1_$m.txt" | head -3; }
    fi
done

# The sharp one: a module that CLAIMS the kernel profile and allocates all
# the same. The declaration is a claim, not a permission slip -- the module
# lands in the same compilation unit and every `syscall` in it is an error
# at the line where it stands. Without this probe the whole rule of round
# 73 would rest on a comment.
mkdir -p "$TMPD/std"
cat > "$TMPD/std/liar.fi" <<'EOF'
profile kernel
export { grab }
fn grab(n: usize) -> u64 {
    return syscall(9, 0, n as i64, 3, 34, -1, 0) as u64
}
EOF
cat > "$TMPD/lie.fi" <<'EOF'
profile kernel
import std.liar
fn kmain() -> i32 { return liar.grab(4096) as i32 }
EOF
if "$FIRNC" -o "$TMPD/x.o" "$TMPD/lie.fi" >"$TMPD/lie0.txt" 2>&1; then
    bad "firnc0: a module that CLAIMS 'profile kernel' and allocates was accepted"
else
    grep -qF "'syscall' does not exist in profile 'kernel'" "$TMPD/lie0.txt" \
        && ok "firnc0: the claim is checked -- the hidden syscall is refused" \
        || { bad "firnc0: refused, but not because of the syscall"; sed 's/^/        /' "$TMPD/lie0.txt" | head -4; }
fi
# firnc1 counts its type check errors and returns 1; it has no message text
# for them (docs/SELF_HOSTING.md -- the diagnostics of stage 1 are the
# exit code). So here what is asked of it is what it can say: a refusal.
if "$FC1" "$TMPD/lie.fi" -o "$TMPD/x1.o" >"$TMPD/lie1.txt" 2>&1; then
    bad "firnc1: a module that CLAIMS 'profile kernel' and allocates was accepted"
else
    ok "firnc1: the claim is checked -- the hidden syscall is refused"
fi

# And the other direction: the same import in the APP profile stays legal.
# A rule that forbids everything would pass every counter-check above.
printf 'import std.io\nimport std.core\nfn main() -> i32 { return core.isqrt(144) as i32 - 12 }\n' \
    > "$TMPD/app.fi"
"$FIRNC" -o "$TMPD/app" "$TMPD/app.fi" >"$TMPD/app.txt" 2>&1 \
    && ok "firnc0: std.core and std.io side by side in the app profile" \
    || { bad "firnc0: the app profile broke"; sed 's/^/        /' "$TMPD/app.txt" | head -4; }

echo "== 5. the memory proof: an arena does not grow =="
if ! "$FIRNC" -o "$TMPD/soak" tools/core/soak.fi >"$TMPD/soak.log" 2>&1; then
    bad "tools/core/soak.fi does not compile"
    sed 's/^/        /' "$TMPD/soak.log" | head -8
else
    field() { echo "$1" | tr ' ' '\n' | grep -A1 "^$2\$" | tail -1; }
    A_OUT=$(printf 'arena\n%s\n' "$ROUNDS" | "$TMPD/soak")
    L_OUT=$(printf 'leak\n%s\n' "$LEAK_ROUNDS" | "$TMPD/soak")

    a_bad=$(field "$A_OUT" bad)
    a_given=$(field "$A_OUT" given)
    a_calls=$(field "$A_OUT" syscalls)
    a_first=$(field "$A_OUT" rss_first)
    a_last=$(field "$A_OUT" rss_last)
    l_first=$(field "$L_OUT" rss_first)
    l_last=$(field "$L_OUT" rss_last)
    l_leaked=$(field "$L_OUT" leaked)

    [ "$a_bad" = 0 ] && ok "soak: $a_given blocks out of the arena, none wrong" \
                     || bad "soak: $a_bad rounds with a wrong block"
    [ "$a_calls" = 1 ] && ok "soak: exactly ONE system call for memory in the whole run" \
                       || bad "soak: $a_calls system calls for memory, expected 1"
    drift=$((a_last - a_first))
    [ "$drift" -lt 0 ] && drift=$((0 - drift))
    if [ "$drift" -gt "$SLACK" ]; then
        bad "soak: RSS drifted by $drift pages ($a_first -> $a_last), at most $SLACK allowed"
    else
        ok "soak: RSS $a_first -> $a_last pages (drift $drift, limit $SLACK)"
    fi
    want=$((LEAK_ROUNDS / 2))
    growth=$((l_last - l_first))
    if [ "$growth" -lt "$want" ]; then
        bad "counter-check does NOT strike: RSS grew by only $growth pages ($l_first -> $l_last) over $l_leaked leaked pages -- the measurement is broken"
    else
        ok "counter-check strikes: without free RSS $l_first -> $l_last pages (+$growth)"
    fi
fi

echo
if [ "$fail" -eq 0 ]; then
    echo "CORE: $pass proofs, 0 failures"
    exit 0
fi
echo "CORE: $fail of $((pass + fail)) failed"
exit 1
