# Round 39: `lib/std/` — the facade, and `f"..."` — string interpolation

Two parts, two separate commits (limit lesson). Part A does not extend the
language: a search path and a library. Part B builds the first sugar
feature: interpolation, resolved at compile time.

## Part A — the import search path and the std facade

### The search path (firnc0 and firnc1, same order)

`import a.b` now looks for `a/b.fi` in up to four places:

1. next to the **importing** file (previous rule 1)
2. next to the **root file** (previous rule 2)
3. in **`$FIRNLIB`** (environment variable; empty/unset = step is skipped)
4. in **`<directory of the compiler binary>/../lib`** — the installation
   layout `bin/firnc` + `lib/`. `firnc0` reads `current_exe` for that;
   `firnc1` reads `/proc/self/exe` via the `readlink` syscall.

The new places come AFTER the old ones: existing resolutions do not change
(test.sh 640/640, self 186/0/0, fixpoint character-identical — measured
again before the library commit). `firnc1` finds `envp` in the start block
behind `argv` (`umgebung()` in `bin/firnc1.fi`); `test.sh` and
`tools/self_compare.sh` export `FIRNLIB=<repo>/lib`.

Proof from a foreign directory (`/tmp/firnproj`): `import std.math`
compiles and runs — via `FIRNLIB` and via the installation layout
(`/tmp/firninst/{bin,lib}`, without the variable), on both compilers.

### What is in `lib/std/`

| Module | Content | Construction |
|---|---|---|
| `std.io` | `print`, `eprint` (stderr), `read_file`, `write_file`, the `Fmt` builder | hand-written on top of `import rt` |
| `std.math` | `PI()`, `E()`, `abs`, `min`, `max`, `clamp`, `isqrt`, `pow` (integer) + `sqrt`, `powi` (f64) | hand-written, self-sufficient |
| `std.str` | `Bytes`, `Str16`, UTF-8 (`utf8_push_cp`, `utf8_to_str16`, ...), `AtomTable` | **generated** from lib/str via `tools/strlib/expand.py` |
| `std.num` | `dtoa`, `strtod`, `bignum` (`Bn`) | **generated** from lib/num via expand.py |
| `std.vec` / `std.map` / `std.rt` / `std.intern` / `std.rc` | re-export of lib/rt resp. lib/rc | **symlinks** |
| `std.mem` | `alloc`/`free` (raw, mmap) + `heap_*` (rc heap) | hand-written on top of `import rt` + `import rc` |

Three honest construction decisions:

* **lib/str and lib/num are include libraries** (stage 0 legacy):
  their files reference each other textually (`//#include`) and carry no
  module structure. They cannot be imported — that is why `std.str`
  and `std.num` are assembled textually into ONE module each with the
  existing tool (`expand.py`, new TARGETS) and written into the repo,
  exactly like the generated tests 300–308. lib/str and lib/num themselves
  stay unchanged.
* **`std.vec`/`std.map` are symlinks, not wrappers.** A wrapper file
  `lib/std/vec.fi` with `import rt.vec` would itself be called `vec` and
  would collide with the target module of the same name (module name =
  file stem). The symlink is the honest re-export: the same content,
  canonically even the same file (`firnc0` deduplicates canonically;
  `firnc1` sees the same path string `lib/std/rt.fi` throughout, because
  all std-internal `import rt` resolve next to the facade).
* **Generic names need no facade.** `Vec[T]`, `vec_neu`,
  `Map[K, V]`, `Zaehlverweis[T]`, `rc_neu` are valid program-wide in Firn
  (round 21: templates are looked up under their original name).
  Whoever imports `std.mem` has thereby loaded `rc` and calls
  `rc_neu[i64](..)` without any qualifier. `std.mem` therefore only wraps
  the non-generic heap management (`heap_init`, `heap_lebende`, ...) and
  the raw `alloc`/`free`. The GC runtime is deliberately left out: it is
  pulled in automatically as soon as `gc class` appears — an import would
  be neither necessary nor visible.

Limits, named: `const` cannot do `f64` (sema evaluates constants as
integers) — `PI`/`E` are functions. An empty module file is a valid module
(the hit is in the return value of `lies_datei`, not in the buffer length).
`firnc1` deduplicates paths as strings: whoever includes the same file via
two different paths (mixing `std.vec` AND `rt.vec`, say) loads it twice —
`firnc0` canonicalizes, `firnc1` does (not yet); do not mix.

`tests/790_std_core.fi` touches every facade once and runs to the same
output on both compilers (self-comparison: `GLEICH`).

## Part B — `f"..."`: string interpolation

`f"x = {x}"` is now Firn. The parser decomposes the body **at compile
time** into a call chain on the `Fmt` builder from `std.io` — no varargs,
no runtime parsing:

```text
f"x = {x}!"  ==>
io.fmt_text(                       // "x = " as a hidden let _fsegN
    io.fmt_number(                 // (x) as i64
        io.fmt_text(io.fmt_new(), &_fseg0[0] as u64, 4),
        (x) as i64),
    &_fseg1[0] as u64, 1)          // "!"
```

The value of an `f"..."` is a `std.io.Fmt` — it is printed with
`io.fmt_druck(...)` (or passed on: `fhelfer.meldung(7)` returns
a `Fmt`). Whoever writes `f"..."` needs `import std.io`;
without the import, resolution reports `io` as with any other
module access.

