#!/usr/bin/env bash
# tools/posix/run.sh -- THE PROOF THAT OSUM HAS A POSIX FLOOR.
#
# Round 62 gave the kernel seventeen system calls of its own invention,
# round K1 added `exec`, and that was the ceiling of the system: every
# program written for Unix asks for `open`, `lseek`, `fstat`,
# `getdents64`, `brk`, `mmap`, `fork`, `execve`, `wait4`, `pipe`, `dup2`,
# and not one of them was there.
#
# Round K4 lays the floor. What is measured here, and every point has a
# COUNTER-CHECK, because a property without one is a claim:
#
#   1. THE NUMBERS ARE LINUX'S NUMBERS. Read out of `demos/kernel/sys.fi`
#      and compared against the x86-64 table of Linux. That is not
#      decoration: stage 2 of this plan is to run a statically linked
#      Linux binary, and such a binary puts its number in rax without
#      asking anybody. Counter-check: the two error lists (kernel and
#      libc) are compared name by name -- two copies of a list are two
#      lists, and the day they disagree an error means two things.
#   2. EVERY CALL WORKS. `/bin/posix` makes each of them and prints the
#      answer as `posix: <name> = <number>`; every number below is checked
#      against what POSIX says it has to be.
#   3. EVERY CALL FAILS PROPERLY. A file that is not there, a descriptor
#      that is not open, a pointer into the kernel, a null pointer, a
#      buffer too short for one directory entry, a directory read as a
#      file, a file opened as a directory, a seek before the start, an
#      unknown call number -- fourteen ways to be wrong, fourteen negative
#      numbers, and a kernel that is still alive at the end of them.
#   4. `fork` REALLY SPLITS A PROCESS. The child writes into a pipe what it
#      sees of its parent's memory, and leaves with a code of its own.
#   5. `fork` + `dup2` + `execve` IS A REDIRECTION. `/bin/sh` understands
#      `ls > /out.txt` since this round, and that one line is what `fork`
#      is for: between the split and the image the child is still the
#      shell, and what it changes about itself is what the program finds.
#      Counter-check: without the redirection the same command writes to
#      the console and no file is made.
#   6. NOTHING LEAKS. Frames free before the run = frames free after it,
#      after a dozen processes, a pipe and a heap.
#   7. BOTH COMPILERS MEASURE THE SAME NUMBERS.
#
# Measured the way rounds 59, 62 and K1 measured: QEMU per case, with a
# time limit, serial output against expectations, exit code out of
# `isa-debug-exit` (21 = the kernel ended it, 63 = an exception).
#
# Usage:  bash tools/posix/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
LDSCRIPT=demos/kernel/kernel.ld
ULD=demos/kernel/user/user.ld
PROGS="sh ls cat echo rm hello posix"
BLOCKS=2048

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

