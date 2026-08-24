#!/usr/bin/env bash
# tools/pci/run.sh -- THE PROOF THAT THE KERNEL READS ITS MACHINE.
#
# Round 62 proved that the kernel is an operating system. Everything it
# spoke to, though, it knew by heart: 0x3F8 is the serial port, 0x60 the
# keyboard, 0x1F0 the disk, and the interrupt controller is the pair of
# 8259s from 1981. Those numbers are true because they have been true
# since 1986. They say nothing about the machine the kernel is on.
#
# This round replaces three of those certainties with questions, and every
# one of them is checked here WITH A COUNTER-CHECK -- a second run in
# which the thing is switched off and the measurement has to collapse.
# Without the second run the first one proves nothing: a list of devices
# that came out of a table in the kernel passes any test that only ever
# asks whether the expected line is there.
#
#   1. PCI. The device list is held against the `-device` arguments QEMU
#      was STARTED with, in both directions. Started without an NVMe
#      controller the line with class 01:08:02 must NOT be there; started
#      with one it must, with the BAR size the controller really has.
#      Three configurations, and the difference between them is the
#      measurement.
#   2. APIC. The local APIC's own timer replaces the PIT, the I/O APIC
#      replaces the two 8259s. Counter-check `noioapic`: the PIC is mute
#      and the keyboard line has no entry -- not one key may arrive. And
#      `noapic`: the old controller keeps the machine and everything works
#      as it did in round 62, which is what makes the switch a choice
#      rather than a rewrite.
#   3. NVMe over DMA. The controller fetches its own commands out of
#      memory and moves the data itself. Counter-check `nobm`: without
#      the bus master bit it may answer registers but may not fetch
#      anything -- the file system on it has to FAIL, and the image on the
#      host has to stay empty. Counter-check `noirq`: the message vector
#      is masked -- the transfer still happens and the interrupt does not,
#      and the two are told apart in the same run.
#   4. THE NUMBER. The same 128 KiB over the same interface, once through
#      the processor over ATA PIO and once past it over NVMe.
#
# Measured the way this project measures: QEMU per case, a time limit,
# serial output against expectations, exit code out of `isa-debug-exit`
# (21 = the kernel ended it, 63 = it stopped at an exception).
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

num() { # name value op expected
    local name=$1 value=$2 op=$3 want=$4
    if [ -z "$value" ]; then bad "$name: no number found (expected $op $want)"; return; fi
    if [ "$value" -"$op" "$want" ] 2>/dev/null; then ok "$name: $value"
    else bad "$name: $value, expected $op $want"; fi
}

has() { # file text name
    grep -qF "$2" "$1" && ok "$3" || bad "$3 -- '$2' is missing"
}

hasnot() { # file text name
    grep -qF "$2" "$1" && bad "$3 -- '$2' should not be there" || ok "$3"
}

value() { # file pattern -> the number at the end of the first match
    grep -oE "$2" "$1" 2>/dev/null | head -1 | grep -oE '[0-9]+$'
}

[ -x "$FIRNC" ] || { echo "firnc0 is missing: $FIRNC"; exit 1; }
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "PCI: skipped, qemu-system-x86_64 is not available"
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

echo "== 1. build the kernel with both compilers =="
as --64 -o "$TMPD/boot.o" demos/kernel/boot.s 2>/dev/null \
    && ok "boot.s assembles" || bad "boot.s"
as --64 -o "$TMPD/isr.o" demos/kernel/isr.s 2>/dev/null \
    && ok "isr.s assembles" || bad "isr.s"
as --64 -o "$TMPD/switch.o" demos/kernel/switch.s 2>/dev/null \
    && ok "switch.s assembles" || bad "switch.s"

for s in 0 1; do
    if [ "$s" = 0 ]; then C="$FIRNC"; else C="$FC1"; fi
    "$C" -o "$TMPD/k$s.o" "$SOURCE" >"$TMPD/e$s" 2>&1 \
        || { bad "firnc$s does not compile the kernel"; sed 's/^/        /' "$TMPD/e$s" | head -8; continue; }
    "$C" -o "$TMPD/uprog$s.o" "$USOURCE" >>"$TMPD/e$s" 2>&1 \
        || { bad "firnc$s does not compile the user programs"; continue; }
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
        ok "firnc$s: kernel with pci.fi, apic.fi, nvme.fi linked"
    else
        bad "firnc$s: ld failed"
        grep -v 'GNU-stack\|RWX\|deprecated' "$TMPD/ld$s.err" | sed 's/^/        /' | head -5
    fi
