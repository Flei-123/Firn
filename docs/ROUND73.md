# Round 73 — `std.core`, and an `Allocator` you can see

Branch `r73-core`, base commit `badcd91c` (main after the merges of rounds
65, 67, 68 and 69). Nothing was merged; that is Justin's step.

---

## 1. The numbers first

| measurement | base (`badcd91c`) | after (`r73-core`) |
|---|---|---|
| `bash test.sh`, cases in total | 1046 | **1064** |
| — of which passed on this machine | 1045 | **1064** |
| — of which failed | **1** (see 8) | **0** |
| — new test programs (5 × opt/noopt/dev-fast) | — | +15 |
| — new negative tests | — | +2 |
| — new section in `test.sh` | 1…29 | + section **30** |
| `bash tools/self_compare.sh` | 282 same, 0 differing / 0 faulty | **287 same behaviour, 0 DIFFERING, 0 FAULTY** |
| — CODEGEN MISSING | 0 | **0** |
| `bash tools/fixpoint.sh` | identical (582 345 lines) | stage 2 == stage 3, **character-identical** (3 439 664 octets, 585 472 lines of assembly) |
| `bash tools/kernel/run.sh` | 174 / 0 | **174 passed, 0 failed** |
| `bash tools/freestanding/run.sh` | 41 / 0 | **41 passed, 0 failed** |
| `bash tools/core/run.sh` (new) | — | **46 proofs, 0 failures** |
| `bash tools/english/check.sh` | 0 0 0 **1** 0 | **0 0 0 0 0** (see 8) |
| `firnfmt -c` over every new/changed source | — | **canonical, 0 complaints** |
| the five comparison tools (lex/parser/types/sema/fir) | no deviation | lex 617/0, parser 358/1 (known), types 305/0, sema 170/0, fir 166/1 (known) — **no NEW deviation** |

`PASS 1064/1064`, exit code 0 — including `tests/860_thread_basic`, which
round 69 had to report as a machine flake under load.

The base value was measured separately, in a worktree of its own at
`badcd91c` (see 10): `FAIL 1/1046`. The difference in the count is exactly
this round — 5 new test programs × 3 runs, 2 negative tests, 1 new section
= 18. The one failure at the base is section 21 and is repaired here (8).

**The kernel demo of this round, measured in QEMU** (`demos/kernel/kcore.fi`,
both compilers, serial output compared line by line):

```
FIRN kernel + std.core
text: len=19 at=6 parts=3 root=17 trim=[vga=1 root=17 quiet]
utf8: bytes=4 chars=3 cp=955
math: isqrt=12 gcd=6 ilog2=10
arena: used=12288 high=12288 after=4096 given=2 refused=1 grown=8192
core.ok
```

**The allocator, measured** (`tools/core/soak.fi`, 40 000 rounds à 12
blocks):

| | arena run | the leaking counter-check |
|---|---|---|
| requests | **480 000** | 20 000 |
| system calls for memory | **1** | **20 001** |
| RSS start → end | **59 → 59 (drift 0) pages** | **1 056 → 20 056 (+19 000) pages** |
| blocks that lay wrong | **0** | — |

The counter-check has to strike, otherwise the measurement would prove
nothing: without `free` the RSS climbs by one page per round.

**What it costs in lines:** `lib/std/core.fi` is 3 054 lines — and not one
of them is new code. It is put together out of the include files that
`std.str`, `std.num` and `std.math` are put together out of. Genuinely new
in this round: `lib/mem/core_alloc.fi` (345 lines, the allocator),
`demos/kernel/kcore.fi` (353), `tools/core/` (435), the five new tests
(593), and 178 changed lines in the two compilers.

---

## 2. What started this

Justin's kernel is 6 384 lines of Firn, boots in QEMU and passes 174 checks
in section 22. It cannot use one line of the standard library, because SPEC
§2 has forbidden `import std.*` in the `kernel` profile since round 52.

That ban was right and too coarse at the same time. Look at what it hits:

```firn
span.trim()                 // pointer arithmetic
span.find(part)             // a loop over octets
core.text_to_i64(p, n, &x)  // multiply, add, compare
core.isqrt(144)             // Newton on integers
core.utf8_read(s, i)        // shifts and masks
```

