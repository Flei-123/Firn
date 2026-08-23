# Round 79 — a pointer into a local can no longer leave its frame

Round 66 wrote a JavaScript engine in Firn and, while doing so, found four
holes in the language itself (`docs/ROUND66.md` §6). The first of them is the
one that matters:

```firn
fn bad() -> *mut i64 {
    var x: i64 = 5
    return &x          // x is dead on return -- the pointer is not
}
```

The compiler said nothing. In a language that promises safety that is not an
inconvenience but a silent memory error, and round 66 lost half an hour to it
in `lib/js/gen.fi`.

Since this round it is an error at COMPILE TIME, in both compilers, with the
same text:

```text
error: the address of the local 'x' escapes through the return value
  --> tools/escape/reject/01_return_local.fi:5:5
   |
 5 |     return &x
   |     ^ here
   = note: '&x' at 5:12 points into the frame of 'bad'; 'x' is declared at 4:5
           and dies at 6:1, where 'bad' returns
   = help: return the VALUE, or let the caller own the storage and pass a
           pointer in ('fn bad(out: *mut T)'); '#[allow_escape]' on 'bad'
           switches this check off
```

Three places, because a reader needs three: where the address is TAKEN, where
the local DIES, and where the pointer GETS OUT. The wording of the first line
names the way out, so the message can be understood without reading the rest.

New: `compiler/src/escape.rs` (1,195 lines), `lib/firnc1/escape.fi` (1,723
lines), `tools/escape/` with 36 cases, `tests/1610`–`tests/1613`,
`tests/neg/1610`–`tests/neg/1613`, section 40 of `test.sh`.

## 1. What this is NOT

It is not a borrow checker. Firn grows no lifetime annotations and no
`'a` — `DESIGN_GOALS.md` §2 says Firn is not Rust, and a round that answered
gap 9 with a lifetime system would have answered a different question. The
rule of this round is:

> catch what can be decided from the SHAPE of the program, and where it
> cannot be decided, ALLOW.

That direction is deliberate and it is the expensive one: every case §4 names
as a gap is a case that gets through. The other direction would be worse. A
checker that refuses correct programs makes the language unusable, and the
first thing everybody learns is how to switch it off — after which it catches
nothing at all. That is why 14 of the 36 cases in `tools/escape/` are
COUNTER-CHECKS: correct programs with pointers that have to keep building.
Every false alarm there counts as a failure of the round.

## 2. The model: sources, sinks, summaries

A pointer value carries a **taint** — where it can have come from:

* a **local** of the function being checked: a `let`/`var`, an array or a
  field of one, or a **parameter** (a parameter slot lies in the frame too,
  so `return &p` is just as wrong as `return &x`);
* a **parameter bit** per parameter it is derived from. Nothing is known
  about that frame here; the caller decides.

A taint travels through address arithmetic, casts (`(&x) as u64` is still an
address), aggregate literals and assignments to locals. It does **not**
travel through a LOAD out of memory — `*p`, `(*p).f`, `p[i]`. That one line
is what makes the whole thing work: what lies in the heap is DATA, not the
address of the pointer that led there. Without it every `(*v).ptr` would
count as a pointer into the caller's frame and `vec_push` would never be
recognised as storing anything.

A taint reaches a **sink** when it

* is returned,
* is written through a pointer that does not belong to this frame,
* is handed to the thread primitive `__thread_start` (round 49).

For a local source a sink is the error. For a parameter it is not an error
but a FACT about the function, and it goes into that function's **summary**:

| the summary says | what happens at a call |
| --- | --- |
| parameter `i` reaches the RETURN | the result of the call inherits the taint of argument `i` |
| parameter `i` reaches what parameter `j` points at | the taint of argument `i` lands in what argument `j` points at |
| parameter `i` goes somewhere FOREIGN | handing it the address of a local is the error, reported at the call |

The summaries are driven to a fixed point over the whole program before
anything is reported, so a fact travels along chains of calls
(`tools/escape/reject/12_callee_keeps_it_transitively.fi` goes over two
levels). **Nothing has to be annotated for it.** The requirement was that a
function which stores a parameter pointer must make that visible in its
signature or be refused; the answer of this round is that it does not have to
be written down at all, because the compiler works it out and holds it
against every call site. `firnc --list-attrs` shows `#[allow_escape]`; the
summary itself is not spelled in the source, and that is on purpose — a
`#[captures(p)]` obligation would have meant touching hundreds of correct
signatures in this repository for no gain in what is caught.

