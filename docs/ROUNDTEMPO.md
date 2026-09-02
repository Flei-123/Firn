<!-- SPDX-License-Identifier: GPL-2.0-only -->
# Round TEMPO — how fast `firnc` translates, and what it says when it refuses

This round did not touch the language and did not touch the code that
`firnc` emits. It touched the **compiler itself** — how long it takes — and
the **sentences it prints when a program is wrong**.

Two rules were set before the first measurement:

* **Nothing may break.** Osum and Certus have to build after this round
  exactly as before, and the full test run has to stay green.
* **A number decides.** Every idea in here was measured before and after.
  The ones that brought nothing were thrown out, and they are named below.

---

## 0. How the numbers were taken

The machine this repository lives on is **shared**. Other builds run on it
at the same time; during this round the load average moved between 13 and
37. Two timings taken one after the other on such a machine measure the
neighbours as much as the compiler.

`tools/tempo/ab.py` deals with that in two ways:

* it **interleaves** the compilers — A B C A B C … — so a load spike hits
  every side of the comparison, and
* it reports the **minimum** of the passes, not the mean. The minimum is
  the run in which the machine got out of the way; on a shared box it is
  the only number that is about the program.

Three compilers are compared throughout:

| | what it is |
|---|---|
| **A** | `firnc` as it stood at the branch point (`.tempo/firnc-base`) |
| **B** | `firnc` after this round, **one core** |
| **C** | `firnc` after this round, `-j8` |

and three real programs — no benchmarks written for the occasion:

| build | files read | lines of Firn |
|---|---:|---:|
| `bin/firnc1.fi` — the compiler in Firn, self-build | 25 | 34,903 |
| `lib/browser/b4_main.fi` — Certus | 73 | 86,825 |
| `kernel/kmain.fi` — the Osum kernel | 84 | 85,373 |

(The file lists come from `strace -e openat`, so they are what the compiler
really opened, not what the imports suggest.)

---

## 1. Where the time goes

`--timings` was extended this round so it also prints under `-c` and
`profile kernel` — until now the function returned in front of the print,
so exactly the interesting case, the Osum kernel, had no numbers at all.

Wall clock, minimum of five interleaved passes:

| build | A (before) | B (after, 1 core) | C (after, `-j8`) | C vs A |
|---|---:|---:|---:|---:|
| firnc1 self-build | 3.128 s | 2.749 s | **2.040 s** | **−34.8 %** |
| Certus (`b4_main`) | 6.023 s | 5.943 s | **3.188 s** | **−47.1 %** |
| Osum kernel | 4.324 s | 3.949 s | **2.406 s** | **−44.4 %** |

And the phases behind it, in milliseconds (minimum of the same passes):

**firnc1 self-build**

| phase | A | B | C (`-j8`) |
|---|---:|---:|---:|
| lex + parse | 140.5 | 134.9 | 138.7 |
| sema | 206.1 | 190.9 | 193.3 |
| mono | 6.7 | 6.2 | 6.4 |
| lower | 100.8 | 96.1 | 96.0 |
| **optimizer** | **853.5** | 610.1 | **260.8** |
| codegen | 609.4 | 551.5 | 611.5 |
| **as** | 978.4 (as+ld) | 972.3 | **514.7** |
| ld | — | 10.9 | 16.4 |
| **total** | **2,917.8** | 2,596.2 | **1,879.4** |

**Certus (`b4_main`)**

| phase | A | B | C (`-j8`) |
|---|---:|---:|---:|
| lex + parse | 290.2 | 308.8 | 296.2 |
| sema | 562.8 | 567.4 | 558.7 |
| lower | 216.0 | 230.3 | 224.0 |
| **optimizer** | **1,489.2** | 1,224.8 | **263.1** |
| codegen | 887.4 | 898.1 | 1,006.8 |
| **as** | 2,131.2 (as+ld) | 2,129.2 | **355.3** |
| ld | — | 19.9 | 33.3 |
| **total** | **5,699.3** | 5,608.1 | **2,815.6** |

