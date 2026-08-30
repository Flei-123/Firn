# The gaps between `firnc0` and `firnc1`, measured

**Round BOOTSTRAP, 2026-08-30.** Basis: commit `52a13e750`, x86-64.

`README.md` has claimed self-hosting for a long time, and the claim held:
`tools/fixpoint.sh` really does end in *stage 2 == stage 3, character
identical*. But that sentence answers a narrower question than it looks
like. It says: **the compiler in Firn can compile itself, and the result is
stable.** It does not say: *the compiler in Firn is the same compiler as the
one in Rust.*

This document is the answer to the second question, measured file by file
instead of guessed. The tool is `tools/bootstrap_gaps.sh`; the two tables it
produced are in `docs/bootstrap-gaps-before.tsv` and
`docs/bootstrap-gaps-after.tsv` and can be reproduced.

## Why `tools/self_compare.sh` was not enough

`self_compare.sh` runs over `tests/` and `bench/` and compares behaviour --
and it deliberately leaves out two directories:

```
find tests bench -name '*.fi' ... -not -path 'tests/neg/*' -not -path 'tests/lexneg/*'
```

Those are exactly the programs that have to be **refused**. Almost 200 of
them. A compiler is not only judged by what it accepts; a compiler that
accepts a broken program produces silent wrong code, and that is worse than
one that cannot compile something at all. That half had never been measured.
It is where every gap found in this round was hiding.

## The measurement

For every file: `firnc0` compiles it, `firnc1` compiles it, both results are
run and compared. `tests/neg/` and `tests/lexneg/` are judged the other way
round -- there `firnc0` has to REFUSE, and the question is what `firnc1`
does.

| category | meaning |
|---|---|
| `SAME` | both compile it, and the two programs behave identically (exit code and standard output) |
| `DIFFER` | both compile it, the programs do **not** behave the same |
| `GAP_MUTE` / `GAP_LOUD` | `firnc0` compiles it, `firnc1` does not -- with or without a word about it |
| `NOT_CORE` / `COMPTIME` / `DEFER` / `CODEGEN` | the four declared limits of `firnc1` (exit codes 3-6) |
| `SKIPPED` | `firnc0` cannot compile the file on its own either (module fragments) |
| `ACCEPTED` | **the bad one:** `firnc0` refuses the program, `firnc1` compiles it |
| `REFUSED_MUTE` | `firnc1` refuses it -- and says nothing at all |
| `REFUSED_LOUD` | `firnc1` refuses it and says where |

## The result

| | before | after |
|---|---:|---:|
| **positive corpus** (`firnc0` compiles it) | 359 files | 360 files |
| same behaviour | 335 | **338** |
| differing behaviour | 2 | **0** |
| gaps (`firnc0` yes, `firnc1` no) | 0 | **0** |
| declared limits (not core / comptime) | 3 | 3 |
| module fragments (no standalone program) | 19 | 19 |
| **negative corpus** (`firnc0` refuses it) | 206 files | 208 files |
| **`firnc1` accepts it anyway** | **18** | **10** |
| refused, and says where | 10 | **198** |
| refused, without a word | **178** | **0** |
| **crashes of `firnc1` (SIGSEGV)** | **1** | **0** |

The three programs added in this round (`tests/1620_static_address.fi`,
`tests/neg/static_write_without_mut.fi`,
`tests/neg/atomic_add_needs_u64_ptr.fi`) explain the difference of 1 and 2
files in the totals.

## What was found, and what happened to it

### 1. The address of a `static` — the documented gap (**closed**)

`docs/OSUM-K1.md` section 9 held this since 2026-08-24: `firnc1` compiles a
`static` array and reads out of it, but cannot take its ADDRESS -- exit 1,
without a message. Twenty-two texts in the Osum kernel stayed local
variables spread over five functions because of it.

The cause was in `lib/firnc1/sema.fi`. One function, `lvalue`, decided TWO
things at once: whether an expression is a place, and whether it may be
written to. A `static` without `mut` may not be written to, so `&MSG`
counted as an attempt to write. `firnc0` has always kept the two apart --
`sema.rs::lvalue` yields the type AND the mutability, and only the three
assigning statements look at the second one.

`lvalue` is now split into `lvalue_m` (finds the place, reports through an
out-parameter that it is fixed) and `lvalue` (the caller that turns that
into an error). `&x` uses the first and throws the mutability away, exactly
as `UnOp::AddrOf` does in `sema.rs`.

Proof: `tests/1620_static_address.fi` -- red before, green after.
Counter-proof: `tests/neg/static_write_without_mut.fi` -- writing to an
immutable `static` is still an error.

### 2. `let` was not binding (**closed**)

`firnc1` had no bit for it. `lib/firnc1/sema.fi` pushed name and type onto
the scope stack and nothing else, so `x = 2` after `let x = 1` compiled
without complaint -- and `x++`, and `x += 1`. `firnc0` has carried
`VarInfo::mutable` since always.

