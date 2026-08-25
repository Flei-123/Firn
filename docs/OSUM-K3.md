# Round K3 — a TCP/IP stack of its own, held against the Linux kernel

Round K2 taught the kernel to ask the machine what it is made of: PCI, the
APIC, a disk it reaches over DMA. One of the answers that scan gives is a
network card. This round writes the part that would have nothing to say
to it — Ethernet, ARP, IPv4, ICMP, UDP and TCP, in Firn, without an
allocator, in 2,646 lines under `lib/net/`.

**No driver was needed for any of it, and that is the point.** A network
stack is a function from octet sequences to octet sequences. It has one
door downwards (a frame in, a frame out) and one upwards (listen,
connect, send, receive, close). Whether the octets on the lower door come
from a virtio-net ring, from a `veth` pair or from an array in the same
process is not a question the stack can ask, and it does not ask it. That
is what made it possible to build this and to *measure* it before the
card exists.

What it is measured against matters more than how much of it there is.
Two ends that this repository wrote agree perfectly on a shared
misunderstanding — that is the failure mode of every self-written
protocol. So the most important half of `tools/k3net/run.sh` puts the
**Linux kernel** on the other side: `ping`, `nc`, `curl` and a python
socket, over a `veth` pair in two network namespaces, with `tc netem`
throwing frames away.

| | |
|---|---|
| library | `lib/net/wire.fi` 492, `lib/net/tcp.fi` 1,555, `lib/net/stack.fi` 599 |
| measurement | `tools/k3net/` — `unit.fi` 1,197, `drv.fi` 544, `link.fi` 154, `kprobe.fi` 70, `run.sh` 437 |
| guard | `tools/k3net/run.sh`, 20 proofs, section 55 of `test.sh` |
| profile | `kernel` — no allocator, no `syscall`, no runtime, checked by the compiler |

---

## 1. What is in it

### 1.1 The three files

`wire.fi` holds no state at all. It reads and writes headers, computes
the one checksum the internet protocols share (RFC 1071) and does the
arithmetic modulo 2^32 that TCP sequence numbers live in. Ethernet, ARP,
IPv4, ICMP, UDP and the TCP header, including the walk over the TCP
option list.

`tcp.fi` is the state machine: all eleven states, the three-way
handshake, a send and a receive window, reassembly of out-of-order
segments, retransmission on a timer measured after Jacobson/Karn, fast
retransmit on three duplicate acknowledgements, delayed acknowledgements,
Nagle, slow start with congestion avoidance, a persist timer for a closed
window, silly-window avoidance on the receive side, and an orderly
teardown in both directions.

`stack.fi` is everything in between: which frame belongs to whom, an ARP
cache that **expires**, the outbox in which a datagram waits while its
Ethernet address is still being asked for, ICMP echo, the UDP sockets,
and the two doors.

### 1.2 The shape of the interface is a PULL

There is no callback and no output queue inside TCP. The layer above says

```
tcp_input(t, ip_datagram, length, now)     -- something arrived
tcp_pull(t, buffer, capacity, now, &dst)   -- give me one datagram
```

and `tcp_pull` decides on its own what is due: a SYN, a retransmission,
data, a delayed acknowledgement that has come of age, a window probe, a
FIN, a reset. That removes the whole question of where an output queue
lives and how big it has to be — the answer is that there is none.

**Time comes in from outside**, in microseconds. A kernel has no
`clock_gettime`; whoever calls this hands over the clock it does have.

### 1.3 No allocator, and the compiler says so rather than the comment

`lib/net/` declares `profile kernel` in its first line, so SPEC 2.1
applies: the modules land in the same compilation unit as whoever imports
them, and every rule is checked on them. `tools/k3net/kprobe.fi` sets a
stack up in memory it was handed, listens, feeds an ARP request in and
takes the answer out again — in the kernel profile. What comes out is
measured, not asserted:

```
firnc0: ELF type REL (a freestanding object file)
firnc0: no undefined name other than osum_panic
firnc0: not one syscall instruction in the machine code
firnc0: the library really is in the image
```

and the same four lines out of `firnc1`, the compiler written in Firn.
177 symbols in the object file.

All the state lives in one struct the caller owns — SPEC 14.1 item 5
leaves no other route, there are no global variables in this language.
`net_size()` says how big it is: **1,125,440 octets**, and almost all of
that is eight send and eight receive buffers of 64 KiB.

---

## 2. Measured against the Linux kernel

`/dev/net/tun` does not exist in the container this repository is
measured in and cannot be created there — `mknod` is refused and the
`tun` module is not in the host's kernel. `AF_PACKET` needs no device
node. `tools/k3net/run.sh` builds a `veth` pair across two network
namespaces: on one side Linux with `10.7.0.1/24`, on the other an
interface **without an address**, which nobody but `tools/k3net/drv.fi`
answers for. Checksum offload is switched off on both ends, otherwise the
kernel hands out frames whose checksum it never computed and a stack that
*checks* the checksum would rightly throw all of them away.

### 2.1 The numbers

| what | result |
|---|---|
| `ping -c 5` from Linux | **5 of 5 answered**, rtt min/avg/max 0.069 / 0.095 / 0.125 ms |
| ARP | Linux asked, the stack answered, the entry went into Linux's neighbour table as `REACHABLE` |
| `nc` pushes 1 MiB in | **1,048,576 octets**, 725 frames, 36 acknowledgements, **58–66 MB/s** |
| `nc` through the echo | **1,048,576 octets there and back, md5 identical**, ~21 MB/s |
| `curl http://10.7.0.2:8080/` | status line, headers and `Content-Length: 40` accepted by curl itself |
| the stack connects **actively** to a python server | 1 MiB out, 1 MiB back, **0 wrong octets**, closed cleanly |
| `tc netem loss 10 %` Linux → stack | all 256 KiB arrive **in order**, out-of-order segments reassembled |
| `tc netem loss 10 %` stack → Linux | all 256 KiB arrive, recovered by fast retransmit and the timer |
| UDP, 1400 octets there and back | returned reversed, checksums intact |

The two throughput numbers differ for a reason worth naming: the first is
a stack that only receives and acknowledges, the second is the same
octets going out again through the same single-threaded loop, so every
frame is handled twice and a `read` and a `write` system call stand
between them. Neither number is a claim about how fast Firn is; both are
what came out of `AF_PACKET` on a `veth` pair in a container, and a real
driver would not go through the kernel twice.

### 2.2 The counter-checks

A test that only ever asks whether the expected thing happened passes
with an implementation that always says yes. Every claim above has one
that has to collapse:

* **A wrong checksum has to be dropped.** In the sharp form: a segment
  with four octets of payload and one octet flipped *after* the checksum
  was computed is refused (`rcv_nxt` does not move, the counter goes up
  by one) — and the **same four octets with the sum repaired are taken**.
  Without the second half, a stack that dropped everything would pass.
* **An acknowledgement for octets that were never sent has to be
  refused.** A forged segment acknowledging 100,000 octets beyond what
  this side ever put on the wire is counted and thrown away, the
  connection stays in ESTABLISHED — and the same segment with an honest
  acknowledgement is **not** counted as a forgery.
* **Without retransmission the transfer has to stay incomplete.** The
  same run against the same python server under the same 10 % loss, with
  retransmission switched off: it does not finish. Measured against the
  Linux kernel, not simulated.
* **A port nobody listens on has to refuse.** `nc` to a closed port on
  the Firn stack fails; the stack sent a reset.
* **A broken ICMP echo stays unanswered**, while the intact one is
  answered with the payload identical and a valid checksum.
* **A fragment is refused** — and the same datagram without the More
  Fragments bit is not.

---

## 3. Both ends in one process

