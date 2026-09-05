# Round 88 — the first five minutes with the language

This round has no new feature in it. It has one small program in it.

```firn
import std.io

fn main() -> i32 {
    let a = "test"
    let b = a + " and more"
    io.fmt_print_line(f"b={b} len={a.length()}")
    if a.starts_with("te") { io.print_line("starts with te") }
    return 0
}
```

Eight lines, nothing unusual in them: join two pieces of text, ask for the
length, ask about the beginning, print. Exactly what a stranger writes before
he has read anything. On the state of round 87 this program fell over **four
times**, for four independent reasons — and every one of them was a reason
that nobody who has been working on the compiler for eighty rounds trips over
any more, because he has long since learned to walk around it.

That is the whole content of this round: walking the first five minutes once,
with a stranger's eyes, and clearing away what lies in the way.

A fifth find came in from the owner while the round was being written up
(`let x: string = "test"` said "unknown type"), and a sixth fell out on the
way and is deliberately left standing. Both are at the bottom, after the
four.

---

## The four finds

### 1. The collector had to be started by hand

`let b = a + " und mehr"` allocates. The octets of the result outlive both
operands, they need an owner, and the only owner in this language that nobody
has to name is the collector (SPEC §3.5). So far, so right.

What the program did: it wrote `firn-gc: gc_init() wurde nicht aufgerufen`
onto standard error and ended with exit code 70.

For anybody who has read nothing about a collector this is a riddle. He
wanted to join two pieces of text.

And the compiler knew everything it needed already. It reads off the TOKENS
whether it has to link the collector runtime in at all — the type name `str`,
or a text literal next to `+`, `==`, `!=` (SPEC §8.0,
`strtype.rs::source_uses_str`). Where it links the runtime in, it now also
writes the **setup**: `call gc_init` as the first instruction of the process,
in `_start`, before the first instruction of the user, exactly once.

`compiler/src/codegen_x86.rs`, `compiler/src/codegen_a64.rs` and
`lib/firnc1/codegen.fi` — all three, otherwise the fixpoint breaks.

Why in `_start` and not at the top of `main`: it is the earliest point that
exists, it is reached exactly once, and no source text has to be rewritten
for it. `gc_init` finds the bottom of the stack out of `/proc/self/maps`
(round 47), so it does not matter from which frame it is called.

**What deliberately does NOT change:**

* **`profile kernel`.** There is no `_start` there at all (SPEC §2, round 52),
  and hence no collector. The condition is the same one the state block has
  been using since round 49 — nothing new could go wrong there.
* **The explicit call.** `if !gc_init() { return 90 }` stands in dozens of
  tests and keeps working: `gc_init` is idempotent (`S_INIT` in
  `lib/gc/gc.fi`), the second call returns `true` and sets nothing up a
  second time.
* **`gc_set_max_bytes`.** It writes its own word of the state block and does
  not care whether the setup has already run.
* **`#[no_gc]`.** Untouched — the setup stands outside every user function.
* **A program without text.** It gets nothing. `tools/firstrun/run.sh`
  counter-check B compiles a program of pure arithmetic to assembly and
  asserts that no `gc_init` appears in it.

### 2. The last German run time text

The message `firn-gc: gc_init() wurde nicht aufgerufen`, word for word.

It stood in `lib/gc/gc.fi:402` since round 47 and survived the whole English
changeover of rounds 55 and 57 — and it was precisely the message a beginner
saw FIRST.

The interesting part is not the translation but **why nobody found it**.
`tools/english/check_texts.py` looked into `compiler/src`, `lib/firnc1` and
`bin` — into the two COMPILERS. This message is a RUN TIME text, it lives in
the library, and nobody looked there. The check reported zero, honestly, and
was blind in exactly one place.

So the check now reads `lib/**` as well. The word list that decides what
counts as German still comes from the two compilers alone: if the HTML test
pages joined in, `frame` and `style` would become German words and every
English sentence containing them a false alarm.

The extension immediately turned up more:

