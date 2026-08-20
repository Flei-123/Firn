# Round 53 — `GcVec` and `GcMap`: collections with a variable length in the GC heap

Branch `r53-gcvec`, base `cc1710f`. This round closes the last gap that a
preliminary investigation for the browser engine named as a *real* blocker:
`SPEC.md` §3.5.2 fixes `GcVec[Gc[Node]]` and `GcMap[Atom, Str]` as the
DOM foundation, and §1197 admitted until just now: „No `GcVec`/`GcMap`, no
`virtual`." As long as that was missing, the DOM stayed with a fixed
attribute count (`attr_name: [u32; 4]`) and a sibling chain without an
index.

Result up front, all measured by ourselves:

| | base `cc1710f` | round 53 |
|---|---|---|
| `test.sh` | 751/751 | **763/763** |
| `tools/self_compare.sh` | 213 / 0 / 0 | **217 / 0 / 0** |
| `tools/fixpoint.sh` | character-identical, 427 401 lines | **character-identical, 470 042 lines** |
| longest interruption, **compute time**, median of 7 runs | 476 711 ns | **496 501 ns** |
| interruptions above 1,02 ms (compute time) | 1 of 307 089 | **0 of 538 936** |
| DOM endurance run, RSS | flat | **flat (1644 → 1644 KiB)** |
| reference-counting counter-check | leaks | **leaks (factor 520)** |
| children per node in the DOM | chain without an index | **5000, indexed** |
| attributes per element | **4** | **unlimited** |

---

## 1. The design: a second object, the slot buffer

The collector traces the heap **precisely** via the compiler-generated
type table: one fixed list of field offsets per class
(`gc.rs::typtabelle_asm`, `gc.fi::__gc_trace`). A collection that grows
does not fit into that — its element count is only known at runtime.
That was exactly the reason for §1197, and it is not a blemish but a
property of the method.

The way out is **not** a special path in the collector but a second object:

```text
Gc[GcVec] --buffer--> Gc[GcSlots] --elements--> Gc[T] ...
   ^ an ordinary              ^ block header bit F_SLOTS
     strong field:              -> __gc_trace_slots reads n, the step width
     type table + barrier          and the pointer mask OUT OF THE BUFFER
     come from the compiler
```

The slot buffer is a perfectly ordinary GC block; it only carries one more
bit in the state word of its header. Payload:

```text
[0..8)    u64  n            the number of elements that are traced
[8..16)   u64  desc         step | (mask << 8)
[16..24)  u64  progress     the resumption point of the marking
[24..32)  u64  cap          the capacity in elements
[32..)         the elements
```

With **one** buffer type, `desc` covers everything that is needed:
`GcVec[Gc[T]]` (step 1, mask 1), `GcVec[u64]` (1/0),
`GcMap[Gc[K],Gc[V]]` (2/3), `GcMap[Atom,Gc[V]]` (2/2).

**What the bit costs: nothing.** `F_SLOTS` is bit 5 of the state word
`[4..8)` — bits 5..7 were free (0..1 mark, 2 `F_FIN`, 3 `F_WART`,
4 `F_GETAN`, 8..31 cleanup kind). `__gc_trace` reads and writes this word
anyway; the test is an `and` on a register.

### 1.1 Why the slicing has to be there — with a number

A buffer can carry millions of elements. Traced in *one* piece, it would
tear the 100 µs time budget of a marking slice (rounds 41/44) and with it
the pause promise. So `__gc_trace_slots` traces at most
`SLOT_SCHEIBE = 64` elements per call, remembers the resumption point **in
the buffer** and puts itself back onto the mark stack — the buffer stays
**gray** for that long. Gray means exactly „not yet fully traced"; that is
not a reinterpretation.

If the mark stack overflows in the process, `__gc_nachtragen` finds the
buffer again as a gray block and continues at `fortschritt`. Completeness
therefore does not hang on the stack.