Not one of those asks anybody for memory. Not one makes a system call. They
fell under the ban for exactly one reason: they stand in the same FILE as
`bytes_reserve`, and `bytes_reserve` calls `heap_alloc`, and `heap_alloc` is
`mmap(2)`.

Rust cut the same line and calls the pieces `core` (needs nothing), `alloc`
(needs an allocator) and `std` (needs an operating system). This round cuts
it in Firn.

---

## 3. Part 1 — `std.core`

### 3.1 The name

`std.core`. Three reasons, in this order:

1. **It is the name the reader already knows.** Whoever has seen Rust knows
   what `core` promises the moment he reads it: this works without a
   machine underneath.
2. **It says the property, not the content.** `std.base` would say nothing;
   `std.text` would be wrong (mathematics is in there too).
3. **The alternatives were worse.** `std.pure` promises freedom from side
   effects, which is not what this is about — these functions write into
   foreign memory quite happily. `std.free` collides with `free`.

One collision is worth naming: `demos/kernel/core.fi` is a module named
`core` as well. Both are root files of their own compilation and never
meet. Whoever writes a kernel that wants both renames his own.

### 3.2 The cut, file by file

The library was already an **include library** (a stage 0 legacy: `firnc`
compiles exactly one file, so `tools/strlib/expand.py` resolves
`//#include` textually). That was the lever: the cut runs through the
include files, and the two front doors are put together out of the pieces.
There is **one** source text per function.

| new include file | out of | what is in it |
|---|---|---|
| `lib/str/core_mem.fi` | `lib/str/alloc.fi` | `mem_copy`, `mem_zero`, `mem_eq`, `ld8`…`st64`, `page_round` |
| `lib/str/core_cp.fi` | `lib/str/utf8.fi` | `REPLACEMENT_CP` |
| `lib/str/core_facade.fi` | `lib/str/std_facade.fi` | `Span` and the whole reading layer: views, character classes, comparing, searching, trimming, the splitting cursor, the UTF-8 reader |
| `lib/str/core_impl.fi` | `lib/str/std_impl.fi` | `impl Span` |
| `lib/num/core_bits.fi` | `lib/num/dtoa.fi` | sign, exponent, mantissa, NaN/Inf/zero |
| `lib/num/core_facade.fi` | `lib/num/std_facade.fi` | `digit_value`, `digit_count`, `read_*`, `text_to_*`, the f64 bit helpers, `text_to_f64_at` |
| `lib/num/core_comfort.fi` | (new) | `ParsedI64`/`ParsedU64`, `parse_i64`, `parse_u64*` |
| `lib/math/core_math.fi` | `lib/std/math.fi` | the whole body of `std.math` |
| `lib/mem/core_alloc.fi` | (new) | `Block`, `interface Allocator`, `Arena` |

What stayed behind, and why: everything that OWNS memory (`Bytes`,
`Str16`, `AtomTable`, `Vec`, `Map`), everything that calls `rt.heap_alloc`
(that is `mmap`), everything with `io.*` (that is `SYS_WRITE`/`SYS_READ`),
and `dtoa` (number → text writes into a `Bytes`).

`lib/std/core.fi` and `lib/std/math.fi` are **generated** now, like
`lib/std/str.fi` and `lib/std/num.fi` before them — sources
`tools/strlib/src/std_core.fi` and `tools/strlib/src/std_math.fi`. Whoever
changes them by hand loses the change on the next `expand.py --all`.

### 3.3 Floating point in a kernel: `#[allow_fp]`, and scratch memory as a parameter

Two things stood in the way of taking `math` and `text_to_f64` along.

**The FPU.** In the kernel profile `f64` is allowed only with
`#[allow_fp]`, because the FPU/SSE registers belong to the interrupted
thread (SPEC §2). Since `prof.rs` checks every function of the merged
compilation unit, a `math` without the marking would have produced 42
errors on `import std.core` — for functions the kernel never calls. So the
42 f64 functions in `lib/math/core_math.fi` and the 8 in
`lib/num/core_facade.fi` carry `#[allow_fp]`. That is not a loophole, it is
exactly what the attribute is for: **whoever calls `core.sqrt` in a kernel
saves the FPU state himself.** The integer half — and that is the bigger
half — carries no marking and costs a kernel nothing.

