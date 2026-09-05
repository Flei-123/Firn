# Round 90 — the instruction that writes a register it does not name

Branch `r90-mulbug`. Two things: a wrong-code bug that made
`--opt-level=release-safe` unusable and `--opt-level=dev-fast` — the default
— unreliable, and the speed work that the fix opened the door to.

Everything below was **run**. Every number has the command next to it.

**Machine:** AMD EPYC 7571, 8 vCPU, Debian 12, Linux x86_64.
**Toolchain:** `rustc` 1.99.0-nightly (c98d0cb27 2026-08-12), GNU `as`/`ld` 2.40,
`valgrind` 3.19 (callgrind).
**Base:** `main` at `4536a191`.

---

## 1. The bug

The osum kernel (`karstos`) ported its bitmap frame allocator to Firn, built
it with `--opt-level=release-safe`, and **19 of its 23 cases failed**. On
every other build level the same source was flawless. The minimal case it
sent, twenty lines, is now `tests/1900_mul_clobbers_rdx.fi`.

```
a6:  mov  %rcx,%rdx        # the fourth parameter lives in rdx from here
df:  mul  %rcx             # <-- writes RDX:RAX. rdx is gone.
e2:  jb   ea               # the overflow arm of a CHECKED multiplication
```

x86's one-operand `mul` puts the full product in `rdx:rax` and names neither
register. `regalloc.rs` hands `rdx` out as the home of a value. It has to
know which registers an instruction destroys, and for this one it did not.

### Why it took both halves

It needs **checked arithmetic** (only that emits `mul` at all — an unchecked
`* 8` is a `shl`) **and register allocation** (something has to be living in
`rdx`). The four build levels:

| level | checks | register allocation | affected |
|---|---|---|---|
| `dev` | yes | no — debug lines send every function to the base path | no |
| `dev-fast` | yes | **yes** | **yes** |
| `release-safe` | yes | **yes** | **yes** |
| `release-fast` | no | yes | no |

The report said `release-safe` only. It was wrong about `dev-fast`, and
`dev-fast` is the **default** level since round 72.

### The real cause: one question, two answers

The allocator asks "what gets destroyed while this value is alive" in two
places:

* round 49's list — `div`/`rem`/`select`/`cmpxchg` — in `divsel_pos`, the
  coarse interval question. Round 72 added its four new checked instructions
  **to this list**.
* round 87's `exact_crossings`, the finer control-flow question, which
  listed only what round 49 knew about — and, being finer, **won** wherever
  the liveness analysis converged.

So everything round 72 had added became invisible again the moment round 87
merged. Not a typo: two lists for one question, and no way to notice that
they had drifted apart.

### The fix

Not a third list. `regalloc.rs::inst_clobbers` is now **the single source of
truth**: for every instruction, which registers *out of the twelve the
allocator hands out* its emitted code destroys, as a bit mask.

* The coarse interval answer and the exact control-flow answer are two ways
  of summing **the same masks**.
* `fits(iv, r)` is one line: `iv.killed & reg_bit(r) == 0`.
* `crosses_call` / `crosses_memop` / `crosses_divsel` are gone.
* A new `Op` variant that clobbers something can be forgotten in exactly one
  place, and if it is a new variant the `match` says so at compile time.

The mask is also **narrower** than the three booleans it replaced, which is
worth registers on `release-safe`: a checked `+` or `-` writes no `rdx` at
all (only the unsigned one-operand `mul` does), and `copymem` writes
`rdi`/`rsi` but not `rdx`. All three used to ban `rdx` wholesale.

### The same blind spot, four more times

`descriptor_peephole` (the post pass that strikes redundant reloads) models
which instruction invalidates which register. It did not know `mul`, nor the
one-operand `imul`, nor `cpuid`, nor the `lock`-prefixed read-modify-writes,
nor `crc32` — all of them fell through its allowlist and invalidated
*nothing*.

That is fixed, and more importantly the allowlist is inverted: an unknown
mnemonic now **throws the whole descriptor away** instead of being ignored.
The worst a future instruction can cost there is a missed optimisation.

