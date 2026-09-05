# Round 92 — the value that is written twice

Branch `r92-phi`. One thing: **FIR gets phi nodes**, and `mem2reg` becomes the
algorithm it should always have been. No new optimization is built on top of
them — that is deliberate, and section 7 says what comes next.

Everything below was **run**. Every number has the command next to it.

**Machine:** AMD EPYC 7571, 8 vCPU, Debian 12, Linux x86_64.
**Toolchain:** `rustc` 1.99.0-nightly, GNU `as`/`ld` 2.40, `valgrind` 3.19
(callgrind), `qemu-aarch64` 7.2.
**Base:** `main` at `51b43d46` (acceptance completely green, PASS 1533/1533).

**A word on the wall clock in this round.** The machine ran two other full
acceptance passes at the same time as this one; the load average sat between
10 and 16 for the whole session. A median of seven runs moves by more than
ten percent under that, so the speed section below leans on
`tools/bench90/icount.py` — **executed instructions**, counted by callgrind,
deterministic to the last digit and completely indifferent to the
neighbours. The wall clock is reported next to it and marked as what it is.

---

## 1. The wall

`docs/ROUND90.md` closed with: `release-fast` is a median **1.81x** behind
`rustc -O`, and the code generator has not moved for several rounds. The
cause was one thing, and it was structural.

`compiler/src/fir.rs`, the header, until this round:

```
//!  * no phi nodes: mutable variables sit in `alloca` slots and are
//!    addressed with `load`/`store`
```

That sentence decides everything that follows it. Without a phi there is no
way to write down "the value here is the one from the loop body on the back
edge and the one from the preheader on the first pass". So `mem2reg.rs` could
only resolve an `alloca` that is written **exactly once**, with the `store`
dominating every `load` — and its own comment said so:

```
//!    resolved: every `load` is replaced by the stored value. FIR knows no
//!    phi nodes — that is why the dominance condition is mandatory here and
//!    not merely an optimization
```

Every loop counter is written on every pass. So every loop counter stayed in
**memory**, and everything that wants to reason about it was impossible:

* no induction variables (the counter is a `load`, not a value),
* no loop invariant motion of anything that reads the counter (`licm.rs`
  refuses `load`, and rightly so without alias analysis),
* no range analysis across a back edge,
* no auto-vectorisation.

`compiler/src/opt.rs` even had a test that asserted the wall:

```rust
#[test]
fn loop_stays_untouched_and_terminated() {
    // while (i < 10) { i = i + 1 }  — nothing of that is constant foldable,
    // the optimizer may remove nothing here and must halt.
```

That test was not a wish. It was an honest description, and it is the test
this round rewrote (it is now `loop_counter_becomes_a_phi`, and it demands
that nothing touches memory).

### What the register allocator had been doing about it

`regalloc.rs` has had **cell promotion** since round 40, and its header says
what it is for:

```
//!  2. **Cell promotion:** an `alloca` whose pointer never escapes ... lives
//!     entirely in a register — ... That replaces the phi nodes which FIR
//!     does not have
```

That is the honest half of this round's starting position: on **x86** the
loop counter was **already in a register**. What it could not do is help
anything above it — the optimizer still saw a `load`. And it does not exist
at all on the second machine: `codegen_a64.rs` has no register allocation, so
on aarch64 the counter really did live in the frame, one `ldr` and one `str`
per pass.

---

## 2. What was built

### 2.1 `Op::Phi` and `Op::Copy` (`compiler/src/fir.rs`)

```rust
Phi { incoming: Vec<(BlockId, Val)> },
Copy { src: Val },
```

Three invariants, and they are **checked**, not believed
(`Func::verify_phis`):

1. phis stand at the front of their block,
2. exactly one entry per **distinct** predecessor, none for a block that is
   not one,
3. the entries are sorted by block number — determinism, because two runs of
   the compiler have to write the same text (`tools/fixpoint.sh`).

`Op::Copy` is `dst = src` and nothing else. It has exactly one producer,
`phi.rs`, and no optimizer pass ever sees one.

### 2.2 The lowering does not change, and that is the point

`Op::Phi` is created by **`mem2reg`** — an optimizer pass — and never by
`lower.rs`. `--emit=fir-raw` is therefore phi-free, and `tools/fir_compare.sh`
(which compares that text octet for octet against the compiler written in
Firn) had nothing to notice. Section 6 is about what that means for `firnc1`.

