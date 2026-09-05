# DOM prototype and endurance run — report (acceptance item 2)

State 14.08.2026. All numbers are measured by ourselves; the raw data lie as
TSV in the tree and can be regenerated with `tools/dom_soak/run.sh`.

## What this is about

`FIRN-ANFORDERUNGEN.md` §13, item 2 demands a **DOM prototype with
parent/child back references and listener cycles that does not leak in an
endurance run**. That is the point at which the memory model is decided: the
DOM is a **cyclic** graph, and a reference count never releases a cycle in
principle. Gecko therefore had to add a cycle collector afterwards;
`SPEC.md` §3.5 draws the consequence from that and decides for an
**opt-in tracing GC**.

A report that merely claims „runs without a leak" is worthless. That is why
this setup measures **two** programs with an identical object graph:

| Version | Memory model | Expectation |
|---|---|---|
| `lib/dom/soak_gc.fi` | `gc class` / `Gc[T]`, mark-sweep | **must not grow** |
| `lib/dom/soak_leak.fi` | its own reference count over `mmap(2)` | **must leak** |

If the counter-check stays green, `tools/dom_soak/run.sh` aborts with an
error: a measurement that cannot indicate a leak at all is no measurement.

## The object graph (`lib/dom/dom.fi`, 6 kinds of cycle)

Every kind really occurs in a real engine:

1. **Node ↔ child** — `Node.eltern` is a **strong** `Gc[Node]`, and the
   parent holds its children strongly as well. No `GcWeak`, otherwise the
   cycle would be defined away instead of resolved.
2. **`Element extends Node`** — inheritance with a prefix layout,
   attributes as an atom identifier → value, a checked downcast
   `x.as?[Element]`.
3. **Node ↔ listener** — the node holds the listener, the listener holds
   its node. Exactly this cycle forced Gecko into a cycle collector.
4. **Collection → root** — a live `HTMLCollection`-like structure holds
   its root node.
5. **Observer over `GcWeak[Node]`** — counter-check: it does **not** keep
   its target alive, and `stark(w)` returns the null value after the
   collection.
6. **Node ↔ JS wrapper** — the simulated wrapper holds the node, and the
   node holds the wrapper. That is the boundary DOM ↔ JS engine that
   `FIRN-ANFORDERUNGEN.md` §1 is about.

One set from `dom_zyklus_bauen()` consists of **7 objects**: the root
element, three child nodes, a listener, a collection, a wrapper.

## Result of the endurance run

### GC version — 100 million cycle sets

`tools/dom_soak/longrun/gc-100mio-cycles.tsv` (1.001 samples):

| Quantity | Value |
|---|---|
| cycle sets | **100.000.000** |
| objects created | **700.000.000** |
| runtime | 116.459 ms (≈ 1 min 56 s) |
| **RSS from the first sample on** | **1.364 KiB** |
| **RSS at the end** | **1.364 KiB** |
| RSS maximum over the whole run | **1.364 KiB** |
| live objects (samples) | 8–12 |
| collections | 47.300 |
| heap size | 1.310.720 B, constant |
| longest pause | 3,54 ms |

Samples from across the run:

| Cycles | RSS | live objects | collections |
|---|---|---|---|
| 100.000 | 1.364 KiB | 12 | 47 |
| 1.000.000 | 1.364 KiB | 11 | 473 |
| 10.000.000 | 1.364 KiB | 12 | 4.730 |
| 50.000.000 | 1.364 KiB | 11 | 23.650 |
| 100.000.000 | 1.364 KiB | 8 | 47.300 |

**The consumption is constant to the byte over 700 million objects** —
not a single measuring point deviates. The 8–12 „live" objects are the
chance hits of the conservative stack scan; they fluctuate but do not grow.
A second, identical run delivered the same picture at 131.401 ms and a
longest pause of 6,58 ms — the pause length scatters, the consumption does
not.

### Reference-counting counter-check — same program, different memory model

`tools/dom_soak/longrun/leak-2mio-cycles.tsv`:

| Cycles | RSS | live objects |
|---|---|---|
| 100.000 | 37.536 KiB | 600.000 |
| 1.000.000 | 375.056 KiB | 6.000.000 |
| 2.000.000 | **750.080 KiB** | **12.000.000** |

Per set, **6 of 7 objects** stay behind: everything that hangs in a cycle.
What is released is exactly the **collection** — it is the only one without
a back reference. That establishes that the counter works **correctly** and
fails exclusively at the cycles. That is the core of the matter: it is not
the implementation that is bad, the method cannot do it.

**Ratio at the same measuring point (2 million cycles): 1.364 KiB against
750.080 KiB — factor 550.** By the verdict of `run.sh` (median of the last
quarter): factor **481**.

## What went wrong (and why it is stated here)

The first long run of the counter-check was **uncapped** and was heading
for 100 million cycles. At 384 bytes of leak per cycle that is ≈ 38 GB; the
machine has 12 GB. I aborted the run at 7,0 GB. Since then
`tools/dom_soak/run.sh` has two brakes:

* `SOAK_LECK_ZYKLEN` (default **2.000.000**, ≈ 770 MiB) limits the
  counter-check independently of the cycle count of the GC version;
* `ulimit -v` (default 3 GiB) as a hard limit in case the first one fails.

A tool that can drag the machine down with it is a broken tool — even
if the measurement itself was right.

## What the test really checks (and what it does not)

**Checked:**

