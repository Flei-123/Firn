# Round 64 -- the tools around the language

Firn could already do a lot: structures, enumerations with payload and
`match`, error unions with `try`/`catch`, `defer`/`errdefer`, generics with
bounds, interfaces with dynamic dispatch, methods over `impl`, function
pointers and closures, threads, packages with a manifest, an incremental GC
and a freestanding kernel profile. What was missing was everything that
makes the daily work bearable: a formatter, error messages that suggest
something, a debugger that can show variables, and an editor that
understands the language.

This round builds those four. Everything in it is checked by scripts that
are in `test.sh`, and every number below comes out of one of them.

---

## 1. The result in numbers

| Measurement | Result |
|---|---|
| `bash test.sh` | **920 / 920** |
| `bash tools/fmt/run.sh` | 568 files formatted, 9 deliberately broken ones refused, token stream **0** differences, syntax tree **0** differences out of 487 comparable files, second run **0** differences |
| `bash tools/fmt/run.sh` -- random test | 123 scrambled cases, shape differs **0**, syntax tree differs **0** out of 108 comparable cases |
| `bash tools/dwarf/run.sh` | **48 passed, 0 failed** |
| `bash tools/lsp/run.sh` | **25 passed, 0 failed** |
| `bash tools/self_compare.sh` | 246 same behaviour, **0 differing, 0 faulty** |
| `bash tools/fixpoint.sh` | stage 2 == stage 3, **character-identical** |
| `bash tools/english/check.sh` | **five zeros** |
| negative tests | 159 (147 before, twelve new: 1050--1065) |
| `bash tools/parser_compare.sh` (revived) | 307 same, 2 known deviations |
| `bash tools/sema_compare.sh` (revived) | 157 same, 27,824 expressions, 1 known deviation |
| `bash tools/fir_compare.sh` (revived) | 154 same, 43,635 instructions, 2 known deviations |

---

## 2. The formatter `firnfmt` (point 1)

`tools/fmt/fmt.fi` and `tools/fmt/firnfmt.fi` -- **written in Firn**, about
900 lines, on `lib/rt` alone. No generics, no `Vec[T]`: a token is six
numbers, and they live in six byte buffers with four octets each. That is
the first serious Firn program outside the compiler and thereby itself a
piece of evidence for the language.

### 2.1 What canonical means here

The shape is a function of the token stream and of the line structure alone.
Two texts that differ only in blanks come out the same; a second run changes
nothing.

* indentation: four blanks per open bracket, a line that begins with a
  closing bracket sits one step further out
* blanks between tokens: exactly one or none, following a table of rules
  (`need_space`) -- around binary operators yes, after a prefix operator no,
  in front of a comma no, after it yes, inside brackets no
* empty lines: at most one in a row, none right after `{`, none right before
  `}`, none at the beginning or at the end of the file
* blanks at the end of a line: gone
* exactly one line ending at the end of the file
* one blank in front of a trailing comment. **Column alignment is
  deliberately not done**: it makes every line depend on its neighbours, and
  one changed name then rewrites a whole block.

### 2.2 What it deliberately does not do, and why

**It does not re-break lines.** That is not laziness but the grammar: in
Firn a line break ENDS a statement (`parser.rs::at_line_start`,
`parser.fi::line_start`); an expression may run over several lines only
while a bracket is open. A formatter that moved line breaks around would
not change the layout but the PROGRAM. So firnfmt keeps every line break of
the author.

**It refuses a source text whose brackets do not balance.** Its indentation
would be nonsense -- an unclosed `(` would push every following line one
step to the right for the rest of the file. The negative tests
`bad_call.fi` and `two_errors.fi` are exactly that case, and they are meant
to be; `tools/fmt/run.sh` checks that every refused file is one the compiler
rejects too.

### 2.3 The strongest guarantee comes out of the method

The output is built ONLY out of verbatim pieces of the input -- every token
is copied octet for octet out of the source text -- plus blanks and line
breaks. The token stream therefore cannot change. `tools/fmt/run.sh` proves
exactly that, and not by assertion:

```
== 2. the whole tree: tokens, syntax tree, idempotence ==
   files formatted:        568  (of them changed by the shape: 0)
   refused (deliberately broken): 9
   token stream differs:   0
   syntax tree differs:    0  (out of 487 comparable files)
   second run differs:     0
```

(`changed by the shape: 0` because the tree is already in canonical shape --
the run that changed 250 files is the commit `Round 64, step 2`.)