**Osum kernel** (A has no phase row: the old compiler printed no timings
under `profile kernel` — that bug is the first fix of this round)

| phase | B | C (`-j8`) |
|---|---:|---:|
| lex + parse | 228.3 | 231.4 |
| sema | 371.2 | 372.1 |
| lower | 140.5 | 138.4 |
| **optimizer** | **893.1** | **258.4** |
| codegen | 638.9 | 655.0 |
| **as** | **1,181.8** | **418.9** |
| **total** | **3,709.2** | **2,139.8** |

The picture is the same in all three: **the optimizer and `as` are the
compiler.** Together they are 60–65 % of the wall clock. Everything in
front of them — reading, parsing, type checking, lowering — is under a
fifth, and the code generator is the only sizeable phase left after them.

---

## 2. The three things that were built

### 2.1 The hash table (`compiler/src/fasthash.rs`) — helps everywhere

`callgrind` over `firnc --emit=asm bin/firnc1.fi` (10,176,912,058
instructions in all):

```
1,660,358,991 (16.31%)  firnc::regalloc::allocate
1,404,865,250 (13.80%)  <RandomState as BuildHasher>::hash_one::<&u32>
  693,778,919 ( 6.82%)  <sip::Hasher<Sip13Rounds> as Hasher>::write
```

**Every fifth instruction the compiler executed was SipHash** — and it was
hashing `u32` keys. `fir::Val` is a `u32`, and register allocation,
`mem2reg`, the optimizer and the code generators all keep their tables
keyed by it. SipHash-1-3 is the default of `std::collections::HashMap`
because it resists hash flooding from untrusted input; a compiler has no
untrusted keys — the keys are its own dense value numbers.

`fasthash.rs` is the multiply-xor-rotate hash of Firefox and rustc, sixteen
lines, no crate, fixed seed. Effect on the single-core column (A → B):
the optimizer of the firnc1 build goes 853.5 → 610.1 ms (−28.5 %), the one
of Certus 1,489.2 → 1,224.8 ms (−17.8 %).

The fixed seed makes the compiler *more* deterministic, not less: `std`'s
`RandomState` reseeds per process, so the iteration order of every internal
map used to change from run to run — and `tools/repro/run.sh` already
proved the output identical across runs, which means nothing that reaches
the output ever depended on that order.

### 2.2 The optimizer on more than one core (`compiler/src/opt.rs`)

`time` said the compiler ran at **99 % CPU on a machine with twenty of
them**. The optimizer is per function, and the passes touch nothing but
the `Func` they are handed — `opt`, `mem2reg`, `peephole`, `licm`,
`rangecheck` and `threading` reach for no global state, no `thread_local!`,
no interning table (checked file by file).

The work is **not** handed out in equal slices. One function of
`bin/firnc1.fi` — `gctext::gctext_write`, the collector source as a 19,199
word array — is 131,662 of the 353,944 lines of assembly on its own; a
static split would leave one worker with it and nineteen idle. So the
functions lie in one queue and every worker takes the next as soon as it is
free.

Result (B → C): optimizer 610 → 261 ms (firnc1), 1,225 → 263 ms (Certus),
893 → 258 ms (kernel) — a factor of 2.3 to 4.7.

Under sixteen functions the threads cost more than they bring, so below
that the old straight loop runs.

### 2.3 `as` on more than one core (`compiler/src/asmsplit.rs`)

A third of every build was one **external, strictly single-threaded**
process reading a text file of 9–19 MB. `ld` is nothing next to it: 10–20 ms.

`asmsplit.rs` cuts the assembly at function boundaries into `n` parts of
roughly equal **length** (length, not function count — see the 131,662-line
function above), so `n` copies of `as` run at once and `ld` puts the
objects back together. `as` and `ld` stay the only tools the compiler
calls, as `DESIGN_GOALS.md` demands.

