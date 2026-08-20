#!/usr/bin/env bash
# tools/kernel/run.sh -- THE PROOF THAT THE KERNEL IS AN OPERATING SYSTEM.
#
# Round 59 proved that the kernel does something: its own IDT, exceptions
# with a register dump, a timer that ticks, frames, a heap, a keyboard,
# one excursion into ring 3. Round 62 proves the five things that turn a
# kernel into a system, and every one of them gets a COUNTER-CHECK -- a
# second run of the same kernel in which the thing is switched off and the
# measurement has to collapse:
#
#   1. TASKS. Three workers with the priorities 1, 2 and 3 run interleaved
#      on one processor. Counter-check `nopreempt`: without the timer
#      switch each of them is scheduled EXACTLY ONCE and they run one
#      after the other.
#   2. ADDRESS SPACES. Two processes write to the SAME address and read
#      back what each of them wrote. Counter-check: a process that touches
#      the kernel or an unmapped page of its own gets a #PF, is killed --
#      and the kernel lives on.
#   3. SYSTEM CALLS. write/read/exit/getpid/sleep/spawn/wait, and every
#      wrong argument is answered with a NEGATIVE error code. A program in
#      ring 3 hands the kernel an address out of the kernel and prints the
#      -14 it gets back.
#   4. FILE SYSTEM. Superblock, inodes, directories, block bitmap on a RAM
#      disk. Counter-check: `mount` on an unformatted disk has to fail.
#      And once more on a REAL disk over ATA PIO -- afterwards the image on
#      the host contains what the kernel wrote.
#   5. A SHELL in ring 3: ls, cat, echo, write, rm, mkdir. It can do
#      nothing itself; every command is a system call.
#
# Everything is measured the way round 59 measured: QEMU per case, with a
# time limit, serial output against expectations, exit code out of
# `isa-debug-exit` (21 = the kernel ended it, 63 = it stopped at an
# exception).
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
SOURCE=demos/kernel/kmain.fi
USOURCE=demos/kernel/uprog.fi
LDSCRIPT=demos/kernel/kernel.ld
TMPD=$(mktemp -d)
trap 'rm -rf "$TMPD"' EXIT

pass=0
fail=0
ok()  { pass=$((pass+1)); printf '  OK    %s\n' "$1"; }
bad() { fail=$((fail+1)); printf '  FAIL  %s\n' "$1"; }

# `ok` when the two numbers stand in the asked-for relation.
num() { # name value op expected
    local name=$1 value=$2 op=$3 want=$4
    if [ -z "$value" ]; then bad "$name: no number found (expected $op $want)"; return; fi
    if [ "$value" -"$op" "$want" ] 2>/dev/null; then ok "$name: $value"
    else bad "$name: $value, expected $op $want"; fi
}

has() { # file text name
    grep -qF "$2" "$1" && ok "$3" || { bad "$3 -- '$2' is missing"; }
}

hasnot() { # file text name
    grep -qF "$2" "$1" && bad "$3 -- '$2' should not be there" || ok "$3"
}

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