The distinction between *returning* a parameter pointer and *keeping* one is
the one that keeps the tree green. `fn sema_types(s: *mut Sema) -> *mut
Types { return &(*s).ty }` hands its argument through; that is not a capture,
and blaming its callers would have been the biggest false-alarm source of the
round. `fn vec_push[T](v: *mut Vec[T], value: T)` writes `value` through a
computed pointer into the heap; that IS one, and pushing the address of a
local is refused.

**Place paths, not just root variables.** A mark sits at `sub.lx`, not at
`sub`. `lib/firnc1/parser.fi` copies a whole `Parser`, puts the address of a
local lexer into ONE field and reads a DIFFERENT field out of it one line
later; `lib/js/regexp.fi` builds `M { ..., konts: &konts[0], ... }` and reads
`st.over` from it. Rooted at the variable alone, both correct programs would
be refused. Reading a place sees marks BELOW it (the whole struct carries its
fields) and ABOVE it (a mark on `a` may come back out through `a[0]`); only
SIBLINGS stay apart. An array is index-insensitive — everything that goes in
lands under one `[]`, because the index is a value at run time.

## 3. The way out

`#[allow_escape]` on a function. It is an attribute like `#[no_gc]`, it
stands in `attrs.rs`, and it stays visible in the source:

```firn
#[allow_escape]
fn __gc_sp_below(n: u64) -> u64 {
    var a: u64 = 0
    if n > 0 { return __gc_sp_below(n - 1) }
    return (&a) as u64
}
```

It also **empties the function's summary**. Without that the way out would be
useless: the author would silence the function and the CALLER would go red
instead, at a line that has nothing to do with the decision.

Fifteen places in the tree carry it, every one with the reason written above
it:

* `lib/gc/gc.fi::__gc_sp_below` — that IS how the collector finds the current
  stack pointer without inline assembly (SPEC §3.5.3). The value is a BOUND
  for the conservative scan and is never dereferenced.
* `lib/js/run_main.fi::job` and the eleven JS test harnesses — the realm is a
  GC object and is handed the addresses of a `Ctx`, an `Ast` and a `Names`
  that lie on the stack. It is sound there and only there: the realm comes
  into being in that call, nothing outside gets hold of it, and it is garbage
  the moment the call returns. Round 66 chose to park the interpreter context
  on the stack on purpose (`docs/ROUND66.md` §1); the analysis cannot see the
  argument, so the exception is said out loud.
* `tests/834_arc_thread.fi::run` — the address of a local goes into a page six
  THREADS read. It works because `main` waits for every thread before its
  frame goes away. That is exactly the case rule 5 is about, and exactly the
  case no analysis without lifetimes can decide.

**The granularity is a whole function.** A finer one (a block, a statement)
would have meant a new keyword in the lexer, the parser, the syntax tree and
the lowering of both compilers, and it would have bought nothing here: all
fifteen places are small functions whose whole purpose is the exception. It
is written down as gap E7 all the same.

## 4. What is NOT caught — the gaps, by name

Every one of these lets a real escape through. They are here because
guessing would have cost correct programs.

**E1 — a closure body.** A `gc fn(…)` that captures the address of a local
and outlives the frame is not seen. Closure bodies (`__closure#N`, round 58)
are skipped in BOTH compilers, so that the two never disagree about them:
`firnc0` keeps them inline in the syntax tree, `lib/firnc1` lifts them into
real functions of the tree, and the numbering of the two is not guaranteed to
agree. Closing it means making the two representations agree first.

**E2 — an address smuggled through an integer PARAMETER.** Within one
function `(&x) as u64` is followed (rejected case 09). Across a call it is
followed only where the callee's parameter is a raw pointer, plus the one
hard-wired exception `__thread_start`. `fn spawn(f: u64, arg: u64)` that
passes `arg` on to a thread three levels down is not seen. The reason is
measured, not assumed: seeding every machine-word integer parameter as well
made `rt.st8`, `rt.buf_push_bytes` and every `Vec[u64]` in the tree look like
keepers, and the false alarms would have started at `lib/rt/rt.fi`.

**E3 — `extern fn`.** A foreign function has no body here, so nothing is
known about what it does with a pointer. Nothing is assumed either: handing
`&x` to a C function is allowed. `extern fn` is `unsafe` by nature (SPEC
§14.5) and the round does not pretend otherwise.

**E4 — a function value.** A call THROUGH a function value (`c.hook(a, b)`,
round 68) has no known callee, so it has no summary. Handing a local address
to a function pointer is allowed.

**E5 — shadowing.** The analysis knows one slot per NAME in a function. Two
`var x` in two nested blocks share a mark, and the message names the
declaration of the first. It is conservative in the reporting direction (it
can name the wrong `x`, not the wrong function), never in the allowing one.