The comparison runs against `firnc0 --emit=tokens` and
`firnc0 --emit=ast-canon` -- the two yardsticks the parser in Firn is
measured against as well.

### 2.4 The random test

`tools/fmt/mutate.py` scrambles the blanks of a source text without touching
a single token: indentation grows or vanishes, blanks between tokens grow or
vanish, lines get blanks at the end, empty lines get more empty lines. The
claim:

```
firnfmt(scrambled)  ==  firnfmt(original)
```

That is more than idempotence. Idempotence only says that a second run
changes nothing; this says that the shape depends ONLY on the tokens and on
the line structure -- exactly what "canonical" means. Plus the comparison
the round asked for: format, parse, format again -- the syntax tree has to
be equal before and after.

Where the scrambling holds back, and why, is written down in the head of
`mutate.py`: inside text literals and comments nothing is touched, blanks
are only removed where the two neighbouring characters do not grow together
into something new (`a b` must not become `ab`, `:` `:` not `::`, `/` `/`
not a comment), and empty lines are only added where there already is one.

### 2.5 The whole tree

`firnfmt -w` over all `.fi` files: **250 of 564 files changed**, about 4,300
lines of diff out of roughly 100,000. Its own commit, separate from the tool
(`Round 64, step 2`). The token stream of every single file stayed
identical.

Two mistakes the run over the tree brought to light, both fixed:

* `in` had to come out of the word list of the formatter. In an `asm` block
  it names an input operand and a `(` follows it directly:
  `asm("nop", in("rax") p)`. Without the correction it became `in ("rax")`.
* `class`, `interface`, `impl` and `dyn` are CONTEXTUAL keywords in Firn and
  may be ordinary names. `class` really is one in `lib/gc/gc.fi`, where
  `class * 8` had turned into `class *8`.

---

## 3. Suggestions in the error messages (point 2)

The premise of the task was one round out of date: the excerpt with the
marker has been there since round 21, in BOTH compilers
(`compiler/src/diag.rs` and `lib/firnc1/diag.fi` are twins). What was
missing is the **suggestion** -- the line that says what to write instead.

```
error: unknown name 'valu'
  --> a.fi:8:18
   |
 8 |     let a: i32 = valu
   |                  ^^^^ here
   = help: did you mean 'value'?
```

### 3.1 In both compilers, word for word

The `help` slot is new in both diagnostic layers and is rendered identically.
`nearest()` picks the closest candidate: Levenshtein distance, with a limit
of a third of the length of the wrong name, at most 3, and none at all for a
name shorter than two characters -- otherwise `p.z` would suggest `x` and
the message would be noise instead of help.

Where `firnc0` now suggests: unknown name, unknown function, unknown struct
type, unknown type, unknown field (in the access and in the literal).

Where BOTH lexers now suggest (`lexer.rs::char_hint` /
`lexer.fi::char_hint`): typographic quotation marks, apostrophe, en dash and
em dash, non-breaking blanks, `~`, `@`. Those are exactly the characters that
come out of copying from a document or a web page.

**The proof of word-identity is mechanical**: `tools/lex_compare.sh` compares
the whole error output of both lexers octet for octet over the corpus (533
same, 16 files with diagnostics, one known deviation named since round 20).

### 3.2 A real divergence found and fixed

`firnc0` skipped U+00A0 and friends as blanks (`char::is_whitespace`),
`firnc1` reported them (`is_empty` knows only ASCII). A non-breaking blank
pasted in from a document therefore vanished in one compiler and not in the
other. Now only ASCII counts as a blank -- in both -- and the character gets
a suggestion of its own:

```
   = help: that is the blank U+00A0, not the blank U+0020
```

### 3.3 A small old bug on the way

`diag.fi` wrote eight octets of `" = note: "` instead of nine. Invisible so
far, because no diagnostic of the lexer carries a note -- and the lexer is
the only place where `firnc1` produces diagnostic text at all. Fixed.

### 3.4 Twelve new negative tests

Numbers 1050--1065. Six for the suggestions of the type checker, six for the
ones of the lexer, and one of them (`1055_hint_no_nonsense.fi`) is a
COUNTER-CHECK: a name that resembles nothing must get NO suggestion.

### 3.5 What is honestly missing

`lib/firnc1/sema.fi` produces **no diagnostic texts at all** -- its
`complain()` counts, it does not write. The type checker in Firn therefore
cannot carry any suggestions, and the demand "word for word identical in
both compilers" can only be met where `firnc1` says anything: in the lexer.
That is a gap of the self-hosting, not of this round, and it is named here
so that it does not disappear.

