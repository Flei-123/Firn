#!/usr/bin/env bash
# tools/userland/run.sh -- THE PROOF THAT OSUM HAS A USERLAND.
#
# Round K1 proved that a program comes off the disk: the kernel reads
# `/bin/sh`, lays it into an address space and starts it. What it did not
# prove -- because there was nothing to prove it with -- is that the
# programs on that disk are a SYSTEM. A shell that can only start a
# program is a program launcher; what makes it a shell is that it decides
# where the output of that program goes.
#
# So this file does not measure "it starts without crashing". It runs
# REAL COMMAND CHAINS in the shell that came off the disk and holds the
# whole transcript against a file written by hand, octet for octet:
#
#   1. THE TOOLS SAY THE RIGHT THING. sort, head, tail, wc, grep, uniq,
#      cat, echo, ls -- against an expected transcript, not against a
#      "did not crash".
#   2. REDIRECTION AND PIPES. `>` `>>` `<` `|`, three stages deep, and the
#      numbers that come out the far end have to be the right numbers.
#   3. THE SHELL IS A SHELL. A working directory that the CHILD sees,
#      variables, quoting, `$?` out of the process and not out of the
#      shell, `&` and `wait`, and a line editor with backspace and the
#      line before this one.
#   4. WHAT IS BROKEN IS SAID SO. A file that is not there, a directory
#      where a file was asked for, an empty input, a command that does not
#      exist, a directory that is not empty -- every one of them with its
#      message and its exit code.
#   5. THE DISK REALLY CHANGED. After the run the HOST reads the image
#      back: what `mkdir`, `cp`, `mv` and `rm` did is in the octets.
#   6. NOTHING LEAKS. Frames free before = frames free after, over runs
#      with more than a hundred processes in them.
#   7. THE OTHER COMPILER. firnc1 builds the same userland, and the same
#      script produces the same transcript.
#
# Usage:  bash tools/userland/run.sh
set -uo pipefail
cd "$(dirname "$0")/../.."

export FIRNLIB="$(pwd)/lib"
FIRNC=compiler/target/release/firnc
FC1=${FIRNC1:-./.firnc1}
LDSCRIPT=demos/kernel/kernel.ld
ULD=demos/kernel/user/user.ld
# The whole userland. `hello` and `hurt` belong to round K1 and travel
# with it, because tools/osum/run.sh measures them on the same image.
PROGS="sh ls cat echo cp mv rm mkdir rmdir touch head tail wc grep sort
       uniq true false sleep ps kill uname date df hello hurt"
BLOCKS=4096

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
matches() { grep -qE "$2" "$1" && ok "$3" || bad "$3 -- nothing matches /$2/"; }

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "USERLAND: skipped, qemu-system-x86_64 is not available"
    exit 0
fi

# The freshness trap of rounds 35/45/46: an outdated `.firnc1` measures
# yesterday's compiler.
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