**The scratch memory of `strtod`.** Reading a floating point number
correctly rounded runs in exact big number arithmetic and needs 12 288
octets of workspace. `num.text_to_f64` fetches them with `mmap` — which is
precisely what a kernel must not do. So `core` has the `_at` forms, and
they take the workspace as a **parameter**:

```firn
var ws: [u8; 12288] = [0; 12288]        // core.strtod_work_bytes()
var x: f64 = 0.0
if core.text_to_f64_at(&pi[0], 16, &ws[0], &x) { … }
```

That is the same thought as the `Allocator`, one level down: what costs
memory says so at the call site. `num.text_to_f64` stays where it was and
now calls the `_at` form — one core, two front doors, no second parser.

### 3.4 Two honest costs

**`core.Span` and `str.Span` are different types.** Whoever imports both
into one program has two structs of the same shape that cannot be swapped.
Stage 0 has no type aliases (SPEC §14), and `interface`/`enum` are the only
things that hold program wide. In practice it costs nothing: the two
modules are meant for two different profiles. `tests/1404_core_unbroken.fi`
nails the limit down instead of hiding it — it uses both side by side and
compares the ANSWERS.

**`lib/num/core_comfort.fi` duplicates 40 lines of wrapper.** The round 69
version (`tools/strlib/src/num_comfort.fi`) takes a `str.Span` and thereby
names the module a kernel must not have. What is duplicated is the
signature and a struct literal; the arithmetic below is called, not copied
— both go through the same `read_i64`/`read_u64_base` in
`lib/num/core_facade.fi`.

**And one rename:** `strtod`'s `is_digit` became `strtod_digit`. In
`lib/std/core.fi` the number reader and the text facade land in ONE module,
and the facade has an `is_digit` of its own. Of the two names, this is the
one only `strtod` uses.

---

## 4. Part 2 — the `Allocator`

SPEC §2 has promised since round 52: *"kernel: heap allocation only through
an explicitly passed `Allocator`"*. There was no `Allocator` — not in the
compiler, not in the library. Now there is.

### 4.1 The shape

```firn
struct Block { p: *mut u8, n: usize, ok: bool }

interface Allocator {
    fn raw_alloc(*mut self, size: usize, align: usize, out: *mut u64) -> bool
    fn raw_free(*mut self, address: u64, size: usize, align: usize)
    fn raw_grow(*mut self, address: u64, old: usize, size: usize,
        align: usize, out: *mut u64) -> bool
}

fn alloc(a: dyn Allocator, size: usize, align: usize) -> Block
fn free(a: dyn Allocator, b: Block, align: usize)
fn grow(a: dyn Allocator, b: Block, size: usize, align: usize) -> Block
```

**Why the interface takes only primitive types.** `interface` holds program
wide and is not renamed when modules are merged; a `struct` belongs to its
module and becomes `core__Block`. A method signature inside `interface`
therefore cannot name a struct of the same module — the compiler says
`unknown type 'Block'`, and `core.Block` in an interface is a parse error.
So the interface carries the MACHINE contract (addresses as `u64`, the
answer as `bool`) and the layer above it the HUMAN one.

That turned out to be the better shape anyway: **an implementation cannot
hand back a null pointer silently, because there is no pointer in its
return value at all.** It has to say `false`. And `alloc` turns that into a
`Block` whose `ok` is `false`, so no caller ever sees a bare address.

**The failure case is a value.** `block_none()` is the one and only
spelling of "it did not work". A size of 0 is refused instead of being
answered with an empty block; an alignment that is not a power of two is
refused instead of being rounded down.

### 4.2 Two implementations

| | `core.Arena` | `mem.PageAllocator` |
|---|---|---|
| where | `lib/mem/core_alloc.fi` (in `std.core`) | `lib/std/mem.fi` |
| memory from | whoever hands it in | `mmap(2)` |
| system calls | **none, ever** | one per block |
| profile | kernel **and** app | app |
| alloc | compare + add | `rt.heap_alloc` |
| free | the LAST block, or `reset` for all of it | `munmap` |
| grow in place | yes, if it is the last block | no (mmap cannot; `grow` copies) |

