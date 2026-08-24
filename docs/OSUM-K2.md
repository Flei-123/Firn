# Round K2 — the kernel stops guessing what machine it is on

Round 59 gave the kernel its own interrupt table, a timer, frames, a heap,
a keyboard and one excursion into ring 3. Round 62 turned that into an
operating system: tasks, address spaces, system calls, a file system, a
shell. Both rounds are green and neither of them ever asked the machine a
single question.

Everything the kernel spoke to it knew by heart. `0x3F8` is the serial
port. `0x60` is the keyboard. `0x1F0` is the disk. The interrupt
controller is the pair of 8259s, remapped to vectors 0x20 and 0x28. Those
numbers are true, and they are true because they were true in 1986 and
nobody has dared change them since. What they are not is *information
about this machine*. A kernel built out of them runs on the machine of
1995 and on nothing else, and it finds out about a device only by already
knowing it is there.

This round replaces three of those certainties with questions:

1. **PCI** — walk the configuration space and print what is actually
   plugged in, with the addresses the devices actually decode
   (`demos/kernel/pci.fi`, 591 lines).
2. **APIC** — the local APIC's own timer instead of the PIT, the I/O APIC
   instead of the two 8259s, and the old chips shut up for good
   (`demos/kernel/apic.fi`, 507 lines).
3. **NVMe over DMA** — a disk the processor does not carry. The kernel
   writes a 64-octet command into a queue in memory, rings a doorbell, and
   the controller does the rest, including saying when it is done
   (`demos/kernel/nvme.fi`, 699 lines).

And one number that says whether it was worth it.

The guard is `tools/pci/run.sh` (96 checks, section 53 of `test.sh`).
`tools/kernel/run.sh` keeps measuring what it measured — 174 checks, and
the three workers of round 62 still interleave, only on a different clock.

---

## 1. PCI: the machine answers

### What was built

`pci.fi` reaches the configuration space through the port pair
`0xCF8`/`0xCFC`, walks bus 0, follows every PCI-to-PCI bridge into its
secondary bus, and records for each function: vendor, device, class,
subclass, programming interface, header type, interrupt pin and line, the
offsets of the MSI and MSI-X capabilities, and all six base address
registers with **their size**.

The size is the part that is not a lookup. A BAR does not say how much
room it wants; it only answers when all ones are written into it. The bits
that come back as zero are the ones the device does not decode, and the
lowest bit that stays one is the size. The register has to be put back
afterwards, and the device's decoders have to be **off** while it happens
— for the length of two port accesses the BAR reads as `0xFFFFFFF0`, and a
device that decoded that address would answer for the whole upper end of
memory.

64-bit BARs are handled as one register of two halves, and the second half
is not reported as a BAR of its own.

### What it finds under QEMU

Started as `qemu-system-x86_64 -m 128 -device nvme,drive=d0,serial=k2disk
-device ahci,id=ahci0`:

```
pci: 00:00.0 8086:1237 class=06:00:00 host-bridge
pci: 00:01.0 8086:7000 class=06:01:00 isa-bridge
pci: 00:01.1 8086:7010 class=01:01:80 ide  bar4=io0xc040/0x10
pci: 00:01.3 8086:7113 class=06:80:00 bridge  irq=9
pci: 00:02.0 1234:1111 class=03:00:00 vga  bar0=0xfd000000/0x1000000  bar2=0xfebf4000/0x1000
pci: 00:03.0 8086:100e class=02:00:00 network  bar0=0xfebc0000/0x20000  bar1=io0xc000/0x40  irq=11
pci: 00:04.0 1b36:0010 class=01:08:02 nvme  bar0=0xfebf0000/0x4000  irq=11  msix
pci: 00:05.0 8086:2922 class=01:06:01 ahci  bar4=io0xc040/0x20  bar5=0xfebf5000/0x1000  irq=10  msi
pci: devices=8
```

The IDE controller at `00:01.1` is the disk round 62 drove over ATA PIO.
It was always there; the kernel simply had no way of knowing it.

### How it is checked, and why the counter-check matters

A device list is the easiest thing in an operating system to fake. A
kernel that printed a fixed table would pass any test that only asks
whether the expected line is there. `tools/pci/run.sh` therefore runs the
**same kernel image three times** and holds the output against the
`-device` arguments QEMU was started with, **in both directions**:

| run | QEMU arguments | expectation |
|---|---|---|
| A | none | 6 devices, and **no** line with class `01:08:02` or `01:06:01` |
| B | `-device nvme` | exactly one device more, and it is class `01:08:02`, `1b36:0010`, BAR0 16 KiB |
| C | `+ -device ahci` | exactly two more, the AHCI one is `8086:2922` class `01:06:01` |