done
[ -f "$TMPD/k0.mb" ] || { echo; echo "PCI: $pass passed, $fail failed"; exit 1; }

# The freestanding property has to survive three new files: no libc, no
# runtime, and no `syscall` in the kernel's own code. `nvme.fi` is the
# first file of this kernel that touches memory a DEVICE also reads, and
# it does it through the volatile intrinsics -- not through a library.
for s in 0 1; do
    [ -f "$TMPD/k$s.o" ] || continue
    undef=$(nm -u "$TMPD/k$s.o" 2>/dev/null | awk '{print $NF}' | sed '/^$/d' | grep -vxF osum_panic)
    [ -z "$undef" ] && ok "k$s.o: no undefined name other than osum_panic" \
                    || { bad "k$s.o: undefined symbols"; echo "$undef" | sed 's/^/        /'; }
    n=$(objdump -d "$TMPD/k$s.o" | grep -cE '^\s+[0-9a-f]+:.*\bsyscall\b')
    [ "$n" -eq 0 ] && ok "k$s.o: no syscall in the kernel's own code" \
                   || bad "k$s.o: $n syscall instructions"
done
# The configuration space is reached with 32-bit port accesses -- that is
# the width of a configuration register and the only width 0xCF8 knows.
n=$(objdump -d "$TMPD/k0.o" | grep -cE '\b(in|out)[a-z]*\s+.*%eax')
num "k0.o: 32-bit port accesses (the configuration space)" "$n" ge 2
# The APIC is memory and one MSR, not ports.
n=$(objdump -d "$TMPD/k0.o" | grep -cE '\b(rdmsr|wrmsr)\b')
num "k0.o: MSR accesses (the APIC base register)" "$n" ge 2

# ------------------------------------------------------------- one run

# $1 = image, $2 = command line, $3 = output file, rest = QEMU arguments.
run_kernel() {
    local image=$1 append=$2 out=$3
    shift 3
    timeout 300 qemu-system-x86_64 -kernel "$image" -m 128 -append "$append" \
        -serial "file:$out" -display none -no-reboot "$@" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
    return $?
}

# A fresh pair of images per case. Two reasons: a case must not see what
# the case before it wrote, and QEMU takes a write lock on an image -- a
# run that was killed by the time limit would otherwise block the next.
images() {
    rm -f "$TMPD/ata.img" "$TMPD/nv.img"
    dd if=/dev/zero of="$TMPD/ata.img" bs=1M count=2 2>/dev/null
    dd if=/dev/zero of="$TMPD/nv.img" bs=1M count=8 2>/dev/null
}

ATA_ARGS=(-drive "file=$TMPD/ata.img,format=raw,if=ide,index=0")
NVME_ARGS=(-drive "file=$TMPD/nv.img,format=raw,if=none,id=d0"
           -device "nvme,drive=d0,serial=k2disk")
AHCI_ARGS=(-device "ahci,id=ahci0")

# =====================================================================
# 1. PCI -- the list against the command line
# =====================================================================

echo "== 2. the device list against the '-device' arguments of QEMU =="
# Configuration A: nothing but what the machine has by itself.
images
run_kernel "$TMPD/k0.mb" "nokbd nosched noproc nofs noring3" "$TMPD/a.txt"
rc=$?
A="$TMPD/a.txt"
[ "$rc" -eq 21 ] && ok "plain machine: the kernel gets to the end (exit 21)" \
                 || bad "plain machine: exit code $rc, expected 21"