**E6 — the index of an array.** `a[0] = &x; a[1] = other` marks the whole
array; reading `a[1]` sees the mark. That direction produces a FALSE ALARM in
principle, and it is the only place in the round where one is possible. It
did not strike anywhere in the tree, and the alternative — index sensitivity
— cannot be had without constant folding of every index.

**E7 — the granularity of the way out** is a whole function, see §3.

**E8 — `#[allow_escape]` empties the summary.** A vouched-for function that
really does keep pointers stops warning ITS callers. That is the price of the
way out being usable; it is the same trade as `#[no_gc]`, which also trusts
what it is told at the boundary.

**E9 — one message per sink.** Where two different locals meet in one value,
the FIRST one is named. Both compilers name the same one (the state is kept
in insertion-ordered lists on both sides for exactly this reason), but the
second local is not reported separately.

## 5. What it found in a tree that was green

**A real dangling pointer in the standard library.**
`lib/std/nbt.fi::nbt_type_name` returned `(&end[0]) as u64` — the address of
a `var` array of its own frame — **fourteen times**, under a comment that
claimed the names lay "in read only data". A `var` array never does; it lies
in the frame, and the frame is gone at the `return`. That is gap 9 of round
66, realised in shipped library code. Nothing ever crashed because nothing in
the tree calls the function; it is exported, so anything outside the tree
could have. It now copies into a buffer the caller owns
(`fn nbt_type_name(t: u8, out: *mut u8, cap: usize) -> usize`), and
`tests/1610_nbt_type_name.fi` checks all fourteen names, the length, the
terminating null octet and the cutting off. This is the one place where the
answer was a signature change and not an `#[allow_escape]`.

**A bug in the diagnostics that nothing had shown.** `Parser::join` built its
joined span with `Span::new`, which sets the file number to 0. Every joined
span of a MODULE therefore pointed into the ROOT file. It went unnoticed for
seventy-nine rounds because no message ever used those spans; the escape
analysis is the first pass after the parser that can produce a message out of
a module, and it showed a `return` of `lib/gc/gc.fi` under a line number of
the test file. Fixed with `Span::in_file`.

**Nothing else.** 374 programs of `tests/`, `bench/`, `examples/`, `demos/`,
`bin/` and `lib/` were compiled one by one against the new check. After the
fifteen deliberate exceptions and the one real bug, the count of remaining
findings is **0**.

## 6. The other three gaps of round 66

**Gap 10 — a text literal has to know its own length. CLOSED.**

```firn
var greeting: [u8; _] = "hello"
var numbers: [i64; _] = [10, 20, 30]
var zeros: [u8; _] = [7 as u8; 5]
```

Round 66 wrote about two hundred message texts as `var m: [u8; 40] = "…"` and
counted every one of them by hand. The bug that produces is not a compile
error: the text is padded with blanks until the count fits, and the LENGTH
passed on afterwards is a second, independent number that a reader cannot
check. The length now comes out of the literal. It is the PARSER that fills
it in, in both compilers, as soon as it has read the initializer — so nothing
downstream ever sees a `_`, and the type checker, the layout and the code
generator are untouched.

Honest scope: only the OUTERMOST length (`[[u8; _]; 3]` stays an error) and
only from a LITERAL — a text literal, an array literal or `[v; n]`. A call
gives the parser no length to read, and guessing one would be exactly the
quiet mistake this removes. Both cases are refused with a message
(`tests/neg/1612`, `tests/neg/1613`); `tests/1612_array_length_inferred.fi`
is the proof it works, and it runs in both compilers.

**Gap 11 — no dispatch over a number. NOT in this round.** The tree walker of
`lib/js` decides over the kind of a syntax node with 25 comparisons in an
`if`/`else` chain. A `switch` over a dense integer is a code generator
feature (a jump table in `.rodata`, a bounds check, a default arm) and it
belongs to the pattern matching of SPEC §6.3, not to a round about
lifetimes. It stays open and it stays named.

**Gap 12 — a function value in a struct field cannot be called directly.
ALREADY CLOSED, by round 68.** Round 66 reported it as open because that
round deliberately did not touch the compiler; round 68 built function values
in a struct field (`tools/fnfield/run.sh`, section 27 of `test.sh`) and
thereby answered it without knowing. `tests/1613_fnfield_through_a_pointer.fi`
is the shape round 66 named, character for character —
`(*c).gen_start(r, f, env)` — so that this paragraph is a measurement and not
an assertion. It compiles, it runs, the field can be exchanged and the call
follows it.

## 7. Why the two compilers say the same thing