Whether the slicing is really necessary was **measured** instead of
claimed (`build.fi`, one node with 120 000 children in a `GcVec`, compute
time, median of 3 runs):

| `SLOT_SCHEIBE` | longest interruption (compute time) | objects in 3 s |
|---|---|---|
| 16 | 468 900 ns | 9 542 018 |
| 32 | 499 770 ns | 9 892 018 |
| **64 (chosen)** | **503 890 ns** | 10 172 018 |
| 128 | 478 460 ns | 10 928 018 |
| **10⁹ (= no slicing)** | **2 671 821 ns** | 10 284 018 |

**Without slicing the longest interruption is 5,3 times as long.** Between
16 and 128, by contrast, there is no dependable difference — the values lie
within the same scatter that three runs of the same state have among
themselves. The constant is therefore a **safeguard against the
pathological case**, not a fine tuning knob. 64 is the middle of the
measured plateau.

---

## 2. The barrier question — the actual work of this round

Since round 44 the collector marks in slices. So it may run in the middle
of growing. Three places are critical; all three hang on the same
Dijkstra insertion barrier from `gc.fi`.

**(B1) The new buffer.** `gcvec_platz` allocates a larger buffer — and
`__gc_alloc_raw` is exactly the place at which a slice runs. Afterwards the
vector may long since be **black**. The new buffer is found nevertheless,
because it is hooked in over the ordinary strong field `v.puffer` and the
compiler places `__gc_barrier` there.

**(B2) The old buffer during the copying.** Between the allocation and the
hooking in, *nothing* allocates — so no slice can run. The old
buffer hangs on the vector the whole time, the new one on the stack
(conservatively captured). If the old one drops out afterwards, it has
either already been traced or still lies gray on the mark stack; in both
cases its elements have been seen.

**(B3) Every entered element.** An element is written with `__gc_st64`,
not through a typed field — the compiler cannot place a barrier here,
because it does not know the buffer at all. So the library calls it
itself (`gcvec_anhaengen_roh`, `gcvec_setzen_roh`, `gcmap_setzen` for the
key *and* the value).

What is **not** necessary: coloring the buffer gray by hand again when it
grows. Every element that is ever in it has either become gray via (B3) or
was in the old buffer before, which is traced according to (B2). This
justification stands in the header comment of `lib/gc/gcvec.fi` as well --
it cannot be read off the code.

### 2.1 The first hypothesis was wrong, and that is the interesting part

The obvious assumption was: *„A freshly appended object disappears
behind the black buffer."* The test for it was built, the counter-check ran
— and stayed **green, even without the barrier**.

The reason is in `__gc_alloc_raw` and is a promise from round 38:

> Objects freshly allocated during a cycle are **gray** and go onto
> the mark stack.

For freshly allocated elements the case therefore does not exist at all.
The case that **really** exists is the **relinking of an object that
already exists** — that is, exactly `appendChild`:

```text
target  traced already (black)
source  not traced yet (white)
then move `z` out of the source and into the target
```

The collector sees `z` nowhere afterwards: not in the target, because that
is already done, and not in the source, because it no longer stands there.
`z` stays white and is swept.

### 2.2 How often the case occurs by itself: almost never

Measured over 200 000 appends with the usual garbage stream:

| | Count | Share |
|---|---|---|
| appends in phase 1 (marking) | 526 | 0,26 % |
| appends in phase 2 (sweeping) | 32 | 0,02 % |
| **buffer growth during a cycle** | **0** | **0 %** |

That is not chance but arithmetic: a marking phase lasts
`lebende_objekte / SCHEIBE_TRACE` slices, and between two collections
there are `GRENZE / Blockgröße` allocations. The ratio is around 1 : 512.
**A test that waits for chance checks nothing.**

That is why there are two new runtime knobs in `gc.fi` -- the same trick as
`S_CPUUHR` (round 44) and `S_INKRAB`: default 0, in which case the code is
bit-identical to before, and only test programs change them.