# What every QEMU `pc` machine has, and what the kernel therefore has to
# find without being told: the host bridge, the ISA bridge and the IDE
# controller of the PIIX3.
has "$A" "pci: 00:00.0 8086:1237 class=06:00:00 host-bridge" "the host bridge, read out of the configuration space"
has "$A" "pci: 00:01.0 8086:7000 class=06:01:00 isa-bridge" "the ISA bridge (PIIX3)"
has "$A" "class=01:01:80 ide" "the IDE controller -- the disk of round 62 has a place on the bus"
n=$(value "$A" 'pci: devices=[0-9]+')
num "devices found on the plain machine" "$n" ge 5
# AND THE OTHER DIRECTION: what QEMU was NOT started with must not be in
# the list. Without this line the whole section would pass with a kernel
# that prints a list it was born with.
hasnot "$A" "class=01:08:02" "counter-check: no NVMe controller was given, so none is listed"
hasnot "$A" "class=01:06:01" "counter-check: no AHCI controller was given, so none is listed"

# Configuration B: the same machine plus an NVMe controller.
images
run_kernel "$TMPD/k0.mb" "nokbd nosched noproc nofs noring3" "$TMPD/b.txt" \
    "${NVME_ARGS[@]}"
B="$TMPD/b.txt"
has "$B" "1b36:0010 class=01:08:02 nvme" "'-device nvme' shows up as class 01:08:02 with the right vendor"
nb=$(value "$B" 'pci: devices=[0-9]+')
num "one device more than on the plain machine" "$nb" eq $((n + 1))
# The BAR is not read out of a table either: its SIZE was determined by
# writing all ones into the register and reading back which bits stayed.
grep -qE 'class=01:08:02 nvme  bar0=0x[0-9a-f]+/0x4000' "$B" \
    && ok "the BAR of the NVMe controller is 16 KiB, determined by sizing" \
    || { bad "wrong BAR size"; grep 'nvme' "$B" | sed 's/^/        /'; }
grep -qE 'class=01:08:02 nvme .*  msix' "$B" \
    && ok "the capability list says MSI-X (that is why this controller was chosen)" \
    || bad "no MSI-X capability found on the NVMe controller"

# Configuration C: and an AHCI controller on top. This is the run the
# choice of driver was made from -- both controllers, side by side.
images
run_kernel "$TMPD/k0.mb" "nokbd nosched noproc nofs noring3" "$TMPD/c.txt" \
    "${NVME_ARGS[@]}" "${AHCI_ARGS[@]}"
C="$TMPD/c.txt"
has "$C" "class=01:08:02 nvme" "with both controllers: the NVMe one is found"
has "$C" "8086:2922 class=01:06:01 ahci" "with both controllers: the AHCI one is found"
# The measured difference the decision rests on: AHCI still carries a
# PORT bar, NVMe does not.
grep -qE 'class=01:06:01 ahci .*bar4=io0x' "$C" \
    && ok "AHCI still decodes a port range (bar4) -- the legacy this round leaves" \
    || { bad "no port BAR on the AHCI controller"; grep 'ahci' "$C" | sed 's/^/        /'; }
grep -qE 'class=01:08:02 nvme  bar0=0x[0-9a-f]+/0x4000  irq' "$C" \
    && ok "NVMe has exactly one BAR and no port range" \
    || bad "the NVMe controller decodes more than one BAR"
nc=$(value "$C" 'pci: devices=[0-9]+')
num "two devices more than on the plain machine" "$nc" eq $((n + 2))

echo "== 3. counter-check: without the scan the kernel knows nothing =="
images
run_kernel "$TMPD/k0.mb" "nopci nvme nokbd nosched noproc nofs noring3" \
    "$TMPD/nopci.txt" "${NVME_ARGS[@]}"
rc=$?
N="$TMPD/nopci.txt"
[ "$rc" -eq 21 ] && ok "without the scan the kernel still gets to the end" \
                 || bad "nopci: exit code $rc, expected 21"
has "$N" "pci: skipped" "the scan was really left out"
hasnot "$N" "pci: devices=" "no list without a scan"
# And the driver cannot find its controller either -- the disk is found
# THROUGH the bus, not at a fixed address.
has "$N" "nvme: no device" "counter-check: without the scan there is no NVMe controller for the driver"

# =====================================================================
# 2. APIC
# =====================================================================