run_kernel() { # image append out [extra qemu arguments]
    local image=$1 append=$2 out=$3
    shift 3
    timeout 120 qemu-system-x86_64 -kernel "$image" -m 128 -append "$append" \
        -serial "file:$out" -display none -no-reboot "$@" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

# Every case gets a COPY of the image: a case that writes to the disk must
# not change what the next one sees. The copy stays behind as
# `after-<name>.img`, so that the host can read back what the userland did.
run_case() { # kernel image name script
    local copy="$TMPD/live-$3.img"
    cp "$2" "$copy"
    run_kernel "$1" "osum nokbd nosched noproc nofs noring3 script=$4" \
        "$TMPD/$3.txt" -drive "file=$copy,format=raw,if=ide,index=0"
    local rc=$?
    cp "$copy" "$TMPD/after-$3.img"
    return $rc
}

# What the SHELL said, and nothing else: everything between the two
# markers the script writes itself, without the lines the kernel writes
# over the same serial port.
block() { # rawfile
    awk '/^==BEGIN==$/ {f=1; next} /^==END==$/ {f=0} f' "$1" \
        | grep -vE '^(elf: |key: |osum: |user fault: |\*\*\*)'
}

# ------------------------------------------------------- 1. the build

echo "== 1. build: a kernel, and a userland of twenty-five programs =="
for f in boot isr switch; do
    as --64 -o "$TMPD/$f.o" "demos/kernel/$f.s" 2>"$TMPD/as.err" \
        || { bad "$f.s does not assemble"; sed 's/^/        /' "$TMPD/as.err" | head -5; }
done
as --64 -o "$TMPD/crt.o" demos/kernel/user/crt.s 2>/dev/null \
    || bad "crt.s does not assemble"

build_stage() { # 0 = firnc0, 1 = firnc1
    local s=$1 cc p
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
        || { bad "firnc$s: ld failed on the kernel"; return 1; }
    objcopy -O elf32-i386 "$TMPD/k$s.elf" "$TMPD/k$s.mb" 2>/dev/null
    for p in $PROGS; do
        "$cc" "demos/kernel/user/$p.fi" -o "$TMPD/$p$s.o" >"$TMPD/e$p$s" 2>&1 \
            || { bad "firnc$s does not compile $p.fi"; sed 's/^/        /' "$TMPD/e$p$s" | head -6; return 1; }
        ld -T "$ULD" --defsym=USER_ENTRY="_F$s.u_start" \
            -o "$TMPD/$p$s.elf" "$TMPD/crt.o" "$TMPD/$p$s.o" 2>"$TMPD/ldu.err" \
            || { bad "firnc$s: ld failed on $p"; return 1; }
        strip --strip-all "$TMPD/$p$s.elf"
    done
    return 0
}

build_stage 0 || { echo "USERLAND: $pass passed, $fail failed"; exit 1; }
ok "firnc0: the kernel and $(echo $PROGS | wc -w) standalone programs are built"
build_stage 1 && ok "firnc1: the same, out of the compiler written in Firn" \
              || bad "firnc1 did not build the userland"

# Every program is a static executable that has nothing under it.
undef=""
for p in $PROGS; do
    u=$(nm -u "$TMPD/${p}0.elf" 2>/dev/null | awk '{print $NF}' | sed '/^$/d')
    [ -n "$u" ] && undef="$undef $p:$u"
done
[ -z "$undef" ] && ok "no program has an undefined symbol (no libc, no kernel, no runtime)" \
               || bad "undefined symbols:$undef"
biggest=0; total=0
for p in $PROGS; do
    for s in 0 1; do
        [ -f "$TMPD/$p$s.elf" ] || continue
        z=$(stat -c%s "$TMPD/$p$s.elf")
        [ "$z" -gt "$biggest" ] && biggest=$z
    done
    total=$((total + $(stat -c%s "$TMPD/${p}1.elf" 2>/dev/null || stat -c%s "$TMPD/${p}0.elf")))
done
num "the biggest program, against the 2135552 octets a file may hold" "$biggest" lt 2135552
num "the whole userland in octets, against the $((BLOCKS * 512)) of the drive" \
    "$total" lt $((BLOCKS * 512 - 100000))

# ------------------------------------------------ 2. the disk and the cases

echo "== 2. the image: the programs, the fixtures and the scripts =="
printf 'banana\napple\ncherry\n'        > "$TMPD/three.txt"
printf 'a\na\nb\nb\nb\nc\n'             > "$TMPD/dup.txt"
: > "$TMPD/empty.txt"
: > "$TMPD/nums.txt"
for i in $(seq 1 12); do echo "line $i" >> "$TMPD/nums.txt"; done

# ---- case 1: the tools themselves.
cat > "$TMPD/t1.sh" <<'SCRIPT'
echo ==BEGIN==
echo hello world
cat /d/three.txt
sort /d/three.txt
sort -r /d/three.txt
wc -l /d/nums.txt
head -n 3 /d/nums.txt
tail -n 2 /d/nums.txt
grep an /d/three.txt
grep -v an /d/three.txt
grep -c line /d/nums.txt
uniq /d/dup.txt
uniq -c /d/dup.txt
wc /d/three.txt
echo ==END==
SCRIPT
cat > "$TMPD/t1.want" <<'WANT'
hello world
banana
apple
cherry
apple
banana
cherry
cherry
banana
apple
12 /d/nums.txt
line 1
line 2
line 3
line 11
line 12
banana
apple
cherry
12
a
b
c
2 a
3 b
1 c
3 3 20 /d/three.txt
WANT

# ---- case 2: redirection and pipes.
cat > "$TMPD/t2.sh" <<'SCRIPT'
echo ==BEGIN==
echo one > /out.txt
cat /out.txt
echo two >> /out.txt
cat /out.txt
wc -l < /out.txt
cat /d/nums.txt | grep 1 | wc -l
sort /d/three.txt | head -n 1
echo pipe | cat
cat /d/dup.txt | uniq | sort -r
cat /d/three.txt | sort | uniq -c
echo -n short > /out.txt
wc -c /out.txt
cat /nope 2> /err.txt
echo code=$?
cat /err.txt
echo ==END==
SCRIPT
cat > "$TMPD/t2.want" <<'WANT'
one
one
two
2
4
apple
pipe
c
b
a
1 apple
1 banana
1 cherry
5 /out.txt
code=1
cat: cannot open /nope
WANT

# ---- case 3: the shell as a shell.
cat > "$TMPD/t3.sh" <<'SCRIPT'
echo ==BEGIN==
cd /d
pwd
ls
cd ..
pwd
cd /bin
pwd
cd /nope
echo code=$?
cd /
X=hello
echo $X
Y=a-b
echo $Y
Z="q r"
echo "$Z"
echo '$X'
echo "x$X y"
true
echo t=$?
false
echo f=$?
export X
export
cd /d
cat three.txt
wc -l < nums.txt
cd /
echo ==END==
SCRIPT
cat > "$TMPD/t3.want" <<'WANT'
/d
./ ../ three.txt dup.txt nums.txt empty.txt 
/
/bin
sh: cannot cd to /nope
code=1
hello
a-b
q r
$X
xhello y
t=0
f=1
X=hello
banana
apple
cherry
12
WANT

# ---- case 4: what is broken.
cat > "$TMPD/t4.sh" <<'SCRIPT'
echo ==BEGIN==
cat /nope
echo code=$?
cat /d
echo code=$?
ls /nope
echo code=$?
rm /nope
echo code=$?
rmdir /d
echo code=$?
mkdir /d
echo code=$?
nosuchcommand
echo code=$?
grep needle /d/empty.txt
echo code=$?
wc -l /d/empty.txt
sort /d/empty.txt
head -n 2 /d/empty.txt
tail -n 2 /d/empty.txt
cat /d/empty.txt | wc -c
cp /nope /x
echo code=$?
echo end
echo ==END==
SCRIPT
cat > "$TMPD/t4.want" <<'WANT'
cat: cannot open /nope
code=1
cat: is a directory /d
code=1
ls: cannot list /nope
code=1
rm: cannot remove /nope
code=1
rmdir: cannot remove /d
code=1
mkdir: cannot create /d
code=1
sh: cannot run nosuchcommand -> -2
code=127
code=1
0 /d/empty.txt
0
cp: cannot open /nope
code=1
end
WANT

# ---- case 5: the line editor. The octets 8 and 14 are what a keyboard
# sends, and a file can therefore hold them just as well -- which is how
# an editor is measured without a person in front of the machine.
printf 'echo ==BEGIN==\necho fooX\bbar\necho keep\n\016\necho ==END==\n' \
    > "$TMPD/t5.sh"
cat > "$TMPD/t5.want" <<'WANT'
foobar
keep
keep
WANT

# ---- case 6: the disk really changes.
cat > "$TMPD/t6.sh" <<'SCRIPT'
echo ==BEGIN==
mkdir /w
touch /w/a
echo hi > /w/b
ls /w
cp /w/b /w/c
cat /w/c
mv /w/c /w/d
ls /w
cat /w/d
cp /d/nums.txt /w
wc -l /w/nums.txt
rm /w/a
rm /w/b
rm /w/d
ls /w
echo keep > /keep.txt
echo ==END==
SCRIPT
cat > "$TMPD/t6.want" <<'WANT'
./ ../ a b 
hi
./ ../ a b d 
hi
12 /w/nums.txt
./ ../ nums.txt 
WANT

# ---- case 7: the programs whose output nobody can write down in advance.
cat > "$TMPD/t7.sh" <<'SCRIPT'
echo ==BEGIN==
uname
uname -a
df
date
date -u
ps
sleep -m 50
echo slept=$?
sleep -m 4000 &
kill $!
wait
echo after=$?
ls -l /d
echo ==END==
SCRIPT

SPEC="/bin/"
for p in $PROGS; do SPEC="$SPEC /bin/$p=$TMPD/${p}0.elf"; done
DATA="/d/ /d/three.txt=$TMPD/three.txt /d/dup.txt=$TMPD/dup.txt
      /d/nums.txt=$TMPD/nums.txt /d/empty.txt=$TMPD/empty.txt"
CASES="/t/"
for c in t1 t2 t3 t4 t5 t6 t7; do CASES="$CASES /t/$c.sh=$TMPD/$c.sh"; done
python3 tools/osum/mkfs.py build "$TMPD/disk0.img" $BLOCKS $SPEC $DATA $CASES \
    > "$TMPD/mkfs.txt" 2>&1 \
    && ok "mkfs.py built an image of $BLOCKS blocks with the whole userland on it" \
    || { bad "mkfs.py failed"; sed 's/^/        /' "$TMPD/mkfs.txt" | head -5; }
n=$(python3 tools/osum/mkfs.py list "$TMPD/disk0.img" | grep -c '^/bin/[a-z]')
num "programs in /bin according to the host" "$n" eq "$(echo $PROGS | wc -w)"

# ------------------------------------------------------- 3. the cases

echo "== 3. the transcripts: what the shell really said =="
for c in t1 t2 t3 t4 t5 t6; do
    run_case "$TMPD/k0.mb" "$TMPD/disk0.img" "$c" "sh /t/$c.sh;exit"
    rc=$?
    if [ "$rc" -ne 21 ]; then
        bad "$c: QEMU exit code $rc, expected 21"
        tail -5 "$TMPD/$c.txt" | sed 's/^/        /'
        continue
    fi
    block "$TMPD/$c.txt" > "$TMPD/$c.got"
    if diff -q "$TMPD/$c.want" "$TMPD/$c.got" >/dev/null; then
        ok "$c: the transcript is exactly what it should be ($(wc -l < "$TMPD/$c.got") lines)"
    else
        bad "$c: the transcript differs"
        diff "$TMPD/$c.want" "$TMPD/$c.got" | head -20 | sed 's/^/        /'
    fi
    hasnot "$TMPD/$c.txt" "*** EXCEPTION" "$c: not one exception in the run"
    hasnot "$TMPD/$c.txt" "osum_panic" "$c: no checked-arithmetic panic"
    free_line=$(grep -m1 '^osum: frames_free=' "$TMPD/$c.txt")
    now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
    was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
    if [ -n "$now" ] && [ "$now" = "$was" ]; then
        ok "$c: every frame of every program came back ($now free)"
    else
        bad "$c: frames after the run: $now, before: $was"
    fi
done

echo "== 4. how many programs one script really starts =="
execs=$(grep -c '^elf: start ' "$TMPD/t1.txt")
num "programs loaded off the disk in the first case alone" "$execs" ge 13
execs=$(grep -c '^elf: start ' "$TMPD/t2.txt")
num "programs loaded off the disk in the pipe case" "$execs" ge 18

echo "== 5. the disk after the run, read by the HOST =="
python3 tools/osum/mkfs.py list "$TMPD/after-t6.img" > "$TMPD/after6.txt" 2>&1
has "$TMPD/after6.txt" "/w/nums.txt" "the file 'cp' wrote into the new directory is in the image"
hasnot "$TMPD/after6.txt" "/w/a" "and the three files 'rm' took away are gone"
hasnot "$TMPD/after6.txt" "/w/d" "including the one 'mv' had renamed"
has "$TMPD/after6.txt" "/keep.txt" "a file the shell wrote with '>' is in the image"
has "$TMPD/after6.txt" "/bin/sh" "and nothing else on the image was harmed"
python3 tools/osum/mkfs.py list "$TMPD/after-t2.img" > "$TMPD/after2.txt" 2>&1
has "$TMPD/after2.txt" "/out.txt" "the file of the redirection case is on the disk"
has "$TMPD/after2.txt" "/tmp" "and so is the directory the shell makes for its pipes"
size=$(awk '$1 == "/out.txt" {print $2}' "$TMPD/after2.txt")
num "'echo -n short > /out.txt' wrote exactly five octets" "${size:-0}" eq 5

echo "== 6. the programs that ask the kernel about itself =="
run_case "$TMPD/k0.mb" "$TMPD/disk0.img" "t7" "sh /t/t7.sh;exit"
rc=$?
[ "$rc" -eq 21 ] && ok "the case with ps, df, date and kill in it ends by itself (exit 21)" \
                 || bad "t7: QEMU exit code $rc, expected 21"
block "$TMPD/t7.txt" > "$TMPD/t7.got"
has "$TMPD/t7.got" "osum" "uname says what the system is called"
matches "$TMPD/t7.got" "^osum firn K[0-9]+ x86_64$" "uname -a says the round out of the KERNEL"
matches "$TMPD/t7.got" "^blocks total=4096 free=[0-9]+ used=[0-9]+ size=512$" \
    "df counts the blocks of the drive the host built"
matches "$TMPD/t7.got" "^inodes total=128 used=[0-9]+$" "df counts the inodes"
matches "$TMPD/t7.got" "^2[0-9]{3}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}$" \
    "date reads the CMOS clock of the machine"
matches "$TMPD/t7.got" "^up [0-9]+ ms$" "date -u reads the timer instead"
matches "$TMPD/t7.got" "^  PID PPID STATE   KIND   PRIO TICKS$" "ps prints its table"
matches "$TMPD/t7.got" "^ +[0-9]+ +[0-9]+ (run|ready|sleep|wait) +user +[0-9]+ +[0-9]+$" \
    "and a line of it is the running process itself"
matches "$TMPD/t7.got" "^ +[0-9]+ +[0-9]+ (ready|run|sleep) +idle" "the idle task is in the table"
has "$TMPD/t7.got" "slept=0" "sleep ends with 0"
matches "$TMPD/t7.got" "^\[1\] [0-9]+$" "a background job says which job it is and which pid"
matches "$TMPD/t7.got" "^\[[0-9]+\] -> 137$" \
    "and 'kill \$!' really ended it: wait picks up 137 = 128 + 9"
matches "$TMPD/t7.got" "^-       87 nums.txt$" "ls -l says how big a file is"
matches "$TMPD/t7.got" "^-        0 empty.txt$" "and that the empty one is empty"
free_line=$(grep -m1 '^osum: frames_free=' "$TMPD/t7.txt")
now=$(echo "$free_line" | grep -oE 'frames_free=[0-9]+' | cut -d= -f2)
was=$(echo "$free_line" | grep -oE 'of [0-9]+' | awk '{print $2}')
[ "$now" = "$was" ] && ok "a killed process gave its frames back as well ($now free)" \
                    || bad "frames after the kill case: $now, before $was"

echo "== 7. the same shell over the console, the way round K1 drives it =="
# Not out of a file this time: the lines come off the kernel command line
# and through descriptor 0, which is the path a keyboard takes. The shell
# is CHATTY here -- and that is what tools/osum/run.sh measures.
run_case "$TMPD/k0.mb" "$TMPD/disk0.img" "con" \
    "echo a b c;echo x > /c.txt;cat /c.txt;wc -c < /c.txt;sort /d/three.txt | head -n 1;false;echo code=\$?;exit"
rc=$?
C="$TMPD/con.txt"
[ "$rc" -eq 21 ] && ok "the console run ends by itself (exit 21)" \
                 || bad "console run: exit code $rc"
has "$C" "sh: ready, osum" "the interactive shell says it is ready, as in round K1"
has "$C" "osum\$ echo a b c" "it echoes the line it read"
has "$C" "a b c" "and the program printed its arguments"
has "$C" "osum\$ echo -> 0" "the exit code of a program is reported after it"
has "$C" "osum\$ cat -> 0" "and 'cat' came back with 0"
grep -q '^2$' "$C" && ok "'wc -c < /c.txt' counted the two octets 'x' and newline" \
                   || { bad "wc over a redirected input"; grep -A2 'wc' "$C" | head -5 | sed 's/^/        /'; }
has "$C" "apple" "a pipe works over the console as well"
has "$C" "code=1" "and \$? is the 1 that /bin/false really returned"
has "$C" "sh: bye" "'exit' ends the shell"
has "$C" "kernel: done" "and the kernel gets to the end"

echo "== 8. the counter-check: the shell writes into a FILE, not to the port =="
# The same command twice, once with `>` and once without. What the serial
# port shows has to differ, and the difference has to be exactly the line.
run_case "$TMPD/k0.mb" "$TMPD/disk0.img" "vis" "echo visible;exit"
run_case "$TMPD/k0.mb" "$TMPD/disk0.img" "inv" "echo visible > /v.txt;exit"
grep -q '^visible$' "$TMPD/vis.txt" \
    && ok "without a redirection the line is on the serial port" \
    || bad "the line is missing from the plain run"
grep -q '^visible$' "$TMPD/inv.txt" \
    && bad "with '> /v.txt' the line still went to the serial port" \
    || ok "COUNTER-CHECK: with '> /v.txt' the port stays quiet -- descriptor 1 really moved"
python3 tools/osum/mkfs.py list "$TMPD/after-inv.img" > "$TMPD/afterinv.txt" 2>&1
has "$TMPD/afterinv.txt" "/v.txt" "and the octets are in the file instead"
size=$(awk '$1 == "/v.txt" {print $2}' "$TMPD/afterinv.txt")
num "the file holds the seven octets of 'visible' and a newline" "${size:-0}" eq 8

echo "== 9. the same userland out of the other compiler =="
if [ -f "$TMPD/k1.mb" ] && [ -f "$TMPD/sh1.elf" ]; then
    SPEC1="/bin/"
    for p in $PROGS; do SPEC1="$SPEC1 /bin/$p=$TMPD/${p}1.elf"; done
    python3 tools/osum/mkfs.py build "$TMPD/disk1.img" $BLOCKS $SPEC1 $DATA $CASES \
        > "$TMPD/mkfs1.txt" 2>&1 \
        && ok "an image whose programs were all built by firnc1" \
        || { bad "mkfs for the firnc1 image failed"; sed 's/^/        /' "$TMPD/mkfs1.txt" | head -3; }
    for c in t1 t2 t3 t4; do
        run_case "$TMPD/k1.mb" "$TMPD/disk1.img" "f1$c" "sh /t/$c.sh;exit"
        rc=$?
        if [ "$rc" -ne 21 ]; then
            bad "firnc1/$c: QEMU exit code $rc, expected 21"
            continue
        fi
        block "$TMPD/f1$c.txt" > "$TMPD/f1$c.got"
        if diff -q "$TMPD/$c.want" "$TMPD/f1$c.got" >/dev/null; then
            ok "firnc1/$c: the same transcript, octet for octet"
        else
            bad "firnc1/$c: the two compilers produce a different userland"
            diff "$TMPD/$c.want" "$TMPD/f1$c.got" | head -12 | sed 's/^/        /'
        fi
    done
else
    bad "firnc1 produced no image -- section 9 measured nothing"
fi

echo "== 10. and round K1 still holds =="
# The interactive shell of round K1 is this same program. Its script runs
# here as well, and the lines that round measured have to be there.
run_case "$TMPD/k0.mb" "$TMPD/disk0.img" "k1" \
    "ls;ls /bin;echo one two three;hello a b;nothing;exit"
K="$TMPD/k1.txt"
has "$K" "sh: ready, osum" "round K1: the banner"
has "$K" "one two three" "round K1: argc/argv arrive unchanged"
has "$K" "hello: argc3" "round K1: the argument block of a program off the disk"
has "$K" "hello: bss 0" "round K1: the loader still zeroes the .bss"
has "$K" "sh: cannot run nothing -> -2" "round K1: an unknown command is -ENOENT"
has "$K" "sh: bye" "round K1: 'exit' ends the shell"

echo
echo "USERLAND: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
exit 0