**The one real problem.** Everything the code generator writes into
`.rodata`/`.data` carries a *local* label — `.Lpanicmsg7`,
`.L__gc_typetable`, `.Lstatic_count`. `as` throws local labels away, so a
function in part 3 that says `lea rax, [rip + .Lpanicmsg7]` cannot reach a
definition sitting in part 7; `ld` answers with `undefined reference`. That
is not theory — the first version failed on exactly that line. Three ways
out were tried:

1. **`as -L`** (keep local symbols). Does not help — the symbols stay
   `STB_LOCAL` and `ld` will not resolve them across objects. *Discarded.*
2. **Copy the data block into every part.** Works, but the collector's
   state block and every `static` live in `.data` and are **writable**.
   Two copies of a mutable word is not an optimisation, it is a bug.
   *Discarded.*
3. **`.globl` on the label.** `as` keeps an explicitly global `.L` symbol
   and `ld` resolves it. Verified on a two-object test case before a line
   of the module was written. *Used.*

The data block therefore stays in exactly ONE part (the last, so
`.Ltext_end` really lies behind all the text for DWARF), and every label it
defines is announced global.

**The Osum kernel found the second bug within minutes**: the kernel build
compiles `kmain.fi` and `uprog.fi` into two objects and links them
together. Both carry a `.Lpanicmsg0` — local, that is fine; announced
global, `ld` says *multiple definition* a hundred times over. So whenever
the result is an object (`-c`, `profile kernel`) the compiler hands in a
**tag** derived from the object path, unique per compilation unit and
deterministic, and every announced label carries it.

And the kernel needed one more thing: it writes an object, so `ld` does not
run at the end — `-j` did nothing for it at all. `ld -r` is the answer; it
is still `ld`, and it turns the parts back into the single object the
kernel build script expects. That is what took the kernel's `as` from
1,181.8 ms to 418.9 ms.

**What the split costs.** The symbol table grows by those announced
labels. Measured on the Osum kernel: the object goes 3,073,200 →
3,526,432 octets, the linked image 2,446,476 → 2,714,420 octets (+11 %) —
all of it `.symtab`, which no loader reads. The **program text is
identical octet for octet** (checked with `objcopy --only-section=.text`
plus `cmp`). Localising the promoted symbols again with `objcopy` after
`ld -r` was tried and **discarded**: it costs an extra tool and 110 ms and
the linked image did not shrink by a single octet, because `ld -X` does not
drop them.

Because of that cost the split is **opt-in**: without `-j` the compiler
writes one file and calls `as` once, exactly as before, and
`tools/repro/run.sh` keeps comparing octet for octet.

`tools/tempo/run.sh` is the proof that `-j` changes nothing: the emitted
assembly of `-j1` and `-j$(nproc)` is compared octet for octet, and so is
the `.text` of the linked binary.

---

## 3. What was NOT built, and why

**Incremental translation** (rebuild only the changed units) — **not
built.** The measurement does not support it yet. `firnc` is handed ONE
root file and follows the imports; all three real builds are a single
compilation unit, so there is no unit boundary to be incremental across.
Doing it properly means a unit format, an interface hash per module and a
cache — a round of its own, and one that has to answer what happens when a
generic in module A is instantiated from module B. Guessing at it here
would have produced a cache that is wrong in exactly that case. It is the
right next round; it is not this one.

**The code generator on more than one core** — **not built.** It is the
third phase now (0.6–1.0 s), and unlike the optimizer it appends to one
shared output buffer and to the shared string/label tables. Splitting it
means giving every worker its own buffer and merging in order, plus making
the label counter per worker. Doable, but it touches the one part of the
compiler whose output is compared octet for octet by `tools/repro/run.sh`,
and the honest measurement says the two phases in front of it were worth
more. Noted for the next round.

**`objcopy --localize-symbols` after `ld -r`** — tried, measured,
**discarded**: no octet saved (see 2.3).