```firn
gc_set_slice(2)          // 2 objects per marking slice instead of 512
gc_set_time_budget(ns)   // the time budget of marking and sweeping
gc_phase()               // 0 idle, 1 marking, 2 sweeping, 3 finalizers
```

With `gc_set_slice(2)` the marking phase extends over as many
allocations as there are live objects. In `tests/841` **78 762
of 78 762** relinkings thereby fall into a running marking phase.

### 2.3 The counter-check — really carried out

| Intervention | Result |
|---|---|
| `tests/841` **with** the barrier in `gcvec_anhaengen_roh` | 0 |
| `tests/841` **without** this barrier | **96** — mark sum 81 672 010 instead of 81 926 400, **254 390** were missing |
| `tests/843` **with** the value barrier in `gcmap_setzen` | 0 |
| `tests/843` **without** this barrier | **17** |

The test finds a loss only if three things come together: the relinking
falls into a running marking phase, **the very same cycle is carried
through to the end as well** (otherwise the next full run finds everything again),
and afterwards a wave of garbage is allocated so that a wrongly released
block is **reused** — the content of a free block otherwise stays readable
and the loss does not show up. All three points are stated in the test.

---

## 3. `GcMap`: state in the key

The same slot buffers, step 2. Open addressing, linear probing,
power-of-two capacity, load limit 3/4 — the same construction as
`lib/rt/map.fi`, so that the two maps of the language do not behave
differently (the hash function is taken over bit-identically).

The state per slot is **in the key**, not in a side field:

```text
key 0 = never occupied   (a search may stop)
key 1 = deleted          (a search may NOT stop)
```

That is safe because `__gc_mark(1)` looks for the chunk at address 1 via
`__gc_block_von` — there is none, so nothing happens. A tombstone cannot
lead the collector astray. The price is a promise to the outside, and it is
stated in the file:

> **0 and 1 are reserved as keys.**

For Gc pointers that costs nothing (no address is 0 or 1), for raw
identifiers the two smallest values. The alternative would be a second
field per slot — a third more memory and a second memory access per
probing step, for nothing.

The traced range of a `GcMap` is **always the full capacity**, not the
number of entries: the slots lie scattered, not densely. Empty slots carry
0 and cost one comparison when tracing.

---

## 4. The compiler hooks

Four interventions, all in **both** compilers, all small.

**(a) The collections come in files of their own** — `lib/gc/gcvec.fi`,
`lib/gc/gcmap.fi` — and are *appended* to `gc.fi`; together they are one
module and therefore see its names without an `import`. New files instead
of interventions in the middle of `gc.fi`, so that the merge with round 49
(thread safety of the same collector) does not fall apart. In `gc.fi`
itself there are only: the three constants,
`__gc_trace_slots`, one `if` in `__gc_trace`, the two knobs and
`gc_phase()`.

**(b) The appending happens only on demand.** `quelle_braucht_sammlungen`
looks in the token stream for an identifier with the prefix
`GcVec`/`GcMap`/`gcvec_`/`gcmap_` — the same mechanism as
`quelle_braucht_gc` and `quelle_hat_allocerror`. A program with
`gc class` but without collections thereby produces **the same code as
before**. A module test records that.

**(c) `GcVec[E]` and `GcMap[K,V]` as type names.** Both are pointers to the
runtime classes; `GcVec[Gc[Node]]` is exactly `Gc[GcVec]`, only written the
way the SPEC has it. The type arguments are parsed completely (a
typo is supposed to show up) and then discarded.

**(d) `Gc[T]` and `GcWeak[T]` may stand in a generic template.** The
parser turns them into the names `__gc#p:T` / `__gc#w:T`; without this hook
the type resolution would later look for a gc class named `T` and report
„unbekannte gc-klasse 'T'". With that, a generic function over a Gc pointer
was impossible — and exactly that is the type-safe surface of the
collections:

```firn
gcvec_anhaengen[Node](eltern.kinder, kind)
let k: Gc[Node] = gcvec_lesen[Node](eltern.kinder, i)
```

