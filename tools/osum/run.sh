#!/usr/bin/env bash
# tools/osum/run.sh -- THE PROOF THAT OSUM IS A SYSTEM AND NOT AN IMAGE.
#
# Round 62 proved that the kernel has tasks, address spaces, system calls,
# a file system and a shell. It also had a hole in the middle of it, and
# the hole was this: EVERY PROGRAM IT COULD RUN WAS COMPILED INTO IT.
# `demos/kernel/uprog.fi` travelled in the kernel image, the list of
# programs was a list of `if`s, and adding a command meant building a new
# kernel. A machine like that is a demonstration with a fixed programme.
#
# Round K1 closes it. `/bin/sh` is an ELF FILE ON A DISK that the host
# built and the kernel has never seen; the kernel reads it, lays its
# segments into a fresh address space with the rights the file asks for,
# and starts it. Everything the shell then runs it runs the same way.
#
# The rule of this project is that a property without a counter-check is
# a claim. So every point below has a second measurement that has to
# COLLAPSE, and the collapse is what is really being checked:
#
#   1. THE PROGRAM COMES OFF THE DISK. Counter-check: put the octets of
#      `/bin/echo` at the path `/bin/ls` and boot the SAME kernel -- `ls`
#      then echoes. Nothing but the file changed.
#   2. WHAT IS BROKEN IS REFUSED AND THE KERNEL LIVES. Twenty-one files
#      with exactly one thing wrong each, from a missing magic number to a
#      program header at offset 0xFFFFFFFFFFFFFF00. Every one of them gets
#      a named reason, the shell goes on, the kernel shuts itself down.
#   3. THE RIGHTS OF A PAGE ARE REAL. `/bin/hurt` writes into its own code
#      page (#PF err 0x7 -- there, write, ring 3), runs its own stack
#      (err 0x15 -- there, instruction fetch, ring 3), reads the kernel
#      (err 0x5) and touches a page nobody mapped (err 0x4). Four faults,
#      four different error codes, one living kernel.
#   4. THE LOADER ZEROES WHAT THE FILE DOES NOT FILL. `/bin/hello` prints
#      an all-zero `static mut` out of `.bss`. Counter-check `nobss`: the
#      loader leaves its poison pattern standing and the same program
#      prints 12297829382473034410.
#   5. THE KEYBOARD REACHES THE SHELL. Three keys through the QEMU
#      monitor -- l, s, return -- and the root directory comes back.
#   6. NOTHING LEAKS. Frames free before the shell = frames free after it,
#      and the deepest kernel stack of the run is reported as a number.
#
# Everything is measured the way rounds 59 and 62 measured: QEMU per case,
# with a time limit, serial output against expectations, exit code out of
# `isa-debug-exit` (21 = the kernel ended it, 63 = an exception).
#
# Usage:  bash tools/osum/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
LDSCRIPT=demos/kernel/kernel.ld
ULD=demos/kernel/user/user.ld
PROGS="sh ls cat echo rm hurt hello"
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

