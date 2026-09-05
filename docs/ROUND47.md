# Round 47 — finalizers, `Arc[T]`, weak references: the remaining work on memory management

Branch `r47-arc`, base `a492d26`. This round closes the three items that had
stood unchanged as „open" in `ACCEPTANCE.md` and `docs/ROUND44.md` since round
4: **finalizers (`S4`)**, **`Arc[T]`** and **weak references that are really
zeroed on collection (`S3`)**.

Result up front, all measured by ourselves:

| | base (a492d26) | round 47 |
|---|---|---|
| longest interruption, **compute time**, median of 7 runs | 469 µs | **460 µs** |
| throughput `build.fi` (cycles in 5 s, median) | 554 000 | **554 000** |
| interruptions above 1 ms in compute time (150 s run with finalizers) | — | **0** of 253 698 |
| RSS over 140 s of continuous operation with 48 million finalizers | — | **1372 KiB, drift-free** |
| `test.sh` | 696/696 | **727/727** |

---

## 1. Finalizers (`S4`)

### 1.1 The form, and why it is not `fn finalize(inout self)`

`SPEC.md` §3.5.3 says `fn finalize(inout self)`. Stage 0 has neither methods
nor `inout` nor function pointers — and **indirect calls belong to round 46**
(interfaces/vtables), not to this one. The honest equivalent without
function pointers is two parts:

```firn
gc_finalizer_set(p, kind)           // one clean-up kind per OBJECT (1..16777215)
fn __gc_finalize(kind: u64, p: *mut u8) { ... }   // ONE dispatcher in the program
```

The cleanup kind is recorded **in the block header**, not in a side table:
the word `[4..8)` so far only carried the mark (0/1/2) and has 30 bits free.
New:

```
bits 0..1   mark          0 white, 1 gray, 2 black
bit  2      F_FIN         the object has a finalizer
bit  3      F_WART        it stands in the queue / is running right now
bit  4      F_GETAN       its finalizer has run
bits 8..31  clean-up kind 24 bit
```

That costs **not a single byte per object** and makes the detection during
the sweep a test of one bit in a word that is read there anyway.

If the **root file** declares `fn __gc_finalisiere`, the compiler takes it;
otherwise it adds an empty default. That is exactly the mechanism with which
`error AllocError` is already handled (token search in
`gc.rs::quelle_hat_allocerror`), and it is built the same way in **both**
compilers (`gc.rs::quelle_hat_finalisierer` / `lib/firnc1/gc.fi::gc_quelle_scan`,
bit 4). Only the root file counts: in a module the function would be called
`modul____gc_finalisiere` and the runtime would no longer find it.

### 1.2 Resurrection — the decision

Java lets a finalizer hang its object back into the live graph.
The collector needs a second cycle for that, the semantics are
notoriously hard, and `finalize()` has been deprecated since Java 9. **Firn
does not do that.** The decision is:

> A finalizer cannot keep its object alive. The block is
> released as soon as the finalizer returns — no matter what it
> did.

That is not only a request to the programmer but is **enforced**:

1. **Before** the call, all `Gc[T]` and `GcWeak[T]` fields of the object are
   zeroed. A finalizer therefore never sees a pointer to a neighboring
   object that may already have been collected in the same run. Instead of
   a dangling pointer it sees a **0**, and that is deterministic.
2. `stark(w)` on a waiting object yields 0 (flag `F_WART`).
3. A **GC allocation** in the finalizer aborts visibly (return value
   **71**), `gc_collect()` likewise (**72**), and writing a Gc pointer
   into a heap field (**73**).

The three refusals: the runtime writes one line to stderr and stops the
program. The wording stands in `lib/gc/gc.fi` and names the case together
with the rule it breaks (SPEC 3.5.3 S4) -- allocation during a finalizer,
`gc_collect()` during a finalizer, resurrection in a finalizer.

**Item 3 solves the reentrancy question at the same time.** A finalizer that
allocates would start a collection in the middle of a collection:
the queue, the free lists and the sweep markers are half finished at that
moment. Because it cannot even allocate, the case is not handled but
**impossible**.

**And it costs the normal case nothing.** The lock sits in a check that
stands at the beginning of every allocation anyway: `S_INIT` was 0 („no
`gc_init`") or 1 („ready") and is now 2 while a finalizer runs.
`== 0` became `!= 1` — the same instruction. The test in the
insertion barrier sits **inside** the branch „a cycle is currently
running", which existed already.

