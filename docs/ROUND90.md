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

**Stage 2c:** a promoted `alloca` cell used to get the interval
`[0, last access]` — from the *start of the function*. In `matmul`'s `main`
that meant three `alloc()` calls in the first ten instructions, so all nine
cells "crossed a call" and all nine competed for the five callee-saved
registers; five lost and went to the stack, among them the counters of the
innermost loop (`FIRN_RA_STATS=1`: `cellivs=9 cellslost=5`). A cell now
lives from its **first access to its last, widened over the loops those
accesses lie in** — strictly more than the accesses, strictly less than the
whole function. `cellslost` 5 -> 3, cells in registers 4 -> 6.

### 2.3 What it measured

Wall clock, median of 7 runs, both sides on the same machine:

    python3 tools/bench90/bench.py

| | before (main) | after |
|---|---:|---:|
| median release-fast vs `rustc -O` | 1.82x | 1.80x |
| **median release-safe vs `rustc -O +checks`** | **3.24x** | **~1.9x** |
| **median price of the checks inside Firn** | **1.90x** | **~1.2x** |

Instructions really executed (`valgrind --tool=callgrind`, deterministic —
the wall clock on this shared machine still moves by ten percent between two
passes, which is enough to hide a real five percent and to invent one that
is not there):

    python3 tools/bench90/icount.py

| benchmark | release-safe before | release-safe after | change |
|---|---:|---:|---:|
| matmul | 2,624,875,071 | 2,125,013,804 | **-19.0 %** |
| statemachine | 1,152,595,372 | 946,235,543 | **-17.9 %** |

`release-fast` is untouched by all of this, as it must be: it has no checks.

---

## 3. What did not work, and is written down so nobody repeats it

* **Cells first into the callee-saved pool.** A cell lives long and the four
  temp registers are what the short lived values around it have; letting a
  cell ask `rbx`/`r12`–`r15` first reads well. Measured: `statemachine`
  691.2 -> 699.6 million instructions, `matmul` unchanged. Reverted, with
  the measurement in the comment.
* **The tighter cell interval is not free.** It helps `statemachine`
  (-4.6 %) and `bytecount` (-0.9 %) and costs `matmul` (+5.8 %) at
  `release-fast`; over the eight measured programs the instruction total
  moves by -0.16 %. It is kept because it removes a real conservatism (a
  value does not live before it exists), not because it was a win.

---

## 4. Where Firn is still behind, and why

Measured, not guessed. `bench/firn/matmul.fi`, `release-fast`, the innermost
loop, next to `rustc -O` on the same source:

```
mov r10d, r13d                    ; s
imul r8, rbx, 240                 ; r*n   -- LOOP INVARIANT, recomputed
lea rdx, [r8+r15]
mov rdx, qword ptr [rbp-1256]     ; a     -- SPILLED, reloaded every pass
mov r8d, dword ptr [rdx+r8*4]
imul rdx, r15, 240                ; k*n   -- no strength reduction
...
```

Three named causes, in the order of what they cost:

1. **Loop counters live in memory in FIR.** `mem2reg` promotes only cells
   written once, FIR has no phi nodes, and `regalloc.rs` promotes cells to
   registers only at the very end — after the optimiser has already given
   up. So `licm` cannot hoist `r * n` out of the `k` loop (it depends on a
   `load`), and no induction-variable analysis can turn `k * n` into an
   addition. This is the single biggest item left and it is an
   architectural one: real SSA with phis, or cell promotion moved in front
   of the optimiser.
2. **The allocator does not split intervals.** A value that crosses a call
   is on the stack for its *whole* life, not just across the call. `matmul`'s
   `main`: 88 values in registers, 87 on the stack, `maxlive=15` against
   twelve registers.
3. **No auto-vectorisation.** `rustc` turns `matmul`'s inner loop into SSE;
   Firn does not vectorise at all. `lib/std` uses the vector instructions by
   hand where it matters (round 82), the code generator never on its own.

---

## 5. Acceptance

    bash test.sh                  # now including release-safe and section 46
    bash tools/optlevels/run.sh   # the four levels agree, in both compilers
    bash tools/checked/run.sh     # 150 / 150
    bash tools/self_compare.sh
    bash tools/fixpoint.sh
    cargo test --release --manifest-path compiler/Cargo.toml