has() { grep -aqF "$2" "$1" && ok "$3" || bad "$3 -- '$2' is missing"; }
hasnot() { grep -aqF "$2" "$1" && bad "$3 -- '$2' should not be there" || ok "$3"; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "POSIX: skipped, qemu-system-x86_64 is not available"
    exit 0
fi

# The same freshness trap as in tools/kernel/run.sh: an outdated `.firnc1`
# measures yesterday's compiler.
fresh=0
[ -x "$FC1" ] || fresh=1
if [ -x "$FC1" ]; then
    [ "$FIRNC" -nt "$FC1" ] && fresh=1
    while IFS= read -r q; do
        [ "$q" -nt "$FC1" ] && { fresh=1; break; }
    done < <(find bin lib -name '*.fi' -not -type l)
fi
[ "$fresh" -eq 1 ] && { rm -f "$FC1"; "$FIRNC" bin/firnc1.fi -o "$FC1" || exit 1; }

# The value of one measurement out of the run: `posix: <name> = <number>`.
# The value of a `const NAME: u64 = <number>` -- the line may carry a
# comment behind it, so the number is taken from the middle and not from
# the end.
number_in() { # file name
    grep -aE "^const $2: u64 = [0-9]+" "$1" | head -1 \
        | sed -E 's/^const [A-Za-z0-9_]+: u64 = ([0-9]+).*/\1/'
}

value_of() { # file name
    grep -a -m1 "^posix: $2 = " "$1" | sed 's/.* = //'
}

# `ok` when the program reported exactly this number for this name.
say() { # file name expected description
    local got
    got=$(value_of "$1" "$2")
    if [ -z "$got" ]; then bad "$4 -- no line 'posix: $2 ='"; return; fi
    if [ "$got" = "$3" ]; then ok "$4 ($2 = $got)"
    else bad "$4 -- $2 = $got, expected $3"; fi
}

# ------------------------------------------------------------- section 1

echo "== 1. the numbers are the numbers of Linux x86-64 =="
check_number() { # name expected
    local got
    got=$(number_in demos/kernel/sys.fi "SYS_$1")
    if [ "$got" = "$2" ]; then ok "SYS_$1 = $2"
    else bad "SYS_$1 = ${got:-missing}, Linux says $2"; fi
}
check_number READ 0
check_number WRITE 1
check_number OPEN 2
check_number CLOSE 3
check_number STAT 4
check_number FSTAT 5
check_number LSEEK 8
check_number MMAP 9
check_number MUNMAP 11
check_number BRK 12
check_number PIPE 22
check_number YIELD 24
check_number DUP 32
check_number DUP2 33
check_number NANOSLEEP 35
check_number GETPID 39
check_number FORK 57
check_number EXECVE 59
check_number EXIT 60
check_number WAIT4 61
check_number GETCWD 79
check_number MKDIR 83
check_number UNLINK 87
check_number GETPPID 110
check_number GETDENTS64 217
check_number EXIT_GROUP 231
# What Osum has of its own lives far above the Linux table on purpose: a
# number that could one day BE a Linux call would be a trap for stage 2.
own=$(grep -aoE "^const SYS_OSUM_[A-Z]+: u64 = [0-9]+" demos/kernel/sys.fi \
    | sed -E 's/.*= ([0-9]+).*/\1/' | sort -n | head -1)
num "the lowest number Osum invented for itself" "${own:-0}" ge 1000

# The libc says the same numbers. Two tables that disagree would be a
# program that asks for `fstat` and gets `lseek`.
diffs=0
while read -r name value; do
    other=$(number_in lib/osum/libc/kcall.fi "$name")
    [ "$other" = "$value" ] || { diffs=$((diffs+1)); echo "        $name: kernel $value, libc ${other:-missing}"; }
done < <(grep -aoE "^const SYS_[A-Z0-9_]+: u64 = [0-9]+" demos/kernel/sys.fi \
    | sed -E 's/^const ([A-Z0-9_]+): u64 = ([0-9]+).*/\1 \2/' \
    | grep -vE '^SYS_(MARK|LEAVE) ')
# SYS_MARK and SYS_LEAVE are not in that comparison: they are the two
# marks of round 59, they live only while the ring 3 excursion of
# `user.fi` runs, and no program can reach them -- a libc that offered
# them would be offering something that does not exist.
num "system call numbers on which kernel and libc disagree" "$diffs" eq 0

ediffs=0
ecount=0
while read -r name value; do
    ecount=$((ecount+1))
    other=$(number_in lib/osum/libc/errno.fi "$name")
    [ "$other" = "$value" ] || { ediffs=$((ediffs+1)); echo "        $name: kernel $value, libc ${other:-missing}"; }
done < <(grep -aoE "^const E_[A-Z0-9_]+: u64 = [0-9]+" demos/kernel/errno.fi \
    | sed -E 's/^const ([A-Z0-9_]+): u64 = ([0-9]+).*/\1 \2/')
num "error numbers the kernel defines" "$ecount" ge 30
num "error numbers on which kernel and libc disagree" "$ediffs" eq 0
for pair in "E_NOENT 2" "E_BADF 9" "E_CHILD 10" "E_FAULT 14" "E_INVAL 22" \
            "E_NOSYS 38" "E_ISDIR 21" "E_NOTDIR 20" "E_SPIPE 29" "E_PIPE 32"; do
    set -- $pair
    got=$(number_in demos/kernel/errno.fi "$1")
    [ "$got" = "$2" ] && ok "$1 = $2 (the number Linux uses)" \
                      || bad "$1 = ${got:-missing}, expected $2"
done

# ------------------------------------------------------------- section 2

echo "== 2. build: the kernel, the libc and a program that measures it =="
# ROUND K5 ADDED A FOURTH ASSEMBLY FILE, and this file was written
# before it landed: `isr.s` names `smp_vectors` and `KERNEL_AP_MAIN`, and
# a link without `smp.s` and without that symbol stops with two undefined
# references. `tools/osum/run.sh` already links it; these three lines are
# the same three.
for f in boot isr switch smp; do
    as --64 -o "$TMPD/$f.o" "demos/kernel/$f.s" 2>"$TMPD/as.err" \
        || { bad "$f.s does not assemble"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
done
as --64 -o "$TMPD/crt.o" demos/kernel/user/crt.s 2>"$TMPD/ascrt.err" \
    && ok "crt.s assembles" \
    || { bad "crt.s"; sed 's/^/        /' "$TMPD/ascrt.err" | head -5; }

build_stage() { # 0 = firnc0, 1 = firnc1
    local s=$1 cc
    if [ "$s" = 0 ]; then cc="$FIRNC"; else cc="$FC1"; fi
    "$cc" demos/kernel/kmain.fi -o "$TMPD/k$s.o" >"$TMPD/e$s" 2>&1 \
        || { bad "firnc$s does not compile the kernel"; sed 's/^/        /' "$TMPD/e$s" | head -8; return 1; }
    "$cc" demos/kernel/uprog.fi -o "$TMPD/u$s.o" >>"$TMPD/e$s" 2>&1 \
        || { bad "firnc$s does not compile uprog.fi"; return 1; }
    ld -n -T "$LDSCRIPT" \
        --defsym=KERNEL_MAIN="_F$s.kernel_main" \
        --defsym=KERNEL_TRAP="_F$s.trap__entry" \
        --defsym=KERNEL_SYSCALL="_F$s.sys__entry" \
        --defsym=KERNEL_TASK_MAIN="_F$s.tasks__main" \
        --defsym=KERNEL_USER_START="_F$s.proc__user_start" \
        --defsym=KERNEL_AP_MAIN="_F$s.smp__ap_main" \
        --defsym=USER_MAIN="_F$s.u_enter" \
        -o "$TMPD/k$s.elf" "$TMPD/boot.o" "$TMPD/isr.o" "$TMPD/switch.o" "$TMPD/smp.o" \
        "$TMPD/k$s.o" "$TMPD/u$s.o" 2>"$TMPD/ld$s.err" \
        || { bad "firnc$s: ld failed on the kernel"; grep -v 'GNU-stack\|RWX\|deprecated' "$TMPD/ld$s.err" | sed 's/^/        /' | head -5; return 1; }
    objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
    ok "firnc$s: kernel linked and turned into a multiboot image"
    local p
    for p in $PROGS; do
        "$cc" "demos/kernel/user/$p.fi" -o "$TMPD/$p$s.o" >"$TMPD/e$p$s" 2>&1 \
            || { bad "firnc$s does not compile $p.fi"; sed 's/^/        /' "$TMPD/e$p$s" | head -6; return 1; }
        ld -T "$ULD" --defsym=USER_ENTRY="_F$s.u_start" \
            -o "$TMPD/$p$s.elf" "$TMPD/crt.o" "$TMPD/$p$s.o" 2>"$TMPD/ldu.err" \
            || { bad "firnc$s: ld failed on $p"; grep -v 'GNU-stack\|RWX' "$TMPD/ldu.err" | sed 's/^/        /' | head -5; return 1; }
        strip --strip-all "$TMPD/$p$s.elf"
    done
    ok "firnc$s: $(echo $PROGS | wc -w) programs linked as standalone ELF64 files"
    return 0
}

build_stage 0 || { echo "POSIX: $pass passed, $fail failed"; exit 1; }
build_stage 1 || true

undef=""
for p in $PROGS; do
    u=$(nm -u "$TMPD/$p"0".elf" 2>/dev/null | awk '{print $NF}' | sed '/^$/d')
    [ -n "$u" ] && undef="$undef $p:$u"
done
[ -z "$undef" ] && ok "not one program has an undefined symbol -- the libc is complete" \
               || bad "undefined symbols:$undef"
foreign=0
for p in $PROGS; do
    if nm "$TMPD/$p"0".elf" 2>/dev/null | grep -qiE '(malloc|printf|memcpy|__gc_|gc_alloc|mmap|__libc)'; then
        foreign=$((foreign+1))
    fi
done
num "programs carrying a host libc name" "$foreign" eq 0
size=$(stat -c%s "$TMPD/posix0.elf")
num "the measuring program, against the 2135552 octets a file may hold" "$size" lt 2135552

echo "== 3. the disk image, and a file past the old ceiling =="
printf 'firn round K4: a file for the reading\n' > "$TMPD/readme.txt"
SPEC="/bin/"
for p in $PROGS; do SPEC="$SPEC /bin/$p=$TMPD/${p}0.elf"; done
python3 tools/osum/mkfs.py build "$TMPD/disk.img" $BLOCKS $SPEC \
    /readme.txt="$TMPD/readme.txt" > "$TMPD/mkfs.txt" 2>&1 \
    && ok "mkfs.py built an OFS image of $BLOCKS blocks" \
    || { bad "mkfs.py failed"; sed 's/^/        /' "$TMPD/mkfs.txt"; }
# THE COUNTER-CHECK TO THE DOUBLE INDIRECT BLOCK: a file BIGGER than the
# old ceiling of 38912 octets goes onto an image and comes back off it
# unchanged. Round K4 needed that for a program that carries a libc, and
# this is the check that host and kernel agree about the new shape.
python3 - "$TMPD/big.bin" <<'PY'
import sys
n = 100000
with open(sys.argv[1], "wb") as f:
    f.write(bytes((i * 7 + (i >> 8)) & 0xFF for i in range(n)))
PY
if python3 tools/osum/mkfs.py build "$TMPD/big.img" $BLOCKS \
        /big.bin="$TMPD/big.bin" > "$TMPD/bigfs.txt" 2>&1; then
    ok "a file of 100000 octets -- past the old ceiling -- fits on an image"
    python3 tools/osum/mkfs.py cat "$TMPD/big.img" /big.bin > "$TMPD/big.out" 2>/dev/null
    if cmp -s "$TMPD/big.bin" "$TMPD/big.out"; then
        ok "and every octet of it comes back unchanged"
    else
        bad "the big file changed between writing and reading"
    fi
else
    bad "mkfs.py cannot write a file over 38912 octets"; sed 's/^/        /' "$TMPD/bigfs.txt"
fi

run_kernel() {
    local image=$1 append=$2 out=$3
    shift 3
    timeout 180 qemu-system-x86_64 -kernel "$image" -m 128 -append "$append" \
        -serial "file:$out" -display none -no-reboot "$@" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

run_disk() { # image append out diskimage
    local copy="$TMPD/live.img"
    cp "$4" "$copy"
    run_kernel "$1" "$2" "$3" -drive "file=$copy,format=raw,if=ide,index=0"
}

QUIET="nokbd nosched noproc nofs noring3"

# ------------------------------------------------------------- section 4

echo "== 4. every system call, and every one of its errors =="
run_disk "$TMPD/k0.mb" "osum $QUIET script=posix;exit" "$TMPD/run0.txt" \
    "$TMPD/disk.img"
rc=$?
F="$TMPD/run0.txt"
[ "$rc" -eq 21 ] && ok "the kernel shut down on its own (exit 21)" \
                 || { bad "QEMU exit code $rc, expected 21"; tail -8 "$F" | tr -d '\000' | sed 's/^/        /'; }

echo "   -- open, write, read, close"
say "$F" "open" 3 "open gave the lowest free descriptor"
say "$F" "write" 11 "write wrote every octet it was given"
say "$F" "close" 0 "close said nothing went wrong"
say "$F" "read" 11 "read gave back exactly what was written"
say "$F" "read again" 0 "a read at the end of the file is 0, not an error"
say "$F" "same" 1 "and the octets are the same ones"

echo "   -- lseek, stat, fstat"
say "$F" "seek" 4 "lseek SEEK_SET answers with the new position"
say "$F" "seek char" 101 "and the next octet read is really the fifth one"
say "$F" "seek cur" 7 "lseek SEEK_CUR counts from where it stood"
say "$F" "seek end" 11 "lseek SEEK_END lands on the size of the file"
say "$F" "fstat size" 11 "fstat knows the size behind an open descriptor"
say "$F" "stat size" 11 "stat knows the size behind a path"
say "$F" "stat mode" 1 "st_mode says S_IFREG for a file"
say "$F" "stat dir" 1 "and S_IFDIR for a directory"

echo "   -- mkdir, getdents64"
say "$F" "mkdir" 0 "mkdir made the directory"
say "$F" "mkdir twice" -17 "the second time it is -EEXIST"
say "$F" "found dir" 1 "the new directory shows up in the listing"
num "entries the root directory has" "$(value_of "$F" entries)" ge 5
num "of them directories" "$(value_of "$F" 'dot dir')" ge 3
say "$F" "dents short" -22 "a buffer too short for one entry is -EINVAL"

echo "   -- dup, dup2"
say "$F" "dup" 4 "dup gave the next free descriptor"
say "$F" "dup positions" 4 "the two descriptors share ONE position (the open file table)"
say "$F" "dup2" 1 "the original still reads after the copy was closed"
say "$F" "dup2 close" -9 "and a closed descriptor is -EBADF"

echo "   -- brk, mmap, munmap and the allocator on top of them"
say "$F" "brk" 1 "brk(0) says where the program break stands"
say "$F" "alloc" 1 "the allocator handed out a block"
say "$F" "heap holds" 1 "and what is written into it stays there"
say "$F" "brk grew" 1 "the break really moved for it"
say "$F" "alloc reuse" 1 "a released block is used again (the free list is one)"
say "$F" "mmap" 1 "mmap gave anonymous pages"
say "$F" "mmap holds" 1 "the second page of them is really mapped"
say "$F" "munmap" 0 "munmap gave them back"
say "$F" "mmap bad" -19 "without MAP_ANONYMOUS there is nothing to map: -ENODEV"

echo "   -- pipe"
say "$F" "pipe" 0 "pipe made a pair of descriptors"
say "$F" "pipe wrote" 5 "the writing end took the octets"
say "$F" "pipe read" 5 "the reading end gave them back"
say "$F" "pipe same" 1 "unchanged"
say "$F" "pipe seek" -29 "a pipe cannot be seeked: -ESPIPE"
say "$F" "pipe end" 0 "with the last writer gone the reader sees the end"

echo "   -- fork, execve, wait4"
say "$F" "pid" 1 "getpid answers with a pid of its own"
say "$F" "ppid" 1 "getppid with a different one"
num "fork gave the parent the pid of the child" "$(value_of "$F" fork)" ge 1
say "$F" "fork pipe text" 4 "the child wrote through the pipe it inherited"
say "$F" "fork sees" 1 "and what it wrote is what the PARENT had in its memory"
say "$F" "fork code" 217 "wait4 delivered the exit code of the child"
say "$F" "run" 42 "fork + execve + wait4 ran a program off the disk (40 + argc, out of /bin/hello)"
say "$F" "no child" -10 "waiting for a foreign pid is -ECHILD"

echo "   -- and every way to be wrong"
say "$F" "err noent" -2 "a file that is not there: -ENOENT"
say "$F" "err badfd" -9 "a descriptor that is not open: -EBADF"
say "$F" "err fault" -14 "a pointer into the kernel: -EFAULT"
say "$F" "err nil" -14 "a null pointer: -EFAULT"
say "$F" "err nosys" -38 "a call number nobody has: -ENOSYS"
say "$F" "err isdir" -21 "reading a directory as a file: -EISDIR"
say "$F" "err notdir" -20 "opening a file as a directory: -ENOTDIR"
say "$F" "err wrdir" -21 "opening a directory for writing: -EISDIR"
say "$F" "err unldir" -21 "unlinking a directory: -EISDIR"
say "$F" "err seekneg" -22 "seeking before the start of a file: -EINVAL"
say "$F" "err closed" -9 "closing the same descriptor twice: -EBADF"
say "$F" "err execmis" -2 "execve of a file that is not there: -ENOENT"
say "$F" "err sleep" -22 "a sleep of a thousand seconds: -EINVAL"
say "$F" "errno" 2 "and the libc turns the same refusal into -1 with errno = ENOENT"

lines=$(value_of "$F" lines)
seen=$(grep -ac '^posix: ' "$F")
num "measurements the program printed" "${lines:-0}" ge 50
if [ -n "$lines" ] && [ "$seen" = "$((lines + 1))" ]; then
    ok "every line it counted really arrived over the serial port ($seen)"
else
    bad "the program counted ${lines:-0} lines, $seen arrived"
fi
has "$F" "osum\$ posix -> $lines" "the shell waited for it and got its exit code"
has "$F" "kernel: done" "the kernel got to the end after all of that"

# ------------------------------------------------------------- section 5

echo "== 5. fork + dup2 + execve: a redirection in the shell =="
SCRIPT='ls > /out.txt;cat /out.txt;echo plain;exit'
run_disk "$TMPD/k0.mb" "osum $QUIET script=$SCRIPT" "$TMPD/redir.txt" \
    "$TMPD/disk.img"
rc=$?
R="$TMPD/redir.txt"
[ "$rc" -eq 21 ] && ok "the redirection run ended by itself (exit 21)" \
                 || { bad "redirection run: exit code $rc"; tail -6 "$R" | tr -d '\000' | sed 's/^/        /'; }
if grep -aq '^\./ \.\./ bin/ readme.txt out.txt $' "$R"; then
    ok "'cat /out.txt' shows what 'ls' would have written to the console"
else
    bad "the redirected listing did not come back out of the file"
    grep -a -A2 'cat /out.txt' "$R" | head -4 | tr -d '\000' | sed 's/^/        /'
fi
has "$R" "plain" "and a command without a redirection still writes to the console"
SCRIPT2='ls;cat /out.txt;exit'
run_disk "$TMPD/k0.mb" "osum $QUIET script=$SCRIPT2" "$TMPD/noredir.txt" \
    "$TMPD/disk.img"
N="$TMPD/noredir.txt"
if grep -aq '^\./ \.\./ bin/ readme.txt $' "$N"; then
    ok "counter-check: without '>' the listing goes to the console"
else
    bad "counter-check: the plain listing is missing"
fi
# ROUND K6 REPLACED `cat`: it names the file it could not open instead of
# printing the raw error number, and it says so on descriptor 2.
has "$N" "cat: cannot open /out.txt" "counter-check: and /out.txt was never made -- cat says so"

# ------------------------------------------------------------- section 6

echo "== 6. nothing leaks, and the kernel survives all of it =="
free_line=$(grep -a -m1 '^osum: frames_free=' "$F")
now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
if [ -n "$now" ] && [ "$now" = "$was" ]; then
    ok "every frame of every process, pipe and heap came back ($now free, as before)"
else
    bad "frames after the run: $now, before: $was"
fi
deep=$(grep -a -m1 'kstack deepest=' "$F" | grep -oE 'deepest=[0-9]+' | cut -d= -f2)
cap=$(grep -a -m1 'kstack deepest=' "$F" | grep -oE 'of [0-9]+' | awk '{print $2}')
num "the deepest kernel stack of the run (octets)" "$deep" lt "${cap:-0}"
hasnot "$F" "*** EXCEPTION" "not one exception in a run that provokes fourteen errors"
calls=$(grep -a -o -E 'syscalls=[0-9]+' "$F" | head -1 | cut -d= -f2)
num "system calls answered in the run" "${calls:-0}" ge 100
num "processes split by fork" "$(grep -a -o -E 'forks=[0-9]+' "$F" | head -1 | cut -d= -f2)" ge 2
num "images replaced in place by execve" "$(grep -a -o -E 'execves=[0-9]+' "$F" | head -1 | cut -d= -f2)" ge 1
num "pipes made" "$(grep -a -o -E 'pipes=[0-9]+' "$F" | head -1 | cut -d= -f2)" ge 2
num "pages handed out by brk and mmap" "$(grep -a -o -E 'maps=[0-9]+' "$F" | head -1 | cut -d= -f2)" ge 3
num "descriptors opened" "$(grep -a -o -E 'opens=[0-9]+' "$F" | head -1 | cut -d= -f2)" ge 10

# ------------------------------------------------------------- section 7

echo "== 7. the same numbers out of the compiler written in Firn =="
if [ -f "$TMPD/k1.mb" ]; then
    SPEC1="/bin/"
    for p in $PROGS; do SPEC1="$SPEC1 /bin/$p=$TMPD/${p}1.elf"; done
    if python3 tools/osum/mkfs.py build "$TMPD/disk1.img" $BLOCKS $SPEC1 \
        /readme.txt="$TMPD/readme.txt" > "$TMPD/mkfs1.txt" 2>&1; then
        ok "the programs of firnc1 fit on an image as well"
        run_disk "$TMPD/k1.mb" "osum $QUIET script=posix;exit" "$TMPD/run1.txt" \
            "$TMPD/disk1.img"
        rc=$?
        G="$TMPD/run1.txt"
        [ "$rc" -eq 21 ] && ok "firnc1: the kernel shut down on its own" \
                         || bad "firnc1: QEMU exit code $rc, expected 21"
        # The measurements have to be the SAME numbers. What may differ
        # between the two compilers is the speed, and no number here is
        # one -- except the pid of a forked child, which depends on how
        # many tasks the run happened to make before it.
        grep -a '^posix: ' "$F" | grep -v '^posix: fork = ' > "$TMPD/m0.txt"
        grep -a '^posix: ' "$G" | grep -v '^posix: fork = ' > "$TMPD/m1.txt"
        if diff -q "$TMPD/m0.txt" "$TMPD/m1.txt" >/dev/null 2>&1; then
            ok "every measurement is the same in both compilers ($(wc -l < "$TMPD/m0.txt") lines)"
        else
            bad "the two compilers measure different things"
            diff "$TMPD/m0.txt" "$TMPD/m1.txt" | head -8 | sed 's/^/        /'
        fi
    else
        bad "mkfs.py failed on the firnc1 programs"; sed 's/^/        /' "$TMPD/mkfs1.txt"
    fi
else
    echo "   (firnc1 is not there -- skipped)"
fi

echo
echo "POSIX: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