### 1.3 Order, count, point in time

* **Point in time:** after the collector has recognized the object as
  unreachable. For that, the cycle has a **phase 3 of its own** which runs
  in slices like marking and sweeping (time budget `ZEIT_BUDGET_NS`).
* **Order between two objects: no promise.** The queue follows
  the sweep order. Whoever needs an order does not take a finalizer.
* **At most once per object** (`F_GETAN`).
* A finalizer itself is **not** interrupted. Its runtime belongs to
  the program, not to the collector, and is reported separately
  (`gc_fin_ns_max`, only with `gc_set_fin_uhr(1)`).

### 1.4 The queue needs no memory

A waiting block is dead; its **serial number word** `[8..16)` is not
needed any more. That is where the link to the next waiter is stored. With
that the queue cannot overflow and cannot trigger an allocation — both
would be fatal in a collection. That the serial number is destroyed in the
process is harmless and even right: `stark()` additionally checks `F_WART`.

---

## 2. Weak references are really zeroed (`S3`)

Up to round 46 the promise „a weak reference becomes empty on collection"
was only **apparently** fulfilled: `stark(w)` yielded 0 because the serial
number no longer matched — but the field still contained the old,
obfuscated bit pattern.

The type table has known the offsets of the `GcWeak[T]` fields since
round 4; up to now they were „only for the statistics". Since round 47 the
sweep uses them: for every **live** object the weak fields are looked
through, and a target that dies in this run is set to the empty reference.
The decision is made via the **mark** (white = dead), not via the serial
number — that is why it is independent of whether the target block has
already been swept.

`tests/822_gc_weak_zeroed.fi` looks at the **raw word** of the field and
demands that it is 0 after the collection.

**What this cannot do, openly named:** a `GcWeak[T]` in a *local
variable* is not touched — the collector knows the stack only
conservatively and has no field map there. There the serial number still
protects, and `stark()` yields 0.

---

## 3. `Arc[T]` — and what „atomic" really means here

### 3.1 The primitive

`Arc` differs from `Rc` in exactly one thing: the atomic counter.
Without an atomic instruction, `Arc` would only be `Rc` with a different
name. So since this round there is **one** new FIR primitive, the smallest
one that suffices:

```firn
__atomic_add(p: *mut u64, delta: u64) -> u64    // returns the OLD value
```

→ `lock xadd qword ptr [rcx], rax`, one instruction. Decrementing is the
addition of the two's complement; a primitive of its own for it would be
ballast. Built in **both** compilers (`compiler/src/atomic.rs`,
`Op::AtomicAdd`; `lib/firnc1/{fir,sema,lower,codegen}.fi`, `O_ATOMADD`),
FIR text octet-identical.

### 3.2 No „thread-safe" without evidence

Firn has **no threads** in stage 0 (`SPEC` §7). A race can therefore
not be brought about, and the claim „thread-safe" would be uncovered.
What is demonstrated is therefore what can be demonstrated —
`tools/atomic/run.sh`, as section 8b in `test.sh`:

* `__atomar_addieren` produces `lock xadd` — in **three build stages** and
  in **both compilers**, in the assembly text *and* in the finished binary;
* an ordinary `*p = *p + 7` produces **no** `lock` (counter-check: without
  it the proof would be worthless, because it would let everything pass);
* the return value is the **old** value, and the counter is exact after
  100 000 increments and 100 000 decrements;
* the FIR of both compilers is **octet-identical**.

**And the limit is named:** `arc_klonen`/`arc_freigeben` are correct with
`lock xadd` even under concurrency — release happens only from the
call that *sees* the 1 when decrementing, and exactly one call sees it.
`aufwerten_atomar` (weak → strong), by contrast, needs a **compare and
exchange** (`compare_exchange`); with pure fetch-add the race „the
last strong reference falls away right now" cannot be closed. Round 47
deliberately builds only fetch-add. In today's Firn `aufwerten_atomar` is
correct; as a thread promise it does **not** hold.

### 3.3 The interplay with the tracing GC

The Arc heap is an mmap area of its own. The collector knows only its own
chunks. From that follows a trap that was written down nowhere before: **a
`Gc[T]` inside the value of an `Arc` is invisible to the collector**, and
its target gets collected although it is still in use.