`tools/k3net/unit.fi` joins two stacks by a simulated wire
(`tools/k3net/link.fi`) that can lose, reorder and corrupt frames on
purpose. Everything in it is **deterministic**: the generator is the
project's usual one (Knuth's MMIX constants), the seed comes from
outside, and a run with the same seed drops exactly the same frames. A
loss test nobody can run twice is a test nobody can debug.

One detail of that harness turned out to matter more than it looks.
Delivering a whole burst of frames and only then letting the receiver run
is not a faster simulation, it is a **different** one: the duplicate
acknowledgements that fast retransmit lives on all collapse into a single
one. With batch delivery the measurement said fast retransmit never
fires (0 of 96 recoveries); with one frame at a time, as a real card
hands them over, it fires 78 times out of 96. The stack had not changed.

The fifteen cases and what they measured:

```
the handshake, 40 000 octets, the close, TIME_WAIT      got=40000 wrong=0 time_wait=1 tw_freed=1
6 % of the octets flipped: caught, and repaired         200000 octets, tcp sums refused 12, ip sums 9
5 % lost, 3 % reordered: a megaoctet, whole, in order   1048576 octets, 85 lost, 96 retransmits, 170 out of order
COUNTER-CHECK without retransmission: incomplete        20440 of 1048576
the sequence number runs over 2^32 mid transfer         300000 octets, wrong=0
forged segments: false acknowledgement, false checksum  all four sub-checks
ICMP echo answered, and a broken one is not             payload identical, checksum valid
ARP answered and aged out; fragment and bad IP sum      alive at 0.5 s, gone at 2 s
UDP there and back, and a broken checksum refused       800 octets each way
Nagle collects: 1000 single octets in fewer frames      2 frames with, 1000 without
delayed acknowledgement: fewer answers than segments    45 segments in, 22 answers out
a window that closes: the sender waits, probes, goes on 10 probes, 200000 octets, wrong=0
a closed port refuses, an open one answers              resets sent 1
200 000 random frames: nothing falls over               22638 accepted, 27537 refused
all eleven states were entered, not just described      bitset 2047 of 2047
```

The last line is the one that keeps this document honest. A state machine
with eleven states in a table is a claim; **2047** is a measurement. Every
`state = X` in `tcp.fi` goes through one function that sets a bit, and
the run has to end with all eleven of them set — including CLOSING, which
only a simultaneous close reaches, and LAST_ACK, which only the passive
side reaches.

---

## 4. Five real bugs, and what found them

None of these were found by reading the code.

**1. `snd_end` started one sequence number before the data space.** The
SYN occupies `iss`; data starts at `iss + 1`. Starting `snd_end` at `iss`
made `snd_end - snd_una` wrap to 2^32 − 1 the moment the SYN was
acknowledged, so the send buffer looked permanently full and the
connection carried exactly zero octets. Found by the first case that
tried to send anything: five frames total, then silence.

**2. `snd_nxt` was used where `snd_max` belongs.** On a retransmission
this stack winds `snd_nxt` back to `snd_una` and sends again from there.
That is legitimate — but `snd_nxt` is then no longer "the highest thing
we ever sent", and the RFC 793 test *"if SEG.ACK > SND.NXT this is an
acknowledgement for something never sent"* starts refusing the peer's
perfectly good acknowledgements. The connection died of its own recovery:
`badack=123`, transfer stuck at 33,580 of 1,048,576 octets. BSD calls the
second variable `snd_max` and now so does this.

**3. The FIN was never sent when the send buffer was empty.** This is the
cliff a previous attempt at this round went over and it is worth the
paragraph in the source: `close` set a flag, and the only place that
could put anything on the wire was the loop that sends *data*. With
nothing left to send, that loop produces no segment, so no FIN was built,
the peer never learned the connection was over, the state machine stood
in ESTABLISHED for twenty thousand rounds and TIME_WAIT was never
reached. The measurement said `state B: 4` and was perfectly right. The
branch that fixes it is unconditional in exactly the way the bug was not.