* that the collector resolves cycles over two and over seven objects,
* that `GcWeak` does not keep its target alive,
* that the consumption stays flat over 100 million sets (RSS, not
  self-reporting),
* that all three build stages (`release-fast`, `--no-opt`, `dev-fast`)
  deliver the same result — a memory model that only holds with the
  optimizer is worthless,
* that the counter-check really fires.

**Not checked, honestly named:**

* **The 24-hour run from the acceptance is outstanding.** 131 seconds with
  100 million sets are a strong indication, but not the promise demanded.
* **Fragmentation** over a long time with *changing* object sizes. The
  endurance run always uses the same set; that is the friendly case.
  `SPEC.md` §3.5 names exactly that as the main risk of the non-compacting
  collector.
* **No incremental collection** (`S5`), **no finalizers** (`S4`), no
  `GcVec`/`GcMap`, no `virtual`. The longest measured pause is **6,58 ms** —
  for a browser with a 16 ms frame interval that is already too much and the
  reason why `S5` is in the ROADMAP.
  *(State 14.08.2026. Incremental collection came in round 44, finalizers in
  round 47, `GcVec`/`GcMap` in round 53 — see the addendum at the end of
  this report.)*
* **One thread.** The state block is meant to be thread-local, and stage 0
  has only one thread.

## The uncomfortable spot: the conservative stack scan

The collector scans the stack and the registers **conservatively**
(`SPEC.md` §3.5.3). That has a consequence one can see while testing and
which is therefore stated here:

> An old copy of a strong pointer in a **live** stack frame keeps the object
> alive.

Measured on the observer test: if `stark(b.ziel)` stands directly in the
body of the checking caller, the strong pointer lies in that caller's frame
— the target survives and the test measures something the language never
promised. With `--no-pass=inline` the same test passed, with inlining it
did not. Therefore:

* `dom_observer_lebt()` returns a `bool` instead of a `Gc[Node]` and is
  **recursive**, so that the inliner does not pull it into the live frame;
* `dom_zyklus_verwerfen()` overwrites the dead stack area below the
  current frame (the same construction as `__gc_scrub_tief` in the
  runtime).

That is not a trick but the documented price of the construction. Whoever
does not want to pay it needs precise stack maps — and those need a
different codegen model (ROADMAP, after real register allocation).

## Files

| File | Content |
|---|---|
| `lib/dom/dom.fi` | DOM prototype, 6 kinds of cycle, self-test |
| `lib/dom/meas.fi` | time, RSS from `/proc/self/statm`, TSV output (without GC) |
| `lib/dom/soak_gc.fi` | endurance run with `Gc[T]` |
| `lib/dom/soak_leak.fi` | counter-check with reference counting, self-contained |
| `tools/dom_soak/run.sh` | build in three stages, both runs, evaluation, verdict |
| `tools/dom_soak/messung-gc.tsv` | last measurement series GC |
| `tools/dom_soak/messung-leck.tsv` | last measurement series counter-check |
| `tools/dom_soak/longrun/gc-100mio-cycles.tsv` | the 100 million run |
| `tests/560_dom_cycles.fi` | structure test, runs in all three build stages |

---

## Addendum round 53 (19.08.2026): the prototype now uses collections

`docs/ROUND53.md` brought `GcVec`/`GcMap`, and `lib/dom/dom.fi` has been
converted to them. What changed in the object graph:

| before | now |
|---|---|
| `erstes_kind` / `letztes_kind` / `naechstes` | `kinder: GcVec[Gc[Node]]` |
| `listener: Gc[Listener]` (chain) | `listener: GcVec[Gc[Listener]]` |
| `attr_name: [u32; 4]`, `attr_wert: [u32; 4]` | `attrs: GcMap[u32, Gc[Str]]` |

Cycle kinds 1, 2 and 3 thereby run **over collections**. A set now consists
of **14** objects instead of 7: the root element (1), the attribute table +
buffer + `Str` (3), three children (3), the child list + buffer (2), the
listener + listener list + buffer (3), the collection (1), the wrapper (1).

**The counter-check was pulled along** — `lib/dom/soak_leak.fi` rebuilds the
same set object by object (a 128-byte object, generic reference columns,
14 objects, of which 13 leak). Without that, the same two graphs would no
longer have been compared, and the whole report would hang in the air.

Measurement with `tools/dom_soak/run.sh` (within `test.sh`, 19.08.2026):

| | GC version | reference-counting counter-check |
|---|---|---|
| RSS median 2nd quarter → last quarter | **1.644,0 → 1.644 KiB** | 357.572 → 853.258 KiB |
| live objects | 27 → 27 | 6.825.000 |
| verdict | **no leak** | **LEAK** (as demanded) |

Standalone run with a larger budget: GC **1.640 → 1.640 KiB**, counter-check
364.884 → 853.256 KiB, **factor 520**.

`SOAK_LECK_ZYKLEN` has stood at **600.000** instead of 2.000.000 since then:
a set now leaks 13 objects of 128 bytes instead of 6 of 64, which is around
1,0 GiB instead of 770 MiB. The same brake, the same reason as above.

**Pauses:** with 120.000 live nodes, the longest interruption in compute
time is at a median of **497 µs** (base before round 53: 477 µs), and not a
single one of 538.936 measured interruptions is above 1,02 ms. Without the
slicing of the buffer tracing it would be **2,67 ms** — the number is in
`docs/ROUND53.md` §1.1.
