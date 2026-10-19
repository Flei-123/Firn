# tailrec: self tail-recursion elimination

FIR pass `tailrec` (`compiler/src/tailrec.rs`), firnc0 only, release levels only
(not debug preserving). Runs once per `optimize_func` call, at the end of the
first fixpoint; the cheap passes get one more round on the new loop.
Off: `--no-pass=tailrec` or `FIRN_NO_TAILREC=1`.

## What it does

* Plain tail call `return f(args)` (also void `f(args); return`) becomes a
  jump to a new loop header. Changed parameters become phis; unchanged ones
  stay as they are. Any build level that runs optimizer passes.
* Accumulator form `return e OP f(args)` / `f(args) OP e`, OP in `+ * | ^`
  (unchecked `Bin`) and explicit wrapping `+% *%`. A phi `acc` starts at the
  identity; each such site does `acc = acc OP e` and loops, every other
  `return v` becomes `return acc OP v`. `e OP f(..)` with the first call of a
  two-call body (`fib(n-1) + fib(n-2)`) works: the first call stays, the
  second becomes the loop.

## What it does not do (on purpose)

* Checked `+` / `*` (`CheckedBin`, release-safe): re-association changes which
  partial result overflows (panic text, time of the panic relative to later
  side effects). Test `4444_tailrec_safe_overflow.fi` pins this. After
  `bce` proves a checked op safe it is a plain `Bin` and qualifies.
* `and`, floats, saturating ops, bool results.
* Functions with `#[no_inline]`, constant time / secret code, interrupt
  handlers, inline asm, volatile ops, an entry block that is a jump target,
  or a stack cell whose address is used for more than a direct load/store
  (the frame slot would be reused by the next iteration). Test
  `4439_tailrec_forms.fi` (`through`) covers the last one.
* Mutual recursion.

Observable change: stack depth no longer grows with such recursions. A
recursion that used to overflow the stack runs now (test
`4443_tailrec_deep.fi`, 10^7 levels, release-fast).

## Measurements (x86-64, release-fast, host load ~25, so wall times are noisy)

| program | metric | before (`FIRN_NO_TAILREC=1`) | after |
|---|---|---|---|
| fib(40) | wall, median of 5 | 1.31 s | 0.65 s |
| fib(30) | callgrind instructions | 51.16 M | 35.00 M (-31.6 %) |
| binary_trees | wall, median of 5 | 0.92 s | 0.75 s (noisy) |
| binary_trees | callgrind instructions | 2954.98 M | 2906.74 M (-1.6 %) |
| qsort | callgrind instructions | 423.10 M | 423.10 M (binary identical: the source already loops on the tail call) |

gcc -O2 does fib(40) in 0.234 s, clang -O2 in 0.518 s; the remaining gap is
call cost in the backend (~165 M calls left, ~3.9 ns each), not recursion shape.

## Self-inlining one level (item c): measured, not built

Hand-written source with the first level of `fib` inlined into itself (same
shape the pass would produce): 0.63 s -> 0.59 s (-6 %). Too little for the
code-size and frame-budget cost (see TK2 in `inline.rs`), so it is not
implemented. Next lever is the call sequence (prologue/epilogue, callee-saved
saves) of small recursive functions.