That is why there are now **external root areas**:

```firn
gc_wurzel_anmelden(arc_wert_adresse(a) as *mut u8, groesse)
gc_wurzel_abmelden(arc_wert_adresse(a) as *mut u8)
```

A registered area is scanned conservatively at every cycle start, exactly
like the stack. `tests/833_arc_gc_root.fi` measures **both** sides in
one run: without registration the target dies (the bug stands there as a
measurement, not as a warning), with registration it survives 2000 garbage
objects and several collections, and after deregistration it dies again.

**No double release** — and for a structural reason, not out of
carefulness: the counter decides exclusively about the **Arc block** (which
goes into the free list of the Arc heap), the collector exclusively about
**GC objects** (it knows only its own chunks). The two memory areas
do not overlap.

**Cycles leak**, exactly as with `Rc` — the atomic counter changes nothing
about that. `tests/832_arc_cycle_leak.fi` shows both in one run: 1000
cycle pairs with two strong references leak completely (2000 live
blocks, 0 releases), and the same 1000 pairs with one weak side become
completely free. If the leak were gone, the documentation would be wrong —
the test then fires.

**Price, openly named:** the start pause of a cycle grows with the
registered total size (conservative scan, 8 bytes per word).

---

## 4. Measurement

### 4.1 The measuring instrument had to be repaired first

The assignment demands instruction counts with callgrind. That did not
work: **every program with `gc class` died under valgrind with a
segmentation fault** — with the compiler of the base as well, so it is not
a new bug.

Cause: `__gc_stapel_boden()` read field 28 from `/proc/self/stat`. That is
the start address of the stack as the kernel noted it at program start —
under valgrind, however, the client runs on a stack provided by valgrind.
The conservative scan ran from its stack pointer to an
address that does not belong to it at all.

Now `/proc/self/maps` is read first and the mapping in which the own stack
pointer really lies is looked up; its end is the bottom. Field 28 remains
the fallback. With that the collector is measurable with callgrind for the
first time.

### 4.2 Instructions (callgrind, deterministic)

Same source, same workload (60 000 rounds of 6 objects each, ring
closed, 4000 live objects), once with the compiler of the base
(plus the same stack bottom repair, so that callgrind runs at all),
once with round 47:

| Workload | base | round 47 | Δ |
|---|---|---|---|
| class **without** a `GcWeak` field | 128 067 085 | 134 015 379 | **+4,6 %** |
| class **with** a `GcWeak` field, set every round | 138 288 807 | 154 997 400 | **+12,1 %** |
| ditto **+ finalizers** on every 4th ring | — | 162 856 205 | +5,1 % against round 47 without |

That is the **most expensive conceivable** case: every round writes a weak
reference, and *all* 4000 live objects have a weak field that is looked
through at every sweep. On the DOM workload (`build.fi`, where only
`Observer` has a weak field) the throughput is **unchanged** (§4.4).

### 4.3 What the way there cost — four measured retractions

The first attempt cost **+21,1 %** instead of +12,1 %. The sweep loop runs
over **every** block of the heap per collection; there every instruction
counts. Four interventions, each measured individually with callgrind:

| Intervention | fest.fi |
|---|---|
| first attempt (`behalten` flag in the sweep loop) | 167 427 654 |
| `continue` instead of a flag — free blocks no longer pay anything | 160 415 861 |
| type mask for weak fields (one shift instead of four dependent memory accesses), cold allocation branch moved into a function of its own, finalizer enqueueing moved into a function of its own, reentrancy test factored out of `__gc_barrier` | **154 997 400** |

Two individual findings that one does not guess but measures:

* **Two additional blocks in `__gc_alloc_raw` cost 3,3 million
  instructions** (2,4 %) — not because of the check itself, but because
  register allocation tipped over in the hottest function of the collector.
  With *one* block (cold branch in a function of its own) it is 0,7
  million.
* The counter-check of reading the type tag in the sweep loop **twice
  instead of binding it once** was **worse** by 0,3 million instructions.
  The frame slot is cheaper here than the second memory access — so the
  binding stayed.

### 4.4 Pauses: `build.fi`, 120 000 live nodes, 5 s, 7 runs each