* the three finalizer messages standing right next to it in `gc.fi`
  (allocation / `gc_collect` / resurrection inside a finalizer),
* the whole of `lib/html/entities_failure.fi` — seven messages,
* `lib/browser/soak_tree.fi`, whose two sisters `soak_style.fi` and
  `soak_layout.fi` were translated in round 55 and which alone was not; its
  reader `tools/html/gc_tree.sh` moved along (`# created=`, ` live=`).

Two smaller ones on the way: `tools/gen_gctext.sh` reported its result in
German, and `check_comments.py` read the `MIT` of the licence line in the
README as the preposition `mit` — proper names are now matched
case-sensitively.

`tools/english/check.sh`: 0 identifiers, 0 texts, 0 lengths, 0 path names,
0 comment lines.

### 3. `str` reached only half of `Span`

`a.length()` worked. `a.starts_with("te")` worked. `b.part(0, 4)` did not:

```
error: type 'str' has no method 'part'
  = note: 'str' has: Bytes__add, Bytes__append, …, Span__part, …
```

The message listed `Span__part` itself. So the function was there and could
not be reached.

**The reason was an accident, not a rule.** The builtin type is called `str`.
The module of the string library is called `str` too. The method resolution
builds `Type__method`, so `a.length()` looked for `str__length` — and found
the **free function** `str.length(s: Span)`. Every method for which the module
happened to have a free function of the same name looked as if it worked:
`length`, `starts_with`, `trim`, `find`. `part` has no free function of that
name (it is called `span_part` there) and therefore "did not exist". It had
nothing to do with `self` by value or by pointer, and nothing to do with the
number of arguments.

SPEC §8.1 promises the whole library on a `str` without a conversion
function. So `impl Span` is really asked now. The receiver of a method call
carries a **list** of names since this round: its own first — so everything
that resolved yesterday resolves to exactly the same function today — and
behind it, for the builtin `str` alone, the layout compatible views
(`{ *mut u8, usize }`, that is `str.Span`), in the order in which they were
declared.

`compiler/src/impls.rs::receiver_prefixes`, `lib/firnc1/sema.fi::method_target`,
`lib/firnc1/lower.fi::method_target_l` — both compilers, otherwise the
fixpoint breaks.

All 22 methods of `impl Span` now work on a `str`: `length`, `is_empty`,
`chars`, `part`, `ab`, `to`, `equal`, `compare`, `equal_without_case`,
`starts_with`, `ends_with`, `find`, `find_back`, `find_char`, `contains`,
`count_char`, `count_part`, `trim`, `trim_left`, `trim_right`,
`without_prefix`, `without_suffix`, `utf8_char`, `utf8_part`.

**The message.** Beside that, the note under a wrong name printed EVERY name
the type had — over 200 entries in one line, ungrouped, with the one that was
meant somewhere in the middle. That is not a message, that is a data dump.
Now the five CLOSEST names come first and the rest is counted:

```
error: type 'str' has no method 'prat'
  = note: 'str' has: part, ab, repeat, set, span … and 180 more
```

Sorted by edit distance, ties broken by the longer common beginning and then
alphabetically — so the order is settled and the message reproducible
(`tests/neg/1620_str_method_nearest.fi`).

And where the name exists, only on ANOTHER type, the message says THAT
instead of claiming the method does not exist:

```
error: type 'str' has no method 'add'
  = note: 'Bytes' has it — 'str' does not, and cannot: it is a view of
    octets that nobody may change any more (SPEC 8.0). Build the text in a
    'Bytes' and hand out its view
```

That is the honest answer for everything that writes, and it is the reason
`part` was allowed through and `add` was not (`tests/neg/1621_str_is_no_buffer.fi`).

### 4. `std.io` had four families and one misleading name

`io.print_line()` took **no** argument. It printed the line feed and nothing
else. So `io.print_line("text")` — the most obvious thing anybody can write —
answered:

```
error: function 'io__print_line' expects 0 argument(s), found 1
```

The name that reads like "print this line" was the one name that could not
print a line. Beside it stood three more families for the same sentence,
grown over four rounds:

| round | spelling | takes |
|---|---|---|
| 39 | `io.print(p, n)` | pointer + length |
| 42/69 | `io.write_line(s)` | a `Span` |
| 70 | `io.println_str(s)` | a `str` |
| 42 | `io.print_line()` | nothing |

**One rule now: the plain name takes TEXT.**

```firn
io.print(s: str)          io.print_line(s: str)
io.eprint(s: str)         io.eprint_line(s: str)
io.new_line()             // only the line feed  (was: print_line())
```

Whoever really has a pointer and a length says so, and the round 39 family
keeps its behaviour under a name that names what it takes: `print_bytes`,
`print_line_bytes`, `eprint_bytes`, `eprint_line_bytes`. Because `str` and
`str.Span` are the same two words (SPEC §8.0), a `Span` fits into the text
form without a conversion — `write`/`write_line` (round 69) and
`print_str`/`println_str` (round 70) stay and do exactly the same, so no
program has to be rewritten. All 32 call sites in the repository moved along.

**What else the list showed** (`grep -nE '^fn ' lib/std/io.fi`): reading a
file needed three lines and two concepts for something that is one sentence —

```firn
var path: [u8; 15] = "notes.txt\0"     // the null octet, counted by hand
var b: rt.Buf = rt.buf_new()           // a buffer the caller has to bring
io.read_file((&path[0]) as u64, &b)
```

`io.read_file_text(path: str) -> Text` takes the name as text and gives back
a `Text` that owns its octets, like `read_text()` and `read_all()` already
do; `io.write_file_text(path, content)` is the counterpart. The round 39
`read_file`/`write_file` stay untouched.

What stays surprising and was deliberately left alone: `Fmt` is a builder
that is passed by VALUE and reassigned (`f = io.fmt_text(f, …)`), and the
`f"…"` interpolation cannot contain a string literal inside its braces. Both
are their own rounds.

---

## Before and after

The same program, unchanged, on round 87 and on round 88:

```firn
import std.io

fn main() -> i32 {
    let a = "test"
    let b = a + " and more"
    io.fmt_print_line(f"b={b} len={a.length()}")
    if a.starts_with("te") { io.print_line("starts with te") }
    return 0
}
```

**Round 87** — it does not even compile:

```
error: function 'io__print_line' expects 0 argument(s), found 1
```

Take the argument away and write `io.write_line(...)` instead, and it
compiles — and then dies with exit code 70 and
`firn-gc: gc_init() wurde nicht aufgerufen`.

Put `if !gc_init() { return 90 }` in front of it, and it runs. Three changes
to eight lines, and two of them are about a collector the program never
mentions.

**Round 88** — unchanged, compiled, run:

```
$ firnc first.fi -o first && ./first
b=test and more len=4
starts with te
```

And what round 87 refused entirely:

```firn
let c = b.part(0, 4)          // 'str' has no method 'part'   →  works
```

---

## The proof

`tools/firstrun/run.sh` (section 46 of `test.sh`). Seven programs of the kind
a stranger writes, in `tools/firstrun/cases/`:

| case | what it does |
|---|---|
| `01_join` | join, length, `starts_with` — the program above |
| `02_compare` | `==`, `!=`, joining twice |
| `03_parts` | `trim`, `part`, `ab`, `to`, `find`, `contains`, `length` |
| `04_number` | a number, a double and a bool inside a sentence |
| `05_file` | read a file by its name |
| `06_many` | 40,000 joins — a real load, `gc_collections() > 0` |
| `07_gc_init_by_hand` | COUNTER-CHECK: the explicit setup still works |
| `08_string_alias` | `string` and `str` are ONE type (the fifth find, below) |

Not one of `01`..`06` and `08` says a word about a collector. Each has to
compile, run and print exactly its `.out` file — with the optimizer, without
it, and through the self hosted compiler `firnc1`. Four counter-checks on
top, because a check that only ever says yes proves nothing:

* **A** the sources of `01`..`06` and `08` really contain no `gc_init` (a
  check that measures a program which sets up by hand would be empty),
* **B** a program without text gets NO setup in `_start`, and the joining one
  gets it EXACTLY once,
* **C** `profile kernel` gets neither an entry point nor a collector,
* **D** both spellings of the text type pull the same runtime in and name
  the same canonical type in an error message (the fifth find).

```
PASS 38/38 first-run checks
```

## The fifth find, handed in by the owner: `let x: string = "test"`

The round was already written up when the question came in whether `string`
works as well. It did not:

```
error: unknown type 'string'
```

Nobody had decided that. Rounds 70 and 71 handed out a whole family of
second spellings, and the list is recognisably the one of C#:

| second spelling | canonical | | second spelling | canonical |
|---|---|---|---|---|
| `sbyte` | `i8` | | `byte` | `u8` |
| `short` | `i16` | | `ushort` | `u16` |
| `int` | `i32` | | `uint` | `u32` |
| `long` | `i64` | | `ulong` | `u64` |
| `double` | `f64` | | `float` | `f32` |

In that family the text type is called `string`. It was the only one
missing — a gap, not a decision, and it sat on exactly the line a stranger
writes first.

### Why it could not simply be added to the table

The obvious move — one more entry in `types.rs::alias_of` and in the two
tables of `firnc1` — is right for one half and wrong for the other. All ten
pairs above map a name onto a **primitive type**, and `firnc1` exploits that
literally: `types.fi::alias_ty` holds the second spellings in a text of
fixed length, in the SAME ORDER as the canonical table above it, so that a
hit yields the KIND NUMBER directly (`int` is entry 2, and entry 2 of the
other table is `i32`). The three empty fields `||||` are the places of
`usize`, `isize` and `bool`, which have no second spelling; they can never
be hit, because a name is never empty.

`str` has no kind number. It is not a primitive type but the **builtin
struct** of `strtype.rs` — two machine words, declared once at the start of
the type check. An entry in `alias_ty` would have handed out kind 13, which
does not exist, and the type checker would have computed with a type that
is not there.

So the fold happens ONE STEP LATER, in both compilers at the same place: not
where a name becomes a primitive type, but where a name becomes a struct.

| place | what stands there now |
|---|---|
| `compiler/src/types.rs::alias_of` | `"string" => "str"` — plus the note why it may not go into the primitive tables of `layout_canon.rs`/`iface.rs` (they match on the canonical name and let `str` fall through to the struct lookup by themselves) |
| `compiler/src/sema.rs::resolve_ty`, `resolve_ty_quiet` | the struct lookup asks for `canon_name(name)` |
| `compiler/src/strtype.rs` | `NAME_ALIAS`, and `source_uses_str` knows both names |
| `lib/firnc1/types.fi::canon_str` | the same fold in Firn, called in `resolve` right before `struct_index` |
| `lib/firnc1/types.fi::alias_ty` | a note saying why `string` is NOT in this table |
| `lib/firnc1/parser.fi::canon_alias` | there it DOES belong — that table works on NAMES, not on kind numbers, so `impl Ord for string` creates the same method as `impl Ord for str`. `from` grew from 59 to 66 octets, `to` from 54 to 58, `string`/`str` are entry 13 in both |
| `lib/firnc1/gc.fi::gc_source_scan` | the trigger reads tokens and now knows both names |

The token trigger is the part that is easy to forget. `source_uses_str`
decides on the raw token stream whether the collector runtime has to be
linked in. A program that never writes `str`, only `string`, needs it just
as much — without that line it would compile and then fall over at run time
with the message from find 1. Counter-check D1 measures exactly that.

### What was checked before, not after

`grep -rnw string` over the whole repository: not a single **identifier**
carries that name. Every hit is inside a comment, inside a text literal
(`"the string does not end"`, `lib/std/json.fi`) or in test data. Nothing
was run over.

### The proof