Run A is the counter-check. A hard-coded list fails it.

The fourth run switches the scan off entirely (`nopci`): the list is gone,
and the disk driver reports `nvme: no device` — the controller is found
*through* the bus, not at a fixed address.

### What was not done, and why it is written down

**ECAM.** The modern door into the configuration space is a flat memory
window that covers all 256 MiB of it. Its address is not architectural: it
stands in the MCFG table of ACPI, and a kernel that wants ECAM has to
parse ACPI first. QEMU's default `pc` machine (i440FX) **has no MCFG at
all** — the window only exists on `q35`. A kernel that spoke only ECAM
would therefore find nothing on the machine this project tests on. The
ports are not the old way; they are the way that always works, and ECAM is
the optimisation for machines that announce themselves. Implementing it
would mean implementing ACPI, which is a round of its own.

**BAR assignment.** The BARs above were assigned by SeaBIOS, which QEMU
runs before the multiboot image. The kernel *sizes* them and could
reassign them (the code path is one `cfg_write32`), but it does not,
because overwriting a working assignment to prove that it can is how an
operating system breaks a machine it does not fully understand. What it
does set is the bus master bit, and that is the bit that matters — see
section 3.

---

## 2. APIC: a clock inside the processor

### What was built

* The local APIC is enabled through `IA32_APIC_BASE` (MSR 0x1B) and its
  spurious vector register; TPR is set to 0, LINT0/LINT1/ERROR are masked.
* Its **timer** replaces the PIT as the tick source, periodic, on the same
  vector 32 the timer has had since round 59. Nothing above `trap.fi`
  learns that the clock changed — `sched.on_tick` counts what it counted.
* The **I/O APIC** gets a redirection entry for the keyboard line (GSI 1 →
  vector 33, edge, active high) and, when the disk driver uses its
  interrupt pin, one for that too (level, active low — a PCI line that was
  entered as edge would deliver once and never again).
* Both 8259s are masked, and the IMCR of the MP specification is written
  (`0x22`/`0x23`). QEMU has no IMCR and ignores it; a real board of the
  last thirty years does not, and a kernel that skips it gets every
  interrupt twice there.
* End-of-interrupt is one word to the local APIC instead of one or two
  port writes to the 8259s. `trap.fi` picks the right one at run time, so
  the `noapic` path stays exactly what round 62 measured.

### The frequency is measured, not assumed

Nobody knows how fast the local timer counts. On real hardware it is the
bus clock divided by the divisor; under QEMU it is the virtual clock
divided by the divisor. So it is **calibrated** against the one clock whose
frequency is written down in the architecture: the PIT at 1193182 Hz.
Channel 2 is used for it — the channel whose output can be *read* on port
0x61 instead of raising an interrupt, which makes it the one timer a
kernel can use to measure another timer before it has interrupts.

Measured over a 10 ms window, with divisor 16:

```
apic: id=0  hz=62581800  ioapic=24  tick=100
```

62.58 MHz. The expected value under QEMU is exactly 1 GHz / 16 = 62.5 MHz;
the 0.13 % on top is the granularity of the polling loop that watches
port 0x61 (one port read is about 480 ns under TCG). The same window is
used to calibrate the processor's cycle counter, which every measurement
in section 4 is given in.

If the PIT does not answer, `calibrate` returns 0 after about a second,
`init` returns false, and **the PIC keeps the machine**. A kernel that
switches off the working controller before the new one has answered has no
way back.

### The counter-checks

| run | what is switched off | what has to collapse |
|---|---|---|
| `notimer` | the local timer's LVT entry | the spin loop counts **0** ticks (round 59's own counter-check, now against a different clock) |
| `noapic` | the whole switch | the PIT keeps ticking, 21 ticks arrive, the keyboard works — round 62 unchanged |
| `noioapic` | the keyboard's redirection entry | PIC masked **and** no entry: **not one** key event arrives, `kbd: (none)` |

The `noioapic` run is the one that proves the I/O APIC is really carrying
the keyboard. Four keys are sent through the QEMU monitor in both runs; the
routed run reports `kbd: firn` and exactly 4 key events, the unrouted run
reports 0. If the old PIC were still delivering underneath, both runs would
report 4.

### The bug that cost the most, and what it teaches

The first version of the device mapping hung a second page directory into
slot 3 of the PDPT and mapped 3 GiB…4 GiB there. It worked — until the
first interrupt arrived while a **process** was running:

```
*** EXCEPTION 14 #PF  err=0x2  cr2=0xfee000b0
  rip=0x121ca6  cs=0x8  rflags=0x46
```

`0xfee000b0` is the end-of-interrupt register of the local APIC. A process
has an address space of its own (`proc.space_build`) and inherits exactly
**one** thing from the kernel: entry 0 of the kernel PDPT, the page
directory of the first gigabyte. Everything else of the kernel's own
tables does not exist down there. The handler wrote its end-of-interrupt
into a page that was not mapped in the address space it happened to be
standing in.

The fix is the one Linux calls a fixmap: device memory is **aliased into
the top of the identity-mapped gigabyte**. The last eight 2 MiB slots of
the kernel page directory (virtual `0x3F000000`…`0x3FFFFFFF`, far above
the 128 MiB this machine has) are repointed at the physical addresses of
the devices, uncached. That directory is the one thing every address space
shares, so the mapping is valid in all of them for free.

The general lesson is not about the APIC. It is that in a kernel with
per-process address spaces, **anything an interrupt handler touches has to
live in the part of the tables that every address space has**, and the
compiler will not tell you when it does not.

---

## 3. NVMe: a disk the processor does not carry

### Why NVMe and not AHCI — decided by measuring, not by taste

Both controllers were enumerated first, side by side, before a line of
driver was written (run C above). Three differences decided it, and all
three are readable in that output:

* **AHCI still carries a port BAR** (`bar4=io0xc040/0x20`). It is an IDE
  controller that also speaks SATA, and driving it means keeping one foot
  in exactly the legacy this round is trying to leave. NVMe has one memory
  BAR and no port range at all.
* **AHCI wraps the ATA command set** that `blk.fi` already speaks over
  PIO. An AHCI driver would prove that DMA beats PIO for the *same*
  protocol. NVMe is a different protocol, so the comparison in section 4 is
  between two real designs rather than between two ways of issuing
  `READ DMA EXT`.
* **The capability lists differ.** The NVMe controller offers MSI-X, the
  AHCI controller only MSI. Item 4 of this round (message signalled
  interrupts) is therefore measurable on the one and not on the other.

`tools/pci/run.sh` checks all three of those facts, so the reason for the
choice is part of the test suite and not part of a paragraph.

### What was built

Admin queue and one I/O queue pair, 64 entries each, in the region
`boot.s` hands over. `identify` for namespace 1 gives the size and the
block size — 16384 blocks of 512 octets for an 8 MiB image, read out of
the controller rather than assumed. Then create-CQ, create-SQ, and from
there: build a 64-octet command, write the physical address of the buffer
into PRP1 (and PRP2 when the 512 octets cross a page boundary), ring the
doorbell, and wait.

`blk.fi` gets it as device 2. `fs.fi` did not change by a line — the same
`format`, `mount`, `create`, `write_at`, `read_at` that ran on the RAM
disk and on ATA now run on a controller that fetches its own commands:

```
nvme: blocks=16384  lbasz=512  irq=msix  master=1
nvme: format 1  mount=1
nvme: wrote=30  read=30  same=1  irqs=5  waits=0
nvme: list .:2 ..:2 nvme.txt:1
```

And afterwards the **image on the host** contains the magic number of
OFS (`OSUM-OFS`, little-endian `SFO-MUSO` in the raw image), the
directory entry `nvme.txt` and the line the kernel wrote.
Nothing in the kernel ever copied those octets.

### The three counter-checks

**No bus master bit** (`nobm`). One bit in the command register of the
configuration space decides whether a device may start a transfer of its
own — and every DMA is a transfer of the device's own. With it off:

```
nvme: blocks=16384  lbasz=512  irq=msix  master=0
nvme: format 0  mount=0
nvme: wrote=0  read=0  same=0
```

and the image on the host stays empty. The kernel reaches its end
regardless — a bounded wait, not a hang. Without this run, the run above
would not prove that anything was fetched by the device.

**No interrupt** (`noirq`). The MSI-X vector is left masked. The
controller still fetches the command and still moves the data; only nobody
says so. Both halves are separated **in the same run**:

```
nvme: noirq  onirq=0  found=1  polled=1  irqs=0
```

The read that waits for the interrupt fails after three seconds
(`onirq=0`). The completion is lying in the queue all the same
(`found=1`). The same read succeeds the moment the driver looks instead of
waiting (`polled=1`). Not one interrupt arrived in the whole run
(`irqs=0`). That is the cleanest available separation of "the DMA worked"
from "the notification worked".

**The other interrupt path** (`nomsix`). The same completion over the
device's interrupt pin through a redirection entry of the I/O APIC, on the
same vector and into the same handler. The whole file system runs over it.