### 2.3 `mem2reg` becomes Cytron (`compiler/src/mem2reg.rs`)

The textbook algorithm, in the three parts it has always had:

* **`idoms`** — immediate dominators, Cooper/Harvey/Kennedy: reverse
  postorder, one `u32` per block, an intersection that walks two chains
  upwards. Deliberately not the existing `dominators()`, which returns an
  n × n matrix of `bool` — `bin/firnc1.fi` has functions with thousands of
  blocks, and the matrix alone would be megabytes per function.
* **`dom_frontiers`** — Cytron et al., figure 10. `y` is in `DF(x)` when `x`
  dominates a predecessor of `y` but not `y` itself: the first place at which
  a definition made in `x` meets one made somewhere else, which is exactly
  where a phi belongs.
* **`promote_allocas`** — phi placement per variable over the frontiers, then
  one walk over the dominator tree with a stack per variable. A `load`
  becomes the top of the stack, a `store` pushes, and the phi entries of the
  successors are filled as the walk leaves a block. Iterative, not recursive:
  a recursive walk dies on `gctext__gctext_write`.

Plus **`simplify_phis`**, the hygiene: entries for blocks that stopped being
predecessors are dropped, and a phi with only one answer left stops being a
phi. Entries are never *invented* — a pass that adds an edge into a block
with a phi would have to say what value travels along it, and none does.

What is deliberately **not** promoted, each for a reason:

| left in memory | why |
|---|---|
| the address escapes | it is not a variable, it is storage |
| mixed access widths (`store` 8, `load` 4) | that is memory semantics |
| `secret` and anything an untouchable instruction reads | SPEC §9.2: a `select`'s operands are never rewritten, so a value it reads may never be deleted out from under it |
| `v128` | no backend has a phi-capable vector path |
| the bool cells of a fork block | see 2.5 |

### 2.4 `phi.rs` — elimination, once, for all three backends

No machine has a phi. Somewhere it has to become what it always meant: a copy
at the end of every predecessor. Round 90's finding was that **one question
must not have two answers**; there are three backends (the x86 base path, the
register aware path in `regalloc.rs`, the A64 path), so this happens **once**,
on FIR, in `main.rs` between the optimizer and the code generator. The arms
that catch `Op::Phi` in the three backends return an internal error instead of
emitting anything.

The hard part is that the copies of one edge happen **simultaneously**:

```
bb3:  %a = phi [bb2 %b, ...]        two values swapping on a back edge
      %b = phi [bb2 %a, ...]
```

Written out one after another that gives both the same content. The
sequentialization is the standard one — emit any copy whose target is nobody
else's source; when nothing is left that is safe, the rest is a cycle, so
rescue one target into a fresh value and the cycle opens at that one place.
`tools/phi/loops.fi::rotate` is a three-cycle and `swap_n` a two-cycle, and
both are checked by their exit code.

**Critical edges are deliberately not split.** The textbook splits them
because a *coalesced* copy placed too early can overwrite a value another path
still needs. Nothing here coalesces across paths: a copy defines a **new**
value that no other path reads, and `regalloc.rs` computes liveness from the
code it actually gets. Splitting would add a block per critical edge for no
gain. **This has to change the day the allocator learns to coalesce** — it is
written into the header of `phi.rs` so it is not rediscovered.

### 2.5 What had to give way

* **`threading.rs`** (jump threading through bool cells, the short circuit of
  `&&`/`||`) folds a bool cell that is read in a block of its own into the
  jumps of its predecessors. That only works while the cell is still there.
  `mem2reg` runs first in the round and would have eaten it — the
  optimization would silently have stopped happening and no test would have
  failed. So `mem2reg` asks `threading::fork_cells` and leaves those cells
  alone for exactly one round; afterwards the fork block is unreachable and
  the cell is promoted like any other. `threading.rs` in turn refuses to work
  on a function that already has phis, because it **redirects edges**, and no
  pass can invent the value that travels along a new one.
* **`merge_blocks`** does not bridge an empty block whose target has phis
  (two edges that arrive through two different empty blocks may carry two
  different values; afterwards they arrive from the same predecessor and
  there is no honest single answer). It does still *merge* a single
  predecessor into it, and re-keys the entry — see 4.4 for the trap in that.