echo "== 4. the local APIC is the clock now =="
has "$A" "apic: id=0" "the local APIC answered with its identity"
hz=$(grep -oE 'apic: id=[0-9]+  hz=[0-9]+' "$A" | grep -oE 'hz=[0-9]+' | cut -d= -f2)
# The frequency is MEASURED against the PIT, not assumed. Under QEMU the
# local timer counts the virtual clock divided by 16; on real hardware it
# is the bus clock. The window checked here is wide on purpose -- what is
# checked is that a number was measured at all and is not nonsense.
num "the measured frequency of the local timer" "$hz" ge 1000000
num "the measured frequency of the local timer (upper bound)" "$hz" le 20000000000
ent=$(grep -oE 'ioapic=[0-9]+' "$A" | head -1 | cut -d= -f2)
num "redirection entries the I/O APIC reports" "$ent" ge 16
has "$A" "apic: keyboard gsi 1 ->33" "the keyboard line was entered into the I/O APIC"
# How many 2 MiB blocks of device memory had to be aliased into the first
# gigabyte. Two before the disk driver runs: the local APIC and the I/O
# APIC. A zero here would mean the registers were reached through a
# mapping that does not exist.
num "device memory blocks aliased into the shared page directory" \
    "$(value "$A" 'window=[0-9]+')" ge 2
# The timer test of round 59 measures the same thing it always did -- only
# the clock behind it is a different one.
ticks=$(value "$A" 'ticks: [0-9]+')
num "timer interrupts on the local APIC timer" "$ticks" ge 20
spin=$(value "$A" 'spin: ticks=\+[0-9]+')
num "ticks during the spin loop" "$spin" ge 1

echo "== 5. counter-check: the same clock, switched off =="
images
run_kernel "$TMPD/k0.mb" "notimer nokbd noring3 nosched noproc nofs" "$TMPD/notimer.txt"
rc=$?
[ "$rc" -eq 21 ] && ok "with the local timer masked the kernel gets to the end" \
                 || bad "notimer: exit code $rc, expected 21"
has "$TMPD/notimer.txt" "spin: ticks=+0" "counter-check: with the local timer masked the spin loop counts 0 ticks"

echo "== 6. counter-check: the old controller keeps the machine =="
images
run_kernel "$TMPD/k0.mb" "noapic nokbd nosched noproc nofs noring3" "$TMPD/noapic.txt"
rc=$?
P="$TMPD/noapic.txt"
[ "$rc" -eq 21 ] && ok "with 'noapic' the kernel gets to the end" \
                 || bad "noapic: exit code $rc, expected 21"
has "$P" "apic: off, pic keeps" "the switch really was left out"
hasnot "$P" "apic: id=" "no local APIC was set up"
ticks=$(value "$P" 'ticks: [0-9]+')
num "on the PIT the ticks arrive as they did in round 62" "$ticks" ge 20
spin=$(value "$P" 'spin: ticks=\+[0-9]+')
num "ticks during the spin loop, on the PIT" "$spin" ge 1

echo "== 7. the keyboard through the I/O APIC, and without an entry =="
# The same measurement as section 7 of tools/kernel/run.sh -- four keys
# through the QEMU monitor -- but now the line runs through the I/O APIC
# and the two 8259s are masked. And then once more with the entry left
# out: the PIC is mute, the line goes nowhere, and nothing may arrive.
keyboard_run() { # $1 = command line, $2 = output file
    rm -f "$TMPD/mon.sock" "$2" "$TMPD/kbd.rc"
    ( timeout 120 qemu-system-x86_64 -kernel "$TMPD/k0.mb" -m 128 \
        -append "$1" -serial "file:$2" -display none -no-reboot \
        -monitor "unix:$TMPD/mon.sock,server,nowait" \
        -device isa-debug-exit,iobase=0xf4,iosize=0x04 >/dev/null 2>&1
      echo $? > "$TMPD/kbd.rc" ) &
    local qemu_pid=$!
    local i
    for i in $(seq 1 200); do
        [ -f "$2" ] && grep -q "kbd: ready" "$2" && break
        sleep 0.2
    done
    if [ -S "$TMPD/mon.sock" ] && grep -q "kbd: ready" "$2" 2>/dev/null; then
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
    fi
    wait $qemu_pid 2>/dev/null
    cat "$TMPD/kbd.rc" 2>/dev/null || echo 99
}

images
rc=$(keyboard_run "noring3 nosched noproc nofs" "$TMPD/kbd.txt")
[ "$rc" -eq 21 ] && ok "keyboard run: the kernel shut down on its own (exit 21)" \
                 || bad "keyboard run: exit code $rc, expected 21"