### Two bugs worth writing down

**A vector armed too late is a vector nobody knows about.** The first
version enabled MSI-X *after* creating the queues, so that the polled
admin phase could not race with the interrupt handler. Every command
completed, every completion entry stood in the queue with its phase bit
set — and not one interrupt ever arrived. A queue remembers which vector
it reports on at the moment it is created: the admin queue when `CC.EN`
goes up, the I/O queue when the create command is answered. QEMU makes
this explicit (`msix_vector_use` is called from `nvme_init_cq` only if
MSI-X is already enabled), and the NVMe specification implies it. The
interrupt is now armed before the controller is enabled.

**A driver that waits has to be allowed to wait.** `fs.fi` was written for
a device that could do nothing but spin, and it holds interrupts **off**
across a whole block operation (`sched.irq_save`, six places). A driver
that halts inside that critical section waits for an interrupt that cannot
arrive; `fs.format` stood still for as long as it was given. The answer is
the one every kernel gives: when the caller cannot take an interrupt, the
driver **polls**. `nvme.interruptible()` reads the flags register and picks
the path. The data still moves by DMA either way — only the completion is
discovered by looking instead of by being told, and the difference is
visible in the `irqs=` and `halts=` counters.

This is why the file system run above shows `waits=0`: it ran polled,
because `fs.fi` asked for it. The measurement below runs with interrupts
on and halts 86 times.

---

## 4. The number

The same amount of data, over the **same interface** — `blk.read`, block
by block, 512 octets — once through the processor and once past it. Then
once more with sixteen blocks in one command, which is what the per-block
interface costs.

QEMU 7.2 (TCG, no KVM), `-m 128`, host cycle counter calibrated against
the PIT at 2.190 GHz. 256 blocks = 128 KiB, the configuration
`tools/pci/run.sh` reproduces:

| path | cycles | µs | KiB/s | words through the CPU | halts |
|---|---:|---:|---:|---:|---:|
| ATA PIO, 1 block per command | 61 740 756 | 28 186 | 4 541 | 65 536 | 0 |
| NVMe DMA, 1 block per command | 43 227 800 | 19 734 | 6 485 | **0** | 86 |
| NVMe DMA, 16 blocks per command | 2 870 802 | 1 310 | 97 663 | **0** | 0 |

**1.43× for the same interface, 21.5× when the interface stops asking for
one block at a time.**

The same measurement over 512 KiB (1024 blocks), one run:

| path | cycles | µs | KiB/s |
|---|---:|---:|---:|
| ATA PIO | 342 602 370 | 156 601 | 3 269 |
| NVMe DMA, 1 block per command | 206 024 720 | 94 172 | 5 436 |
| NVMe DMA, 16 blocks per command | 12 824 394 | 5 861 | 87 342 |

1.66× and 26.7×. And over the interrupt pin instead of the message, same
size: 1.86× and 30.1× — the path the completion takes is not what makes
the difference.

### What the numbers mean, and what they do not

**"Words through the CPU" is the honest column.** ATA PIO executes 256
`in ax, dx` per block of 512 octets, by construction (`blk.ata_read`);
65 536 of them for 128 KiB. The DMA path executes **none**. That number is
structural, not measured, and it does not change with the host, the
emulator or the year.

**"Halts" is measured.** 86 times in the 256-block run the processor
executed `hlt` and gave the machine up while the controller worked. Over
PIO that number is 0 and cannot be anything else — the processor *is* the
transfer. This is the part of "CPU load" that a cycle count cannot show:
the PIO cycles are cycles nobody else can have, and the DMA cycles are
mostly cycles spent halted.

**The factor is a QEMU factor.** Under TCG, `in ax, dx` costs an
instruction-level exit into the emulator, and a DMA transfer costs a
`memcpy` in the host. The ratio on real hardware is different — larger for
the per-block case, since real PIO waits on a real bus. What transfers to
real hardware without an asterisk is the structural column and the shape:
per-block DMA is barely better than per-block PIO because **the per-block
interface, not the transfer, is the cost**; the 16-block command is 21×
because one command moves 8 KiB.

That last point is the actionable result of this round. `blk.fi`'s
interface — "block n, 512 octets" — was the right interface for a device
that could not do better. It is now the bottleneck. A `read_blocks(lba, n,
dst)` in `blk.fi` and a `fs.fi` that asks for extents instead of blocks
would collect the 21× without touching the driver.

---

## 5. What the language could not do, said plainly

Two gaps showed up, and neither was worked around silently.