### 4.1 What this round does NOT deliver, and says so explicitly

* **No container of its own per element type.** Stage 0 has no generic
  `gc class`. `GcVec[Gc[Node]]` and `GcVec[Gc[Element]]` are the same
  nominal type; the element type is checked at the **access**
  (`gcvec_anhaengen[Node]`), not at the **field**. Whoever writes `[Node]`
  once and `[Listener]` once into the same vector gets no error. That is a
  real gap and not a formality.
* **No methods.** `SPEC.md` §3.5.2 prescribes `parent.children.push(child)`;
  here it reads `gcvec_anhaengen[Node](eltern.kinder, kind)`. The same
  decision as with the finalizers of round 47, for the same reason.
* **No `virtual`.** Unchanged open (round 46 brought interfaces,
  `virtual` not).

---

## 5. The DOM uses the collections

`lib/dom/dom.fi`:

| before | now |
|---|---|
| `erstes_kind` / `letztes_kind` / `naechstes` | `kinder: GcVec[Gc[Node]]` |
| `listener: Gc[Listener]` (chain) | `listener: GcVec[Gc[Listener]]` |
| `attr_name: [u32; 4]`, `attr_wert: [u32; 4]` | `attrs: GcMap[u32, Gc[Str]]` |

Cycles 1, 2 and 3 thereby run **over collections** — the case from
`SPEC.md` §3.5.2. The collections come into being **only on demand**: a
leaf without attributes and without listeners still costs exactly one
object, and in the DOM that is the most frequent node of all.

New in the self-test and previously impossible: **5000 children on one
node** (with an index, not as a chain), **192 attributes on one element**,
and `removeChild` with the sibling order preserved.

`DOM_OBJEKTE_JE_SATZ` rises from 7 to **14**: root element (1),
attribute table + buffer + `Str` (3), three children (3), child list +
buffer (2), listener + listener list + buffer (3), collection (1),
wrapper (1).

**The counter-check was pulled along.** `lib/dom/soak_leak.fi` rebuilds the
set object by object — a 128-byte object with generic reference columns, a
child list and its buffer as objects of their own, 14 per set, of which 13
leak (what becomes free is the collection, the only one without a back
reference). Otherwise the same two graphs would no longer be compared, and
the report in `docs/reports/dom.md` would hang in the air.

### 5.1 Endurance run

`tools/dom_soak/run.sh`, within `test.sh`:

| | GC version | reference-counting counter-check |
|---|---|---|
| RSS median 2nd quarter → last quarter | **1644,0 → 1644 KiB** | 357 572 → 853 258 KiB |
| live objects | 27 → 27 | 6 825 000 |
| verdict | **no leak** | **LEAK** (as demanded) |

Standalone run with a larger budget: GC **1640 → 1640 KiB**, counter-check
364 884 → 853 256 KiB, **factor 520**.

`SOAK_LECK_ZYKLEN` had to come down from 2 000 000 to 600 000: a set now
leaks 13 objects of 128 bytes instead of 6 of 64, which is around 1,0 GiB
instead of 770 MiB. A tool that can drag the machine down with it is a
broken tool (round 44).

---

## 6. Measurement: have the pauses become worse?

`tools/gc_meas/r53_pauses.sh` (new). Three cases, **the same** measuring
program (`build.fi`, 120 000 live nodes, `AB_SCHWELLE = 0`, i.e. always
incremental):

* **A BASIS** — tree at `cc1710f`, own compiler, old DOM
* **B KERN** — compiler and GC runtime of round 53, but the **old** DOM.
  The collections are never used; what is measured is the surcharge of the
  `F_SLOTS` branch alone.
* **C SAMMLUNGEN** — round 53 as it is.

What counts is the **compute time of the thread**. Several rounds ran at
the same time on this machine; the wall clock then measures preemption,
not the collector (wrong finding of round 40). `callgrind` is out of the
question — it shifts the stack (`docs/ROUND47.md` §4.1).

