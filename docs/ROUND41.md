# Round 41 — slice histogram, time budget in the collector, and a miscompile in the optimizer

Two planned items (diagnostics and the time budget of the GC) and one
unplanned one that outweighed everything else: the optimizer of `firnc0`
had been generating wrong code since round 40, and the checking apparatus
did not report it.

## 1. Histogram of the INDIVIDUAL slices (task 1 from round 40)

Until now `__gc_pause_buche` only booked maxima: longest pause per cycle,
longest per type. For the question „is the slice too large?" that is
useless — a maximum is noisy, a distribution is not. New in `lib/gc/gc.fi`:

* `S_HIST`: 7 × 16 buckets in the state block (448…1344, block size 4096).
  Bucket 0 = below 1 µs, bucket k = [2^(k−1) µs, 2^k µs), bucket 15 = from
  16,384 ms on.
* Types 0–4 as in `gc_pause_max_typ` (start/mark/sweep/end/full), **type 5 =
  re-marking after a mark stack overflow** (in the maximum it hides under
  type 1), type 6 = all slices together.
* Query `gc_hist(typ, fach)`, zeroing with `gc_hist_reset()`.
  `tools/gc_meas/pause_big.fi` zeroes after the build-up and prints all
  occupied buckets.

The first finding came right away with the new tool: type 5 stayed
**empty** across all runs — the mark stack never overflows, the expensive
re-marking does not occur in continuous operation. In round 40 that had
only been surmised.

## 2. Time budget instead of an object count (the open point from round 40)

Until then the marking slice traced **512 objects**, no matter how
expensive they are. A node with many pointer fields costs a multiple of a
text node — in the histogram the slices therefore scattered over three
orders of magnitude. New: `ZEIT_BUDGET_NS = 100 000` (100 µs), and the
clock is read every `ZEIT_PROBE = 16` objects (`__gc_jetzt_ns` itself costs
~25 ns).

Measurement `pause_big.fi`, 120 000 live nodes, heap 13 MiB, 20 s each,
marking slices (type 1):

| Bucket (slice duration) | before (object count only) | probe 64 | **probe 16 (built in)** |
|---|---|---|---|
| 64–128 µs   | 0      | 122 330 | **172 824** |
| 128–256 µs  | 10 306 | 32 268  | 137 |
| 256–512 µs  | 50 101 | 140     | 140 |
| ≥ 512 µs    | 458    | 3       | 2 |

Throughput (cycles in the same time window) unchanged within the noise
(2 198 000 / 2 347 000 / 2 231 000), sum of pauses unchanged. The budget
therefore costs nothing and cuts off the tail.

60 second control run on the final state (`tools/gc_meas/slices60.tsv`):
518 137 marking slices, of those **99,88 % in 64–128 µs**, 284 above that,
three individual cases above 1 ms (descheduling by the operating system,
not the collector). `pause_max_typ4 = 11,7 ms` are still the **three full
runs of the build-up phase** below `INKR_AB` — known and named.

## 3. The actual find: `.astdump` hung on every `||`

While verifying, `test.sh` stopped in section 12 (parser comparison) —
not slow, **hanging**: `./.astdump bin/lexdump.fi` ran for 45 minutes at
full CPU and ignored SIGTERM. Narrowed down with gdb on the hanging
process:

```
#0 rt__buf_wachse (bin/rt.fi:187)   rsi = 0xffff8002d74f884b   <- Länge < 0
#1 rt__buf_push_bytes
#2 druck__schreib
#3 druck__drucke_binop
```

`buf_grow` doubles `cap` until `cap >= needed`; with an underflowed
length `kap` overflows past 0 and the loop spins forever. Minimal case:
**every file that contains `||`.** `&&` was inconspicuous.

The cause is not a parser bug but **wrong machine code from
`firnc0`** — in the assembly of `drucke_binop`:

```
lea r12, [rbp+r12-1491]   ; &tab[start]  overwrites the cell register of start
mov r13, 43
sub r13, r12              ; 43 - ADRESSE statt 43 - start
```

The guilty party is the **cell alias from round 40**
(`compiler/src/regalloc.rs`): a `load` may read the cell register directly
instead of loading. But the allocation had long since run by then — it did
not know about the **lifetime extended** by the alias and was allowed to
hand the same register to another value. Two holes, both now closed:

1. Between the load and the use **no other value** may write into the
   cell register (`belegt_register`).
2. If the cell register is **not** in `CALLEE_SAVED`, an intervening
   `call`/`syscall` ends the alias — a call destroys caller-saved
   registers. (Symptom: `bin/layoutdump.fi` crashed in
   `intern_finde` with `t = 0`.)

**Cost of the correction: none.** callgrind on the realweb corpus,
`tokenize_bench`:

| Compiler | I refs realweb |
|---|---|
| before the correction (faulty) | 1 297 226 146 |
| after the correction | 1 297 226 150 (+4) |

## 4. Why 649/649 did not notice this

Two reasons, both annoying:

* The dump binaries (`.astdump`, `.layoutdump`, …) had been **reused in a
  stale state** over a long time. Round 40 did build the rebuilding in —
  but the version that contained the bug was only built afterwards. The
  first honest run with fresh binaries was this one, and it hung
  immediately.
* Two simultaneous runs (main repo + worktree) used **the same
  `/tmp` files** (`/tmp/parv_a.txt`, `/tmp/lexv_a.txt`, …) and
  overwrote each other's comparison outputs. That produced
  „148 UNGLEICH" which disappeared on an individual re-run. All six
  comparison tools now create their own `mktemp -d`.

The regression test `tests/opt/cells_alias_clobber.fi` describes the
pattern; **honestly noted**: at this size it does not trigger the bug
itself (whether the register is overwritten depends on the register
pressure of the whole module). The reliable guard remains section 12 of
`test.sh`.

## 5. Verification of this round

* `bash ./test.sh` → **PASS 652/652**
* `bash tools/self_compare.sh` → **189 identical behavior**, 0 differences
* `bash tools/fixpoint.sh` → stage 2 == stage 3, character-identical,
  **309 468 lines** of assembly (grown because `lib/gc/gc.fi` got bigger
  with the histogram and the time budget)
* callgrind realweb unchanged (see above)

## 6. Open

* **realweb ≤ 2×** (currently 2,68×) — next documented lever:
  interval splitting in the regalloc; `tokenize` is the biggest item with
  38 %, `tok_attr_value_push` 8 %.
* **Build-up phase of the GC**: three full runs of up to 11,7 ms, as long as
  the heap is below `INKR_AB` (8 MiB). For a 16 ms frame that is too much;
  the remedy would be to use the incremental cycle during growth already.
* Finalizers and `Arc[T]` still open (named in round 38).