has "$TMPD/kbd.txt" "kbd: firn" "IRQ1 through the I/O APIC: the four keys arrived as 'firn'"
n=$(grep -c '^key: ' "$TMPD/kbd.txt" 2>/dev/null)
[ "$n" -eq 4 ] && ok "IRQ1 through the I/O APIC: exactly 4 key events" \
               || bad "IRQ1: $n key events, expected 4"

images
rc=$(keyboard_run "noioapic noring3 nosched noproc nofs" "$TMPD/nokey.txt")
[ "$rc" -eq 21 ] && ok "counter-check run: the kernel gets to the end anyway" \
                 || bad "noioapic: exit code $rc, expected 21"
has "$TMPD/nokey.txt" "apic: keyboard NOT routed" "the entry was really left out"
n=$(grep -c '^key: ' "$TMPD/nokey.txt" 2>/dev/null)
[ "$n" -eq 0 ] && ok "COUNTER-CHECK: PIC masked and no I/O APIC entry -- not one key arrives" \
               || bad "noioapic: $n key events arrived, expected 0"
has "$TMPD/nokey.txt" "kbd: (none)" "the kernel says it saw nothing"

# =====================================================================
# 3. NVMe over DMA
# =====================================================================

echo "== 8. the disk that fetches its own commands =="
images
run_kernel "$TMPD/k0.mb" "nvme nokbd nosched noproc nofs noring3" "$TMPD/nv.txt" \
    "${NVME_ARGS[@]}" "${ATA_ARGS[@]}"
rc=$?
V="$TMPD/nv.txt"
[ "$rc" -eq 21 ] && ok "NVMe run: the kernel gets to the end (exit 21)" \
                 || bad "NVMe run: exit code $rc, expected 21"
# The size of the namespace is not a constant in the kernel: it comes out
# of the controller's answer to `identify`, which is itself a transfer by
# DMA. 8 MiB image / 512 = 16384 blocks.
has "$V" "nvme: blocks=16384  lbasz=512" "identify: 16384 blocks of 512 octets, read out of the controller"
has "$V" "irq=msix" "the completion is signalled by message (MSI-X)"
has "$V" "master=1" "the bus master bit is set"
# THE POINT OF `blk.fi`: the same file system, one layer lower, without a
# line of `fs.fi` changing.
has "$V" "nvme: format 1  mount=1" "the same file system formats and mounts on the NVMe disk"
has "$V" "nvme: wrote=30  read=30  same=1" "written over DMA, read back over DMA, identical"
grep -q '^nvme: list .*nvme.txt:1' "$V" \
    && ok "the directory of the NVMe disk lists the file" \
    || { bad "nvme.txt is missing in the listing"; grep '^nvme: list' "$V" | sed 's/^/        /'; }
# AND THE PROOF THAT THE CONTROLLER REALLY MOVED IT: the image on the
# host. Nothing in the kernel ever copied these octets -- the processor
# wrote a 64-octet command and rang a doorbell.
if grep -qa "nvme wrote this line over DMA" "$TMPD/nv.img"; then
    ok "the image on the host contains the line the controller fetched itself"
else
    bad "the image on the host does not contain the text"
fi
grep -qa "26SFNRIF" "$TMPD/nv.img" \
    && ok "the image on the host starts with the magic number of the file system" \
    || bad "no magic number in the NVMe image"
grep -qa "nvme.txt" "$TMPD/nv.img" \
    && ok "the name of the file stands in a directory block of the image" \
    || bad "the file name is not in the image"

echo "== 9. COUNTER-CHECK: no bus master bit, no DMA =="
# One bit in the command register of the configuration space. With it the
# controller may start transfers of its own; without it it may answer
# registers and nothing else. If this run succeeded, the run above would
# not have proved that anything was fetched by the device.
images
run_kernel "$TMPD/k0.mb" "nvme nobm nokbd nosched noproc nofs noring3" \
    "$TMPD/nobm.txt" "${NVME_ARGS[@]}"
rc=$?
M="$TMPD/nobm.txt"
[ "$rc" -eq 21 ] && ok "without the master bit the kernel gets to the end instead of hanging" \
                 || bad "nobm: exit code $rc, expected 21"