That the two find each other across the module boundary is no accident:
`impl Allocator for PageAllocator` stands in a different file from the
contract, and `interface` holds program wide (docs/ROUND46.md §9).

**Why the arena frees only the last block.** That is the nature of a bump
allocator, and it is written into the header of the file rather than hidden.
The counterpart is `arena_reset`: a frame, a request, a parse run is freed
IN ONE GO, in one instruction. `arena_high` remembers the highest fill
level ever reached — that is the number a soak run measures, and it
survives a `reset`.

### 4.3 The kernel demo satisfies the interface

`demos/kernel/mem.fi` already had a working frame allocator
(`frame_alloc`/`frame_free`/`block_size`) since round 59.
`demos/kernel/kcore.fi` shows the same shape at the level the round is
about: an `Arena` over a region of physical memory the kernel owns (64 KiB
at 3 MiB — `start.s` maps the first gigabyte identically, the image sits at
1 MiB), and every step through the interface, that is through a method
table in `.rodata`. Handing out, aligning, refusing what does not fit,
growing in place, freeing, resetting — with no byte from anybody.

### 4.4 Existing callers

Not one broke, and not one needed a second variant. The reason is that
nothing in the library takes an allocator YET: `Bytes`, `Vec`, `Map` and
`Rc` keep calling `rt.heap_alloc` exactly as before. That is deliberate.
Threading the allocator through the collections is a round of its own —
every signature in `lib/rt/`, `lib/str/` and `lib/dom/` changes, and the
browser, the JS engine and the compiler in Firn hang off them. This round
builds the thing, proves it and leaves the conversion for the next one
(see 9).

---

## 5. Part 3 — the profile check

### 5.1 The rule

Up to round 72: every `import std.*` rejected under `kernel`.
Since round 73: **a `std` module may be imported under `kernel` if it
declares `profile kernel` in its own first line.** Everything else stays
forbidden, and the message for the forbidden part is unchanged, word for
word.

### 5.2 Why that is a checked property and not a list

Justin asked for a verifiable property instead of a hand-kept name list.
This is one, and the check costs no line of compiler code:

**Firn compiles whole programs.** An imported module lands in the SAME
compilation unit as the kernel that imports it. `sema::check_profile`
already walks every function of that unit. So a `syscall` hidden inside
`lib/std/core.fi` is an error **at the line where it stands** — and so are
`gc class`, `#[unwinds]` and unmarked floating point.

The declaration is a **claim**, and the apparatus that already guards a
kernel proves it. Two consequences worth having:

* a new freestanding module needs **no compiler change**: one line in the
  module, and the compiler does the rest;
* the property is checked on the SOURCE TEXT that is actually compiled, not
  on a name that somebody wrote into a table once.

The counter-check that this is not decoration is
`tests/neg/kernel_liar.fi`: `tests/neg/std/kernel_liar.fi` declares
`profile kernel` and calls `mmap` all the same. It passes the import rule
and dies at the `syscall`, in both compilers.

What the claim does NOT cover, said plainly: a module may declare
`profile kernel` and be imported into an APP program, where nobody checks
it. That costs nothing — under `app` a `syscall` is allowed anyway. The
guarantee arises exactly where it is needed.

### 5.3 The compiler places touched — the complete list

Round 70 is working in the translator at the same time (`str`, type
aliases, literals, `+=`/`++`). Everything below stays away from types,
literals and the lexer.

**`compiler/src/prof.rs`** (+46/−4)
* `hook_import` gains a fourth parameter `module_is_kernel: bool` and
  returns early when it is set. The error text, the note and the position
  are byte for byte the ones of round 52.
* Header documentation: the table row and a new section "Round 73".

**`compiler/src/modules.rs`** (+21/−3), inside `build_program`
* before the import loop: `declares_kernel: Vec<bool>` — for every parsed
  module whether its `profile` declaration says `kernel`.
* inside the import loop: `target`/`at` are computed BEFORE the hook (they
  were computed after it) and the answer is handed to `prof::hook_import`.
  `known` now reads `at.is_some()` instead of searching a second time.