`tools/firstrun/run.sh` grew case `08_string_alias.fi` and counter-check D.
The case does not merely show that both spellings compile — that would prove
nothing about them being ONE type. It hands each into the other's function,
assigns each to the other's variable, compares them, joins them across the
two spellings, and calls the library of 8.1 on both:

```firn
fn takes_str(s: str) -> usize { return s.length() }
fn takes_string(s: string) -> usize { return s.length() }

let x: string = "test"
let y: str = "test"
io.fmt_print_line(f"{takes_str(x)} {takes_string(y)}")
let a: str = x
let b: string = y
let c: string = x + " and " + b
```

Counter-check D adds the three things a case cannot show:

* **D1** a program that only ever writes `string` carries the collector
  setup in `_start` exactly once,
* **D2** a type error names the SAME canonical type in both spellings
  (`expected type i32, found str` — whoever read `found string` here would
  go looking for a second type),
* **D3** a method written on `str` is reachable through `string`.

The round test `sema.rs::the_alias_is_the_same_type` was extended too, but
deliberately not with `prim_type("string") == prim_type("str")`: both are
`None`, and the test would pass without a single line of the change. It now
holds `canon_name("string") == "str"` and says in its comment that the real
proof is the end to end one in `firstrun`.

---

## A sixth thing, found on the way — and NOT repaired here

While case 08 was being written, this fell out:

```firn
import std.io
fn a1() -> str { return "AAAA" }
fn main() -> i32 {
    io.print_line(a1())
    return 0
}
```

With the optimizer it prints `AAAA`. With `--no-opt` it prints **four
spaces**. The length is right, the content is gone.

The assembly says why. A text literal is built OCTET BY OCTET into a stack
slot of the function it stands in, and the returned `str` points at it:

```
_F0.a1:
    sub rsp, 144
    lea rax, [rbp-132]        ; the literal gets a slot in THIS frame
    ...
    mov byte ptr [rcx], al    ; 'A', four times
    mov rcx, qword ptr [rbp-8]
    mov rax, qword ptr [rbp-32]
    mov qword ptr [rcx], rax  ; p = rbp-132 -- a pointer into a dead frame
```

On `ret` the frame is gone; what the caller reads is whatever ran over it.
With the optimizer the literal is folded into read-only data and the bug
disappears — which is why it has never shown up.

Three things about it:

* It is **inherited, not of this round.** The same four spaces come out on
  `main` (`6cf62949`), with the round 87 spelling of the program. It was
  measured, not assumed.
* It is **not the collector's doing.** The pointer never was a heap pointer.
* It is **not a small repair.** A text literal would have to land in
  read-only data instead of the frame, in the lowering of BOTH compilers.
  That is a round of its own, and it needs its own tests; hanging it onto
  this one would put the fixpoint at risk for a change that has nothing to
  do with the five finds above.

So it is written down here rather than half done: **the first item for the
next round.** Case 08 avoids the pattern on purpose, so that the first-run
suite measures what this round changed and not what it did not.

---

## Acceptance of the round

*(Re-measured in full after the fifth find; the numbers below are the ones
of that run, not of the run before it.)*