has "$M" "master=0" "the bus master bit really is off"
has "$M" "nvme: format 0  mount=0" "COUNTER-CHECK: the file system cannot be written without the master bit"
has "$M" "nvme: wrote=0  read=0  same=0" "COUNTER-CHECK: not one block goes through"
if grep -qa "nvme wrote this line over DMA" "$TMPD/nv.img"; then
    bad "COUNTER-CHECK failed: the text is in the image although DMA was forbidden"
else
    ok "COUNTER-CHECK: the image on the host stays empty"
fi

echo "== 10. COUNTER-CHECK: the transfer without the interrupt =="
# The message vector is masked. The controller still fetches the command
# and still moves the data -- only nobody says so. Both halves are
# measured in the SAME run: the read that waits has to fail, the
# completion has to be lying in the queue afterwards all the same, and
# the read that looks for itself has to succeed.
images
run_kernel "$TMPD/k0.mb" "nvme noirq nokbd nosched noproc nofs noring3" \
    "$TMPD/noirq.txt" "${NVME_ARGS[@]}"
rc=$?
I="$TMPD/noirq.txt"
[ "$rc" -eq 21 ] && ok "with the vector masked the kernel gets to the end" \
                 || bad "noirq: exit code $rc, expected 21"
has "$I" "irq=msix masked" "the message vector really is masked"
has "$I" "nvme: noirq  onirq=0" "COUNTER-CHECK: the read that waits for the interrupt fails"
found=$(value "$I" 'found=[0-9]+')
num "COUNTER-CHECK: completions that were lying in the queue unannounced" "$found" ge 1
has "$I" "polled=1" "the same read succeeds the moment the driver looks instead of waiting"
irqs=$(value "$I" 'irqs=[0-9]+')
num "COUNTER-CHECK: interrupts in the whole run" "$irqs" eq 0

echo "== 11. the same completion over the interrupt pin =="
# The other path: no message, but the pin of the device through a
# redirection entry of the I/O APIC. Same vector, same handler -- which
# is what makes the two comparable at all.
images
run_kernel "$TMPD/k0.mb" "nvme nomsix nokbd nosched noproc nofs noring3" \
    "$TMPD/intx.txt" "${NVME_ARGS[@]}"
rc=$?
X="$TMPD/intx.txt"
[ "$rc" -eq 21 ] && ok "over the interrupt pin the kernel gets to the end" \
                 || bad "nomsix: exit code $rc, expected 21"
has "$X" "irq=intx" "the driver used the pin and not the message"
has "$X" "nvme: format 1  mount=1" "the file system works over the pin as well"
has "$X" "nvme: wrote=30  read=30  same=1" "the same 30 octets, over the pin"
irqs=$(value "$X" 'irqs=[0-9]+')
num "completion interrupts over pin and I/O APIC" "$irqs" ge 1

# =====================================================================
# 4. THE NUMBER
# =====================================================================

echo "== 12. ATA PIO against DMA, the same data over the same interface =="
images
run_kernel "$TMPD/k0.mb" "nvme bench nokbd nosched noproc nofs noring3" \
    "$TMPD/bench.txt" "${NVME_ARGS[@]}" "${ATA_ARGS[@]}"
rc=$?
BM="$TMPD/bench.txt"
[ "$rc" -eq 21 ] && ok "the measurement run gets to the end" \
                 || bad "bench: exit code $rc, expected 21"
# The cycle counter is worth nothing until it is held against a clock
# whose frequency is written down. It was calibrated against the PIT in
# the same window as the local timer.
tsc=$(value "$BM" 'bench: tsc=[0-9]+')
num "the calibrated frequency of the cycle counter" "$tsc" ge 100000000
ata_c=$(grep -m1 '^bench: ata ' "$BM" | grep -oE 'cycles=[0-9]+' | cut -d= -f2)
nv_c=$(grep -m1 '^bench: nvme ' "$BM" | grep -oE 'cycles=[0-9]+' | cut -d= -f2)
m_c=$(grep -m1 '^bench: nvme16 ' "$BM" | grep -oE 'cycles=[0-9]+' | cut -d= -f2)
num "blocks read over ATA PIO" \
    "$(grep -m1 '^bench: ata ' "$BM" | grep -oE 'ok=[0-9]+' | cut -d= -f2)" eq 256