Three programs out of `tests/neg/`: `assign_let`, `assign_op_let`,
`step_let`. Fixed with a third vector `v_mut` in `Sema`. Everything that is
not a `var` is fixed: parameters, `for` variables, `match` bindings, the
binding of a `catch` -- exactly the list `sema.rs` passes `false` for. A
`const` counts as fixed too.

The GC exception had to come along: `let a: Gc[Node]` binds the HANDLE
immutably, the object at the other end stays writable (`sema.rs`, the `gc`
hook in `ExprKind::Field`). Without it 45 programs that `firnc0` accepts
would have been refused -- the first version of this change did exactly
that, and the measurement found it.

### 3. A jump out of a `defer` (**closed** — and it was a crash)

`tests/neg/defer_return.fi` did not make `firnc1` refuse it, it made it
**segfault**. The check `sema.rs::defer_jump` was missing entirely, and the
lowering then walked into a `return` inside a deferred body. A compiler may
say no; it may not fall over.

`defer_jump`/`defer_jump_block` are ported. `break` and `continue` inside a
loop that begins within the deferred body are allowed -- they do not leave
it -- which is what `defer_return_only` says in `sema.rs`.

### 4. A function without `return` (**closed**)

`tests/neg/no_return.fi`: `fn f() -> i32 { let x: i32 = 1 }`. `firnc1`
compiled it. The generated function then falls through and hands back
whatever happens to be in the return register.

`block_returns`/`stmt_returns` are ported. One place is **deliberately
weaker** than `firnc0`: a `match` as a statement counts as returning without
checking whether all of its arms really do (`sema_match.rs::match_returns`
does check). Conservative in the safe direction -- this check never refuses
a program that `firnc0` accepts, it only catches fewer of the broken ones.

### 5. The atomics did not look at the width (**closed**)

`__atomic_add(p, d)` and `__atomic_swap(p, e, n)` accepted ANY pointer.
The instruction exchanges exactly one 64-bit word, so a `*mut u32` would
write over the four octets next to it -- silently, and only sometimes.
`atomic.rs::is_u64_ptr` is ported.
Counter-proof: `tests/neg/atomic_add_needs_u64_ptr.fi`.

### 6. Integer literals were truncated without a word (**closed**)

`let x: i32 = 5000000000` became 705032704 and the program ran on with a
number nobody had written down. `sema.rs::lit_fits` is ported, sign for
sign -- including the detail that `firnc0` does not look at signedness
here either, because the two compilers have to agree.

### 7. Every refusal was silent (**closed**)

This is the biggest number in the table: **178 of 188** refusals said
nothing at all. `firnc1` had a complete diagnostics module, `diag.fi`,
checked octet for octet against `firnc0` by `tools/lex_compare.sh` -- and
`bin/firnc1.fi` never printed it. The parser, the type check and the
lowering did not even have a position: they counted errors in a `usize` and
returned exit code 1.

Since this round:

* **lexer errors** are printed in full and are **byte identical to
  `firnc0`** (`diag_render`), including the source line and the `^^^` marker.
* **parser, type check and lowering** carry the position of the first error
  and print
  `error: the type check refused this program` + `--> file:line:col`.
  The text is not `firnc0`'s sentence -- that is still missing -- but the
  place is right, and the position is taken from the tree's file table, so
  it names the MODULE the error is in and not the root file.