**There is no volatile field access.** A submission queue entry is 64
octets with fields at fixed offsets; a completion entry is 16. Firn
*does* define struct layout (SPEC §13: declaration order, natural
alignment, no reordering), so a `struct` could describe both exactly. But
the volatile promise of round 52 hangs on the intrinsics
`__mmio_read32`/`__mmio_write32`, which take an **address**, not on `a.b`.
A queue entry that the controller reads while the kernel writes it is
precisely the case in which the optimizer must not merge, move or drop a
store — and a plain field store carries no such promise. `nvme.fi`
therefore computes offsets by hand, through named constants, and every
access to shared memory goes through an intrinsic. What is missing is a
way to say "this struct lives at this address and every access to it is
volatile".

**`#[align(n)]` is registered but not implemented**
(`compiler/src/attrs.rs`, `implemented: false`). An NVMe queue has to start
on a 4096-octet boundary. The language cannot say that. Here the alignment
comes from the region `boot.s` hands over, which the linker script aligns
to 4 KiB — true, but true by an accident of the layout rather than by a
promise of the language. On a kernel that allocated its queues anywhere
else, this would be a silent corruption waiting for the first unaligned
page.

**No memory fence intrinsic.** The doorbell must become visible after the
queue entry. On x86 that is free — stores are not reordered against
stores, and both go through the volatile intrinsics, which the compiler
may not move past one another. On aarch64 (which this project already
targets, round 80) it would not be free, and there is no way to write the
`dmb` from Firn without an inline `asm` block. Not needed here; written
down because it will be.

---

## 6. A finding that belongs to nobody's file

The command line of the boot loader lies **above `kernel_end`** — in this
build at `0x184000`, with `kernel_end` at `0x183038`. `mem.scan` reserves
the kernel image up to `kernel_end`; everything above is free memory as
far as the frame allocator is concerned. The first thing the kernel asks
for is 64 frames for the heap, and it gets `0x184000`. From that moment
the command line reads as a heap block header.

Round 62 never noticed, because it read the command line in the first
twenty lines of `kernel_main`. This round wanted to read it again further
down and found a string that had turned into a number. The workaround is
in `hw.parse`, which is called before the frame allocator exists.

The proper fix is to reserve the multiboot structures in `mem.scan`, and
it is deliberately **not** made here: it moves every frame address in the
kernel and would change the numbers three other rounds are currently
measuring against. It belongs to whoever owns `mem.fi` next.

---

## 7. Files, and what is open

| file | lines | what |
|---|---:|---|
| `demos/kernel/pci.fi` | 591 | configuration space, bus walk, BAR sizing, capabilities, bus master |
| `demos/kernel/apic.fi` | 507 | local APIC, timer calibration, I/O APIC, device window, PIC shutdown |
| `demos/kernel/nvme.fi` | 699 | queues, PRP, identify, MSI-X and pin, the block interface |
| `demos/kernel/hw.fi` | 548 | the seam into `kmain.fi`, the file system on NVMe, the measurement |
| `demos/kernel/blk.fi` | +30 | the third device — dispatch, nothing else |
| `demos/kernel/trap.fi` | +30 | which controller gets the end-of-interrupt, two more vectors |
| `demos/kernel/kmain.fi` | +14 | three calls |
| `tools/pci/run.sh` | 546 | 96 checks, section 53 of `test.sh` |

`fs.fi` changed by zero lines. That was the point of `blk.fi`.

**Open, in the order it matters:**

1. **A multi-block block interface.** The 21× above is lying there
   untouched. `blk.read_many(lba, n, dst)` and an `fs.fi` that asks for
   extents.
2. **PRP lists.** The driver handles transfers up to two pages (8 KiB),
   because two PRP entries are all a command carries directly. Anything
   longer needs a PRP list — a page of physical addresses — and that is
   what stands between 8 KiB and a real read-ahead.
3. **More than one queue pair.** One I/O queue means one outstanding
   command. NVMe's whole design is deep queues; this driver waits for
   every command before submitting the next, which is why `halts` is
   roughly equal to the number of blocks.
4. **ACPI, and with it ECAM, the real interrupt routing and more than one
   processor.** The I/O APIC entry for the keyboard is correct because ISA
   IRQ 1 is GSI 1 on every machine; PCI lines are read out of the
   configuration space, which is right under QEMU and only mostly right in
   general. Interrupt source overrides live in ACPI, and so does the second
   processor the local APIC exists for.
5. **Reserve the multiboot structures** (section 6).
6. **AHCI.** Not built, and the reason is written down in section 3
   rather than left as an implication.