### 6.1 Run I — 7 runs of 5 s each

| Metric (min / median / max) | A base | B core | C collections |
|---|---|---|---|
| **longest interruption, compute time** | 425 790 / **476 711** / 1 049 161 ns | 407 900 / **446 020** / 925 520 ns | 471 420 / **496 501** / 566 110 ns |
| interruptions above 1,02 ms (compute time) | **1** of 307 089 | **0** of 285 135 | **0** of 538 936 |
| RSS (median) | 12 904 KiB | 12 644 KiB | 13 860 KiB |
| collections / full STW (median) | 62 / 0 | 58 / 0 | 290 / 0 |

### 6.2 Runs II and III — 3 runs of 3 s each, with deterministic counters

| | A | B | C |
|---|---|---|---|
| longest interruption, compute time (median, run II) | 456 641 ns | 443 980 ns | 566 890 ns |
| longest interruption, compute time (median, run III) | 528 830 ns | 476 620 ns | 604 371 ns |
| **objects allocated** (run III) | 2 325 001 | 2 325 001 | **10 690 018** |
| **sum of all collector pauses** (run III) | 2 804 931 396 ns | 2 835 007 979 ns | **1 969 280 572 ns** |
| marking / sweeping slices (run III) | 25 967 / 493 | 26 240 / 488 | 47 089 / 2 349 |

**A and B allocate exactly the same amount down to the object**
(2 325 001) — case B is therefore really the same workload, only with the
new collector.

### 6.3 What follows from that

* **The rebuild of the collector costs nothing.** B lies *below* A in all
  three runs (446 / 444 / 477 µs against 477 / 457 / 529 µs). The
  `F_SLOTS` branch and the two additional state words are below the
  measurement threshold; that B systematically looks better is code layout
  noise and is not sold here as an improvement.
* **The rebuild of the DOM costs 10 to 15 % on the longest interruption**
  (C 497 / 567 / 604 µs). That is in the same order of magnitude as the
  0,45 ms from round 44 and the 460 µs from round 47 and still clearly
  below 1 ms. Over all runs together: **1 interruption above 1,02 ms out of
  837 087** measured (base: 1 out of 466 024).
* **The throughput is not comparable** and is therefore not compared:
  a set now has 14 objects instead of 7. What the deterministic counters
  show is remarkable nonetheless: in the same time C allocates **4,6 times
  as many objects** and spends **less** time in the collector while doing
  so (1,97 s instead of 2,80 s out of 3 s). The reason is obvious but is
  not the subject of this round and is therefore only noted as an
  observation: instead of three list pointers (`erstes_kind`,
  `letztes_kind`, `naechstes`) a node now carries only one reference to its
  `GcVec`, and the children lie densely in one buffer instead of scattered
  in a chain. The marking work per collection thereby drops to about a
  third. **What is established is the number, not the cause.**

---

## 7. What went wrong

**The packer only packed the core.** `tools/gen_gctext.sh` writes the
runtime as u64 words for the self-hosting compiler. During the conversion
to the concatenation `gc.fi + gcvec.fi + gcmap.fi`, the loop bound stayed
at the length of `gc.fi` — the collections ended up in the length entry but
not in the data. `firnc1` reported **nothing at all** about it: stage 0
counts errors, it does not print them. Found by bisection with minimal
programs.

**The templates of the runtime were known too late.** `bin/firnc1.fi`
collects the generic names of all files in a first pass (`gen_vorab`) —
but the collector runtime is not on disk and is only parsed at the very
end. The parser therefore saw `gcvec_anhaengen[Z](…)` in the root file as
indexing instead of as an instantiation. Fixed with a pre-scan of the
runtime before parsing; the price is an additional lexing pass, exactly as
with the modules.

**The first provocation test checked nothing.** See §2.1 — it tested a
case that does not exist and stayed green without the barrier. Only the
counter-check showed that. A test whose counter-check does not fire is no
test.