**`bin/firnc1.fi`** (+113/−15) — the twin
* `ModuleList` gains `from_std: Vec[u32]`; `ml_add` gains a parameter,
  `ml_from_std` reads it.
* `says_kernel(l)` — does this token stream carry `profile kernel`? Read
  off the TOKENS, not off the text: the word `kernel` stands in many a
  comment, and the lexer has already thrown those away.
* `complain_std_module(it, alias)` — the message of `prof.rs`, word for
  word. Byte arrays of fixed length 23/75/125 (the trap of the project
  history: whoever changes the text changes the number).
* in `imports_collect`: was the path `std.<name>`? (compared against the
  interned first path element).
* in the module loop of `main`: the check itself.
* **and one bug that came with the round:** `firnc1` took the profile out
  of the MERGED tree (`ast.prof(tree)`), because `ast.profile_set` writes
  into the one shared tree. As soon as a MODULE declares `profile kernel`,
  that declaration overwrote the root file's — and every app program that
  imports `std.math` would have been compiled into an ELF object instead of
  an executable. `firnc0` has always taken it from the root file
  (`merged.profile = progs.first()`); `firnc1` now does the same
  (`kernel_early`, read off the root token stream). Found by
  `tests/1404_core_unbroken.fi` refusing to run.

Nothing else in the translator was touched.

---

## 6. Part 4 — the proofs

### 6.1 Section 30 of `test.sh` — `tools/core/run.sh`

46 checks, all of them with both compilers where that is possible:

1. `demos/kernel/kcore.fi` compiles in the kernel profile although it says
   `import std.core`.
2. ELF type REL, **no undefined symbol**, **not one `syscall` instruction**
   in the machine code. Plus: `core__find_ab`, `core__read_i64`,
   `core__utf8_read` and `core__Arena__raw_alloc` really are in the image —
   otherwise the test would prove nothing about `std.core`.
3. It boots in QEMU and the six serial lines are compared literally
   (section 1 of this document).
4. The counter-checks: `std.io`, `std.str`, `std.vec` and `std.rc` stay
   refused, with the unchanged message, in BOTH compilers. The lying module
   is refused because of its `syscall`. And the other direction — `std.core`
   and `std.io` side by side in the APP profile still compile, because a
   rule that forbids everything would pass every counter-check above.
5. The memory proof (section 1).

### 6.2 The tests

| file | what it nails down |
|---|---|
| `tests/1400_core_span.fi` | 53 checks on the reading text layer: views, character classes, comparing, searching, trimming, the splitting cursor with the empty piece, UTF-8 including an invalid octet |
| `tests/1401_core_number.fi` | 50 checks: strict and reading-on forms, base prefixes, **overflow reported instead of hushed up**, the comfort layer, and floating point with scratch memory of one's own — the hard case `2.2250738585072011e-308` compared as a BIT PATTERN against the compiler's own number reader |
| `tests/1402_core_allocator.fi` | 42 checks on the arena: the failure case as a value, alignment, no overlap, refusal, `grow` in place and copying, `reset`, the high water mark, one call site with two allocators |
| `tests/1403_core_page_allocator.fi` | the second implementation over `mmap`, the same call site, and the pattern of a real program: an arena over mmap memory — 1 000 requests, ONE system call |
| `tests/1404_core_unbroken.fi` | **the old names still answer.** Both halves side by side in one program; the same question asked of `core` and of `str`/`num`/`math`, the answers compared |
| `tests/neg/kernel_std_io.fi` | `import std.io` under `kernel` is refused at 14:1 with the known message |
| `tests/neg/kernel_liar.fi` | the module that claims `profile kernel` and allocates dies at its `syscall` |

### 6.3 One wart found on the way (not this round's)

The error of `tests/neg/kernel_liar.fi` is reported with the **line and
column of the module** and the **file name and source line of the root
file**:

```
error: 'syscall' does not exist in profile 'kernel'
   --> tests/neg/kernel_liar.fi:18:12
    |
 18 | fn kmain() -> i32 {
```

Line 18 column 12 is right — it is the `syscall` in
`tests/neg/std/kernel_liar.fi`. The rendered file name and source line are
the root file's. That is a rendering bug in the diagnostics for spans
across module files, it exists at the base commit as well, and it is not
repaired here: it belongs to whoever takes on the source map, not to a
round that is working three modules away from it.