# One run: $1 = image, $2 = command line, $3 = output file, $4 = extra
# arguments for QEMU (optional). Returns the exit code of QEMU
# (21 = the kernel shut down cleanly, 63 = it stopped at an exception,
# 124 = the time limit struck).
run_kernel() {
    local image=$1 append=$2 out=$3
    shift 3
    timeout 90 qemu-system-x86_64 -kernel "$image" -m 128 -append "$append" \
        -serial "file:$out" -display none -no-reboot "$@" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

# Addresses differ between the compilers (different code, different
# lengths); everything else must not.
normalise() {
    sed -E 's/0x[0-9a-f]+/0xX/g; s/k[01]\.mb/kernel.mb/g; s/[0-9]+/N/g' "$1"
}

# The line of the worker with priority $2 out of the task dump, field $3.
worker() { # file prio field
    grep -E "^task: .*kind=2 .*prio=$2 " "$1" | head -1 \
        | grep -oE "$3=[0-9]+" | head -1 | cut -d= -f2
}

value() { # file pattern
    grep -oE "$2" "$1" | head -1 | grep -oE '[0-9]+$'
}

echo "== 1. build the kernel and the user programs with both compilers =="
as --64 -o "$TMPD/boot.o" demos/kernel/boot.s 2>"$TMPD/as1.err" \
    && ok "boot.s assembles (multiboot, long mode, GDT, TSS)" \
    || { bad "boot.s"; sed 's/^/        /' "$TMPD/as1.err" | head -5; }
as --64 -o "$TMPD/isr.o" demos/kernel/isr.s 2>"$TMPD/as2.err" \
    && ok "isr.s assembles (48 vectors, syscall, ring 3)" \
    || { bad "isr.s"; sed 's/^/        /' "$TMPD/as2.err" | head -5; }
as --64 -o "$TMPD/switch.o" demos/kernel/switch.s 2>"$TMPD/as3.err" \
    && ok "switch.s assembles (context switch, into ring 3)" \
    || { bad "switch.s"; sed 's/^/        /' "$TMPD/as3.err" | head -5; }

"$FIRNC" -o "$TMPD/k0.o" "$SOURCE" 2>"$TMPD/e0" \
    && ok "firnc0: $SOURCE -> k0.o" \
    || { bad "firnc0 does not compile the kernel"; sed 's/^/        /' "$TMPD/e0" | head -10; }
"$FIRNC" -o "$TMPD/uprog0.o" "$USOURCE" 2>"$TMPD/eu0" \
    && ok "firnc0: $USOURCE -> uprog0.o (the programs of ring 3)" \
    || { bad "firnc0 does not compile the user programs"; sed 's/^/        /' "$TMPD/eu0" | head -10; }
"$FC1" "$SOURCE" -o "$TMPD/k1.o" >"$TMPD/e1" 2>&1 \
    && ok "firnc1: $SOURCE -> k1.o" \
    || { bad "firnc1 does not compile the kernel"; sed 's/^/        /' "$TMPD/e1" | head -10; }
"$FC1" "$USOURCE" -o "$TMPD/uprog1.o" >"$TMPD/eu1" 2>&1 \
    && ok "firnc1: $USOURCE -> uprog1.o" \
    || { bad "firnc1 does not compile the user programs"; sed 's/^/        /' "$TMPD/eu1" | head -10; }

for s in 0 1; do
    [ -f "$TMPD/k$s.o" ] || continue
    [ -f "$TMPD/uprog$s.o" ] || continue
    if ld -n -T "$LDSCRIPT" \
          --defsym=KERNEL_MAIN="_F$s.kernel_main" \
          --defsym=KERNEL_TRAP="_F$s.trap__entry" \
          --defsym=KERNEL_SYSCALL="_F$s.sys__entry" \
          --defsym=KERNEL_TASK_MAIN="_F$s.tasks__main" \
          --defsym=KERNEL_USER_START="_F$s.proc__user_start" \
          --defsym=USER_MAIN="_F$s.u_enter" \
          -o "$TMPD/k$s.elf" "$TMPD/boot.o" "$TMPD/isr.o" "$TMPD/switch.o" \
          "$TMPD/k$s.o" "$TMPD/uprog$s.o" 2>"$TMPD/ld$s.err"; then
        objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
        ok "firnc$s: linked and turned into a multiboot image"
    else
        bad "firnc$s: ld failed"
        grep -v 'GNU-stack\|RWX\|deprecated' "$TMPD/ld$s.err" | sed 's/^/        /' | head -5
    fi
done

echo "== 2. freestanding: what the object files say about themselves =="
for f in k0 k1 uprog0 uprog1; do
    o="$TMPD/$f.o"
    [ -f "$o" ] || continue
    kind=$(readelf -h "$o" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "REL" ] && ok "$f.o: ELF type REL (object file, not an executable)" \
                        || bad "$f.o: ELF kind '$kind', expected REL"
    undef=$(nm -u "$o" 2>/dev/null | sed '/^$/d')
    [ -z "$undef" ] && ok "$f.o: no undefined symbol (no libc, no runtime)" \
                    || { bad "$f.o: undefined symbols"; echo "$undef" | sed 's/^/        /'; }
    foreign=$(nm --defined-only "$o" | awk '{print $3}' | grep -vE "^_F[01]\." || true)
    [ -z "$foreign" ] && ok "$f.o: every defined symbol is its own" \
                      || { bad "$f.o: foreign symbols"; echo "$foreign" | sed 's/^/        /'; }
    if nm "$o" | grep -qiE '(malloc|printf|memcpy|__gc_|gc_alloc|mmap)'; then
        bad "$f.o: a libc or collector name is in the symbol table"
    else
        ok "$f.o: no libc name, no collector in the symbol table"
    fi
done
# The kernel's own code contains no `syscall` (round 52) -- the user
# programs consist of little else, and that is exactly the difference.
for s in 0 1; do
    [ -f "$TMPD/k$s.o" ] || continue
    n=$(objdump -d "$TMPD/k$s.o" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    [ "$n" -eq 0 ] && ok "k$s.o: no syscall in the kernel's own code" \
                   || bad "k$s.o: $n syscall instructions in the kernel"
    # How MANY there are differs between the compilers and says nothing
    # about the language: firnc0 inlines `u_sys` at every call site,
    # firnc1 leaves the call standing. What has to hold is that the user
    # program has the instruction at all and the kernel has none.
    n=$(objdump -d "$TMPD/uprog$s.o" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    num "uprog$s.o: system calls in the user program" "$n" ge 1
done
# And in the image every single one of them lies on the pages of ring 3.
for s in 0 1; do
    [ -f "$TMPD/k$s.elf" ] || continue
    tot=$(objdump -d "$TMPD/k$s.elf" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    utx=$(objdump -d -j .utext "$TMPD/k$s.elf" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    ktx=$(objdump -d -j .text "$TMPD/k$s.elf" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    uob=$(objdump -d "$TMPD/uprog$s.o" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    # Two of them come out of isr.s: the user program of round 59.
    if [ "$ktx" -eq 0 ] && [ "$tot" -eq "$utx" ] && [ "$utx" -eq $((uob + 2)) ]; then
        ok "firnc$s: all $tot syscall instructions lie in .utext, none in the kernel text"
    else
        bad "firnc$s: $tot syscalls, $utx of them in .utext, $ktx in .text, $uob in uprog$s.o"
    fi
done
# The user pages are a closed range of whole pages -- the kernel opens
# exactly them and nothing else.
if [ -f "$TMPD/k0.elf" ]; then
    ub=$(nm "$TMPD/k0.elf" | grep ' __user_begin' | awk '{print $1}')
    ue=$(nm "$TMPD/k0.elf" | grep ' __user_end' | awk '{print $1}')
    ubd=$((16#$ub)); ued=$((16#$ue))
    pages=$(( (ued - ubd) / 4096 ))
    if [ $((ubd % 4096)) -eq 0 ] && [ $((ued % 4096)) -eq 0 ] && [ "$pages" -ge 6 ]; then
        ok "image: the user region is page aligned and $pages pages long"
    else
        bad "image: user region 0x$ub..0x$ue ($pages pages)"
    fi
fi

echo "== 3. the full run (both compilers) =="
expected=(
    "firn kernel r62, profile kernel"
    "idt: 48 gates, vector 8 IST"
    "pic: master 0x20, slave 0x28"
    "pit: 100 Hz, divisor 11931"
    "frame test: ok"
    "heap test: ok"
    "sched: done"
    "proc: done"
    "fs: done"
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
    ticks=$(grep -oE '^ticks: [0-9]+' "$TMPD/full$s.txt" | head -1 | awk '{print $2}')
    num "firnc$s: timer interrupts before the tasks" "$ticks" ge 20
    spin=$(grep -oE '^spin: ticks=\+[0-9]+' "$TMPD/full$s.txt" | head -1 | grep -oE '[0-9]+$')
    num "firnc$s: ticks during the spin loop" "$spin" ge 1
    frames=$(grep -oE '^frames: [0-9]+ covered, [0-9]+ free' "$TMPD/full$s.txt" | head -1)
    [ -n "$frames" ] && ok "firnc$s: memory map read ($frames)" \
                     || bad "firnc$s: no frame line"
    if grep -q '^heap test: ok' "$TMPD/full$s.txt" && grep -q '1M refused=1' "$TMPD/full$s.txt"; then
        ok "firnc$s: the heap refuses a request bigger than itself"
    else
        bad "firnc$s: the counter-check of the heap is missing"
    fi
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
    # The two differ in one respect that says nothing about the language:
    # how many turns the scheduler needed. Code of a different length runs
    # at a different speed, and the workers are done after a different
    # number of ticks. The trace and the counters are therefore compared
    # by their SHAPE, not word by word.
    normalise "$TMPD/full0.txt" | grep -vE '^(trace|task|sched|proc|fs|sh|ata):' > "$TMPD/n0.txt"
    normalise "$TMPD/full1.txt" | grep -vE '^(trace|task|sched|proc|fs|sh|ata):' > "$TMPD/n1.txt"
    if diff -q "$TMPD/n0.txt" "$TMPD/n1.txt" >/dev/null; then
        ok "the serial output of both compilers is equal apart from addresses"
    else
        bad "the two kernels behave differently"
        diff "$TMPD/n0.txt" "$TMPD/n1.txt" | head -10 | sed 's/^/        /'
    fi
    for s in 0 1; do
        w1=$(worker "$TMPD/full$s.txt" 1 exit)
        w2=$(worker "$TMPD/full$s.txt" 2 exit)
        w3=$(worker "$TMPD/full$s.txt" 3 exit)
        if [ "$w1" = "40" ] && [ "$w2" = "40" ] && [ "$w3" = "40" ]; then
            ok "firnc$s: all three workers did their full work (exit 40)"
        else
            bad "firnc$s: exit codes of the workers: $w1 $w2 $w3"
        fi
        sep=$(value "$TMPD/full$s.txt" 'separated=[0-9]+')
        num "firnc$s: the two processes are separated" "$sep" eq 1
        same=$(grep -c 'same=1' "$TMPD/full$s.txt")
        num "firnc$s: file contents read back unchanged" "$same" ge 2
    done
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
    ( timeout 90 qemu-system-x86_64 -kernel "$TMPD/k0.mb" -m 128 \
        -append "noring3 nosched noproc nofs" \
        -serial "file:$TMPD/kbd.txt" -display none -no-reboot \
        -monitor "unix:$TMPD/mon.sock,server,nowait" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
      echo $? > "$TMPD/kbd.rc" ) &
    qemu_pid=$!
    for _ in $(seq 1 150); do
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
    run_kernel "$TMPD/k0.mb" "noring3 nosched noproc nofs" "$TMPD/nokey.txt"
    grep -q "^kbd: (none)$" "$TMPD/nokey.txt" \
        && ok "counter-check: without keys the kernel reports '(none)'" \
        || { bad "counter-check keyboard"; grep '^kbd' "$TMPD/nokey.txt" | sed 's/^/        /'; }
fi

echo "== 8. counter-check for the timer: masked IRQ0 counts nothing =="
if [ -f "$TMPD/k0.mb" ]; then
    run_kernel "$TMPD/k0.mb" "notimer nokbd noring3 nosched noproc nofs" "$TMPD/notimer.txt"; rc=$?
    [ "$rc" -eq 21 ] && ok "masked run: the kernel gets to the end (exit 21)" \
                     || bad "masked run: exit code $rc, expected 21"
    grep -q "^spin: ticks=+0$" "$TMPD/notimer.txt" \
        && ok "counter-check: with IRQ0 masked the spin loop counts 0 ticks" \
        || { bad "counter-check timer"; grep '^spin' "$TMPD/notimer.txt" | sed 's/^/        /'; }
fi

echo "== 9. counter-check for ring 3: hlt is forbidden down there =="
if [ -f "$TMPD/k0.mb" ]; then
    run_kernel "$TMPD/k0.mb" "ring3fault nokbd nosched noproc nofs" "$TMPD/ring3.txt"; rc=$?
    [ "$rc" -eq 63 ] && ok "ring 3 counter-check: the kernel stops at the exception (exit 63)" \
                     || bad "ring 3 counter-check: exit code $rc, expected 63"
    grep -qE '^\*\*\* EXCEPTION 13 #GP' "$TMPD/ring3.txt" \
        && ok "ring 3 counter-check: 'hlt' in the user program yields #GP" \
        || { bad "ring 3 counter-check: no #GP"; tail -4 "$TMPD/ring3.txt" | sed 's/^/        /'; }
    grep -qE '^  rip=0x[0-9a-f]+  cs=0x2b' "$TMPD/ring3.txt" \
        && ok "ring 3 counter-check: cs=0x2b -- the processor really was in ring 3" \
        || { bad "ring 3 counter-check: the fault did not come out of ring 3"; grep '  rip=' "$TMPD/ring3.txt" | sed 's/^/        /'; }
fi

# =====================================================================
# ROUND 62
# =====================================================================

F="$TMPD/full0.txt"

echo "== 10. point 1: three tasks on one processor =="
if [ -f "$F" ]; then
    has "$F" "sched: preempt=1" "the run is the preemptive one"
    sw=$(value "$F" 'switches=[0-9]+')
    num "context switches" "$sw" ge 20
    alt=$(value "$F" 'alternations=[0-9]+')
    num "changes from one worker to a different one" "$alt" ge 10
    tr=$(grep -m1 '^trace: ' "$F")
    lo=$(grep -m1 '^sched: workers ' "$F" | awk '{print $3}')
    mid=$(grep -m1 '^sched: workers ' "$F" | awk '{print $4}')
    hi=$(grep -m1 '^sched: workers ' "$F" | awk '{print $5}')
    if [ -n "$lo" ] && echo "$tr" | grep -q " $lo " && echo "$tr" | grep -q " $mid " \
       && echo "$tr" | grep -q " $hi "; then
        ok "the trace holds all three worker pids ($lo $mid $hi)"
    else
        bad "the trace is missing a pid: $tr"
    fi
    # The three of them do the same work; what differs is the priority.
    for p in 1 2 3; do
        r=$(worker "$F" $p runs)
        num "worker with priority $p was scheduled" "$r" ge 5
    done
    for p in 1 2 3; do
        w=$(worker "$F" $p work)
        num "worker with priority $p did its full work" "$w" eq 40
    done
    # THE PRIORITY, MEASURED: the length of a turn is the priority in
    # ticks. ticks/runs therefore has to come out at about the priority.
    for p in 1 2 3; do
        t=$(worker "$F" $p ticks); r=$(worker "$F" $p runs)
        if [ -n "$t" ] && [ -n "$r" ] && [ "$r" -gt 0 ]; then
            slice=$(( t * 100 / r ))
            loB=$(( p * 100 - 60 )); hiB=$(( p * 100 + 60 ))
            if [ "$slice" -ge "$loB" ] && [ "$slice" -le "$hiB" ]; then
                ok "priority $p: a turn lasts $slice/100 ticks"
            else
                bad "priority $p: a turn lasts $slice/100 ticks, expected around ${p}00"
            fi
        else
            bad "priority $p: no ticks/runs found"
        fi
    done
    e1=$(worker "$F" 1 end); e2=$(worker "$F" 2 end); e3=$(worker "$F" 3 end)
    if [ -n "$e1" ] && [ "$e3" -lt "$e2" ] && [ "$e2" -lt "$e1" ]; then
        ok "the higher the priority the earlier it is done ($e3 < $e2 < $e1)"
    else
        bad "the finishing ticks are not ordered: prio3=$e3 prio2=$e2 prio1=$e1"
    fi
    free_line=$(grep -m1 '^sched: frames_free=' "$F")
    now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
    was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
    if [ -n "$now" ] && [ "$now" = "$was" ]; then
        ok "the three kernel stacks came back ($now frames free, as before)"
    else
        bad "frames after the tasks: $now, before: $was"
    fi
    idle=$(grep -E '^task: .*kind=1 ' "$F" | grep -oE 'runs=[0-9]+' | cut -d= -f2)
    num "the idle task exists and hardly ran" "$idle" le 3
fi

echo "== 11. counter-check for point 1: without preemption nothing interleaves =="
if [ -f "$TMPD/k0.mb" ]; then
    run_kernel "$TMPD/k0.mb" "nopreempt nokbd noproc nofs noring3" "$TMPD/nopre.txt"; rc=$?
    N="$TMPD/nopre.txt"
    [ "$rc" -eq 21 ] && ok "without preemption the kernel gets to the end (exit 21)" \
                     || bad "nopreempt: exit code $rc, expected 21"
    has "$N" "sched: preempt=0" "the timer does not switch tasks in this run"
    for p in 1 2 3; do
        r=$(worker "$N" $p runs)
        num "counter-check: worker with priority $p was scheduled exactly once" "$r" eq 1
    done
    alt=$(value "$N" 'alternations=[0-9]+')
    num "counter-check: changes between workers" "$alt" le 2
    for p in 1 2 3; do
        w=$(worker "$N" $p work)
        num "counter-check: worker $p did its work anyway" "$w" eq 40
    done
fi

echo "== 12. point 2: every process has its own memory =="
if [ -f "$F" ]; then
    pages=$(value "$F" 'proc: pages=[0-9]+')
    if [ -n "$pages" ] && [ -n "${pages:-}" ] && [ "$pages" = "$(( (ued - ubd) / 4096 ))" ]; then
        ok "the kernel opened exactly the $pages pages of the user region"
    else
        bad "opened pages: $pages, the region has $(( (ued - ubd) / 4096 ))"
    fi
    hpid=$(value "$F" 'proc: hello pid=[0-9]+')
    hexit=$(value "$F" 'proc: hello exit=[0-9]+')
    if [ -n "$hpid" ] && [ "$hpid" = "$hexit" ]; then
        ok "the process left with its own pid as the exit code ($hpid)"
    else
        bad "hello: pid=$hpid, exit code=$hexit"
    fi
    has "$F" "user: hello #7" "the argument arrived in ring 3"
    da=$(grep -m1 '^proc: data ' "$F" | awk '{print $3}')
    db=$(grep -m1 '^proc: data ' "$F" | awk '{print $5}')
    if [ -n "$da" ] && [ "$da" != "$db" ]; then
        ok "the same address lies on different frames ($da and $db)"
    else
        bad "both processes use the frame $da"
    fi
    has "$F" "user: wrote here 111" "process A wrote its value"
    has "$F" "user: wrote here 222" "process B wrote its value"
    has "$F" "user: read here 111" "process A read back ITS value"
    has "$F" "user: read here 222" "process B read back ITS value"
    sep=$(value "$F" 'separated=[0-9]+')
    num "the kernel confirms the separation" "$sep" eq 1
    # The fork-like half of spawn: inheritance AND separation in one run.
    has "$F" "user: parent wrote 12345" "the parent wrote a mark into its data page"
    has "$F" "user: child inherited 12345" "the child inherited the CONTENTS of that page"
    has "$F" "user: fork child exit=5" "the child confirmed the inherited value"
    has "$F" "user: parent still has 12345" "the child's overwrite did not reach the parent"
    kept=$(value "$F" 'proc: fork parent kept [0-9]+')
    num "the kernel reads the parent's page unchanged as well" "$kept" eq 12345
fi

echo "== 13. counter-check for point 2: what is not mine gives a #PF =="
if [ -f "$F" ]; then
    grep -qE '^user fault: pid=[0-9]+  vector=14  err=0x[0-9a-f]+  cr2=0x100000' "$F" \
        && ok "a process that touches kernel memory gets a #PF at cr2=0x100000" \
        || { bad "no #PF on kernel memory"; grep '^user fault' "$F" | sed 's/^/        /'; }
    grep -qE '^user fault: pid=[0-9]+  vector=14  err=0x[0-9a-f]+  cr2=0x40010000' "$F" \
        && ok "a page of its own that is not mapped gives a #PF as well" \
        || { bad "no #PF on 0x40010000"; grep '^user fault' "$F" | sed 's/^/        /'; }
    killed=$(value "$F" 'proc: killed=[0-9]+')
    num "processes killed by a fault" "$killed" eq 2
    n=$(grep -c 'process killed' "$F")
    num "reports of a killed process" "$n" eq 2
    # 128 + vector, the way a system with signals counts it.
    n=$(grep -m1 '^proc: killed=' "$F" | grep -oc 'exit=142')
    grep -m1 '^proc: killed=' "$F" | grep -q 'exit=142  exit=142' \
        && ok "both faults left the exit code 142 (128 + vector 14)" \
        || bad "the exit codes of the killed processes are wrong"
    hasnot "$F" "user: READ THE KERNEL" "the process did NOT get to see kernel memory"
    has "$F" "kernel: done" "the kernel lived on after two dead processes"
    alive=$(value "$F" 'proc: alive=[0-9]+')
    num "tasks still alive at the end (boot task and idle)" "$alive" eq 2
    free_line=$(grep -m1 '^proc: frames_free=' "$F")
    now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
    was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
    if [ -n "$now" ] && [ "$now" = "$was" ]; then
        ok "every address space came back ($now frames free, as before)"
    else
        bad "frames after the processes: $now, before: $was"
    fi
fi

echo "== 14. point 3: system calls with a real error behaviour =="
if [ -f "$F" ]; then
    has "$F" "user: pid=" "getpid answers in ring 3"
    has "$F" "user: write(kernel)=-14" "write with a kernel pointer gives -EFAULT"
    has "$F" "user: write(nil)=-14" "write with a null pointer gives -EFAULT"
    has "$F" "user: read(badfd)=-9" "read on descriptor 77 gives -EBADF"
    has "$F" "user: nosys=-38" "an unknown call number gives -ENOSYS"
    has "$F" "user: sleep(1000s)=-22" "an absurd sleep gives -EINVAL"
    has "$F" "user: wait(bad)=-10" "wait for a foreign pid gives -ECHILD"
    has "$F" "user: child exit=41" "wait delivered the exit code of the first child"
    has "$F" "user: child exit=42" "wait delivered the exit code of the second child"
    has "$F" "user: child arg=1" "spawn passed the argument on (child 1)"
    has "$F" "user: child arg=2" "spawn passed the argument on (child 2)"
    sp1=$(grep -m1 'user: spawned pid' "$F" | grep -oE '[0-9]+$')
    sp2=$(grep -m2 'user: spawned pid' "$F" | tail -1 | grep -oE '[0-9]+$')
    if [ -n "$sp1" ] && [ -n "$sp2" ] && [ "$sp1" != "$sp2" ]; then
        ok "spawn out of ring 3 gave two different pids ($sp1, $sp2)"
    else
        bad "spawn: pids $sp1 and $sp2"
    fi
fi

echo "== 15. point 4: the file system on the RAM disk =="
if [ -f "$F" ]; then
    has "$F" "fs: mount unformatted=0" "counter-check: mount refuses a disk without a magic number"
    grep -q "^fs: format 1=1" "$F" \
        && ok "format and mount succeed afterwards" \
        || { bad "format/mount"; grep '^fs: format' "$F" | sed 's/^/        /'; }
    free0=$(grep -m1 '^fs: format ' "$F" | grep -oE 'free=[0-9]+' | cut -d= -f2)
    num "free blocks right after the format" "$free0" ge 2000
    ino=$(grep -m1 '^fs: format ' "$F" | grep -oE 'inodes=[0-9]+' | cut -d= -f2)
    num "inodes in use after the format (the root directory)" "$ino" eq 1
    has "$F" "fs: small wrote=14  read=14  same=1" "a small file: written, read back, identical"
    has "$F" "fs: big wrote=1500  read=1500  same=1" "1500 octets over four blocks: identical"
    free1=$(grep -m1 '^fs: big ' "$F" | grep -oE 'free=[0-9]+' | cut -d= -f2)
    if [ -n "$free0" ] && [ -n "$free1" ] && [ "$free1" -lt "$free0" ]; then
        ok "the blocks of the files are taken ($free1 free instead of $free0)"
    else
        bad "block accounting: after the format $free0, after the files $free1"
    fi
    # 12 direct blocks + 64 through the indirect one = 76 * 512 = 38912.
    lim=$(grep -m1 '^fs: limit ' "$F")
    la=$(echo "$lim" | grep -oE 'asked=[0-9]+' | cut -d= -f2)
    lw=$(echo "$lim" | grep -oE 'wrote=[0-9]+' | cut -d= -f2)
    ls_=$(echo "$lim" | grep -oE 'size=[0-9]+' | cut -d= -f2)
    if [ "$la" = "49152" ] && [ "$lw" = "38912" ] && [ "$ls_" = "38912" ]; then
        ok "counter-check: a file stops at 12+64 blocks and reports 38912 of 49152 written"
    else
        bad "the size limit: asked=$la wrote=$lw size=$ls_, expected 49152/38912/38912"
    fi
    grep -q '^fs: list .*hello.txt:1' "$F" \
        && ok "the root directory lists the file" \
        || { bad "hello.txt is not in the listing"; grep '^fs: list' "$F" | sed 's/^/        /'; }
    grep -q '^fs: list .*docs:2' "$F" \
        && ok "the root directory lists the subdirectory as type 2" \
        || bad "docs is not in the listing"
    grep -qE '^fs: subdir \.:2 \.\.:2 big.bin:1' "$F" \
        && ok "the subdirectory has . and .. and the file in it" \
        || { bad "the subdirectory is wrong"; grep '^fs: subdir' "$F" | sed 's/^/        /'; }
    grep -q '^fs: unlink 1  gone=1' "$F" \
        && ok "delete: the name is gone" \
        || bad "unlink failed"
    ul=$(grep -m1 '^fs: unlink ' "$F")
    after=$(echo "$ul" | grep -oE 'after=[0-9]+' | cut -d= -f2)
    before=$(echo "$ul" | grep -oE 'of [0-9]+' | awk '{print $2}')
    if [ -n "$after" ] && [ "$after" -gt "$before" ]; then
        ok "delete: the blocks came back ($before -> $after free)"
    else
        bad "after the delete $after blocks are free, before it was $before"
    fi
fi

echo "== 16. point 4 on a REAL disk: ATA PIO =="
if [ -f "$TMPD/k0.mb" ]; then
    dd if=/dev/zero of="$TMPD/disk.img" bs=1M count=2 2>/dev/null
    run_kernel "$TMPD/k0.mb" "ata nokbd nosched noproc nofs noring3" "$TMPD/ata.txt" \
        -drive "file=$TMPD/disk.img,format=raw,if=ide,index=0"
    rc=$?
    A="$TMPD/ata.txt"
    [ "$rc" -eq 21 ] && ok "ATA run: the kernel gets to the end (exit 21)" \
                     || bad "ATA run: exit code $rc, expected 21"
    has "$A" "ata: drive present" "the drive was found over the status port"
    has "$A" "ata: format 1  mount=1" "the same file system formats and mounts on the disk"
    has "$A" "ata: wrote=29  read=29  same=1" "written over PIO, read back over PIO, identical"
    grep -q '^ata: list .*ata.txt:1' "$A" \
        && ok "the directory of the real disk lists the file" \
        || { bad "ata.txt is missing in the listing"; grep '^ata: list' "$A" | sed 's/^/        /'; }
    # THE PROOF THAT IT REALLY WENT TO THE DISK: the image on the host.
    if grep -qa "ata disk holds this line r62" "$TMPD/disk.img"; then
        ok "the image on the host contains the text the kernel wrote"
    else
        bad "the image on the host does not contain the text"
    fi
    if grep -qa "26SFNRIF" "$TMPD/disk.img"; then
        ok "the image on the host starts with the magic number of the file system"
    else
        bad "no magic number in the image"
    fi
    if grep -qa "ata.txt" "$TMPD/disk.img"; then
        ok "the name of the file stands in a directory block of the image"
    else
        bad "the name of the file is not in the image"
    fi
    # Counter-check: without a drive the kernel says so and goes on.
    run_kernel "$TMPD/k0.mb" "ata nokbd nosched noproc nofs" "$TMPD/noata.txt"
    if grep -q "ata: no drive" "$TMPD/noata.txt" || grep -q "ata: drive present" "$TMPD/noata.txt"; then
        grep -q "kernel: done" "$TMPD/noata.txt" \
            && ok "counter-check: without a disk image the kernel gets to the end anyway" \
            || bad "counter-check ATA: the kernel did not get to the end"
    else
        bad "counter-check ATA: no statement about the drive"
    fi
fi

echo "== 17. point 5: a command line in ring 3 =="
if [ -f "$TMPD/k0.mb" ]; then
    script='ls;cat /hello.txt;echo hallo welt;write /neu.txt zeile aus der shell;cat /neu.txt;ls;rm /neu.txt;ls;quatsch;exit'
    run_kernel "$TMPD/k0.mb" "nokbd nosched noproc noring3 script=$script" "$TMPD/sh.txt"
    rc=$?
    S="$TMPD/sh.txt"
    [ "$rc" -eq 21 ] && ok "shell run: the kernel gets to the end (exit 21)" \
                     || bad "shell run: exit code $rc, expected 21"
    has "$S" "sh: ready, type away" "the shell started in ring 3"
    grep -q '^\./ \.\./ hello.txt docs/' "$S" \
        && ok "'ls' lists the root directory, directories with a slash" \
        || { bad "'ls' says something else"; grep -A1 'sh\$ ls' "$S" | head -4 | sed 's/^/        /'; }
    has "$S" "firn round 62" "'cat /hello.txt' printed the contents of the file"
    has "$S" "hallo welt" "'echo hallo welt' arrived"
    has "$S" "sh: written 19" "'write' wrote 19 octets into a new file"
    has "$S" "zeile aus der shell" "'cat' read back what 'write' had written"
    grep -q '^\./ \.\./ hello.txt docs/ neu.txt' "$S" \
        && ok "after 'write' the new file is in the listing" \
        || bad "the new file does not show up in 'ls'"
    has "$S" "sh: rm 0" "'rm' deleted the file"
    n=$(grep -c '^\./ \.\./ hello.txt docs/ $' "$S")
    num "listings without the deleted file" "$n" ge 2
    has "$S" "sh: what is quatsch" "an unknown command is named"
    has "$S" "sh: bye" "'exit' ends the shell"
    cmds=$(value "$S" 'sh: commands=[0-9]+')
    num "commands carried out (the exit code of the shell)" "$cmds" eq 10
    # Counter-check: without a script there is nothing to read, and the
    # kernel says so instead of hanging on an empty console.
    run_kernel "$TMPD/k0.mb" "nokbd nosched noproc nofs noring3" "$TMPD/noscript.txt"
    has "$TMPD/noscript.txt" "sh: no script" "counter-check: without a script no shell starts"
fi

echo "== 18. the kernel survives what a system has to survive =="
if [ -f "$TMPD/k0.mb" ]; then
    # Everything at once, and afterwards the kernel is still the one that
    # ends the machine itself: tasks, processes, two faults, file system,
    # shell, and the excursion of round 59 on top.
    script='ls;cat /hello.txt;exit'
    run_kernel "$TMPD/k0.mb" "nokbd script=$script" "$TMPD/all.txt"; rc=$?
    A="$TMPD/all.txt"
    [ "$rc" -eq 21 ] && ok "the full run with everything switched on ends cleanly (exit 21)" \
                     || bad "full run: exit code $rc, expected 21"
    for line in "sched: done" "proc: done" "fs: done" "sh: bye" "ring3: back in ring 0" "kernel: done"; do
        has "$A" "$line" "full run: '$line'"
    done
    calls=$(grep -c '^user: ' "$A")
    num "lines that a program in ring 3 wrote over write()" "$calls" ge 15
    # Exactly the two faults the test provokes -- no more. A process that
    # is started before its address space is complete would show up here
    # (that happened during this round, see docs/ROUND62.md).
    n=$(grep -c 'user fault' "$A")
    num "faults in ring 3 during the full run" "$n" eq 2
    cmds=$(value "$A" 'sh: commands=[0-9]+')
    num "the shell in the full run carried out its commands" "$cmds" eq 3
    traps=$(grep -oE '^ticks: [0-9]+  traps=[0-9]+' "$A" | grep -oE 'traps=[0-9]+' | cut -d= -f2)
    num "interrupts before the tasks" "$traps" ge 20
fi

echo
echo "KERNEL: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
exit 0