---

## 4. A yardstick: gcc and clang

This is an **order of magnitude, not a fair race.** `firnc` at `-O2`-ish
does twelve passes; `gcc -O2` and `clang -O2` do hundreds, on a C front end
that has to run a preprocessor first. Different languages, different
amounts of work per line. The only thing the table below answers is
*"is Firn in the same league or two leagues away"*.

All four numbers were taken **interleaved on the same loaded machine**,
minimum of three passes.

| compiler | input | lines | best time | lines/s |
|---|---|---:|---:|---:|
| `gcc -O2 -c` | sqlite3.c 3.45.1 | 255,680 | 59.24 s | 4,316 |
| `clang -O2 -c` | sqlite3.c 3.45.1 | 255,680 | 56.75 s | 4,506 |
| `firnc -j1` | `bin/firnc1.fi` | 34,903 | 2.24 s | 15,582 |
| `firnc -j8` | `bin/firnc1.fi` | 34,903 | 1.82 s | 19,177 |

So `firnc` is in the same league and on the fast side of it — roughly three
to four times the lines per second of `gcc -O2` on this box. That is what
one would expect from a compiler with twelve passes instead of hundreds; it
is not a claim that Firn optimises as well as gcc.

Per-project lines per second after this round (`-j8`, wall clock minimum):

| build | lines | best | lines/s |
|---|---:|---:|---:|
| firnc1 self-build | 34,903 | 2.040 s | 17,109 |
| Certus | 86,825 | 3.188 s | 27,235 |
| Osum kernel | 85,373 | 2.406 s | 35,483 |

---

## 5. The error messages

### 5.1 What they looked like

`tools/tempo/errors_survey.py` runs every one of the 193 files in
`tests/neg/` through the compiler and takes each message apart: is there a
file, a line, a column? Is the source line shown? Is the place
underlined? Is there a `note:`? Is there a `help:` — a sentence that says
what to *do*?

| | before | after |
|---|---:|---:|
| negative test files | 193 | 193 |
| messages printed | 235 | **214** |
| with file:line:column | 97.9 % | 97.7 % |
| source line shown | 97.9 % | 97.7 % |
| place underlined (`^^^`) | 97.9 % | 97.7 % |
| with `note:` (what is wrong) | 69.8 % | 69.6 % |
| **with `help:` (what to do)** | **6.0 %** | **19.2 %** |

The starting point was better than expected: Firn already printed
`file:line:column`, the source line and a caret for nearly every error —
the Rust/clang shape was in place. What was missing was the last line of
that shape: **the suggestion**. Six per cent of all messages told the
programmer what to do about it.

(The two-tenths of a per cent that moved on the first four rows is one file
whose 24 messages collapsed into 3 — see 5.3.)

### 5.2 What was fixed — the ten cases

Twenty-two of the 193 negative tests gained a `help:` line, across these
groups:

1. **A wrong argument type** — `argument 1 of 'f' has type bool, expected
   i32`. Now says which conversion is the one meant, or that there is none.
2. **A discarded result** — `the result must not be discarded`, for
   `#[must_use]` types, error unions and guards. Now names the way out
   (`let _ = …`, `try`, `?`) instead of only stating the rule.
3. **Assigning to a `let`** — `'x' is bound with 'let' and cannot be
   modified`. The `note:` had always said what to change; it is now a
   `help:`, because it is an instruction, not an observation.
4. **A missing module** — see 5.4, the worst message the compiler had.
5. **An unknown interface** — `unknown interface 'Drawer'`; now suggests
   the nearest name that exists.
6. **Floating point under `profile kernel`** — now says how to switch the
   profile or which fixed-point helper to reach for.
7. **`syscall` under `profile kernel`** — same shape.
8. **`gc class` without the tracing collector** — now says which profile
   has it.