---

## 4. DWARF: the debugger sees variables (point 3)

Up to now the assembler produced the debug sections out of `.file`/`.loc`.
It can only make LINES out of that: no names, no types, no variables. `gdb`
could set breakpoints and step, but `print s` said "No symbol".

Now the compiler writes `.debug_abbrev` and `.debug_info` (DWARF 4) itself;
the line table stays with the assembler, which knows the addresses.

```
compiler/src/dwarf_info.rs   the two sections as a stream of PIECES: literal
                             octets plus eight-octet addresses that the
                             assembler relocates. That way every offset
                             inside .debug_info is exactly computable -- and
                             DWARF is nothing but offsets.
compiler/src/dwarf.rs        DType, VarNote, declare_var, set_fn_type
compiler/src/lower.rs        the hook in declare_ty -- the ONE place where a
                             name out of the source text is bound to storage
compiler/src/codegen_x86.rs  the end label per function, the frame offsets
                             out of `layout`, and the two sections
```

### 4.1 What gdb can do now

```console
Breakpoint 1, summe (n=10) at docs/gdb_example.fi:3
#0  summe (n=10) at docs/gdb_example.fi:3
#1  0x0000000000400257 in main () at docs/gdb_example.fi:11
n = 10                       <- info args
$1 = 1  $2 = 2  $3 = 10      <- print s, print i, print n
type = i32 (i32)             <- ptype summe

Breakpoint 1, shift (p=0x7fffffffeaa8, by=3) at tools/dwarf/probe.fi:12
$1 = {x = 5, y = 7}          <- print *p
$2 = 5                       <- print p->x
type = struct Point { i32 x; i32 y; }
Value returned is $3 = 18    <- finish
$5 = {1, 2, 3, 4}            <- print field
$6 = 3                       <- print field[2]
type = i32 [4]               <- ptype field
```

`tools/dwarf/run.sh` drives those sessions in batch mode and compares the
output line by line: **48 checks, 0 failed**. The full session with all the
details is in `docs/DEBUGGER.md`.

### 4.2 Counter-checks, and why they matter

WITH the optimizer there must be **no** variable information. `mem2reg`
pulls an `alloca` into a register, and the frame offset recorded before that
then points at storage that is no longer written to. A wrong value in the
debugger is worse than none, so with the optimizer the variable information
is left out entirely -- and the script checks that it really is left out
(`0 variables`). It also checks that `gdb` refuses an unknown name, and that
a stripped binary has nothing left.

### 4.3 A bug this round produced and killed

`gc class Node { next: Gc[Node] }` points at itself, and the straightforward
recursion in `dtype_of` sent the compiler into the stack -- with `--no-opt`,
on eleven GC tests. Found by `test.sh`, not by looking. Now every struct
being unfolded lies on a stack; whoever meets himself again becomes an
INCOMPLETE type (`DW_AT_declaration`), which `gdb` joins back to the complete
DIE of the same name.

### 4.4 Limits

Written down in full in `docs/DEBUGGER.md`: no lexical blocks, no CFI
(`.eh_frame`), function values and error unions stay opaque, the language
number is `DW_LANG_C99`. And `ACCEPTANCE.md` item 4 criterion B still demands
that a REAL bug has been found with the debugger -- that is still open.

---

## 5. The language server (point 4)

`firnc --lsp` speaks the Language Server Protocol over standard
input/output, sitting ON TOP of the compiler that is already there: the same
lexer, the same parser, the same type checker. An editor must not see a
second, slightly different Firn.

Without a foreign library -- the protocol is JSON over a `Content-Length`
header, and both are small enough to write out (`compiler/src/lsp.rs`).

| Ability | What it does |
|---|---|
| `publishDiagnostics` | the errors of `firnc` while typing, with the suggestions of section 3 |
| `definition` | jump to the declaration; a local one beats a global one |
| `hover` | what the name under the cursor is |
| `completion` | names in scope plus the keywords |
| `rename` | with the scope taken into account |
| `formatting` | calls `firnfmt` -- the editor and the tool cannot drift apart |

`tools/lsp/client.py` is a REAL client (`Content-Length` head, JSON body,
requests with a number) and holds the answers against expectations: **25
checks, 0 failed**. Among them four counter-checks: nothing in the void, an
unknown request is answered rather than swallowed, a local of ANOTHER
function is not offered, and a struct field with the same name is not
renamed along.