**4. The persist probe did not count as sent.** A window probe sends one
octet beyond the closed window without moving `snd_nxt` — correct, the
probe may be thrown away. But the octet really did go out, so `snd_max`
has to move. Without that, a receiver that *accepts* the probe (the
window opened in the meantime) acknowledges an octet this side believes
it never sent, the acknowledgement is refused as a forgery, the window
update inside it is never read, and both sides wait for each other for
ever. Found by the zero-window case: 64,241 of 200,000 octets, and then
nothing.

**5. A clock that goes backwards aborted the process.** `now` comes in
from outside. `tools/k3net/drv.fi` computed it once, used it, computed it
again inside an inner loop and then handed the *older* value back to the
stack. `now - rtt_at` is then a number close to 2^64, and the next
checked addition took the process down — in the middle of the first
megaoctet against `nc`:

```
panic: integer overflow in 'u64 + u64' at lib/net/tcp.fi:707:25 (a=1521 b=18446744073709550966)
```

Both ends were fixed: the driver never hands over a stale clock, and the
library throws away a sample from a clock that went backwards instead of
believing it. Round 72's checked arithmetic turned what would have been a
quietly absurd round-trip estimate into a stack trace with a line number.

---

## 5. What is NOT built

An honest list is worth more than a claim. None of the following exists
in `lib/net/`, and where something is *recognised* but not *handled* that
is said as such.

**IPv4**

* **No fragment reassembly.** A fragment is recognised (More Fragments,
  or a non-zero offset) and **refused**, counted in `NS_FRAG`. Handing
  half a TCP segment to the state machine would be worse than dropping
  it. Outgoing datagrams always carry Don't Fragment.
* **IP options are skipped, never interpreted.** A header with IHL > 5 is
  accepted and its options stepped over. Source routing is not honoured,
  and it is not rejected either.
* **No routing table.** One subnet plus a single default gateway.
* **No IPv6, no multicast, no IGMP.**
* **No path MTU discovery.** Don't Fragment is set but the ICMP
  "fragmentation needed" that answers it is not processed. On a path with
  a smaller MTU this stack would stall, silently.

**ICMP**

* Only **echo request → echo reply**. No destination unreachable is
  generated (a UDP datagram to a port nobody bound is dropped without an
  answer), no ICMP error is delivered up to TCP or UDP, no redirects, no
  timestamp or address mask requests.

**TCP**

* **No window scaling** (RFC 7323). The window is capped at 65,535
  octets, which caps the bandwidth-delay product this stack can fill.
* **No timestamps and no PAWS.** With a 64 KiB window and no wrap
  protection, a sequence number that comes round again on a very fast
  link is not detected.
* **No SACK and no D-SACK.** Recovery is go-back-N: on a timeout
  `snd_nxt` is wound back to `snd_una` and everything from there is sent
  again, including segments the peer already has.
* **Fast retransmit without proper fast recovery.** Three duplicate
  acknowledgements do trigger a retransmission and `cwnd` is inflated
  once, but partial acknowledgements during recovery are not handled
  (NewReno) and the window is not deflated on exit.
* **No urgent data.** The URG flag and the urgent pointer are parsed and
  ignored.
* **No keepalive.**
* **No SYN cookies.** A SYN that arrives when no connection slot is free
  is dropped; the peer retries.
* **The initial sequence number is a linear congruential generator**, not
  the clock-plus-hash construction of RFC 6528. It is not guessable by
  accident and it is not cryptographically hard. This sentence is here so
  that nobody takes it for the latter.
* **The persist backoff is not exponential.** The probe interval is
  fixed; RFC 1122 wants it to back off.
* **Eight connections** (`MAXCONN`), **eight** out-of-order ranges per
  connection. A ninth hole in the receive stream drops the segment, which
  costs a retransmission and is correct but not free.