has() { grep -qF "$2" "$1" && ok "$3" || bad "$3 -- '$2' is missing"; }
hasnot() { grep -qF "$2" "$1" && bad "$3 -- '$2' should not be there" || ok "$3"; }
value() { grep -oE "$2" "$1" | head -1 | grep -oE '[0-9]+$'; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "OSUM: skipped, qemu-system-x86_64 is not available"
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

# ---------------------------------------------------------------- running

# $1 = kernel image, $2 = command line, $3 = output file, rest = extra
# arguments for QEMU. Returns the QEMU exit code.
run_kernel() {
    local image=$1 append=$2 out=$3
    shift 3
    timeout 120 qemu-system-x86_64 -kernel "$image" -m 128 -append "$append" \
        -serial "file:$out" -display none -no-reboot "$@" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

# A run against a COPY of the image: the kernel writes to its disk, and a
# case that changed it must not change what the next case sees.
run_disk() { # image append out diskimage
    local copy="$TMPD/live.img"
    cp "$4" "$copy"
    run_kernel "$1" "$2" "$3" -drive "file=$copy,format=raw,if=ide,index=0"
}

QUIET="nokbd nosched noproc nofs noring3"

echo "== 1. build: the kernel, and a userland that is NOT in it =="
for f in boot isr switch; do
    as --64 -o "$TMPD/$f.o" "demos/kernel/$f.s" 2>"$TMPD/as.err" \
        || { bad "$f.s does not assemble"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
done
as --64 -o "$TMPD/crt.o" demos/kernel/user/crt.s 2>"$TMPD/ascrt.err" \
    && ok "crt.s assembles (_start, osum_panic -- the four instructions of a user program)" \
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
        --defsym=USER_MAIN="_F$s.u_enter" \
        -o "$TMPD/k$s.elf" "$TMPD/boot.o" "$TMPD/isr.o" "$TMPD/switch.o" \
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
        cp "$TMPD/$p$s.elf" "$TMPD/$p$s.dbg"
        strip --strip-all "$TMPD/$p$s.elf"
    done
    ok "firnc$s: $(echo $PROGS | wc -w) programs linked as standalone ELF64 files"
    return 0
}

build_stage 0 || { echo "OSUM: $pass passed, $fail failed"; exit 1; }
build_stage 1 || true

echo "== 2. what a program on the disk says about itself =="
for p in $PROGS; do
    e="$TMPD/$p"0".elf"
    [ -f "$e" ] || continue
    kind=$(readelf -h "$e" | awk -F: '/^  Type:/ {print $2}' | awk '{print $1}')
    [ "$kind" = "EXEC" ] || bad "$p: ELF type '$kind', expected EXEC"
done
ok "all programs are ET_EXEC (a static executable, no interpreter, no relocation)"
# No undefined name at all -- not even `osum_panic`. A program on the disk
# has no kernel to fall back on, so crt.s defines it itself.
undef=""
for p in $PROGS; do
    u=$(nm -u "$TMPD/$p"0".dbg" 2>/dev/null | awk '{print $NF}' | sed '/^$/d')
    [ -n "$u" ] && undef="$undef $p:$u"
done
[ -z "$undef" ] && ok "no program has a single undefined symbol (no libc, no kernel, no runtime)" \
               || bad "undefined symbols:$undef"
for p in $PROGS; do
    if nm "$TMPD/$p"0".dbg" | grep -qiE '(malloc|printf|memcpy|__gc_|gc_alloc|mmap|__libc)'; then
        bad "$p: a libc or collector name is in the symbol table"
    fi
done
ok "no libc name and no collector in any of the symbol tables"
# Every one of them talks to the kernel, and only that way.
for p in $PROGS; do
    n=$(objdump -d "$TMPD/$p"0".elf" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    [ "$n" -ge 1 ] || bad "$p: not one `syscall` instruction -- how does it say anything?"
done
ok "every program carries at least one syscall instruction"
# The three segments, and that they are page granular: this is what the
# loader demands, and what makes one page have one set of rights.
seg=$(readelf -l "$TMPD/sh0.elf" | grep -c '^  LOAD')
num "sh: PT_LOAD segments" "$seg" ge 2
segh=$(readelf -l "$TMPD/hello0.elf" | grep -c '^  LOAD')
num "hello: PT_LOAD segments (it has a .bss, so it has three)" "$segh" eq 3
# Page granularity, read off the finished file: EVERY PT_LOAD has to start
# on a page boundary in the file AND in memory. Two segments that share a
# page cannot have two sets of rights, and the loader refuses such a file
# (reason 16/17) -- so this is the check that the linker script keeps its
# side of that bargain.
misaligned=0
while read -r off vad; do
    [ -z "$off" ] && continue
    o=$((off)); v=$((vad))
    [ $((o % 4096)) -eq 0 ] || misaligned=$((misaligned+1))
    [ $((v % 4096)) -eq 0 ] || misaligned=$((misaligned+1))
done < <(readelf -lW "$TMPD/hello0.elf" | awk '/^  LOAD/ {print $2, $3}')
num "misaligned segment starts in hello" "$misaligned" eq 0
rx=$(readelf -lW "$TMPD/hello0.elf" | awk '/^  LOAD/ {print $7 $8}' | grep -c 'RE')
rw=$(readelf -lW "$TMPD/hello0.elf" | awk '/^  LOAD/ {print $7 $8}' | grep -c '^RW')
num "hello: read+execute segments" "$rx" eq 1
num "hello: read+write segments" "$rw" eq 1
if readelf -SW "$TMPD/hello0.dbg" | grep -q '\.bss .*NOBITS'; then
    ok "hello: the .bss is NOBITS -- not one of its octets is in the file"
else
    bad "hello: no NOBITS .bss, so nothing measures the zeroing"
fi

# The file system stops a file at 11 direct + 64 indirect + 64 * 64
# double indirect blocks (`demos/kernel/fs.fi`, widened in round K6 --
# the shell of that round is over the old 38912 octets). The programs
# have to fit into that, and the margin is worth a number: this is the
# check that speaks up first if a later compiler makes them fatter.
biggest=0
for p in $PROGS; do
    for s in 0 1; do
        [ -f "$TMPD/$p$s.elf" ] || continue
        z=$(stat -c%s "$TMPD/$p$s.elf")
        [ "$z" -gt "$biggest" ] && biggest=$z
    done
done
num "the biggest program on the disk, against the 2135552 octets a file may hold" \
    "$biggest" lt 2135552

echo "== 3. the disk image, built on the host and read back =="
printf 'firn round K1: this file lies on a real disk\n' > "$TMPD/readme.txt"
SPEC="/bin/"
for p in $PROGS; do SPEC="$SPEC /bin/$p=$TMPD/${p}0.elf"; done
python3 tools/osum/mkfs.py build "$TMPD/disk.img" $BLOCKS $SPEC \
    /readme.txt="$TMPD/readme.txt" > "$TMPD/mkfs.txt" 2>&1 \
    && ok "mkfs.py built an OFS image of $BLOCKS blocks" \
    || { bad "mkfs.py failed"; sed 's/^/        /' "$TMPD/mkfs.txt"; }
python3 tools/osum/mkfs.py list "$TMPD/disk.img" > "$TMPD/list.txt" 2>&1
has "$TMPD/list.txt" "/bin/sh" "the host can read its own image back: /bin/sh is in it"
n=$(grep -c '^/bin/[a-z]' "$TMPD/list.txt")
num "programs in /bin according to the host" "$n" eq 7
# The magic is a 64 bit number, so on this machine the octets stand in the
# file the other way round -- the same string round 62 looks for in the
# image its own kernel wrote.
if grep -qa "SFO-MUSO" "$TMPD/disk.img"; then
    ok "the image carries the magic number of the kernel's file system"
else
    bad "no OSUM-OFS magic in the image"
fi
# The one thing that says host and kernel agree on the format: the kernel
# mounts it. That is measured in section 4; here only the shape.
size=$(stat -c%s "$TMPD/disk.img")
num "image size in octets" "$size" eq $((BLOCKS * 512))

echo "== 4. the full run: a shell that came off the disk =="
SCRIPT='ls;ls /bin;cat /readme.txt;echo one two three;hello a b;rm /readme.txt;ls;nothing;exit'
run_disk "$TMPD/k0.mb" "osum $QUIET script=$SCRIPT" "$TMPD/full0.txt" "$TMPD/disk.img"
rc=$?
F="$TMPD/full0.txt"
[ "$rc" -eq 21 ] && ok "the kernel shut down on its own (exit 21)" \
                 || { bad "QEMU exit code $rc, expected 21"; tail -6 "$F" | sed 's/^/        /'; }
has "$F" "osum: mount=1" "the kernel mounted the image the HOST wrote"
has "$F" "osum: bin .:2 ..:2 sh:1 ls:1 cat:1 echo:1 rm:1 hurt:1 hello:1" \
    "the kernel sees exactly the seven programs the host put in /bin"
has "$F" "sh: ready, osum" "/bin/sh started -- an ELF file, loaded from a drive"
has "$F" "ustack=0x40007ff0" \
    "the programs got a stack pointer just under their argument block"
grep -q '^elf: seg 5 v=0x40100000 .* w=0 x=1$' "$F" \
    && ok "the code segment was mapped read+execute and NOT writable" \
    || { bad "no read+execute code segment in the report"; grep '^elf: seg' "$F" | head -4 | sed 's/^/        /'; }
grep -q '^elf: seg 6 .* w=1 x=0$' "$F" \
    && ok "the data segment was mapped writable and NOT executable" \
    || { bad "no writable non-executable data segment"; grep '^elf: seg' "$F" | head -6 | sed 's/^/        /'; }
grep -q '^elf: seg 4 .* w=0 x=0$' "$F" \
    && ok "the constants were mapped read only and NOT executable" \
    || bad "no read-only segment in the report"
grep -q '^\./ \.\./ bin/ readme.txt $' "$F" \
    && ok "'ls' lists the root of the image, directories with a slash" \
    || { bad "'ls' says something else"; grep -A1 'osum\$ ls$' "$F" | head -4 | sed 's/^/        /'; }
grep -q '^\./ \.\./ sh ls cat echo rm hurt hello $' "$F" \
    && ok "'ls /bin' lists the programs" || bad "'ls /bin' is wrong"
has "$F" "firn round K1: this file lies on a real disk" "'cat' read a file off the disk"
has "$F" "one two three" "'echo one two three' -- argc/argv arrived unchanged"
has "$F" "hello: argc3" "'hello a b' got three arguments (argv[0] is its own name)"
has "$F" "hello: arg 0 hello" "argv[0] is the name of the program"
has "$F" "hello: arg 1 a" "argv[1] arrived"
has "$F" "hello: arg 2 b" "argv[2] arrived"
has "$F" "hello: bss 0" "the .bss of the program is zero -- the loader zeroed it"
has "$F" "hello: data 3735928559" "the .data of the program arrived unchanged"
has "$F" "hello: ro 424242" "the .rodata of the program is readable"
has "$F" "osum\$ hello -> 43" "wait() delivered the exit code of the program (40 + argc)"
has "$F" "osum\$ ls -> 4" "wait() delivered the exit code of ls (four entries)"
has "$F" "osum\$ cat -> 45" "wait() delivered the exit code of cat (45 octets)"
has "$F" "osum\$ echo -> 3" "wait() delivered the exit code of echo (three words)"
has "$F" "osum\$ rm -> 1" "'rm' deleted one name"
grep -q '^\./ \.\./ bin/ $' "$F" \
    && ok "after 'rm' the file is gone from the listing" \
    || bad "'rm' did not change what 'ls' sees"
has "$F" "sh: cannot run nothing -> -2" "an unknown command gives -ENOENT out of exec"
has "$F" "elf: refused, reason 1  no such file" "and the kernel names the reason"
has "$F" "sh: bye" "'exit' ends the shell"
has "$F" "kernel: done" "the kernel got to the end"
execs=$(grep -c '^elf: start ' "$F")
num "programs loaded off the disk in this one run" "$execs" ge 6
# Nothing leaks: the frames the images took came back with the processes.
free_line=$(grep -m1 '^osum: frames_free=' "$F")
now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
if [ -n "$now" ] && [ "$now" = "$was" ]; then
    ok "every frame of every image came back ($now free, as before)"
else
    bad "frames after six programs: $now, before: $was"
fi
deep=$(value "$F" 'kstack deepest=[0-9]+')
cap=$(grep -m1 'kstack deepest=' "$F" | grep -oE 'of [0-9]+' | awk '{print $2}')
num "the deepest kernel stack of the run (octets)" "$deep" lt "${cap:-0}"
# THE COUNTER-CHECK TO THE STACK SIZE: this number is over 8192, which is
# what round 62 gave a task. `exec` through the loader and the ATA driver
# did not fit into it, and the kernel died in the ATA driver with a rip
# that pointed into its own stack. The number is why sched.KSTACK_FRAMES
# is 4 and not 2.
num "and it is deeper than the two frames of round 62" "$deep" gt 8192
# What the USERLAND wrote really reached the drive on the host.
cp "$TMPD/live.img" "$TMPD/after.img"
python3 tools/osum/mkfs.py list "$TMPD/after.img" > "$TMPD/after.txt" 2>&1
hasnot "$TMPD/after.txt" "/readme.txt" "the host sees that 'rm' really took the file off the disk"
has "$TMPD/after.txt" "/bin/sh" "and nothing else on the image was harmed"

echo "== 5. counter-check: the loader stops zeroing =="
run_disk "$TMPD/k0.mb" "osum nobss $QUIET script=hello;exit" "$TMPD/nobss.txt" "$TMPD/disk.img"
rc=$?
N="$TMPD/nobss.txt"
[ "$rc" -eq 21 ] && ok "the nobss run gets to the end as well (exit 21)" \
                 || bad "nobss run: exit code $rc"
has "$N" "hello: bss 12297829382473034410" \
    "with the zeroing switched off the .bss holds the poison pattern (0xAA..)"
hasnot "$N" "hello: bss 0" "and the zero of the normal run is gone"
has "$N" "hello: data 3735928559" "what the FILE fills is filled either way"

echo "== 6. the rights of a page, measured in faults =="
run_disk "$TMPD/k0.mb" "osum $QUIET script=hurt text;hurt stack;hurt kernel;hurt wild;exit" \
    "$TMPD/hurt.txt" "$TMPD/disk.img"
rc=$?
H="$TMPD/hurt.txt"
[ "$rc" -eq 21 ] && ok "four dead processes later the kernel still ends the machine itself (exit 21)" \
                 || { bad "hurt run: exit code $rc"; tail -6 "$H" | sed 's/^/        /'; }
grep -qE '^user fault: pid=[0-9]+  vector=14  err=0x7  cr2=0x40100000' "$H" \
    && ok "writing into its own code page: #PF err=0x7 (page there, write, ring 3) -- the text is read only" \
    || { bad "no #PF err=0x7 on the code page"; grep '^user fault' "$H" | sed 's/^/        /'; }
grep -qE '^user fault: pid=[0-9]+  vector=14  err=0x15  cr2=0x4000' "$H" \
    && ok "executing its own stack: #PF err=0x15 (page there, instruction fetch, ring 3) -- the no-execute bit works" \
    || { bad "no #PF err=0x15 out of the stack"; grep '^user fault' "$H" | sed 's/^/        /'; }
grep -qE '^user fault: pid=[0-9]+  vector=14  err=0x5  cr2=0x100000' "$H" \
    && ok "reading kernel memory: #PF err=0x5 (page there, ring 3) -- the kernel stays closed" \
    || bad "no #PF err=0x5 on kernel memory"
grep -qE '^user fault: pid=[0-9]+  vector=14  err=0x4  cr2=0x40050000' "$H" \
    && ok "touching an unmapped page of its own region: #PF err=0x4 (not there, ring 3)" \
    || bad "no #PF err=0x4 on the unmapped page"
hasnot "$H" "hurt: WROTE ITS OWN CODE" "the store into the code page did NOT go through"
hasnot "$H" "hurt: STACK RAN" "the stack did NOT execute"
hasnot "$H" "hurt: READ THE KERNEL" "the process never saw an octet of the kernel"
n=$(grep -c '^user fault' "$H")
num "faults in this run" "$n" eq 4
n=$(grep -c 'osum\$ hurt -> 142' "$H")
num "times the shell was told its child died of vector 14 (128 + 14)" "$n" eq 4
has "$H" "sh: bye" "the shell survived all four of them"
has "$H" "kernel: done" "and so did the kernel"
free_line=$(grep -m1 '^osum: frames_free=' "$H")
now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
[ "$now" = "$was" ] && ok "four killed processes gave every frame back ($now free)" \
                    || bad "frames after four killed processes: $now, before $was"

echo "== 7. twenty-one files with exactly one thing wrong =="
# Every entry is <defect>:<reason>:<what was changed>. The reason numbers
# are `elf.R_*`; checking them ONE BY ONE (and not just that the number
# turns up somewhere) is what makes the measurement say which field the
# loader really looked at. Round K1 needed that: two defects at first both
# came out as reason 14, because the tool that produced them raised
# p_filesz as well and the earlier check fired.
DEFECTS="magic:4:the four octets 0x7F E L F changed
class:5:ELFCLASS32 instead of 64
endian:6:the endianness field turned around
version:7:e_ident version 0
type:8:ET_DYN instead of ET_EXEC
machine:9:e_machine says i386
phent:10:e_phentsize 32 instead of 56
phnum:11:e_phnum 99
phoff:12:e_phoff 0xFFFFFFFFFFFFFF00
short:3:a file of 32 octets
noload:21:every PT_LOAD turned into PT_NOTE
segfile:13:a segment moved past the end of the file
bigoff:13:p_offset 0xFFFFFFFFFFFFF000
memsz:14:p_filesz one octet bigger than p_memsz
range:15:a segment at 0x50000000
bigmem:15:p_memsz 0xFFFFFFFFFFFF0000
align:16:p_vaddr eight octets off a page
overlap:17:two segments onto the same page
noexec:19:the entry segment without PF_X"
BSPEC="/b/"
while IFS= read -r line; do
    d=${line%%:*}
    python3 tools/osum/break.py "$TMPD/ls0.elf" "$TMPD/b_$d" "$d" >/dev/null 2>&1 \
        || bad "break.py could not produce the defect '$d'"
    BSPEC="$BSPEC /b/$d=$TMPD/b_$d"
done <<< "$DEFECTS"
ok "break.py produced $(echo "$DEFECTS" | wc -l) files, each with one field changed"
python3 tools/osum/mkfs.py build "$TMPD/bad.img" $BLOCKS /bin/ \
    /bin/sh="$TMPD/sh0.elf" $BSPEC /readme.txt="$TMPD/readme.txt" \
    > "$TMPD/mkbad.txt" 2>&1 \
    && ok "an image with the twenty-one candidates on it" \
    || { bad "mkfs for the bad image failed"; sed 's/^/        /' "$TMPD/mkbad.txt"; }
BADSCRIPT=""
while IFS= read -r line; do BADSCRIPT="$BADSCRIPT/b/${line%%:*};"; done <<< "$DEFECTS"
BADSCRIPT="$BADSCRIPT/bin;/readme.txt;exit"
run_disk "$TMPD/k0.mb" "osum $QUIET script=$BADSCRIPT" "$TMPD/bad.txt" "$TMPD/bad.img"
rc=$?
B="$TMPD/bad.txt"
[ "$rc" -eq 21 ] && ok "after twenty-one refusals the kernel ends the machine itself (exit 21)" \
                 || { bad "bad-file run: exit code $rc"; tail -8 "$B" | sed 's/^/        /'; }
# The line right AFTER the command is the kernel's answer to exactly that
# file -- so the reason belongs to the defect and not to the run.
check_one() { # path reason description
    # Everything between the echoed command and the NEXT prompt belongs to
    # this file. It is not always the line right after: a file whose first
    # segment is fine and whose second is not reports the first segment
    # before the refusal.
    local got
    got=$(awk -v c="osum\$ $1" '
        $0 == c { f = 1; next }
        f && index($0, "osum$ ") == 1 { exit }
        f && $1 == "elf:" && $2 == "refused," { print $4; exit }' "$B")
    if [ "$got" = "$2" ]; then ok "$3 -> reason $2"
    else bad "$3: the kernel answered reason '${got:-none}', expected $2"; fi
}
while IFS= read -r line; do
    d=${line%%:*}; rest=${line#*:}; want=${rest%%:*}; text=${rest#*:}
    check_one "/b/$d" "$want" "$text"
done <<< "$DEFECTS"
check_one "/bin" 2 "a DIRECTORY handed to exec"
check_one "/readme.txt" 3 "a plain text file handed to exec"
n=$(grep -c '^elf: refused, reason ' "$B")
num "refusals in one run" "$n" eq 21
# THE POINT OF THE THREE HUGE NUMBERS: `phoff`, `bigoff` and `bigmem` are
# 0xFFFF-something. Under `profile kernel` arithmetic is checked, so a
# loader that computed `off + filesz` on them would call `osum_panic` and
# the machine would stop instead of the file being refused. It did not.
hasnot "$B" "osum_panic" "no checked-arithmetic panic on the three huge fields"
has "$B" "sh: bye" "the shell survived twenty-one refusals"
has "$B" "kernel: done" "and the kernel got to the end"
n=$(grep -c 'sh: cannot run ' "$B")
num "times the shell was told the file is not a program" "$n" eq 21
hasnot "$B" "*** EXCEPTION" "not one exception in the whole run"

echo "== 8. THE counter-check: another file at the same path =="
# Nothing about the kernel changes here. The image gets the octets of
# /bin/echo under the name /bin/ls, and the same command in the same shell
# does something else. That is what "the program comes off the disk" MEANS.
SWAP="/bin/ /bin/sh=$TMPD/sh0.elf /bin/ls=$TMPD/echo0.elf"
python3 tools/osum/mkfs.py build "$TMPD/swap.img" $BLOCKS $SWAP >/dev/null 2>&1 \
    && ok "an image whose /bin/ls holds the octets of /bin/echo" \
    || bad "mkfs for the swapped image failed"
run_disk "$TMPD/k0.mb" "osum $QUIET script=ls one two;exit" "$TMPD/swap.txt" "$TMPD/swap.img"
rc=$?
S="$TMPD/swap.txt"
[ "$rc" -eq 21 ] && ok "the swapped run gets to the end (exit 21)" || bad "swap run: exit $rc"
has "$S" "one two" "'ls one two' ECHOED its arguments -- the file decides, not the name"
hasnot "$S" "bin/" "and it did not list a directory, because it is not ls any more"
has "$S" "osum\$ ls -> 2" "even the exit code is echo's (two words)"
# And the same shell against the honest image lists instead of echoing.
grep -q '^\./ \.\./ bin/ readme.txt $' "$F" \
    && ok "the SAME kernel with the honest image lists the directory" \
    || bad "the honest run does not list"

echo "== 9. the keyboard reaches the shell =="
rm -f "$TMPD/mon.sock" "$TMPD/kbd.txt" "$TMPD/kbd.rc"
cp "$TMPD/disk.img" "$TMPD/kbd.img"
( timeout 120 qemu-system-x86_64 -kernel "$TMPD/k0.mb" -m 128 \
    -append "osum nosched noproc nofs noring3" \
    -serial "file:$TMPD/kbd.txt" -display none -no-reboot \
    -drive "file=$TMPD/kbd.img,format=raw,if=ide,index=0" \
    -monitor "unix:$TMPD/mon.sock,server,nowait" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
  echo $? > "$TMPD/kbd.rc" ) &
qemu_pid=$!
for _ in $(seq 1 200); do
    [ -f "$TMPD/kbd.txt" ] && grep -q "sh: ready, osum" "$TMPD/kbd.txt" && break
    sleep 0.2
done
if [ -S "$TMPD/mon.sock" ] && grep -q "sh: ready, osum" "$TMPD/kbd.txt" 2>/dev/null; then
    python3 - "$TMPD/mon.sock" <<'PY'
import socket, sys, time
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[1])
time.sleep(0.4)
for key in ["l", "s", "ret"]:
    s.sendall(("sendkey %s\n" % key).encode())
    time.sleep(0.3)
s.close()
PY
    wait $qemu_pid 2>/dev/null
    rc=$(cat "$TMPD/kbd.rc" 2>/dev/null || echo 99)
    K="$TMPD/kbd.txt"
    [ "$rc" -eq 21 ] && ok "the interactive run ends by itself (exit 21) -- a quiet console is the end of the input" \
                     || { bad "keyboard run: exit code $rc"; tail -6 "$K" | sed 's/^/        /'; }
    n=$(grep -c '^key: ' "$K")
    num "keys that arrived over IRQ1" "$n" eq 3
    has "$K" "osum\$ ls" "the shell read the line 'ls' out of the keyboard"
    grep -q '^\./ \.\./ bin/ readme.txt $' "$K" \
        && ok "AND IT LISTED THE FILE SYSTEM -- typed in, loaded off the disk, printed back" \
        || { bad "the typed 'ls' produced no listing"; grep -A2 'osum\$ ls' "$K" | head -5 | sed 's/^/        /'; }
    has "$K" "elf: start" "the typed command started a program off the disk"
    has "$K" "sh: bye" "and the shell ended when the keyboard went quiet"
else
    kill $qemu_pid 2>/dev/null
    bad "keyboard run: the shell never said it was ready"
fi

echo "== 10. what happens when there is nothing to boot from =="
run_kernel "$TMPD/k0.mb" "osum $QUIET" "$TMPD/nodrive.txt"
rc=$?
D="$TMPD/nodrive.txt"
[ "$rc" -eq 21 ] && ok "without a drive the kernel gets to the end (exit 21)" \
                 || bad "no-drive run: exit code $rc"
has "$D" "osum: no drive" "and it says so instead of reading a device that is not there"
dd if=/dev/zero of="$TMPD/empty.img" bs=512 count=$BLOCKS 2>/dev/null
run_disk "$TMPD/k0.mb" "osum $QUIET" "$TMPD/nofs.txt" "$TMPD/empty.img"
rc=$?
E="$TMPD/nofs.txt"
[ "$rc" -eq 21 ] && ok "with an unformatted drive the kernel gets to the end (exit 21)" \
                 || bad "unformatted run: exit code $rc"
has "$E" "osum: mount=0" "an unformatted drive is refused, not interpreted"
hasnot "$E" "sh: ready" "and no shell is started off it"
python3 tools/osum/mkfs.py build "$TMPD/nosh.img" $BLOCKS /bin/ \
    /bin/ls="$TMPD/ls0.elf" >/dev/null 2>&1
run_disk "$TMPD/k0.mb" "osum $QUIET" "$TMPD/nosh.txt" "$TMPD/nosh.img"
rc=$?
X="$TMPD/nosh.txt"
[ "$rc" -eq 21 ] && ok "an image without /bin/sh: the kernel gets to the end (exit 21)" \
                 || bad "no-sh run: exit code $rc"
has "$X" "osum: sh did not load" "it says the shell is missing"
has "$X" "elf: refused, reason 1  no such file" "with the reason"

echo "== 11. the same system out of the other compiler =="
if [ -f "$TMPD/k1.mb" ] && [ -f "$TMPD/sh1.elf" ]; then
    SPEC1="/bin/"
    for p in $PROGS; do SPEC1="$SPEC1 /bin/$p=$TMPD/${p}1.elf"; done
    python3 tools/osum/mkfs.py build "$TMPD/disk1.img" $BLOCKS $SPEC1 \
        /readme.txt="$TMPD/readme.txt" >/dev/null 2>&1
    run_disk "$TMPD/k1.mb" "osum $QUIET script=$SCRIPT" "$TMPD/full1.txt" "$TMPD/disk1.img"
    rc=$?
    G="$TMPD/full1.txt"
    [ "$rc" -eq 21 ] && ok "firnc1: the kernel and the userland it built shut down on their own (exit 21)" \
                     || { bad "firnc1 run: exit code $rc"; tail -6 "$G" | sed 's/^/        /'; }
    for line in "sh: ready, osum" "hello: bss 0" "hello: data 3735928559" \
                "hello: ro 424242" "one two three" "sh: bye" "kernel: done"; do
        grep -qF "$line" "$G" || bad "firnc1: the line '$line' is missing"
    done
    ok "firnc1: every line the measurement depends on is there as well"
    # Addresses and sizes differ (different code, different lengths); the
    # SHAPE must not. Everything numeric is levelled out, and what is left
    # has to be equal word for word.
    lev() { sed -E 's/0x[0-9a-f]+/0xX/g; s/[0-9]+/N/g' "$1" | grep -vE '^(mb|kernel end|mmap|frames|heap|elf: seg|elf: start|osum: kstack)'; }
    lev "$F" > "$TMPD/n0.txt"
    lev "$G" > "$TMPD/n1.txt"
    if diff -q "$TMPD/n0.txt" "$TMPD/n1.txt" >/dev/null; then
        ok "the two compilers produce a system that says the same thing"
    else
        bad "the two systems behave differently"
        diff "$TMPD/n0.txt" "$TMPD/n1.txt" | head -12 | sed 's/^/        /'
    fi
else
    bad "firnc1 produced no image -- section 11 measured nothing"
fi

echo "== 12. and the old measurements still hold =="
# The kernel of round 62 is the same binary. Its own run has to be
# untouched by everything above -- that is what `tools/kernel/run.sh`
# checks in full; here only that the same image still does round 62.
run_kernel "$TMPD/k0.mb" "nokbd" "$TMPD/r62.txt"
rc=$?
R="$TMPD/r62.txt"
[ "$rc" -eq 21 ] && ok "the same kernel image still runs round 62 to the end (exit 21)" \
                 || bad "round 62 run: exit code $rc"
for line in "sched: done" "proc: done" "fs: done" "ring3: back in ring 0" "kernel: done"; do
    grep -qF "$line" "$R" || bad "round 62: '$line' is missing"
done
ok "round 62: tasks, processes, file system and the excursion of round 59 are all still there"
has "$R" "osum: skipped" "without 'osum' on the command line nothing of round K1 runs"

echo
echo "OSUM: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
exit 0