**The first pause measurement measured the wrong path.** `build.fi` has
`AB_SCHWELLE = 8388608` preset, i.e. atomic full cycles below
8 MiB of heap. What came out was 11,6 ms — those are the three
stop-the-world runs of the build-up phase from round 44, not the
incremental slices. The number was right, it just answered a different
question. `r53_pausen.sh` therefore sets the threshold to 0.

---

## 8. Rejected

* **A `gc class` of its own per element type** (`GcVec[Node]` gets a type
  tag of its own). It would give real nominal type safety, but it demands a
  class registration at parse time — and the order of the type tags would
  have to agree bit for bit between the two compilers, otherwise
  `fixpunkt.sh` falls over. Too much risk for the gain, named as a gap in
  §4.1.
* **A new entry in the type table** instead of the block header bit. The
  entry is 64 bytes large and starts at `tabelle+8`, so it lies across two
  cache lines; every trace would have paid an additional memory access.
  The state word is read anyway.
* **A segmented vector** (a chain of fixed-size buffers, no
  copying). It circumvents the barrier question instead of solving it — and
  it makes the indexed access that the DOM needs expensive again.
* **Registering the buffer as an external root area**
  (`gc_wurzel_anmelden`). It would be trivial and it would be wrong: a
  `GcVec` in a dead cycle would then hold its elements forever.
* **Coloring the buffer gray by hand when it grows.** Not necessary (§2),
  and it would have concealed the bug that `tests/841` is supposed to find.
* **`callgrind` for the pause measurement.** It shifts the stack; known
  since round 47 and justified in `docs/ROUND47.md` §4.1.

---

## 9. Open

* **Nominal type safety of the containers** (§4.1). The most urgent of the
  three gaps.
* **Method syntax** `parent.children.push(child)` — needs methods on
  `gc class`, i.e. a round of its own.
* **`virtual`** — unchanged open.
* **`GcMap` with the keys 0 and 1.** Reserved (§3). Whoever needs them has
  to pay for a second state field per slot.
* **Iterators.** Walking a `GcMap` runs over all slots and
  skips the empty ones. With a very sparsely occupied map that is
  expensive.
* **`gcvec_entfernen` is O(n).** For `removeChild` on a node with very
  many children that is the wrong complexity.
* **Fragmentation with very large buffers.** A buffer above the largest
  size class gets a chunk of its own; a vector that grows for a long time
  leaves behind a trail of ever larger individual chunks. Not measured.
* **The 24-hour run** from the acceptance is still outstanding.

---

## 10. Files

| File | Content |
|---|---|
| `lib/gc/gc.fi` | `F_SLOTS`, `SLOT_KOPF`, `SLOT_SCHEIBE`, `__gc_trace_slots`, one `if` in `__gc_trace`, `gc_set_scheibe`, `gc_set_zeitbudget`, `gc_phase` |
| `lib/gc/gcvec.fi` | `GcSlots`, `GcVec`, growth with a barrier, typed view |
| `lib/gc/gcmap.fi` | `GcMap`, open addressing, tombstones, walking |
| `compiler/src/gc.rs` | `quelle_braucht_sammlungen`, `GcVec[…]`/`GcMap[…]` in the parser, `laufzeit_quelle` with a third parameter |
| `compiler/src/mono.rs` | `Gc[T]`/`GcWeak[T]` in a template |
| `lib/firnc1/parser.fi`, `mono.fi`, `gc.fi` | the same in Firn |
| `tools/gen_gctext.sh`, `lib/firnc1/gctext.fi` | the runtime as data, now with two lengths |
| `bin/firnc1.fi` | pre-scan of the runtime templates |
| `tests/840`–`843` | basics, the incremental case, interplay |
| `lib/dom/dom.fi`, `soak_leak.fi` | the DOM on collections, counter-check pulled along |
| `tools/gc_meas/r53_pauses.sh` | the pause measurement of this round |