* **The delayed acknowledgement timer is a fixed 200 ms** and the second
  segment always triggers an immediate answer. In every run measured here
  the two-segment rule won and the timer never fired — which is why
  `answers_that_waited` reads 0 while `answers_out` is half of
  `segments_in`.
* **No ECN, no TCP-MD5, no TCP-AO.**
* **The MSS is taken from the SYN option only**, clamped to 536…1460.

**ARP**

* No gratuitous ARP is sent, no address conflict detection (RFC 5227), no
  proxy ARP. Entries expire after 60 seconds; a request is sent at most
  once a second per stack.

**UDP**

* Datagrams larger than 1472 octets are refused rather than fragmented.
  Four sockets, four datagrams queued per socket; a fifth is dropped.

**And the thing this round did not do at all:** the stack has never met a
network card. `demos/kernel/pci.fi` from round K2 finds a virtio-net
device; nothing here is wired to it. That is the next round, and it is a
driver, not a protocol.

---

## 6. What the language could not do, said plainly

* **An array length has to be an integer literal** (SPEC 12.1:
  `"[" type ";" int_lit "]"`). `[Tcb; MAXCONN]` is a syntax error, so
  every size in `lib/net/` appears twice — once as a `const` the code
  uses and once as a literal in the struct. They have to agree and
  nothing checks that they do. It is kept in one place with a comment on
  it, which is the best this stage of the language allows.
* **`size_of[T]()` resolves its type argument in the root file of the
  compilation only**, so a module cannot ask for the size of its own
  struct — the same restriction `lib/std/json.fi` names. The detour used
  here costs nothing and is worth knowing: `&(*p).b` is address
  arithmetic and not a memory access, so a null pointer is a perfectly
  good ruler.

  ```firn
  struct Pair { a: Stack, b: Stack, }
  fn net_size() -> usize {
      let p: *mut Pair = 0 as *mut Pair
      return ((&(*p).b) as u64 - (&(*p).a) as u64) as usize
  }
  ```
* **`as` binds tighter than unary `*`.** `*p as u64` parses as
  `*(p as u64)` and is a type error; the value has to be taken into a
  binding first. Cost: one error message and one minute, mentioned here
  because it will cost the next reader the same minute.
* **Checked arithmetic is the reason four of the five bugs above have
  line numbers.** Every sequence-number operation in this library is
  written `+%` / `-%` on purpose and every narrowing conversion masks
  first — `(v & 0xFFFF) as u16`, never `v as u16`, because `(a as u32) << 8`
  on two octets does not fit in a `u16` and the abort would come at the
  first frame. Where the wrap is *not* wanted, the check found the bug.

---

## 7. Files, and what is open

```
lib/net/wire.fi        492   octets: eth, ARP, IPv4, ICMP, UDP, TCP headers,
                             checksums, sequence arithmetic modulo 2^32
lib/net/tcp.fi       1,555   the state machine, all eleven states
lib/net/stack.fi       599   ARP cache, outbox, demultiplexing, the two doors
tools/k3net/unit.fi  1,197   fifteen cases over a wire that misbehaves
tools/k3net/drv.fi     544   AF_PACKET underneath, five modes above
tools/k3net/link.fi    154   the simulated wire: loss, reordering, corruption
tools/k3net/kprobe.fi   70   the same stack in the kernel profile
tools/k3net/run.sh     437   the guard: 20 proofs, section 55 of test.sh
```

Open, in the order it will matter:

1. **A virtio-net driver.** The lower door is two functions; round K2
   already finds the device on the PCI bus.
2. **Window scaling and SACK.** Together they are what stands between
   64 KiB in flight and a link that is actually full.
3. **Fragment reassembly**, or a documented decision never to do it.
4. **ICMP errors upwards** — a destination unreachable that reaches the
   connection that caused it turns a sixty-second timeout into an
   immediate refusal.
5. **More than eight connections**, which is a question about where the
   memory comes from and therefore a question for the kernel, not for
   this file.