### What it was worth

The whole positive corpus (319 programs, `tests/`, `tests/opt/`,
`examples/`), compiler of `main` against this branch:

| build level | failed before | failed after |
|---|---:|---:|
| `--opt-level=release-safe` | **117** | **0** |
| `--opt-level=dev-fast` (the default) | **25** | **0** |
| `--opt-level=release-fast` | 0 | 0 |
| `--opt-level=dev` | 0 | 0 |

    # per level, against the compiler of main and against this one
    for f in tests/*.fi tests/opt/*.fi examples/*.fi; do
        firnc --opt-level=release-safe -o /tmp/b "$f" && /tmp/b; done

`firnc1` (the self-hosted compiler) has **no register allocation** — every
value lies in the frame, so there is nothing to destroy. Verified rather
than assumed: the minimal case and `tests/1900` pass through it in all four
levels.

### The guard that would have caught it

`tools/optlevels/run.sh`, new, and section 46 of `test.sh`. Fourteen
programs through **both** compilers in **all four** levels; every level has
to produce the same exit code and the same output. Plus counter-checks: a
program that goes out of range must **differ** between `release-fast` and
the checked levels (otherwise the comparison is measuring four levels that
all do nothing), and the exact crossing analysis has to agree with the
coarse one (`FIRN_RA_ROUGH=1`).

On top of that, `test.sh` section 3 now compiles every program at
`--opt-level=release-safe` as well. It never did — the level the suite is
supposed to bless was the one level it never ran.

---

## 2. Speed

### 2.1 The benchmark harness said "Firn" and meant "dev-fast"

`bench/bench.py` compiles the Firn side with `firnc -o x y.fi` — no build
level at all — and labels the column **Firn**. Since round 72 the default
level is `dev-fast`, and `dev-fast` **checks** integer arithmetic. Every
number in `bench/RESULTS.md` therefore holds a *checked* Firn build against
an *unchecked* `rustc -O` one and calls the difference "Firn is slower".

`sieve` stands there at **4.16x**. At `release-fast` it is **1.33x**.

`tools/bench90/bench.py` measures four columns and names them:

| column | build |
|---|---|
| firn release-fast | `firnc --opt-level=release-fast` |
| firn release-safe | `firnc --opt-level=release-safe` |
| rustc -O | `rustc -O -C overflow-checks=no` |
| rustc -O +checks | `rustc -O -C overflow-checks=yes` |

That separates the two questions that were mixed together: *how good is the
code generator* (release-fast vs `rustc -O`) and *what do the checks cost*
(release-safe vs `rustc` with the same checks turned on).

Five new pairs, each printing its result so nothing can be optimised away on
either side: `bitmap` (the osum frame allocator that opened this round),
`xxhash` (64 MiB, written with the wrapping operators a hash really wants),
`jsonscan` (the same tokeniser on both sides), `memstride` (256 MiB,
cache-hostile stride) and `branchy` (branches the processor cannot guess).

### 2.2 Where release-safe was really losing

The disassembly of `matmul`'s inner loop, `release-safe`, before this round.
Per **arithmetic operation**:

```
    mov rax, r8 / mov rcx, 240
    push rax / push rcx          <- rescue the operands for the message
    mul rcx
    jc .Lchksite                 <- not taken
    add rsp, 16                  <- drop the rescue
    jmp .Lchkok                  <- over the failure arm
.Lchksite:  pop rcx / pop rdx / lea rdi,msg / mov esi / mov r8 / mov r9 / jmp
.Lchkok:
```

Four instructions and two memory writes of pure overhead on the path that
never fails, plus six instructions of message building sitting **inside the
hot instruction cache lines**.

**The fix (stage 2b):** do not rescue the operands — *reload* them, in the
failure arm, from the homes they already have. Each backend hands
`panic_rt.rs` a closure that emits exactly the loads it used to fill
`rax`/`rcx` in the first place. An operand's home is by definition still
intact at the instruction that reads it, and the arm is reached only from
there. `Emitter::cold` collects the arms and flushes them **behind the
function**.

The price: an operand may no longer live in a register the instruction
itself destroys (`op_pins`). One register at one instruction, against stack
traffic paid every iteration.

**Stage 2d:** a checked `+`/`-` now computes **in the target register** with
the second operand as an immediate or a memory operand, exactly as the
unchecked path has done since round 51. `k = k + 1` went from

```
mov rax, r12 / mov rcx, 1 / add rax, rcx / jc site / mov r12, rax
```
to
```
mov r10, rbx / add r10, 1 / jc site / mov rbx, r10
```

The failure arm can no longer reload `a` when `a` lived in the target
register — it does not have to: for `+` the original is `d - b`, for `-` it
is `d + b`, both exact in two's complement, recomputed out of line on the
path that never returns.

### 2.3 What it measured

Wall clock, median of **9** runs, the four binaries measured in ONE
alternating pass so that machine drift cancels instead of landing on one of
them. "before" is round 90 **stage 1** — the compiler with the wrong-code
bug already fixed; against `main` there is no speed comparison to make,
because all eleven of these programs **segfault** when `main` builds them
with `--opt-level=release-safe`.

    RUNS=9 python3 tools/bench90/bench.py

| | before (stage 1) | after |
|---|---:|---:|
| median release-fast vs `rustc -O` | 1.82x | **1.81x** |
| **median release-safe vs `rustc -O +checks`** | **3.18x** | **1.84x** |
| **median price of the checks inside Firn** | **1.97x** | **1.19x** |

| benchmark | release-safe before | release-safe after | the checks cost, before -> after |
|---|---:|---:|---:|
| fib | 0.051 s | **0.044 s** | 1.14x -> **0.99x** |
| sieve | 0.094 s | **0.052 s** | 2.60x -> **1.47x** |
| matmul | 0.416 s | **0.182 s** | 6.96x -> **3.04x** |
| bytecount | 0.506 s | **0.324 s** | 1.55x -> **1.00x** |
| bubblesort | 0.234 s | **0.104 s** | 2.89x -> **1.27x** |
| statemachine | 0.227 s | **0.157 s** | 1.41x -> **0.98x** |
| bitmap | 0.131 s | **0.080 s** | 1.97x -> **1.22x** |
| xxhash | 0.363 s | **0.232 s** | 1.97x -> **1.25x** |
| jsonscan | 0.243 s | **0.144 s** | 2.01x -> **1.19x** |
| memstride | 0.283 s | **0.234 s** | 1.27x -> **1.05x** |
| branchy | 0.605 s | **0.523 s** | 1.14x -> **0.99x** |

Instructions really executed (`valgrind --tool=callgrind`, deterministic —
the wall clock on this shared machine still moves by several percent between
two passes, which is enough to hide a real five percent and to invent one
that is not there):

    python3 tools/bench90/icount.py

| benchmark | release-safe before | release-safe after | change |
|---|---:|---:|---:|
| fib | 429,999,174 | **303,114,113** | **-29.5 %** |
| sieve | 1,095,032,655 | **526,329,353** | **-51.9 %** |
| matmul | 4,456,691,599 | **2,125,360,125** | **-52.3 %** |
| bytecount | 5,682,464,133 | **2,974,915,175** | **-47.6 %** |
| bubblesort | 1,885,661,378 | **857,315,966** | **-54.5 %** |
| statemachine | 1,548,537,886 | **971,401,368** | **-37.3 %** |
| bitmap | 1,494,353,935 | **1,004,558,579** | **-32.8 %** |
| jsonscan | 2,264,001,094 | **1,202,000,710** | **-46.9 %** |

**`release-fast` is untouched, and not in the "about the same" sense**: the
emitted assembly of all eleven benchmark programs is character-identical to
what went into the round.

---

## 3. What did not work, and is written down so nobody repeats it

Two register allocation ideas were built, measured and **thrown away**. Both
are still in the file as dead code with the measurement in the comment,
because the next person will have the same idea.

* **A promoted cell does not live from the start of the function.** A cell
  had the interval `[0, last access]`; in `matmul`'s `main` that meant three
  `alloc()` calls in the first ten instructions, so all nine cells "crossed
  a call", all nine competed for the five callee-saved registers, and five
  lost and went to the stack — among them the counters of the innermost loop
  (`FIRN_RA_STATS=1`: `cellivs=9 cellslost=5`). `loop_ranges` /
  `widen_to_loops` compute the honest interval instead: first access to last
  access, widened over the enclosing loops (a variable in a loop is read
  again after the back edge). It works — cells in registers 4 -> 6,
  `cellslost` 5 -> 3 — and it buys nothing: `statemachine` -4.6 %,
  `bytecount` -0.9 %, `matmul` **+5.8 %**, everything else identical to the
  instruction, total -0.16 % over eight programs. More values in registers,
  the same amount of work, one clear loser. Reverted, so that `release-fast`
  comes out of this round bit-identical.
* **Cells first into the callee-saved pool.** A cell lives long, and the
  four temp registers are what the short lived values around it have.
  Measured: `statemachine` 691.2 -> 699.6 million instructions, `matmul`
  unchanged. Reverted.

The lesson both times: a register allocation change that reads well and
measures at zero is still a change. A round about a wrong-code bug is the
worst possible place to carry one.

---

## 4. Where Firn is still behind, and why

Measured, not guessed. `bench/firn/matmul.fi`, `release-fast`, the innermost
loop:

```
mov r10d, r13d                    ; s
imul r8, rbx, 240                 ; r*n   -- LOOP INVARIANT, recomputed
lea rdx, [r8+r15]
mov rdx, qword ptr [rbp-1256]     ; a     -- SPILLED, reloaded every pass
mov r8d, dword ptr [rdx+r8*4]
imul rdx, r15, 240                ; k*n   -- no strength reduction
...
```

**The code generator**, median 1.81x behind `rustc -O` over the eleven
programs (range 1.07x – 2.84x). Three named causes, in the order of what
they cost:

1. **Loop counters live in memory in FIR.** `mem2reg` promotes only cells
   written once, FIR has no phi nodes, and `regalloc.rs` promotes cells to
   registers only at the very end — after the optimiser has already given
   up. So `licm` cannot hoist `r * n` out of the `k` loop (it depends on a
   `load`), and no induction-variable analysis can turn `k * n` into an
   addition. This is the single biggest item left and it is an
   architectural one: real SSA with phis, or cell promotion moved in front
   of the optimiser.
2. **The allocator does not split intervals.** A value that crosses a call
   is on the stack for its *whole* life, not just across the call.
   `matmul`'s `main`: 88 values in registers, 87 on the stack, `maxlive=15`
   against twelve registers.
3. **No auto-vectorisation.** `rustc` turns `matmul`'s inner loop into SSE;
   Firn does not vectorise at all. `lib/std` uses the vector instructions by
   hand where it matters (round 82), the code generator never on its own.

**The checks**, median 1.19x (was 1.97x), worst `matmul` at 3.04x. The
reason is no longer the check — round 90 made the check itself as cheap as
x86 allows: one instruction and one not-taken forward branch. The reason is
that **LLVM proves most of its checks away and Firn proves none of them
away**. `i + 1` inside `while i < 240` is still a full checked addition in
Firn, and it cannot be anything else until Firn has a range analysis. That
is the next round's work, and it is the one that turns "fast AND safe" from
nearly true into true — and it wants item 1 above first, because the fact
that would prove the check redundant lives in the loop guard, and the loop
counter is in memory.

## 5. Acceptance

    bash test.sh                  # now including release-safe and section 46
    bash tools/optlevels/run.sh   # the four levels agree, in both compilers
    bash tools/checked/run.sh     # 150 / 150
    bash tools/self_compare.sh
    bash tools/fixpoint.sh
    cargo test --release --manifest-path compiler/Cargo.toml