### 5.1 Limits

* **One file at a time.** `import` is not followed, so „jump to definition"
  does not leave the file.
* **Rename is a scoped textual rename.** Identifiers directly behind a `.`
  or a `:` are left out (that is a field or a path), but a field with the
  same name inside a struct literal of the same file would be renamed along
  if it lies in the scope. For locals -- the usual case -- that cannot
  happen, because the scope is the function.
* **Formatting needs `firnfmt`.** If the tool is not there, nothing is
  formatted rather than something being formatted differently.

---

## 6. Three dead comparison scripts revived

`tools/parser_compare.sh`, `tools/sema_compare.sh` and `tools/fir_compare.sh`
called `--emit=ast-kanon` resp. `--emit=typen`. Those options were renamed in
the English migration (round 55/57) to `--emit=ast-canon` and
`--emit=types`. The scripts caught the error, counted the file as "skipped"
and reported **zeros** -- and `test.sh` counted them as passed. Three of the
most important counter-checks of the self-hosting had been comparing NOTHING
since then.

Revived. What they now really say:

```
parser_compare   SAME 307   DIFFERENT 2 (known)   NOT CORE 137   SKIPPED 91
sema_compare     SAME 157   DIFFERENT 1 (known)   EXPRESSIONS 27824
fir_compare      SAME 154   DIFFERENT 2 (known)   INSTRUCTIONS 43635
```

That is a second, independent proof that the reformatting of the whole tree
changed no program: 307 syntax trees and 157 type-annotated trees of the
FORMATTED sources come out equal in both compilers.

The revival brought TWO deviations to light that had been hidden. Both are
entered in the `KNOWN` list with their reason, and both are named here so
that they do not vanish again.

**One.** `tests/911_css_parser.fi`: a generic call whose type argument is a
user defined name (`gc_null[Cv]()`). `.astdump` runs the parser WITHOUT the
generic table, and instead of reporting "not core language" it reports a
syntax error. Minimal reproduction:

```firn
fn main() -> i32 {
    let a: i32 = gc_null[Cv]()
    return a
}
```

The same thing with `u32` instead of `Cv` goes through. NOT a consequence of
the reformatting: the version out of the base commit `a2a2ed4` fails in
exactly the same way.

**Two.**
`tests/871_closure_plain.fi`. `firnc0` appends the generated closure
functions at the END of the module and numbers them from 0; `firnc1` emits
them where they appear and numbers them from 2. The bodies are the same and
the BEHAVIOUR is the same -- `tools/self_compare.sh` compares exactly that
and finds no difference. What differs is a name and a position in the text,
not a program. It is entered in the `KNOWN` list of `fir_compare.sh` with
that reason, and it is named here so that it does not vanish again.

## 7. What is in test.sh now

Sections 24 (formatter), 25 (debug information) and 26 (language server) are
new. They run on every change, like everything else -- a proof that only runs
once is not a proof.

## 8. Files

```
tools/fmt/fmt.fi          the formatter: scanner and printer, in Firn
tools/fmt/firnfmt.fi      the command line (-w rewrite, -c check)
tools/fmt/run.sh          the proof over the whole tree
tools/fmt/mutate.py       the random test
tools/fmt/cases/          four pinned input/shape pairs
tools/dwarf/probe.fi      the program with struct, pointer, array
tools/dwarf/run.sh        the gdb sessions with expectations
tools/lsp/sample.fi       the file the LSP counter-check works on
tools/lsp/client.py       a real LSP client
tools/lsp/run.sh          the proof
compiler/src/dwarf_info.rs   .debug_abbrev and .debug_info
compiler/src/lsp.rs          JSON, the protocol, the symbol index
docs/DEBUGGER.md          rewritten
docs/ROUND64.md           this file
tests/neg/1050..1065      twelve negative tests for the suggestions
```


---

## 9. One thing about the numbers

`tests/860_thread_basic.fi` failed once during the acceptance runs of this
round, with exit code 14. That is case C of the test: the counter WITHOUT a
lock has to LOSE increments, otherwise the proof that the mutex works would
be worthless. Under extreme machine load (five test suites at the same time)
the four threads got serialised and the unlocked counter came out exact --
so the counter-check did not strike and the test rightly reported a failure.
Ten runs of the same binary on a quiet machine give 0 ten times out of ten.
It is named here because a number that only holds on a quiet machine is
worth naming.