### How it is built (the same in both compilers)

* **Lexer**: `f"..."` is ONE token with the RAW body (braces and
  escapes untouched). firnc0: `strings.rs::lex_fstring_literal`
  next to `lex_string_literal`; firnc1: `L_FSTR` in `lex_text` — there the
  literal table carries the raw octets.
* **Parser**: the brace scan splits the body into text and
  expression segments. Text segments are decoded via `decode_literal`
  (firnc0) resp. by re-lexing as a `"..."` literal (firnc1 — the same
  decoding by construction). Expression segments are re-lexed with
  padded positions and read by ONE sub-parser as
  exactly one expression — errors point at the real spot in
  the file (`tests/neg/interp_unknown_name.fi`: 5:27, in the middle of the
  `f"..."`).
* **The hoisting problem**: `(&_fseg[0])` needs a named variable —
  a bare `(&[104, 105][0])` has no derivable type and is
  not addressable (measured, not guessed). That is why the parser hoists
  every text segment as a hidden `let _fsegN: [u8; N]` in front of the
  surrounding statement (`hoist` list, `block` empties it after every
  statement). In firnc0 the segment is called `_fseg<ExprId>`, in firnc1
  `_fseg<Literal>_<Segment>` — names are an implementation detail,
  the behavior is identical.

### Honest limits (core version)

* **The display is that of `i64`.** The parser does not know the type of
  the expression — it inserts `(ausdruck) as i64`. Integers
  are always right; `u64` above i64::MAX wraps around, `f64`
  truncates (`{2.9}` → `2`), `bool` becomes `0`/`1`.
* **No nesting**: an `f"..."` inside the expression of an `f"..."` is
  an error (nesting depth 1). In practice a `"` in the
  expression ends the outer string anyway.
* **No escaping of the braces**: `{{`/`}}` does not exist (yet);
  a single `}` without `{` is an error
  (`tests/neg/interp_paren_alone.fi`), likewise `{` without `}`
  (`tests/neg/interp_paren_open.fi`) and `{` inside the expression.
* **No strings inside the expression** (the `"` ends the outer
  literal) and no interpolation at item level (`const`) — there is
  no statement in front of which the text segments could be hoisted.
* **Bootstrap order**: `firnc1` itself does not use `f"..."`
  yet — the lexer/parser for it is written before the feature
  exists. Converting one small spot is possible once the fixpoint
  has passed (see below).

### Proof

* `tests/791_interpolation_core.fi`: several segments, operators and
  calls in `{...}`, i32/u64/u8/bool, i64::MIN+1, and one
  interpolation **in an imported module**
  (`tests/modules/fhelper.fi`). Runs to the same output in both
  compilers (self-comparison: `GLEICH`).
* Negative tests (`tests/neg/interp_*.fi`): unknown name in `{...}`,
  `{` without `}`, `}` without `{` — rc=1 on both sides in each case.

### Two findings from the build, for the sake of completeness

* **The parser copy for expression segments must share the tree via
  `fremd`.** In the root mode of the dump tools (`fremd == 0`),
  `par_baum` points to the struct field of the copy — nodes ended up in the
  stack frame and were dead after the return (segfault in `astdump`
  on `f"{x}"`; invisible in module mode of `firnc1`, because there
  `fremd` is set anyway). The copy therefore sets
  `sub.fremd = par_baum(p)`.
* **The hidden names must AGREE between the compilers.**
  `tools/parser_compare.sh` compares the canonical trees
  octet by octet: `_fseg<ExprId>` (firnc0) and `_fseg<node count>`
  (firnc1) are the same number, because both parsers create nodes in the
  same order — otherwise `tests/791` would be a difference there.

### Why `firnc1` itself does not use `f"..."` yet

The fixpoint holds; the optional conversion of one small spot was
deliberately NOT done, for three measurable reasons: (1) the desugaring
calls `io.fmt_*` — `firnc1` would have to include `std.io`, and
`tools/fixpoint.sh` would have to export `FIRNLIB` (otherwise the dumps do
not resolve `std.io`); (2) the core facade only knows
`fmt_zahl`/`fmt_text` — the spots that suggest themselves (lexer and
parser messages) need characters (`{c}` as a letter, not as a
decimal number) or a buffer instead of stdout; otherwise the
message text changes and `tools/lex_compare.sh` (which compares the
error streams as well) tips over; (3) the gain would be cosmetic. What would
unlock it: `fmt_char` + `fmt_content(f, &buf)` in `std.io` and
`FIRNLIB` in `fixpunkt.sh`.

### Measurements (final state of round 39)

* `test.sh`: **649/649** (640 + 790×3 + 791×3 + 3 interpolation negative
  tests)
* `tools/self_compare.sh`: **188/0/0** (790 and 791 are `GLEICH`)
* Fixpoint: stage 2 == stage 3, **289 096 lines**, character-identical
* `tools/lex_compare.sh`, `parser_compare.sh`, `types_compare.sh`:
  green; the interpolation is identical in all three streams
  (tokens, canonical tree, layout)
* `import std.math` from `/tmp/firnproj`: FIRNLIB and installation layout
  (`bin/firnc` + `lib/`), on both compilers