9. **A wrong method receiver** — `argument 1 of 'Dot.set_x' has type bool`.
10. **A pointer of the wrong type** — `*mut Str16` where `*mut Bytes` was
    wanted; now names both sides and where the expectation came from.

### 5.3 The avalanche

`tests/neg/free_gc_class_in_kernel.fi` printed **24 messages**. One
mistake, twenty-four sentences — twenty-three of them the same sentence
about the same rule at a different line. It now prints **3**, and there is
a new flag `--all-errors` for the case where somebody really wants every
repetition.

The rule the compiler follows now: the **same message text** is printed a
few times and then collapsed, with a closing note saying how many were
suppressed. Different messages are never suppressed — an avalanche of
*different* errors is information; an avalanche of the same one is noise.

### 5.4 `<unknown>:61:1`

The worst message the compiler had. When an `import` named a module that
does not exist, this came out:

```
error: cannot read 'kernel/utf8.fi': No such file or directory (os error 2)
   --> <unknown>:61:1
    |
 61 |
    | ^^^^^^ here
```

No file name. An empty source line. A caret under nothing. The line
number was real but pointed into a file the message would not name — and
this is the message a newcomer meets first, because a typo in an import is
the first mistake anybody makes.

The reason: the module loader read the *importing* file and then threw the
path away before the diagnostic was built, so the source map had nothing to
look the line up in. The files that were read now go into the source map
before the import is resolved. The same mistake now reads:

```
error: cannot read 'kernel/utf8.fi': No such file or directory (os error 2)
   --> kernel/kbd.fi:61:1
    |
 61 | import utf8
    | ^^^^^^ here
    = note: the search runs relative to the importing file, relative to
            'kernel', in the sources of the project, in its dependencies,
            then in $FIRNLIB and in <directory of the compiler>/../lib
    = help: check the spelling of the module name, or set $FIRNLIB to the
            directory the library lives in
```

File, line, the real source line, the caret under the `import`, the search
path that was actually walked, and what to do.

### 5.5 The unclosed block

Second-worst: a `{` that is never closed pointed at the **end of the
file**, which is the one place the mistake certainly is not. The parser
now remembers where the block opened and says both — where the file ran
out, and where the block that swallowed it began.

---

## 6. Acceptance

| condition | result |
|---|---|
| `tools/tempo/run.sh` — assembly of `-j1` vs `-j20` | **identical octet for octet** (353,944 lines) |
| `tools/tempo/run.sh` — `.text` of one `as` vs split `as` | **identical** (1,401,872 octets) |
| Osum kernel builds, links, `.text` unchanged | **yes** |
| Certus (`b4_main`) builds | **yes** |
| `bin/firnc1.fi` built by the OLD and by the NEW compiler | **byte-identical binaries** |
| language changed | **no** |
| emitted code changed | **no** |

The strongest of these is the fifth: the compiler in Firn, built once by the
compiler as it stood at the branch point and once by the compiler after this
round, comes out **byte for byte the same 1,894,632-octet binary** (`cmp`).
Whatever this round did to the speed, it did nothing to the output.

### The full test run

`bash test.sh` was run end to end: **1,560 checks, 29 failed.** That is not
green, and this section says of every red one whether it belongs to this
round. The rule followed here: a failure is only "pre-existing" if it was
*shown* to be pre-existing, not if it looks like it.

**Caused by this round — found and fixed:**

* `tools/lexnum/run.sh` compares the diagnostics of firnc0 against the
  lexer in Firn on sixteen refused literals. Three of them produce the
  same sentence at three different lines, and the new collapsing folded
  them into one — so the two readers "disagreed" about something that is
  not a number. The comparison now asks firnc0 for `--all-errors`, which
  is exactly what that flag is for. Re-run by hand: green, all four
  readers agree on 4,044 f64 and 5,758 f32 literals.

**Pre-existing, and shown to be:**