* **`inline.rs`** remaps the block numbers in the callee's phi entries, and
  re-keys the caller's — see 4.2.
* **`opt.rs::collect_uses`** counts phi operands separately, because two phis
  in a loop read each other and would otherwise hold a dead counter alive for
  ever.
* **`remove_unreachable_blocks`** renumbers the phi entries with the blocks.

### 2.6 `coalesce_chains` — giving the copies back

A phi is a copy per edge, and a nested `if`/`else` makes a cascade of joins
with a phi in every one. Left alone that is **+43 % instructions** on
`bench/firn/statemachine.fi` (4.6). `phi.rs::coalesce_chains` merges a phi
whose single reader is one entry of the next phi in the chain into that phi:
they never coexist, so they can share a name, and every copy between them
becomes `x = x`. It is register coalescing done on the phi graph, where the
shape is simple enough that no interference graph is needed. What it does
NOT cover is a value that is read somewhere else as well — that is real
coalescing, and it is section 7.

---

## 3. Measured: the counter really does leave the frame

`bash tools/phi/run.sh` — new, and section 49 of `test.sh`. The programs are
`tools/phi/loops.fi`.

### 3.1 FIR

```
@sum_to: 2 phi nodes, 0 alloca/load/store
without the pass: 14 alloca/load/store in the same function
```

`fn sum_to(n: u64) -> u64 { var s = 0; var i = 0; while i < n { s = s + i; i = i + 1 } return s }`
at `--opt-level=release-fast --emit=fir-opt`:

```
fn @sum_to(%0: u64) -> u64 {
bb0:
  %3 = const.u64 0
  %13 = const.u64 1
  br bb1
bb1:
  %17 = phi.u64 [bb0 %3, bb2 %11]
  %18 = phi.u64 [bb0 %3, bb2 %14]
  %8 = cmp.lt.u64 %18, %0
  brcond %8, bb2, bb3
bb2:
  %11 = add.u64 %17, %18
  %14 = add.u64 %18, %13
  br bb1
bb3:
  ret %17
}
```

Fourteen memory instructions down to none, and `licm` has already taken the
`const 1` out of the loop — something it could not do while the counter was a
`load`.

### 3.2 x86_64 — shorter, not longer

The inner loop, `--opt-level=release-fast --emit=asm`:

| | round 91 (`--no-pass=mem2reg`) | round 92 |
|---|---|---|
| `sum_to` loop body | `lea rdi,[r11+r10]` / `mov r11,rdi` / `lea r10,[r10+1]` | `lea r9,[r9+r10]` / `lea r10,[r10+1]` |
| `sum_to`, whole function | 17 instructions | **15** |
| `rotate`, whole function | 29 instructions | **24** |
| `swap_n`, whole function | 23 instructions | **20** |
| frame accesses in `sum_to` | 0 | **0** |

Zero frame accesses in **both** columns — that is the honest result, and it
is why `regalloc.rs`'s cell promotion is quoted in section 1. What is new on
x86 is that the code got **shorter**, and that took work: a phi is a copy per
back edge, and an unfolded copy is a `mov` in the innermost loop. Measured
before the folding was written, `sum_to`'s body was **one instruction
longer** than round 91's. `phi.rs::fold_into_definitions` gives it back by
letting the instruction that computes the value write the phi's value
directly (`%17 = add %17, %18` instead of `%11 = add %17,%18` + `%17 <- %11`),
under four conditions that are each there because dropping one produces wrong
code — see 4.3.

### 3.3 aarch64 — where there was nothing to rescue the counter

`codegen_a64.rs` has no register allocation. Same function, same flag,
counting `ldr`/`str` in the whole body:

| | loads + stores in `sum_to` |
|---|---|
| `--no-pass=mem2reg` (round 91's state) | **50** |
| round 92 | **18** |

**64 % of the memory traffic of that function is gone.** This is the machine
the round is worth the most on, and it is the one where nothing downstream
was compensating.

---

## 4. What went wrong, and what it cost to find

Four bugs, all found by the corpus, all of them the same shape: something
below the optimizer assumed a property that SSA used to guarantee and no
longer does.

### 4.1 `tests/018_while_sum.fi` returned 0 instead of 55

The first program of the corpus to break, and the smallest. `regalloc.rs`
collects values whose definition is an `Op::Const` and uses them as x86
immediate operands. Above the code generator FIR is SSA, so "this value is
defined by a `const`" and "this value **is** that constant" were the same
sentence.

`phi.rs` ends that. A phi's value is written from every predecessor; a
counter that starts at 0 has a `const 0` writing it in the preheader and an
`add` writing it on the back edge. Every read of the sum was replaced by the
immediate `0`, and the `add` wrote a value nobody looked at again.

The fix is a definition count in `immediate_consts` and in
`codegen_a64.rs::layout`. There is no third place — but there was nearly a
worse one: the **cell alias**, where a `load` out of a register-resident
`alloca` maps its result value to that register. That one cannot be taught to
count cheaply, so `phi.rs::foldable_def` is a **whitelist** of plain
computations rather than a blacklist of dangerous ones. A new `Op` variant is
not foldable until somebody has thought about it.

### 4.2 `tests/303_wtf8_roundtrip.fi` — only at the levels that inline

```
dev          -> 65536 2048 2049 0 1114112 0     (right)
dev-fast     -> 65536 2048 2049 0 1114112 0     (right)
release-safe -> 0 0 2063 0 2048 1112064         (wrong)
release-fast -> 0 0 0 0 2048 1112064            (wrong)
```

The only pass that separates those two pairs is `inline`. Inlining **splits
the calling block** at the call site and moves its **terminator** into a new
continuation block. So everything the calling block used to jump to is now
jumped to by the continuation — and a phi in one of those blocks still named
the calling block as the edge its value comes in on. `phi.rs` then put the
copy at the end of a block the control flow no longer takes.

### 4.3 `sqrt(2.0)` came out as `1.5`

`lib/std/core.fi::sqrt` runs Newton until the value stops moving:

```
while g != old { old = g; g = (g + x / g) / 2.0 }
```

The back edge is the parallel copy `{ g <- gnew, old <- g }`. Folding
`g <- gnew` into the instruction that computes `gnew` writes `g` in the
**middle** of the latch — and the second copy reads `g`. It read the new one,
`old == g` held after a single pass, the loop ended, and **1.5 is exactly the
first Newton step for 2**.

The first version of the fold only looked at the copies it had already
decided about, not at the ones still to come. It now takes the sources of the
**whole edge** before anything moves. `tests/1453_f32_library.fi` printed
`sqrt 1.5`; the regression test is `phi::tests::a_copy_that_another_copy_reads_is_not_folded_away`.

### 4.4 `tests/800_std_str_core.fi` printed nothing at all

Right at `dev` and `dev-fast`, silent at `release-safe` and `release-fast`.
This is the one that would have been hard without a verifier, so the round
built one: `FIRN_VERIFY_PHI=2` checks the invariants **after every pass** and
names the pass. It said:

```
PHI BROKEN after 'dce': @main bb160: phi %1585 has TWO entries for bb159
```

and the FIR showed `%1585 = phi.bool [bb55 %1582, bb55 %1584]` — one
predecessor, two different answers.

The cause: `simplify-term` turns a `brcond` with a constant condition into a
`br` and leaves an entry behind for the edge that no longer exists.
`merge-blocks` then merges A and B and re-keys B's entry to A — on top of the
leftover entry that already named A. Two entries for one predecessor, and
`phi.rs` put two copies of two different values at the end of the same block;
which one won depended on the order the entries happened to stand in.

A's terminator was `br B`, so B was A's **only** successor: an entry naming A
in a block that B jumps to cannot be a live edge. It is thrown away before
the re-key.

### 4.5 The gate that came out of it

The lesson of 4.4 is not the fix, it is that a broken entry list becomes
wrong **machine code** with nothing in between. So `phi.rs::eliminate_func`
now calls `Func::verify_phis` **in every build**, for every function that has
a phi, after `simplify_phis` and before a single instruction is emitted. It
costs one predecessor table per such function. A failure stops the
compilation with a message that names the block, instead of shipping a
program that computes the wrong answer.

Deliberately **not** in `main.rs` after the optimizer: between two passes the
entry lists are *allowed* to be out of date, and making that an error would
report normal work as a fault.

### 4.6 `bench/firn/statemachine.fi` got 43 % slower, and why

Not wrong — slower, which for a round whose whole result is "a foundation"
is the more dangerous failure, because nothing goes red. Counted with
callgrind: **+43.3 % executed instructions** against round 91. The inner
loop, `--opt-level=release-fast`:

```
.Lmain__bb13:  lea r9, [rdi+1]     <- text = text + 1
               mov rbx, r8         <- state, unchanged, copied
.Lmain__bb14:  mov r15, rbx        <- and copied again
               mov r13, rdx
               mov r12, r9
.Lmain__bb11:  lea rsi, [rsi+1]    <- k = k + 1
               mov r8, r15         <- and a third time
               mov rdx, r13
               mov rdi, r12
```

Six register-to-register moves per octet of input that shuffle three
variables through three join blocks. The cause is not a bug, it is what SSA
destruction costs: a six-deep `if`/`else` tree makes a cascade of joins, SSA
construction puts a phi in every one of them, and every phi is a copy per
edge. Round 91 had no joins to pay for — the variable was one memory cell
that `regalloc.rs` kept in one register, and an assignment was a register
write.

The values in such a chain never coexist. `%X` in `bb14` is read by exactly
one thing, the phi in `bb11`, and is dead the moment it is read. So they can
share one name and every copy between them becomes `x = x`.
`phi.rs::coalesce_chains` does that, on the phi graph, where it needs no
interference graph. The conditions are in its header; the one that is easy to
get wrong is the critical edge — every predecessor of `B` must end in
`br B`, or the early write reaches a path that never goes to `C` at all.

Two attempts before it worked, and both are worth writing down:

1. **Bridging the empty join blocks away instead.** `merge_blocks` does not
   bridge into a block with phis (2.5), and the first idea was to teach it
   the re-keying so it could. It works and it is **worse**: the copies then
   move into a predecessor that has several successors, so they run on paths
   that do not need them. `main` went from 154 instructions to 173 while
   losing six blocks. The empty join block is exactly the right place for a
   copy — which is the same argument as "critical edges are not split"
   (2.4), seen from the other end. Reverted.
2. **A filter instead of a matching.** The first coalescing refused every
   pair whose target was another pair's source, to keep the conditions from
   going stale inside one round. In a real chain `x1 -> x2 -> x3` *every*
   pair is of that kind, so it refused all of them: five pairs found in
   `@main`, none applied, and the measurement did not move by one
   instruction. A greedy **matching** takes `x1 -> x2` and `x3 -> x4` in one
   round and `x2 -> x4` in the next; the chain is gone in two.

Afterwards `@main` is **131 instructions where round 91 had 133**, and the
program costs +4.2 % instead of +43.3 %. The rest is the copies that are not
in a chain, and those need real coalescing (section 7).

---

## 5. Speed

**Why instructions and not seconds.** Two other full acceptance passes ran on
this machine for the whole session; the load average sat between 10 and 26.
A wall clock median of seven runs is worth nothing under that.
`tools/bench90/icount.py` counts the instructions the program really executes
with callgrind: deterministic to the last digit, indifferent to the
neighbours, and exactly the right question for "did this change make the loop
shorter". It says nothing about cache misses — for those the wall clock stays
the measurement, and section 5.3 says what is known and what is not.

```
python3 tools/bench90/icount.py --firnc <round 91 build> --tag before \
    --only fib,sieve,matmul,bubblesort,bitmap,statemachine
python3 tools/bench90/icount.py --tag after --only <the same six>
```

The six are the ones that fit into callgrind's budget (it is about fifty
times slower than the machine); `xxhash`, `memstride`, `branchy`,
`bytecount` and `jsonscan` move hundreds of megabytes and were left out.