* the four **declared limits** (exit codes 3-6, "not core language",
  "needs comptime", "the lowering does not carry this", "the code generator
  does not carry this FIR") now say so instead of exiting mutely.
* an **import that resolves to nothing** names the path it looked for.

The position is an approximation: it is the innermost node the checker had
ENTERED when it complained, not always the exact subexpression. It is saved
and restored around every `expr`/`stmt`, so it does not drift upwards. On
the negative corpus it hits the line `firnc0` names in the great majority of
cases; where it does not, it is off by the width of one enclosing
construct -- `tests/neg/defer_return.fi` for instance names the `defer` and
not the `return` inside it.

### 8. A second crash, made by hand and found by the measurement

While the position was being threaded through, a textual replacement turned
the body of the new `oops()` in `lib/firnc1/lower.fi` into a call to itself.
Every error in the lowering then ended in a stack overflow -- 57,698 frames,
`SIGSEGV`. It was in the tree for exactly one measurement round and is
noted here because it is the argument for this whole document: the run over
`tests/neg/` found it within minutes, the fixpoint would never have noticed
it (the compiler compiles ITSELF without an error, so the error path is
never taken).

## What is still open

Ten programs out of `tests/neg/` that `firnc0` refuses and `firnc1` still
compiles. None of them is silent in the sense of "wrong result for a correct
program" -- they are all checks that are MISSING, and the affected programs
are wrong to begin with. Ordered by how much it would cost to close them:

| program | what is missing | why not now |
|---|---|---|
| `core_break_outside` | `break`/`continue` outside a loop | the comment in `sema.fi` says "the parser has already checked the position" -- it does not. Belongs in `parser.fi`, one counter per loop. |
| `core_export` | `import`ing a name a module does not export | `firnc1` resolves modules but does not enforce the export list |
| `887_closure_write_capture` | writing to a captured value inside a closure | needs `is_captured` from `fnval.rs`; `firnc1` has the capture list, but the check is not on the write path |
| `877_closure_nogc` | `gc fn(...)` inside a `#[no_gc]` function | the `nogc` registry exists (`nogc.fi`), the closure literal is not asked |
| `literal_default_overflow` | — | **closed** in this round, see 6 |
| `free_unknown_profile` | `profile tiny` | `firnc1` reads the profile off the token stream and only knows `kernel`/`app`; anything else silently becomes `app` |
| `free_interrupt_ret` / `_parameter` / `_call` | the three rules of `#[interrupt]` | attributes are parsed, the three rules of `prof.rs` are not ported |
| `free_float_without_allow_fp`, `f32_kernel_needs_allow_fp` | floating point in the kernel profile without `#[allow_fp]` | same corner: the kernel profile of `firnc1` carries the barred standard library, not the attribute rules |

All ten are **loud in the other direction**: they concern the kernel
profile, the closures and the module boundary -- areas in which `firnc1`
today does not build any program that leaves this repository. They are
listed here so that nobody has to find them a second time.

One further note, and it is the honest one: `firnc1` has **no error texts**.
It says WHERE, not WHAT. That is a large piece of work (`firnc0` carries
several hundred sentences with notes and suggestions) and it is the next
step for whoever wants to work in `firnc1` full time.

## About the two `DIFFER`s of the first measurement

The first run reported `tests/834_arc_thread.fi` and
`tests/860_thread_basic.fi` as differing -- and in both cases it was the
binary built by **`firnc0`** that gave the wrong exit code (9 resp. 14
instead of 0), while the one built by `firnc1` was right. Both are thread
tests that count with four threads under a mutex, and the machine was
carrying eight parallel compiler runs plus a bootstrap chain at the time.
The final run, on the same corpus and with the same script, reports
**0 differing programs**. They were the load, not the compilers. Recorded
here rather than explained away.

## Speed and size, both compilers, same program

`firnc0` has a register allocator and an optimiser, `lib/firnc1/codegen.fi`
has neither -- every value lives in the frame there. That is the price, and
it is measured rather than estimated (median of one run each, x86-64,
`-o` with `as` and `ld` in both cases):

| program | `firnc0` | `firnc1` | factor | binary `firnc0` | binary `firnc1` |
|---|---:|---:|---:|---:|---:|
| `tests/860_thread_basic.fi` | 159 ms | 631 ms | 3.97x | 180,328 | 297,864 |
| `examples/tour.fi` | 1,637 ms | 2,618 ms | 1.60x | 368,496 | 695,856 |
| `bin/firnc1.fi` (the compiler itself) | 3,437 ms | 27,327 ms | 7.95x | 1,907,816 | 4,725,312 |

The assembly `firnc1` writes for itself is 23,468,484 octets in 780,000
lines. That text is what `bootstrap/` freezes.

## The chain, and that Rust is really out of it

`tools/fixpoint.sh`, after all the changes of this round:

```
STAGE 2: 17,154 ms   4,725,312 octets
STAGE 3: 57,206 ms   4,725,312 octets
FIXPOINT:  stage 2 == stage 3, character-identical (792,853 lines of assembly)
  SAME BEHAVIOUR:     333
  DIFFERING:            0
  FAULTY:               0
  NOT CORE:             2
  COMPTIME:             1
  SKIPPED:             19
CORPUS:    .firnc2 behaves like firnc0
```

Stage 3 is three times slower than stage 2 because stage 3 is done by a
compiler that a compiler without a register allocator built -- the price is
paid twice there. The comparison is on the assembly text AND on the
binaries; both are equal.

`bootstrap/` freezes the assembly text of stage 2: 23,468,484 octets,
gzipped 1,936,224. `tools/no_rust.sh` builds a working compiler out of it in
an environment whose `PATH` contains neither `cargo` nor `rustc` (nor a C
compiler, `make` or python), with the Rust binary `chmod 000`:

```
--- the environment
    no cargo, no rustc, no rustup, no C compiler, no make, no python
--- bootstrap out of bootstrap/ alone
   FIXPOINT: the compiler out of the seed and the one it builds are identical
    100 s
--- the compiler built that way translates a program
    output: hello, world
```

100 seconds from a gzipped text file to a Firn compiler that compiles
itself. That is what "Rust is an archive" means, and it is checkable.

## Reproducing it

```sh
bash tools/bootstrap_gaps.sh                 # -> .gaps.tsv, the table above
JOBS=4 FIRNC1=./.firnc2 bash tools/bootstrap_gaps.sh   # a different stage
```
