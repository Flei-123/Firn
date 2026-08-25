#!/usr/bin/env bash
# tools/smp/run.sh -- ROUND K5: THE PROOF THAT FOUR CORES ARE FOUR CORES.
#
# Everything up to round K2 ran on one processor of a machine that has
# several. This file measures whether that is still true, and it does so
# the way this project measures everything: the number has to come out of
# a run, and every number gets a run in which it has to COLLAPSE.
#
#   1. COUNTING. The same kernel image under `-smp 1`, `-smp 2` and
#      `-smp 4` has to report one, two and four processors, out of the
#      ACPI MADT. Nothing in the kernel knows the number beforehand.
#   2. STARTING. Under `-smp 4` all four have to be online. The
#      counter-check is `nosmp`: the same four-processor machine, the
#      trampoline deliberately not sent -- one core online, three idle.
#   3. SPEED. The SAME twelve units of arithmetic, once on one core and
#      once spread over four. The four-core run has to be measurably
#      faster, and the counter-check is `nosmp` again: identical machine,
#      identical work, the other three cores simply not started.
#   4. THE LOCK. Every core raises the same counter under a lock; the sum
#      has to be exact. Counter-check `nolock`: the same kernel with the
#      lock switched off has to LOSE increments.
#   5. THE ALLOCATOR. Every core takes sixteen frames out of the one
#      allocator; no frame may come twice. Counter-check `nolock`: without
#      the lock the same frame is handed to two cores.
#   6. THE SCHEDULER. Eight kernel tasks in one run queue, taken by
#      whichever core is free. More than one core has to have run them,
#      and all eight have to finish.
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
LDSCRIPT=demos/kernel/kernel.ld
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

num() { # name value op expected
    local name=$1 value=$2 op=$3 want=$4
    if [ -z "$value" ]; then bad "$name: no number found (expected $op $want)"; return; fi
    if [ "$value" -"$op" "$want" ] 2>/dev/null; then ok "$name: $value"
    else bad "$name: $value, expected $op $want"; fi
}

value() { # file pattern -> the number at the end of the first match
    grep -oE "$2" "$1" 2>/dev/null | head -1 | grep -oE '[0-9]+$'
}

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "SMP: skipped, qemu-system-x86_64 is not available"
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

# ---------------------------------------------------------------- build