### 5.1 `--opt-level=release-fast`

| benchmark | round 91 | round 92 | change | vs `rustc -O` before | after |
|---|---:|---:|---:|---:|---:|
| fib | 281,966,500 | 281,966,451 | −0.0 % | 1.81x | 1.81x |
| sieve | 316,748,507 | **241,974,999** | **−23.6 %** | 1.70x | **1.30x** |
| matmul | 709,305,973 | **501,309,945** | **−29.3 %** | 4.10x | **2.90x** |
| bubblesort | 315,882,787 | **306,584,277** | −2.9 % | 1.59x | 1.54x |
| bitmap | 782,265,538 | **656,270,449** | **−16.1 %** | 1.86x | **1.56x** |
| statemachine | 724,776,148 | 754,975,101 | +4.2 % | 1.19x | 1.24x |
| **median vs `rustc -O`** | | | | **1.76x** | **1.55x** |

`fib` is recursion with no loop counter at all and does not move by a single
instruction — which is the right answer and a good check that nothing is
being measured that is not there.

### 5.2 `--opt-level=release-safe`

| benchmark | round 91 | round 92 | change |
|---|---:|---:|---:|
| fib | 303,114,193 | 303,114,138 | −0.0 % |
| sieve | 526,329,429 | 421,552,109 | −19.9 % |
| matmul | 2,125,360,205 | 1,917,012,576 | −9.8 % |
| bubblesort | 857,316,094 | 821,260,528 | −4.2 % |
| bitmap | 1,004,558,731 | 826,728,098 | −17.7 % |
| statemachine | 1,004,955,880 | 983,145,470 | −2.2 % |

