# Round 37 — the optimizer attack on the ≤2× goal

Starting point (commit 97ec31a, round 36): tokenizer workload against
html5ever (`tools/tokenizer/throughput.sh`, best of 3 resp. 7 runs):

| Corpus | Firn MB/s | html5ever MB/s | Factor |
|---|---|---|---|
| html5lib (pathological) | 5,94 | 11,49 | **1,94×** |
| realweb (real pages) | 8,96 | 43,19 | **4,82×** |

## Profile (callgrind, corpus realweb, 2,63 G instructions)

| Share | Function | Finding |
|---|---|---|
| 30,9 % | `dekodiere` | UTF-8→UTF-32 pre-pass (html5ever does not need it) |
| 24,2 % | `eingabe_pruefen` | ~108 Ir per character: call+prologue overhead, 49 FIR insts > inline limit 40 |
| 22,3 % | `tokenize` | state machine, fully call-free after opt |
| 12,7 % | `main` | outer loop, sink management |

Observation on the machine code (`dekodiere`, 483 x86 instr.): **47 % `mov`**
(87 reg→reg, 79 reg→mem, 59 mem→reg) and 57 branches. 49 FIR instructions
become ~135 x86 instructions — a factor of ~2,7 in overhead through
stack traffic and copies. `tokenize` and `dekodiere` are completely
call-free after the optimization pipeline — so the emission path itself was
the lever, not (only) the allocation.

## Measurement series

| # | Optimization | html5lib | realweb | Status |
|---|---|---|---|---|
| 0 | baseline (97ec31a) | 1,94× | 4,82× | — |
| A | inline limits 40/8 → 60/10 | 2,04× | 4,87× | **REJECTED**: no gain, breaks `520_gc_weak [opt]` (exit 6) + selbst_vergleich (firnc1: 127 at bubblesort) — latent inline.rs bug with larger blocks. Reverted. |
| B | branch fallthrough (Br/BrCond/cmp+jcc, target==next block) + `cmp` directly into the target register | 1,76× | 4,64× | ✅ 640/640, commit 977a2ad |
| C | register pools rsi/rdi (without call/memop) + rdx (without call/div/select) in the linear scan | 1,71× | 4,42× | ✅ 640/640, commit ef4e530 |
| D | inline limits 60/32 + DAG exception | 1,50× | 3,69×…8,20× | **REJECTED** (see below) |
| E | shift with a constant distance as an immediate form (`shl $k`), directly into the target register | 1,71× | 4,36× | ✅ (within the noise; Ir −0,003 %) |

## The inline object lesson (D)

Several limit variants were measured through; the insights matter more
than the rejected change:

* **`__gc_scrub_tief` is recursive and scrubs the stack via the recursion
  DEPTH.** Inlining unrolls one level and moves the 4 KiB buffer into the
  caller's frame — the scrubbing loses its effect,
  `520_gc_weak` failed with exit 6, selbst_vergleich with firnc1 rc=127.
  → **Kept: self-reachable (recursive) bodies are never inlined**
  (`erreicht_sich_selbst` in inline.rs, precomputed).
* **`520_gc_weak` is frame-layout-fragile** (conservative stack scan without
  a scrub between `anlegen` and `gc_collect`): even inlining
  `__gc_strong_raw` (value body → result alloca in the caller) tipped the
  test over.
  → Recommendation for the GC round: make the stack scrub in the test path
  robust.
* **Inlining `eingabe_pruefen` (49 insts/29 blocks) blows `tokenize` past
  the regalloc safety limit** (nv×nb ≈ 8M → pure stack model)
  → 7,7×. More aggressive inlining FIRST needs a regalloc that copes with
  large functions (interval splitting; the 8M limit is a safety net, not a
  solution).
* Side finding: `hat_schleife` detected wrong loops after `merge-blocks`
  when using the number test (`ziel <= id`) (join `bb14→bb12`) — replaced by
  a real cycle test; properties are now precomputed once instead of per
  scan (endless compile time fixed).

## Patterns constantly visible in the generated code (map for round 38+)

* **445×** statically `mov [rbp-X], rA` followed by `mov rA, [rbp-X]`
  (spill store with an immediate reload of the same value) — a
  register descriptor cache in the RA emission path would delete them.
* **7391×** reg→reg `mov` statically: SSA form writes every instruction into
  "its" register, and the consumer movs it onward — classical **coalescing**
  (the use gets the register of its single producer) is the next big
  lever, together with interval splitting.
* Structurally: `dekodiere` (31 % Ir) is a separate UTF-8→UTF-32 pass that
  html5ever does not need; `eingabe_pruefen` costs ~108 Ir/character as a
  call. That is the architecture of the benchmark, not an optimizer failure.

## The Firn side (firnc1)

firnc1 deliberately contains NO optimizer (docs/SELF_HOSTING.md: „The
optimizer comes last; firnc1 may work without it"). The fir comparison
runs on `--emit=fir-raw`, i.e. BEFORE any optimization — changes to
`opt.rs`/`inline.rs`/`regalloc.rs` do not change the compared raw FIR.
The fixpoint stays character-identical because the source text of firnc1
remains untouched. There is therefore nothing to mirror in this round;
checked via selbst_vergleich (186/0/0) and fixpunkt.sh in every test run.


## Final numbers (commit bf13ed4)

| Corpus | round 36 baseline | round 37 | Verdict |
|---|---|---|---|
| html5lib | 1,94× | **1,69×** | goal ≤2× reached |
| realweb | 4,82× | **4,34×** | intermediate goal ≤3× missed |

Overall healthy: **test.sh 640/640**, selbst_vergleich 186/0/0,
fixpoint 284207 lines character-identical. Three commits: 977a2ad
(fallthrough + cmp→target register), ef4e530 (register pools), bf13ed4
(inline correctness + shift immediate form).

## The next lever (priority for round 38)

1. **Regalloc: interval splitting + coalescing** (7391 static reg→reg movs,
   445 store/reload pairs; afterwards the compiler tolerates larger
   functions and aggressive inlining of `eingabe_pruefen` & co. becomes
   possible — that is the documented path to ≤3×).
2. Register descriptor in the emission path (delete the store→reload pattern
   directly).
3. GC round: make `520_gc_weak` frame-layout-robust (stack scrub in the
   test path), otherwise every inlining improvement stays a game of roulette.