num "blocks read over NVMe, one command each" \
    "$(grep -m1 '^bench: nvme ' "$BM" | grep -oE 'ok=[0-9]+' | cut -d= -f2)" eq 256
num "blocks read over NVMe, sixteen per command" \
    "$(grep -m1 '^bench: nvme16 ' "$BM" | grep -oE 'ok=[0-9]+' | cut -d= -f2)" eq 256
# The two structural numbers. ATA PIO moves every word through the
# processor -- 256 `in ax, dx` per block of 512 octets, by construction
# (`blk.ata_read`). The DMA path moves none.
num "words ATA PIO carried through the processor" \
    "$(grep -m1 '^bench: ata ' "$BM" | grep -oE 'inwords=[0-9]+' | cut -d= -f2)" eq 65536
num "words the DMA path carried through the processor" \
    "$(grep -m1 '^bench: nvme ' "$BM" | grep -oE 'inwords=[0-9]+' | cut -d= -f2)" eq 0
# And the number that justifies the round.
if [ -n "$ata_c" ] && [ -n "$nv_c" ] && [ "$nv_c" -gt 0 ]; then
    num "DMA against PIO, same interface, in thousandths" \
        "$((ata_c * 1000 / nv_c))" ge 1200
else
    bad "no cycle counts in the measurement"
fi
if [ -n "$m_c" ] && [ -n "$ata_c" ] && [ "$m_c" -gt 0 ]; then
    num "DMA with sixteen blocks per command, in thousandths" \
        "$((ata_c * 1000 / m_c))" ge 5000
fi
# The processor gave the machine up while the controller worked. Over PIO
# that number is zero and cannot be anything else.
num "times the processor halted while the controller worked" \
    "$(grep -m1 '^bench: nvme ' "$BM" | grep -oE 'halts=[0-9]+' | cut -d= -f2)" ge 1
num "times it halted during the PIO run" \
    "$(grep -m1 '^bench: ata ' "$BM" | grep -oE 'halts=[0-9]+' | cut -d= -f2)" eq 0
echo "        --- the measurement itself ---"
grep '^bench: ' "$BM" | sed 's/^/        /'

# =====================================================================
# 5. and the same kernel out of the compiler written in Firn
# =====================================================================

echo "== 13. the same run out of firnc1 =="
if [ -f "$TMPD/k1.mb" ]; then
    images
    run_kernel "$TMPD/k1.mb" "nvme nokbd nosched noproc nofs noring3" \
        "$TMPD/nv1.txt" "${NVME_ARGS[@]}"
    rc=$?
    V1="$TMPD/nv1.txt"
    [ "$rc" -eq 21 ] && ok "firnc1: the kernel gets to the end" \
                     || bad "firnc1: exit code $rc, expected 21"
    has "$V1" "class=01:08:02 nvme" "firnc1: the same device list"
    has "$V1" "apic: id=0" "firnc1: the same local APIC"
    has "$V1" "nvme: blocks=16384  lbasz=512" "firnc1: the same controller answer"
    has "$V1" "nvme: wrote=30  read=30  same=1" "firnc1: the same transfer over DMA"
    # Apart from addresses and measured numbers the two compilers have to
    # say the same thing. The frequency is a measurement and differs from
    # run to run; the shape must not.
    norm() {
        sed -E 's/0x[0-9a-f]+/0xX/g; s/[0-9]+/N/g' "$1" \
            | grep -vE '^(trace|task|sched|proc|fs|sh|ata|bench):'
    }
    norm "$V" > "$TMPD/n0.txt"
    norm "$V1" > "$TMPD/n1.txt"
    if diff -q "$TMPD/n0.txt" "$TMPD/n1.txt" >/dev/null; then
        ok "both compilers produce the same kernel output apart from numbers"
    else
        bad "the two kernels behave differently"
        diff "$TMPD/n0.txt" "$TMPD/n1.txt" | head -10 | sed 's/^/        /'
    fi
fi

echo
echo "PCI: $pass passed, $fail failed"
[ "$fail" -eq 0 ] || exit 1
exit 0
