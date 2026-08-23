# Round 88 — the first five minutes with the language

This round has no new feature in it. It has one small program in it.

```firn
import std.io

fn main() -> i32 {
    let a = "test"
    let b = a + " und mehr"
    io.fmt_print_line(f"b={b} len={a.length()}")
    if a.starts_with("te") { io.print_line("faengt mit te an") }
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

---

## The four finds

### 1. The collector had to be started by hand

`let b = a + " und mehr"` allocates. The octets of the result outlive both
operands, they need an owner, and the only owner in this language that nobody
has to name is the collector (SPEC §3.5). So far, so right.

What the program did:

```
$ ./a
firn-gc: gc_init() wurde nicht aufgerufen
$ echo $?
70
```

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

```
firn-gc: gc_init() wurde nicht aufgerufen
```

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
    let b = a + " und mehr"
    io.fmt_print_line(f"b={b} len={a.length()}")
    if a.starts_with("te") { io.print_line("faengt mit te an") }
    return 0
}
```

**Round 87** — it does not even compile:

```
error: function 'io__print_line' expects 0 argument(s), found 1
```

Take the argument away and write `io.write_line(...)` instead, and it
compiles — and then dies:

```
firn-gc: gc_init() wurde nicht aufgerufen        exit code 70
```

Put `if !gc_init() { return 90 }` in front of it, and it runs. Three changes
to eight lines, and two of them are about a collector the program never
mentions.

**Round 88** — unchanged, compiled, run:

```
$ firnc first.fi -o first && ./first
b=test und mehr len=4
faengt mit te an
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

Not one of `01`..`06` says a word about a collector. Each has to compile, run
and print exactly its `.out` file — with the optimizer, without it, and
through the self hosted compiler `firnc1`. Three counter-checks on top,
because a check that only ever says yes proves nothing:

* **A** the sources of `01`..`06` really contain no `gc_init` (a check that
  measures a program which sets up by hand would be empty),
* **B** a program without text gets NO setup in `_start`, and the joining one
  gets it EXACTLY once,
* **C** `profile kernel` gets neither an entry point nor a collector.

```
PASS 31/31 first-run checks
```

## Acceptance of the round

* `./test.sh` — green, section 46 new.
* `tools/fixpoint.sh` — stage 2 == stage 3, character for character.
* `tools/self_compare.sh` — 0 different / 0 faulty.
* `tools/english/check.sh` — 0 / 0 / 0 / 0 / 0, now including `lib/**`.

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