* `./test.sh` — the run came back `FAIL 3/1204`. Two of the three are the
  SAME INHERITED case, not this round: `tools/aarch64/run.sh` (section 43)
  in its two build stages.
  `tests/1613_crypto.fi` has not compiled for aarch64 since the r80/r82
  merge — `--target=aarch64-linux cannot emit the vector instruction
  CpuFeatures yet`; round 82 built the intrinsics for x86-64 only. It was
  established against `main` while round 86 was running and is written down
  in the README and in `docs/BENCHMARKS.md` (296 of 301, 1 differing). It is
  named here rather than filtered out.

  In the run BEFORE the fifth find the third failure was this report itself:
  it quoted the German program and the German message of round 87 inside
  fenced blocks, and `check_comments.py` counts every line of a `.md` file
  as prose — nine German lines, section 21 red. The quotes now stand in
  backticks, which the check treats as code. In the re-measured run
  section 21 is green.

  The third failure of the re-measured run is a different one and it is
  named here rather than swept away: **`tools/js/round66.sh` (section 34)
  struck once, and only inside the full run.** The cause is written in
  `.js-work/r66/soak.txt`:

  ```
  jobs   rc=-11    4.7s  RSS first 9532 KiB  max 11380 KiB  growth +1848 KiB
         output: jobs 60 0 | OK
  ```

  The program ran to the end and printed its result (`jobs 60 0 | OK`), and
  the growth of 1,848 KiB is far under the limit of 8,192; the process was
  then killed by SIGSEGV (`rc=-11`) while the machine was swapping (1.4 GiB
  of swap in use, `test.sh` and the JS engine at the same time). Run on its
  own, on the same commit and the same binary, the group comes back
  **`RC=0`**:

  ```
  generators    872 / 1056   82.58%
  async        4267 / 4681   91.16%
  classes     11793 / 16689  70.66%
  gen     growth: 0 KiB      genleak growth: 24508 KiB
  jobs    growth: 2340 KiB   clean 4 KiB / leak 19008 KiB
  OK: the features of round 66 hold their limits.
  ```

  It cannot come from this round either way: nothing in the five finds
  touches the JS engine, and the name `string` appears in `lib/js/` only
  inside TEXT LITERALS (`typeof` results), never as an identifier or a type.
  It is a flake under memory pressure, written down with its evidence.

  Everything else green, including the new section 46.
* `tools/fixpoint.sh` — stage 2 == stage 3, character for character
  (651,251 lines of assembly, 3,806,336 octets each), and `.firnc2` behaves
  like `firnc0` over the whole corpus.
* `tools/self_compare.sh` — 321 the same, **0 differing, 0 faulty**.
* `tools/english/check.sh` — 0 / 0 / 0 / 0 / 0, now including `lib/**`.
* `tools/firstrun/run.sh` — `PASS 38/38` (31 before the fifth find, seven
  more for case 08 across the three compilers and counter-check D).

One number belongs here honestly: the setup in `_start` costs every program
that uses text one `mmap` of the mark stack and one read of
`/proc/self/maps` at startup — the price `gc_init()` has always cost,
only now nobody has to remember to pay it. A program without text pays
nothing at all (counter-check B).

## Files

| file | what changed |
|---|---|
| `compiler/src/codegen_x86.rs`, `codegen_a64.rs` | the setup in `_start` |
| `compiler/src/gc.rs` | `FN_INIT` |
| `lib/firnc1/codegen.fi` | the same, in Firn |
| `compiler/src/impls.rs` | `receiver_prefixes`, `nearest_note`, the note about the other type |
| `compiler/src/strtype.rs` | `view_names` |
| `lib/firnc1/sema.fi`, `lower.fi`, `types.fi` | `method_target`, `method_target_l`, `str_is_builtin` |
| `lib/std/io.fi` | the naming, `read_file_text`, `write_file_text` |
| `lib/gc/gc.fi` | four messages in English, `lib/firnc1/gctext.fi` regenerated |
| `lib/html/entities_failure.fi`, `lib/browser/soak_tree.fi` | in English |
| `tools/english/check_texts.py`, `check_comments.py` | `lib/**`, proper names |
| `tools/firstrun/` | new |
| `tests/neg/1620`, `1621` | the two messages |
| `SPEC.md` §8.0 | the setup and the methods written down |
| `compiler/src/types.rs` | `alias_of`: `string` -> `str` (the fifth find) |
| `compiler/src/sema.rs` | the struct lookup asks for the canonical name; the round test |
| `compiler/src/strtype.rs` | `NAME_ALIAS`, the token trigger knows both names |
| `lib/firnc1/types.fi` | `canon_str`, and the note at `alias_ty` |
| `lib/firnc1/parser.fi` | `canon_alias`/`canon_at`: entry 13 is `string`/`str` |
| `lib/firnc1/gc.fi` | `gc_source_scan` knows both names |
| `tools/firstrun/cases/08_string_alias.fi` | new, plus counter-check D |
| `SPEC.md` §13 | `string` in the table of second spellings |