Every one of the six is at worst unchanged at the checked level.

### 5.3 What is NOT claimed

* **No wall clock number from this session.** The load made it meaningless;
  `docs/ROUND90.md`'s 1.81x median stands as the last honest wall clock
  figure and this round did not re-measure it. Instruction counts are not
  seconds: `matmul` writes to memory in a cache-hostile order, and 29 % fewer
  instructions there will not be 29 % less time.
* **`statemachine` is 4.2 % worse at `release-fast`**, and that is the one
  number in this table that went the wrong way. It is the cost of SSA
  destruction on code that is nothing but joins — see 4.6 for what it was
  (43 %) before the coalescing, and section 7 for what would take the rest.


---

## 6. The compiler written in Firn

`bin/firnc1.fi` and `lib/firnc1/` needed **no change**, and that is a
statement with a proof, not a hope.

`firnc1` has no optimizer. Its own source says so
(`bin/firnc1.fi`: *"`firnc1` has no optimiser of its own, so the level says
nothing else"*): the pipeline is lexer → parser → checker → lowering →
code generator, and `--opt-level` selects only the arithmetic checking.
`Op::Phi` is produced by `mem2reg`, an optimizer pass. So `firnc1` never
makes one and never has to take one apart.

The two places where the two compilers are held against each other agree:

* **`tools/fir_compare.sh`** compares `firnc0 --emit=fir-raw` — the FIR
  *directly after the lowering* — octet for octet against `firnc1`'s. The
  lowering is unchanged by this round and `fir-raw` is phi-free by
  construction, so there is nothing there to diverge.
* **`tools/fixpoint.sh`**: stage 2 and stage 3 are **character-identical**,
  757,692 lines of assembly, and the self-compiled compiler behaves like
  `firnc0` over the whole corpus (328 same, 0 differing, 0 faulty).

The day `firnc1` grows an optimizer, it grows this round with it. Until then,
porting `Op::Phi` into `lib/firnc1/fir.fi` would be two instruction kinds that
nothing produces and nothing reads — dead code with a fixpoint to keep green.

---

## 7. What this round did NOT do

On purpose. The phi is the foundation; the things that stand on it are the
next round, and packing them in here would have meant merging a half-tested
foundation.

1. **Induction variables and range analysis across a back edge.** Now
   possible for the first time: `%18 = phi.u64 [bb0 %3, bb2 %14]` with
   `%14 = add %18, 1` is a textbook affine induction variable, and its range
   follows from the loop condition. That is what deletes the remaining bounds
   checks in `matmul` and `bitmap`.
2. **LICM of everything that touches the counter.** `licm.rs` still refuses
   `Op::Load`, and rightly so without alias analysis — but the address
   arithmetic derived from a counter is no longer a `load`, so it can move.
3. **Copy coalescing in the register allocator.** `phi.rs` folds the copy
   into its definition where it can, which covers the common loop shape; what
   it cannot cover is a value that is read somewhere else as well. Real
   coalescing needs an interference graph, and **the day it exists, critical
   edges have to be split** (see 2.4).
4. **Jump threading on phis.** `threading.rs` works on bool cells in memory
   and is now kept alive by the `fork_cells` handshake. The modern form —
   `brcond` on a phi with constant incoming values, redirecting the edges —
   is a better pass, but it adds and removes edges, and this round's rule is
   that no pass invents a phi entry.
5. **Auto-vectorisation.** Needs 1 and 2 first.

---

## 8. Acceptance

<!--ACCEPTANCE-->