**Two further rounds ran in parallel** on this machine during that. The
wall clock is therefore not evaluable (that was the wrong finding of round
40); what counts is the **compute time of the thread**.

| Metric (min/median/max) | round 47 | base |
|---|---|---|
| **longest interruption, compute time** | 434 / **460** / 522 µs | 453 / **469** / 519 µs |
| longest interruption, wall clock | 473 / 962 / 1854 µs | 476 / 541 / 609 µs |
| cycles in 5 s | 545 000 / **554 000** / 562 000 | 546 000 / **554 000** / 554 000 |
| full stop-the-world runs | 0 / 0 / 0 | 0 / 0 / 0 |
| RSS | 12 388 / 13 160 / 13 924 KiB | 12 888 / 13 144 / 13 148 KiB |

**The pauses have not become worse** — in compute time they are even 2 %
better, and in the same order of magnitude as the 0,45 ms from round 44.
The throughput on this workload is unchanged.

### 4.5 Do finalizers blow up the pauses? No — A/B in the same process

`tools/gc_meas/final.fi` measures two phases in the **same** process with
the same code (two binaries would have a different code layout, and that
masks differences in the percent range). Both clocks on, 30 s each, 4000
live objects:

| | phase A **without** finalizers | phase B **with** finalizers |
|---|---|---|
| cycles | 64 425 600 | 39 156 416 |
| collections | 23 581 | 14 325 |
| finalizers run | 0 | **9 789 092** |
| weak fields zeroed | 64 382 273 | 103 512 460 |
| **longest interruption, compute time** | 561 µs | **525 µs** |
| longest interruption, wall clock | 955 µs | 1780 µs |

Histogram of the whole interruption (bucket *k* = [2^(k−1) µs, 2^k µs)):

| Bucket | A wall clock | A compute time | B wall clock | B compute time |
|---|---|---|---|---|
| ≤ 32 µs | 160 684 | 160 580 | 99 854 | 99 802 |
| 64 µs | 27 563 | 27 723 | 18 969 | 19 105 |
| 128 µs | 29 050 | 29 037 | 121 917 | 121 942 |
| 256 µs | 10 728 | 10 708 | 12 750 | 12 674 |
| 512 µs | 106 | 87 | 202 | 174 |
| 1,02 ms | 5 | **1** | 4 | **1** |
| 2,05 ms | 0 | **0** | 2 | **0** |
| above that | 0 | **0** | 0 | **0** |

**In compute time there is not a single interruption above
1,02 ms in either phase** — 253 698 interruptions in phase B, of which one
above 512 µs. The two wall clock values above 1 ms in phase B have **no
counterpart** in compute time: they are preemption by the parallel runs,
not collector behavior. The requirement „98 % below 1 ms in continuous
operation" is met with **100 %**.

The longest **individual** finalizer (incrementing a counter) was measured
at 882 µs — the same preemption; it belongs to the program, not to the
collector, and is therefore reported separately.

### 4.6 Endurance run: 150 s with 48 million finalizers, RSS drift-free

The same setup, phase B over **150 s**:

* 193 776 192 cycles, **70 888 collections**, **48 444 045 finalizers**
  run (all registered ones), queue empty at the end
* 256 693 280 weak fields zeroed
* **RSS constant at 1372 KiB from the first sample (5 s) to the last
  (145 s)**, heap constant at 1 310 720 bytes, live objects 4024–4029
* the 4000 objects of the live chain were **complete and in the
  right order** at the end — the collector collected nothing that was
  alive

No drift over 2,3 minutes, although around 320 000 objects were
finalized and released every second.

---

## 5. Tests