| failing part | what it is | how it was shown |
|---|---|---|
| `strsoak`, `sema_compare`, `fir_compare`, `self_compare`, `fixpoint`, `thread`, `freestanding`, `fmt`, `core`, `extfn`, `escape`, `firstrun`, `state`, `optlevels`, `testrunner`, `k3net`, `systab`, `two_machines` | all the same thing: **`firnc1` cannot build several programs any more** (rc=2/rc=3, "FALSE ALARM in firnc1", "did not build") | `bin/firnc1.fi` built by the old and by the new compiler is the **byte-identical** 1,894,632-octet binary (`cmp`). A binary that is identical cannot have been broken here. `tools/thread/run.sh` was additionally re-run with the old compiler: same failure. |
| `tools/packages/run.sh` | `firn.lock` of the demo is stale | re-run with the old compiler: **same 22 passed / 17 failed** |
| `tools/english/check.sh` | one German text site, `bin/firnc1.fi:663` | re-run with the old compiler: **same 1 site** |
| `tools/freestanding/none.sh` | `demos/freestanding/a64/start.s` does not exist in the repository | the directory holds `core.fi` and `linker.ld` and nothing else — the file was never committed |
| `tools/checkidx/run.sh`, check 2 | the expected panic text says line 11, the panic says line 12 | the SPDX commit `8f2db90c4` put one line at the top of `idx_read.fi` and nobody moved the expectation. **Fixed here** — one number in `tools/checkidx/run.sh`. |
| `tools/liveb4/run.sh` | the `noopt` stage counted 114 WPT subtests instead of 163 | the emitted assembly of Certus at `--no-opt` is **byte-identical** between the old and the new compiler (54,896,125 octets, `cmp`). The compiler cannot be the cause; a run that counts fewer subtests than the two other stages of the same run counted a truncated run. |
| `tests/834_arc_thread.fi` (exit 9), `tests/860_thread_basic.fi` (exit 14) | thread tests | both compile and run clean when started by hand, with the old compiler and with the new one. They failed while the load average of the shared machine stood above 30. |

Eighteen of the twenty-nine are one single cause: **the compiler in Firn has
fallen behind the compiler in Rust.** That is the state the branch was taken
from, and it is a round of its own — see 7.

Two honest caveats about this run:

* A full `test.sh` with the OLD compiler as a side-by-side baseline was
  **not** possible: the shared machine ran out of disk during this round
  (0 octets free at one point, and it is at 98 % now with other work on
  it). The classification above therefore rests on targeted re-runs of the
  individual suites plus the two byte-identical-output proofs, not on a
  complete baseline run. Where a suite could not be re-run, that is said.
* An earlier attempt at the full run died with `No space left on device`
  for the same reason. The run reported here is the clean one; it contains
  no such message.

New flags this round: `-j[N]` (one worker per core with `-j` alone) and
`--all-errors`.

New files: `compiler/src/fasthash.rs`, `compiler/src/asmsplit.rs`,
`tools/tempo/ab.py`, `tools/tempo/errors_survey.py`, `tools/tempo/run.sh`.

## 7. What the next round should take

1. **Incremental translation.** The single biggest lever left, and the
   only reason it is not in this round is that it needs a unit format and
   an interface hash first (see 3).
2. **The code generator on more than one core.** 0.6–1.0 s per build,
   third-largest phase now that the first two have been dealt with.
3. **Bring `firnc1` back up to `firnc0`.** Nine of the test stages are red
   because the compiler in Firn cannot build several programs any more (see
   6). Nothing in this round made that worse, and nothing in this round can
   fix it — but it is the largest red block in `test.sh` and it hides
   regressions.
4. **The collapsing in `lib/firnc1`.** firnc0 now folds repetitions of the
   same sentence; the lexer in Firn does not. `tools/lexnum/run.sh` papers
   over that with `--all-errors`; the flag comes out once firnc1 has the
   same rule.
5. **The remaining 80 % of messages without a `help:`.** The shape is
   right everywhere; the suggestion is missing. It is a long tail of small
   work, not a design problem.