---

## 7. What this round did NOT do

* **The collections do not take an allocator yet.** `Bytes`, `Vec`, `Map`,
  `Rc` still call `rt.heap_alloc`. Reason in 4.4.
* **`std.core` has no `Bytes` of its own.** A kernel that wants to BUILD
  text writes into a `Block` it allocated itself. A `BoundedBytes` over an
  `Allocator` is the obvious next step and is deliberately not smuggled in
  here.
* **`demos/kernel/kmain.fi` was not converted.** The big kernel (6 384
  lines, 174 checks) stays untouched — this round adds a demo next to it
  rather than rebuilding it while section 22 is watching.
* **`dtoa` stayed outside.** Number → text writes into a `Bytes`. It could
  be given the `_at` treatment; it was not, because nothing needed it yet.

---

## 8. Two things repaired on the way

**`tools/english/check.sh` gave `0 0 0 1 0` at the base commit.** The one
hit is `tools/layout/cases/br_flex_basis_percent.expected` — the morpheme
table knows `basis` as German, and `flex-basis` is the CSS property
(css-flexbox-1 §7.2.3). The identifier check has known that since round 67
(`tools/english/exceptions.txt`), but `exceptions.txt` does not apply to
PATH names. One line in `check_names.py`, same reason, same answer. Five
zeros.

**`tools/strlib/expand.py` left a blank line at the seam.** An included
file that ends with an empty line put that line into the product, and
`firnfmt -c` then reported the GENERATED file as not canonical — which
cannot be repaired by hand, because the next `--all` overwrites it. The
seam is now cleaned where it comes into being.

---

## 9. The next step

Thread the `Allocator` through the collections. `Bytes`, `Vec` and `Map`
take it as their first parameter, `Rc` gets a heap of its own that is one,
and the browser/JS engine/compiler-in-Firn follow. That is where the round
becomes visible in the code that already exists — and it is big enough that
it should not ride along in the round that builds the thing.

After that, a `BoundedBytes` in `std.core`: text building over an
`Allocator`, with an honest refusal instead of a growth that cannot fail.
Then a kernel can not only READ text but write it as well.

---

## 10. How the base value was measured

The two runs must not be compared out of a memory, so the base value comes
out of a worktree of its own:

```
git worktree add --detach ../firn-r73base badcd91c
cd ../firn-r73base && bash test.sh
```

```
   281 programs x 3 runs (opt / noopt / dev-fast)     (this branch: 286)
   174 negative tests -> 172                          (this branch: 174)
   SAME BEHAVIOUR:  282   DIFFERING: 0   FAULTY: 0    (this branch: 287)
   FIXPOINT: character-identical (582 345 lines)      (this branch: 585 472)
   FREESTANDING: 41 passed, 0 failed                  (unchanged)
   KERNEL:      174 passed, 0 failed                  (unchanged)
   German path names: 1                               (this branch: 0)

   FAIL 1/1046 failed:
     tools/english/check.sh reports German identifiers
```

The one failure of the base run is section 21 — the same `flex-basis` hit
that section 8 above repairs. Everything else at the base is green, and
the difference in the case count is exactly the 18 cases this round
brings.

Both runs happened on the same machine, with the round 70 run going on
next to them the whole time (load average around 3.3 on 8 cores). That
matters for exactly one case, `tests/860_thread_basic`: its check C wants a
counter WITHOUT a lock to LOSE increments, and under load the threads get
serialised and the counter comes out exact. It did not strike in either
run this time.

A note on the disk, because it cost this round two runs: the machine ran
full (54 GB, 100 %) in the middle of the first acceptance run and the
compilations began to fail with "No space left on device". Freed were only
GITIGNORED build artifacts in already merged worktrees (`.js-work`,
`.test-work`, `.tokenizer-work`, `.firnc2`/`.firnc3` and their `.s`
files), each of them checked with `git check-ignore` beforehand -- and the
five `.js-work` directories that commit `aa65fdc` swept INTO git were
deliberately left alone. 3.7 GB, nothing that is not regenerable in
minutes.