| File | What it checks |
|---|---|
| `tests/820_gc_finalizer.fi` | finalizer runs with the right cleanup kind; Gc fields are zeroed beforehand; **at most once**; a reachable object is not finalized; mass run (300 objects) — the dispatcher of the program ran exactly as often as the collector counts |
| `tests/821_gc_finalizer_limits.fi` | every rejection: null pointer, stack pointer, pointer into the middle of the object, kind 0, kind > 16777215, double registration; `gc_finalisierer_loeschen` really takes it back |
| `tests/822_gc_weak_zeroed.fi` | the **raw word** of the weak field is 0 after collection; a live target is not zeroed; nothing is counted twice |
| `tests/823_gc_finalizer_reentrancy.fi` | allocation in the finalizer aborts with **71** |
| `tests/824_gc_finalizer_resurrection.fi` | self-linking in the finalizer aborts with **73** |
| `tests/830_arc_basic.fi` | counter, last reference releases, **no double release**, block is reused, 20 000 rounds without a remainder |
| `tests/831_arc_weak.fi` | weak does not keep alive, upgrading after death is visibly empty, release exactly once in **both** orders |
| `tests/832_arc_cycle_leak.fi` | cycles leak (2000 blocks, 0 releases) — and not with one weak side |
| `tests/833_arc_gc_root.fi` | GC interplay **without** and **with** `gc_wurzel_anmelden`, deregistration, both sets of bookkeeping |
| `tests/neg/arc_discarded.fi` | `arc_neu` is `#[must_consume]` |
| `tests/neg/atomic_ty.fi` | wrong pointer type at the atomic primitive — an error with line/column, no silent computation on 32 bits |
| `tests/neg/atomic_digits.fi` | wrong digit count — the message names the agreed form |
| `tools/atomic/run.sh` (test.sh 8b) | `lock xadd` in 3 build stages and both compilers, counter-check, FIR octet-identical |

Every positive test runs in **three build stages** (release-fast, no-opt,
dev-fast) and additionally under **firnc1**.

### 5.1 Two existing tests had to become independent of the frame position

`__gc_scrub` only cleans what lies **below** its own frame. The
topmost few hundred octets below the stack pointer of the program — where
the frames of `gc_collect` and `__gc_scrub` itself later lie — remain
standing. A helper that is called **shallowly** deposits its pointers
exactly there, and the conservative scan reads them as roots.

Up to round 46 that went well. When the runtime got bigger in this round,
the frames shifted, and the same gap held on to **1** long unreachable
object in `tests/520` (dev-fast) and **126** in `tests/535` (without the
optimizer). Before that this was **luck, not proof**.

Both tests check the same thing unchanged; the only new thing is that the
pointer-holding frames lie kilobytes deeper (recursive — so never
inlined — and with padding). The project has used the same technique since
round 4 in `dom_observer_lebt()`.

**Rejected:** the first attempt was to give `gc_collect` a 3 KiB zeroed
buffer in its own frame. That repaired `tests/535` but tipped over
`tests/520`, `820` and `822` — and one of them with a
segmentation fault. The reason is the same: the gap does not disappear,
it **wanders** to another depth. Padding in the collector cannot solve the
problem, only shift it; that is why it was retracted.

---

## 6. Acceptance

| Check | base | round 47 |
|---|---|---|
| `bash ./test.sh` | 696/696 | **727/727** |
| `bash tools/self_compare.sh` | 201 / 0 / 0 | **210 / 0 / 0** |
| `bash tools/fixpoint.sh` | character-identical | **character-identical** |

---

## 7. What remains open

* **`compare_exchange`.** Without it, `aufwerten_atomar` is not a thread
  promise (§3.2). It belongs in the round that brings threads — together
  with memory orderings (`acquire`/`release`/`relaxed`), which are given
  on x86-64 with `lock xadd` anyway, but not on aarch64.
* **Finalizers as a language form.** `fn finalize(inout self)` needs
  methods *and* indirect calls. Round 46 builds the indirect calls for
  vtables; after that the dispatcher is a pure question of convenience,
  not of capability. The semantics of this round remain unchanged.
* **Static checking of the finalizer contract.** `SPEC` §3.5.3 says „the
  compiler checks that"; round 47 enforces it at **runtime**. The static
  variant would be to demand `#[no_gc]` on `__gc_finalisiere` — the check
  for it already exists (`nogc.rs`, transitive). Deliberately not done,
  because `#[no_gc]` forbids writing *local* Gc fields as well and therefore
  restricts more than the contract demands.
* **`__gc_block_von` is linear.** With weak fields whose targets scatter
  over many chunks, the chunk list is the most expensive item of the
  zeroing (measured: the largest single share of the +12,1 % in §4.2). A
  chunk array sorted by address with binary search would take care of that
  — it is a rebuild of the chunk management and does not belong in this
  round.
* **`GcVec`/`GcMap`, `virtual`, 24-hour run, fragmentation with
  changing object sizes** — unchanged open (`ACCEPTANCE.md` item 2).