echo "== 1. build the kernel with the trampoline of the other processors =="
for f in boot isr switch smp; do
    as --64 -o "$TMPD/$f.o" "demos/kernel/$f.s" 2>"$TMPD/as.err" \
        && ok "$f.s assembles" \
        || { bad "$f.s"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
done
# The blob really has to fit under the parameter block: the boot processor
# copies it to 0x8000 and writes the parameters at 0x8F00.
blob=$(nm "$TMPD/smp.o" 2>/dev/null | awk '/ap_trampoline_end/ {print $1}')
if [ -n "$blob" ]; then
    n=$((16#$blob))
    num "smp.s: the trampoline blob is this many octets long" "$n" lt 3840
else
    bad "smp.s: no ap_trampoline_end"
fi

for s in 0 1; do
    if [ "$s" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
    "$C" -o "$TMPD/k$s.o" demos/kernel/kmain.fi >"$TMPD/e$s" 2>&1 \
        || { bad "firnc$s does not compile the kernel"; sed 's/^/        /' "$TMPD/e$s" | head -8; continue; }
    "$C" -o "$TMPD/uprog$s.o" demos/kernel/uprog.fi >>"$TMPD/e$s" 2>&1 \
        || { bad "firnc$s does not compile the user programs"; continue; }
    if ld -n -T "$LDSCRIPT" \
          --defsym=KERNEL_MAIN="_F$s.kernel_main" \
          --defsym=KERNEL_TRAP="_F$s.trap__entry" \
          --defsym=KERNEL_SYSCALL="_F$s.sys__entry" \
          --defsym=KERNEL_TASK_MAIN="_F$s.tasks__main" \
          --defsym=KERNEL_USER_START="_F$s.proc__user_start" \
          --defsym=KERNEL_AP_MAIN="_F$s.smp__ap_main" \
          --defsym=USER_MAIN="_F$s.u_enter" \
          -o "$TMPD/k$s.elf" "$TMPD/boot.o" "$TMPD/isr.o" "$TMPD/switch.o" \
          "$TMPD/smp.o" "$TMPD/k$s.o" "$TMPD/uprog$s.o" 2>"$TMPD/ld$s.err"; then
        objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
        ok "firnc$s: kernel with acpi.fi, cpu.fi, atomic.fi, smp.fi linked"
    else
        bad "firnc$s: ld failed"
        grep -v 'GNU-stack\|RWX\|deprecated' "$TMPD/ld$s.err" | sed 's/^/        /' | head -5
    fi
done
[ -f "$TMPD/k0.mb" ] || { echo "SMP: $pass passed, $((fail+1)) failed"; exit 1; }

# THE INSTRUCTIONS. Round 47 said of `lock xadd`: "provable at the emitted
# instruction". Round K5 uses it and `lock cmpxchg` for real, and the two
# have to be IN the image -- a lock built out of ordinary loads and stores
# would look exactly the same in Firn and be worth nothing.
for s in 0 1; do
    [ -f "$TMPD/k$s.o" ] || continue
    cx=$(objdump -d "$TMPD/k$s.o" | grep -cE 'lock cmpxchg')
    xa=$(objdump -d "$TMPD/k$s.o" | grep -cE 'lock xadd')
    num "firnc$s: 'lock cmpxchg' in the image (the lock itself)" "$cx" ge 1
    num "firnc$s: 'lock xadd' in the image (every counter)" "$xa" ge 3
done

# --------------------------------------------------------------- the runs

# MTTCG. QEMU emulates the cores of a guest in ONE host thread unless it
# is told otherwise, and then four cores are four times the work on one
# thread and nothing can get faster. `thread=multi` is what makes the
# measurement a measurement -- and `thread=single` is kept as the third
# counter-check below.
run() { # cores append out [thread mode]
    local cores=$1 append=$2 out=$3 mode=${4:-multi}
    timeout 300 qemu-system-x86_64 -accel "tcg,thread=$mode" -smp "$cores" \
        -kernel "$TMPD/k0.mb" -m 128 -append "nokbd $append" \
        -serial "file:$out" -display none -no-reboot \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

echo "== 2. the machine is counted, not guessed (ACPI MADT) =="
for n in 1 2 4; do
    run "$n" "" "$TMPD/c$n.txt"; rc=$?
    [ "$rc" -eq 21 ] || bad "-smp $n: QEMU exit code $rc, expected 21"
    got=$(value "$TMPD/c$n.txt" 'smp: cpus=[0-9]+')
    if [ "$got" = "$n" ]; then ok "-smp $n: the MADT reports $got processors"
    else bad "-smp $n: the MADT reports ${got:-nothing}, expected $n"; fi
    acpi=$(value "$TMPD/c$n.txt" 'acpi=[0-9]+')
    num "-smp $n: the firmware tables were really read" "$acpi" eq 1
    on=$(value "$TMPD/c$n.txt" 'smp: online=[0-9]+')
    num "-smp $n: processors online" "$on" eq "$n"
    fl=$(value "$TMPD/c$n.txt" 'failed=[0-9]+')
    num "-smp $n: processors that did not come up" "$fl" eq 0
done

echo "== 3. counter-check for the bring-up: nosmp leaves them asleep =="
run 4 "nosmp" "$TMPD/nosmp.txt"; rc=$?
[ "$rc" -eq 21 ] && ok "nosmp: the kernel shut down on its own (exit 21)" \
                 || bad "nosmp: QEMU exit code $rc, expected 21"
got=$(value "$TMPD/nosmp.txt" 'smp: cpus=[0-9]+')
num "nosmp: the MADT still reports four processors" "$got" eq 4
on=$(value "$TMPD/nosmp.txt" 'smp: online=[0-9]+')
num "nosmp: but only this many are online" "$on" eq 1

echo "== 4. THE MEASUREMENT: the same work on one core and on four =="
# The two runs are the same image, the same machine and the same work --
# `-smp 4` in both cases, so that QEMU builds exactly the same guest. The
# only difference is whether the kernel starts the other three cores.
t1=$(value "$TMPD/nosmp.txt" 'smp: bench cores=1  units=12  step=[0-9]+  cycles=[0-9]+')
t4=$(value "$TMPD/c4.txt" 'smp: bench cores=4  units=12  step=[0-9]+  cycles=[0-9]+')
m1=$(grep -oE 'smp: bench .*ms=[0-9]+' "$TMPD/nosmp.txt" | grep -oE '[0-9]+$')
m4=$(grep -oE 'smp: bench .*ms=[0-9]+' "$TMPD/c4.txt" | grep -oE '[0-9]+$')
printf '        one core:   %s cycles, %s ms\n' "${t1:-?}" "${m1:-?}"
printf '        four cores: %s cycles, %s ms\n' "${t4:-?}" "${m4:-?}"
if [ -n "$t1" ] && [ -n "$t4" ] && [ "$t4" -gt 0 ]; then
    permil=$((t1 * 1000 / t4))
    printf '        speed-up:   %s.%03d\n' "$((permil / 1000))" "$((permil % 1000))"
    # Four cores that are really four cores are far past 2.0 on an idle
    # machine. The bar is deliberately below that: this runs on build
    # machines that have other work on them, and a threshold that fails
    # under load is a threshold that gets switched off.
    num "four cores against one, in thousandths" "$permil" ge 1800
else
    bad "no benchmark numbers"
fi
# And every core has to have done its share -- a "speed-up" in which one
# core did all the work and the rest slept would show up here.
busy=$(grep -oE 'smp: percore.*' "$TMPD/c4.txt" | grep -oE 'c[0-9]+=[0-9]+' \
    | cut -d= -f2 | awk '$1 > 1000' | wc -l)
num "cores that really ground away in the four-core run" "$busy" eq 4

echo "== 5. counter-check for the measurement: one QEMU thread cannot be faster =="
# Same kernel, same four guest cores, but QEMU emulates them in ONE host
# thread. The work is interleaved instead of parallel and the total time
# cannot fall. If this run also got faster, the number above would be
# measuring something other than parallelism.
run 4 "" "$TMPD/single.txt" single; rc=$?
ts=$(value "$TMPD/single.txt" 'smp: bench cores=4  units=12  step=[0-9]+  cycles=[0-9]+')
if [ "$rc" -eq 21 ] && [ -n "$ts" ] && [ -n "$t1" ] && [ "$ts" -gt 0 ]; then
    permil=$((t1 * 1000 / ts))
    printf '        one host thread, four guest cores: %s cycles (%s.%03d against one core)\n' \
        "$ts" "$((permil / 1000))" "$((permil % 1000))"
    num "single-threaded emulation stays near 1.0, in thousandths" "$permil" lt 1500
else
    bad "the single-thread counter-check did not run (exit $rc)"
fi

echo "== 6. the lock, and the run in which it is not there =="
w=$(value "$TMPD/c4.txt" 'smp: stress want=[0-9]+')
g=$(value "$TMPD/c4.txt" 'smp: stress want=[0-9]+  got=[0-9]+')
l=$(value "$TMPD/c4.txt" 'lost=[0-9]+')
sp=$(value "$TMPD/c4.txt" 'smp: stress .*spins=[0-9]+')
num "with the lock: increments wanted" "$w" eq 6000
num "with the lock: increments counted" "$g" eq 6000
num "with the lock: increments lost" "$l" eq 0
# A lock nobody ever waited for was never contended, and then the run
# proves nothing about locking either.
num "with the lock: turns spent waiting for it" "$sp" ge 1

run 4 "nolock" "$TMPD/nolock.txt"; rc=$?
[ "$rc" -eq 21 ] && ok "nolock: the kernel still reaches the end (exit 21)" \
                 || bad "nolock: QEMU exit code $rc, expected 21"
g2=$(value "$TMPD/nolock.txt" 'smp: stress want=[0-9]+  got=[0-9]+')
l2=$(value "$TMPD/nolock.txt" 'lost=[0-9]+')
printf '        with the lock: %s of %s   without it: %s of %s\n' "$g" "$w" "${g2:-?}" "$w"
num "WITHOUT the lock: increments lost" "$l2" ge 1
if [ -n "$g2" ] && [ "$g2" -lt 6000 ]; then
    ok "WITHOUT the lock the counter comes out too small: $g2 instead of 6000"
else
    bad "WITHOUT the lock the counter came out right (${g2:-?}) -- then nothing was tested"
fi

echo "== 7. the frame allocator, and the run in which it is not locked =="
d=$(value "$TMPD/c4.txt" 'smp: frames got=[0-9]+  dups=[0-9]+')
gotf=$(value "$TMPD/c4.txt" 'smp: frames got=[0-9]+')
num "with the lock: frames handed out" "$gotf" eq 64
num "with the lock: frames handed out TWICE" "$d" eq 0
d2=$(value "$TMPD/nolock.txt" 'smp: frames got=[0-9]+  dups=[0-9]+')
num "WITHOUT the lock: the same frame in two cores' hands" "$d2" ge 1

echo "== 8. the scheduler across the cores =="
made=$(value "$TMPD/c4.txt" 'smp: sched tasks=[0-9]+')
finished=$(value "$TMPD/c4.txt" 'smp: sched tasks=[0-9]+  done=[0-9]+')
used=$(value "$TMPD/c4.txt" 'cores_used=[0-9]+')
num "kernel tasks put into the run queue" "$made" eq 8
num "kernel tasks that ran to the end" "$finished" eq 8
num "DIFFERENT cores that took work out of the queue" "$used" ge 3
picks=$(grep -oE 'smp: picks.*' "$TMPD/c4.txt" | grep -oE 'c[0-9]+=[0-9]+' \
    | cut -d= -f2 | awk '$1 > 0' | wc -l)
num "cores with at least one task of their own" "$picks" ge 3
# The single-core run of the same section: one core, and it still gets all
# eight tasks done. Round 62 has to keep working.
used1=$(value "$TMPD/nosmp.txt" 'cores_used=[0-9]+')
finished1=$(value "$TMPD/nosmp.txt" 'smp: sched tasks=[0-9]+  done=[0-9]+')
num "one core: tasks finished" "$finished1" eq 8
num "one core: cores that took work" "$used1" eq 1

echo "== 9. the rest of the kernel is unchanged on four cores =="
for f in "$TMPD/c4.txt" "$TMPD/c1.txt"; do
    tag=$(basename "$f" .txt)
    for line in "sched: done" "proc: done" "fs: done" "ring3: back in ring 0" "kernel: done"; do
        grep -qF "$line" "$f" \
            && ok "$tag: '$line'" \
            || bad "$tag: '$line' is missing"
    done
done

echo
if [ "$fail" -eq 0 ]; then
    echo "SMP: $pass passed, 0 failed"
    exit 0
fi
echo "SMP: $pass passed, $fail failed"
exit 1