`tools/escape/run.sh` compares the WHOLE message block of `firnc0` and
`firnc1` with `cmp` — the `error:` line, the arrow line, the source line, the
marker, `= note:` and `= help:`. Two compilers of one language that reject
the same program with two different messages are two languages.

Getting there needed something `lib/firnc1` never had: **source positions in
the syntax tree**. The parser of stage 1 reported its own errors straight out
of the token stream, and no pass after it ever had to name a place. `ast.fi`
now carries one packed `u64` (file, line, column) per operand, per statement,
per parameter and per closing brace, plus the file names of the translation;
`mono.fi` passes them on to every instance of a template. The message itself
goes through `lib/firnc1/diag.fi` — the same renderer the lexer's messages go
through, and therefore byte for byte the rendering of
`compiler/src/diag.rs`. The source texts are gone by the time the check runs
(`bin/firnc1.fi` reuses one buffer per module), so a finding reads its file
again; that costs nothing on a build that has none.

`firnc0` gave up one thing for the agreement: the marker is ONE caret at the
beginning of the construct instead of the width of the statement. Stage 1
keeps no token lengths, and a marker of a different width would have been the
one thing the two could not agree on. It says nothing the position does not.

## 8. The acceptance — the real numbers

Every number below comes out of a run on this machine, not out of an
estimate.

| what | result |
| --- | --- |
| `bash tools/escape/run.sh` | 36 cases, **36 PASS, 0 FAIL** — 22 refused, 14 counter-checks |
| messages identical in both compilers | **22 / 22**, the whole block compared with `cmp` |
| `bash test.sh` | **1,183 of 1,184** — section 40 among them with 36/36. The one failure is section 23 and is INHERITED, see below |
| `bash tools/fixpoint.sh` | stage 2 == stage 3, **character-identical**, 648,723 lines of assembly (stage 2 13.1 s, stage 3 40.4 s, 3,791,936 octets each) |
| `bash tools/self_compare.sh` | **318 same behaviour, 0 differing, 0 faulty**, 0 not core |
| `bash tools/types_compare.sh` | 336 same, **0 different** |
| `bash tools/english/check.sh` | 0 0 0 0 0 |
| `firnfmt -c` over the tree | 0 files out of shape |
| the tree against the new check | 374 programs of `tests/`, `bench/`, `examples/`, `demos/`, `bin/`, `lib/` — **0 remaining findings** |
| deliberate `#[allow_escape]` | 15, each with its reason written at the place |
| real bugs found | 2 (`nbt_type_name`, `Parser::join`) |
| new lines | `compiler/src/escape.rs` 1,195, `lib/firnc1/escape.fi` 1,723 |

### The one failure, and why it is not this round's

Section 23, `tools/layout/run.sh`: **1,082 of 1,087 boxes equal to Chromium
(0.46 % off)**, always the same five in the same four cases (`a4_abs_icb`,
`a2_fixed_bottom_right`, `a3_fixed_percent`, `a7_sticky_bottom`). Round 79
touches nothing in `lib/layout`, `lib/css` or `tools/layout` — `git diff` on
those paths against the branch point is empty.

It is the failure round 76 already recorded (`docs/ROUND76.md` §4.6) and
reproduced on `main`. Round 78 then took the live browser out of the
mandatory acceptance and froze Chromium's answer into the repository
(`tools/layout/reference/`), which is why `main` says 1,087 / 1,087 today
while this branch, which starts BEFORE round 78, still asks a live Chromium
and gets the old five. Measured, both sides, today: `main` with the frozen
reference **1,087 / 1,087, RC=0**; this branch against the live browser
**1,082 / 1,087**. Nothing in between belongs to round 79.

Two more failures turned up in an earlier run of the suite and are gone:
`tools/english/check.sh` found `ende` (a German identifier this round had
introduced in `lib/firnc1/parser.fi`) and the path name
`02_return_array_element.fi` — both renamed. And `tools/self_compare.sh`
reported one deviation in `tests/1001_js_parse.fi` while three test suites
were running on the machine at once; standalone it is 318 / 0 / 0, and the
fixpoint run confirms it.

A word on section 34 (the JS promise endurance run of round 66), because it
is worth knowing: it is **not deterministic**. Measured today, four runs on
`main` and three on this branch, same binary each: `main` 2 of 4 with
`jobs rc=-11` (SIGSEGV), this branch 2 of 3. It struck in one of the two full
runs here and did not in the other. That is a real bug in `lib/js/gen.fi`
waiting for a round of its own; it is neither this round's nor a measurement
artefact, and it is written down here so the next person does not spend the
evening on it.
