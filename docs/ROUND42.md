# Round 42: the standard library

Pure library work. **Not a single line** was changed in the lexer, parser,
sema, lowering or codegen — the two compilers are exactly those of the
base commit. What is described here is Firn code in `lib/`, plus seven
test programs.

**Base: `fe31d13` (round 41).** The work was built and begun on `da3b0d9`;
because round 41 fixed a miscompile in `firnc0` in the meantime that made
`test.sh` hang forever in section 12 (finding D below), this branch was
moved onto it (`git rebase main`) and the acceptance was **measured
completely anew** — with a self-built `firnc0` and freshly built helper
binaries, without any outside intervention.

---

## 1. Stocktaking: what the std could do before this round

`lib/std/` came into being in round 39 as a facade over the existing
building blocks. It was complete **in breadth** (every topic had a module)
and thin **in depth** (every module had the bare necessities).

| Module | Origin | Could do before | Was missing |
|---|---|---|---|
| `std.io` | hand, 92 l. | `print`, `eprint`, `read_file`, `write_file`, `Fmt` with `fmt_neu/text/zahl/len/druck/frei` | line break, C strings, appending to files, stdin, **reading lines**, characters/hex/u64/bool in the builder, result into a buffer instead of stdout |
| `std.math` | hand, 116 l. | `PI`, `E`, `abs`, `min`, `max`, `clamp`, `isqrt`, `pow` (integer), `sqrt`, `powi` (f64) | `floor/ceil/round/trunc`, `fabs/fmin/fmax/fclamp`, `fmod`, `hypot`, `ldexp/frexp`, `exp/ln/log2/log10`, trigonometry, `gcd/lcm`, powers of two, `INF/NAN/EPSILON`, unsigned variants |
| `std.str` | **generated** from `lib/str`, 741 l. | `Bytes`, `Str16`, WTF-8/UTF-16 conversions, `utf8_is_valid`, `AtomTable` | everything one does with text daily: compare, search, trim, split, join, replace, upper/lower, pad — and an **iteration over characters** |
| `std.num` | **generated** from `lib/num`, 1203 l. | `Bn` (16384-bit), `dtoa`, `strtod` — both exact | integer ↔ text in **both** directions, other bases, signs, padding, overflow detection, an f64 wrapper over `dtoa`/`strtod` (the cores compute on bit patterns because they are older than `f64`) |
| `std.vec` | symlink `lib/rt/vec.fi`, 137 l. | `vec_neu/frei/leeren/reserve/push/at/setzen/len/kap/ptr/letztes/pop` | search, insert, remove, copy, compare, reverse, **sort**, binary search |
| `std.map` | symlink `lib/rt/map.fi`, 298 l. | `map_neu/frei/leeren/setzen/hol/hat/loeschen/len/kap/reserve` + slot access | **iteration**, „get or default", „get and say whether it was there", take out, counting patterns, cleaning up tombstones |
| `std.mem` | hand, 54 l. | `alloc`/`free`, `heap_*` of the rc heap | — (not touched in this round) |
| `std.rt`, `std.intern`, `std.rc` | symlinks | runtime core, interner, reference counts | — (not touched) |

In short: with the std one could **compute and manage memory**, but not
comfortably **process text**, not **sort** and not **walk over a map**.

---

## 2. Where the new code belongs — the one construction decision of this round

Three different kinds of module demand three different ways:

**`std.str` and `std.num` are GENERATED.** `lib/str` and `lib/num` are
include libraries from stage 0: their files reference each other textually
(`//#include`) and carry no module structure. `tools/strlib/expand.py`
assembles **one** module each from them and writes it to `lib/std/`. An
extension by hand in `lib/std/str.fi` would be gone at the next generation;
an extension in `lib/str/bytes.fi` would sit in the binary of every
`html`/`dom` user. That is why there are **two new source files that are
included exclusively by the facade**:

```
lib/str/std_facade.fi   <- only from tools/strlib/src/std_str.fi
lib/num/std_facade.fi   <- only from tools/strlib/src/std_num.fi
```

`lib/str/*.fi` and `lib/num/*.fi` are **unchanged**; the generated tests
300–308 do not gain a single byte.

**`std.vec` and `std.map` are symlinks to `lib/rt/vec.fi` and
`lib/rt/map.fi` — and `lib/firnc1/vec.fi` is the same symlink.** Everything
added there is therefore **generic**: a template only becomes code when a
program uses it for a type (monomorphization, round 2).
Measured afterwards: after the extension the fixpoint (`.firnc2.s`) has
**exactly the same line count** as before — the self-hosted compiler carries
nothing of the 21 new `Vec` and 11 new `Map` functions.

**`std.math` and `std.io` are hand-written module files** and are extended
directly; their `export` lists have grown along and are structured by round.

---

## 3. What was added — module by module

### 3.1 `std.str` (+863 lines, `lib/str/std_facade.fi`)

Two types, one rule, readable off every signature:

* **`Spanne { p: *mut u8, n: usize }`** is the **reading** view. It
  owns nothing, allocates nothing, is passed around as a value — like
  `ReadOnlySpan<byte>`. Every *question* about text takes a `Spanne`.
* **`Bytes`** (from `lib/str/bytes.fi`) remains the **owning** buffer. Every
  function that *creates* text writes into a `*mut Bytes`.

```firn
fn npos() -> usize                                  // "not found"

// Views
fn span(p: *mut u8, n: usize) -> Span
fn span_empty() -> Span
fn span_of_bytes(b: *mut Bytes) -> Span
fn span_of_c(p: *mut u8) -> Span
fn c_length(p: *mut u8) -> usize
fn is_empty(s: Span) -> bool
fn length(s: Span) -> usize
fn chars(s: Span, i: usize) -> u8
fn span_part(s: Span, of: usize, how_much: usize) -> Span
fn span_ab(s: Span, of: usize) -> Span
fn span_to(s: Span, to: usize) -> Span

// Character classes (ASCII)
fn is_whitespace(c: u8) -> bool         fn is_digit(c: u8) -> bool
fn is_hex_digit(c: u8) -> bool          fn is_upper_letter(c: u8) -> bool
fn is_lower_letter(c: u8) -> bool       fn is_letter(c: u8) -> bool
fn is_alphanumeric(c: u8) -> bool       fn hex_value(c: u8) -> i32
fn big_char(c: u8) -> u8                fn small_char(c: u8) -> u8

// Comparisons
fn equal(a: Span, b: Span) -> bool
fn compare(a: Span, b: Span) -> i32                 // -1 / 0 / 1
fn equal_without_case(a: Span, b: Span) -> bool
fn compare_without_case(a: Span, b: Span) -> i32
fn starts_with(s: Span, part: Span) -> bool
fn ends_with(s: Span, part: Span) -> bool

// Searching
fn find_char(s: Span, c: u8) -> usize
fn find_char_ab(s: Span, c: u8, ab: usize) -> usize
fn find_char_back(s: Span, c: u8) -> usize
fn find(s: Span, part: Span) -> usize
fn find_ab(s: Span, part: Span, ab: usize) -> usize
fn find_back(s: Span, part: Span) -> usize
fn contains(s: Span, part: Span) -> bool
fn contains_char(s: Span, c: u8) -> bool
fn count_char(s: Span, c: u8) -> usize
fn count_part(s: Span, part: Span) -> usize
fn find_not_out(s: Span, set: Span, ab: usize) -> usize

// Trimming
fn trim(s: Span) -> Span
fn trim_left(s: Span) -> Span
fn trim_right(s: Span) -> Span
fn trim_set(s: Span, set: Span) -> Span
fn without_prefix(s: Span, part: Span) -> Span
fn without_suffix(s: Span, part: Span) -> Span

// Splitting (a cursor, not a list)
struct Divisor { source: Span, sep: Span, i: usize, mode: u32, done: bool }
fn divisor_new(s: Span, sep: Span) -> Divisor
fn divisor_whitespace(s: Span) -> Divisor
fn divisor_next(t: *mut Divisor, out: *mut Span) -> bool
fn parts_count(s: Span, sep: Span) -> usize

// Building text
fn append(out: *mut Bytes, s: Span)
fn append_char(out: *mut Bytes, c: u8)
fn set(out: *mut Bytes, s: Span)
fn repeat(out: *mut Bytes, s: Span, times: usize)
fn join_part(out: *mut Bytes, part: Span, sep: Span)
fn fill_left(out: *mut Bytes, s: Span, width: usize, filler: u8)
fn fill_right(out: *mut Bytes, s: Span, width: usize, filler: u8)

// Upper/lower case and replacing
fn after_big(out: *mut Bytes, s: Span)
fn after_small(out: *mut Bytes, s: Span)
fn big_here(b: *mut Bytes)
fn small_here(b: *mut Bytes)
fn replace(out: *mut Bytes, s: Span, old: Span, new: Span) -> usize
fn replace_first(out: *mut Bytes, s: Span, old: Span, new: Span) -> bool
fn replace_char_here(b: *mut Bytes, old: u8, new: u8) -> usize

// UTF-8
struct CharHit { cp: u32, length: usize, valid: bool }
fn is_cont_byte(c: u8) -> bool
fn utf8_read(s: Span, i: usize) -> CharHit
fn utf8_next(s: Span, i: usize) -> usize
fn utf8_before(s: Span, i: usize) -> usize
fn utf8_is_limit(s: Span, i: usize) -> bool
fn utf8_count(s: Span) -> usize
fn utf8_part(s: Span, of_char: usize, count: usize) -> Span
fn utf8_append(out: *mut Bytes, cp: u32)
```

Rules laid down so that nothing has to be guessed:

* **Splitting** follows C# without `RemoveEmptyEntries`: `"a,b,,c"` with
  `","` gives four pieces, `""` gives exactly one empty one, an empty
  separator gives exactly one piece (the whole source). `divisor_whitespace`
  merges runs of whitespace and never returns an empty piece.
* **An empty search text** is *found* at the start position -- the same
  rule as in C# and Rust. It makes `replace` with an empty `old` a
  no-op instead of an endless loop.
* **`utf8_read` checks the same rules as `utf8_is_valid`**: overlong
  encodings, surrogates and everything above U+10FFFF are invalid. An
  invalid octet is reported as **one** character with `valid = false` and
  `cp = U+FFFD` -- that way every loop is guaranteed to make progress.
* **The creating functions clear `out` and require that `out` does not
  lie in the memory of the source.** Whoever wants to work in place takes
  the `_here` forms.
* **`join_part` recognizes "there is already something there" by the
  length of the target buffer.** A *first* empty piece therefore does not
  get a separator appended after it: `["", "a"]` with `"-"` gives `"a"`,
  not `"-a"`. For the regular case (non-empty pieces, or a buffer into
  which something was written before) that is right; whoever has to join
  empty pieces counts along themselves and calls `append` with the
  separator. Deliberately left this way: the alternative would be state
  *in* the caller or a fourth parameter, and both would be worse for the
  regular case.

### 3.2 `std.num` (+422 lines, `lib/num/std_facade.fi`)

One rule for the direction, readable off every name: `write_*` appends
the text of a number to a `*mut Bytes`, `read_*` reads text and returns the
number of **consumed** octets, `text_to_*` is the strict form (the whole
text or `false`).

```firn
fn u64_max() -> u64          fn i64_max() -> i64        fn i64_min() -> i64
fn digit_value(c: u8) -> i32
fn digit_count(v: u64, base: u32) -> usize

fn write_base(out: *mut Bytes, v: u64, base: u32, big: bool)
fn write_u64(out: *mut Bytes, v: u64)
fn write_i64(out: *mut Bytes, v: i64)
fn write_hex(out: *mut Bytes, v: u64, min: usize)
fn write_hex_big(out: *mut Bytes, v: u64, min: usize)
fn write_binary(out: *mut Bytes, v: u64, min: usize)
fn write_octal(out: *mut Bytes, v: u64, min: usize)
fn write_wide_u64(out: *mut Bytes, v: u64, width: usize, filler: u8)
fn write_wide_i64(out: *mut Bytes, v: i64, width: usize, filler: u8)

fn read_u64_base(p: *mut u8, n: usize, ab: usize, base: u32,
                 out: *mut u64, overflow: *mut bool) -> usize
fn read_u64(p: *mut u8, n: usize, ab: usize, out: *mut u64,
            overflow: *mut bool) -> usize
fn read_i64(p: *mut u8, n: usize, ab: usize, out: *mut i64,
            overflow: *mut bool) -> usize
fn text_to_u64_base(p: *mut u8, n: usize, base: u32, out: *mut u64) -> bool
fn text_to_u64(p: *mut u8, n: usize, out: *mut u64) -> bool
fn text_to_i64(p: *mut u8, n: usize, out: *mut i64) -> bool
fn text_to_u64_auto(p: *mut u8, n: usize, out: *mut u64) -> bool   // 0x/0b/0o

fn f64_bits(x: f64) -> u64            fn bits_f64(b: u64) -> f64
fn f64_is_nan(x: f64) -> bool         fn f64_is_infinite(x: f64) -> bool
fn f64_is_null(x: f64) -> bool        fn f64_sign(x: f64) -> bool
fn dtoa_work_bytes() -> usize         fn strtod_work_bytes() -> usize
fn write_f64(out: *mut Bytes, x: f64) -> bool
fn write_f64_bits(out: *mut Bytes, bits: u64) -> bool
fn read_f64(p: *mut u8, n: usize, out: *mut f64) -> usize
fn read_f64_bits(p: *mut u8, n: usize, out: *mut u64) -> usize
fn text_to_f64(p: *mut u8, n: usize, out: *mut f64) -> bool
fn text_to_f64_bits(p: *mut u8, n: usize, out: *mut u64) -> bool
```

* **Overflow is reported, not concealed.** Stage 0 checks nowhere in
  arithmetic (SPEC 14.1.3); when reading *foreign* input a silent
  wraparound would be a hole, not a blemish. `read_*` sets
  `overflow` and reads the digits to the end anyway (the caller has to
  know where the text continues), `text_to_*` returns `false`.
* **The f64 wrapper costs two `mmap` per call** (14 KiB of working memory
  for `dtoa`, one intermediate buffer, because `dtoa` clears its target).
  That is the comfortable form, not the fast one; whoever writes many
  numbers calls `dtoa` directly with their own working memory. It says so
  in the header of the function.
* `f64_bits`/`bits_f64` reinterpret the **bit pattern** (via memory).
  `as` would be a value conversion: `1.0 as u64` is `1`.

### 3.3 `std.vec` (+312 lines, generic, `lib/rt/vec.fi`)

```firn
fn vec_is_empty[T](v: *mut Vec[T]) -> bool
fn vec_shorten[T](v: *mut Vec[T], n: usize)
fn vec_fill[T](v: *mut Vec[T], value: T)
fn vec_swap[T](v: *mut Vec[T], i: usize, j: usize) -> bool
fn vec_invert[T](v: *mut Vec[T])
fn vec_index_of[T](v: *mut Vec[T], value: T) -> usize      // len = not there
fn vec_contains[T](v: *mut Vec[T], value: T) -> bool
fn vec_count[T](v: *mut Vec[T], value: T) -> usize
fn vec_insert[T](v: *mut Vec[T], i: usize, value: T) -> bool
fn vec_remove[T](v: *mut Vec[T], i: usize) -> T            // order is kept
fn vec_remove_fast[T](v: *mut Vec[T], i: usize) -> T       // O(1)
fn vec_append[T](target: *mut Vec[T], source: *mut Vec[T]) -> bool
fn vec_copy[T](source: *mut Vec[T], target: *mut Vec[T]) -> bool
fn vec_equal[T](a: *mut Vec[T], b: *mut Vec[T]) -> bool
fn vec_min[T](v: *mut Vec[T]) -> T
fn vec_max[T](v: *mut Vec[T]) -> T
fn vec_lower[T](v: *mut Vec[T], root: usize, end: usize)
fn vec_sort[T](v: *mut Vec[T])
fn vec_is_sorted[T](v: *mut Vec[T]) -> bool
fn vec_lower_bound[T](v: *mut Vec[T], value: T) -> usize
fn vec_binary_search[T](v: *mut Vec[T], value: T) -> usize // len = not there
fn vec_sorted_insert[T](v: *mut Vec[T], value: T) -> bool
```

**Why heapsort and not quicksort** -- three verifiable reasons:
O(n log n) even in the worst case (quicksort degenerates to O(n^2) on input
that is already sorted or uniformly distributed -- exactly the kind that
occurs constantly in a compiler), no extra memory (mergesort would need n
elements), no recursion (its depth would otherwise hang on the input, and
stage 0 has no stack check). The price is stated along with it: **not
stable**.

The order is that of the type (`<` on `T`) -- for `u64` therefore the
unsigned one. That is why `tests/802` deliberately sorts a `Vec[u64]` as
well, with 2^63 and 2^64-1 in it.

### 3.4 `std.map` (+139 lines, generic, `lib/rt/map.fi`)

```firn
fn map_is_empty[K, V](m: *mut Map[K, V]) -> bool
fn map_dead[K, V](m: *mut Map[K, V]) -> usize
fn map_next[K, V](m: *mut Map[K, V], ab: usize) -> usize   // cap = the end
fn map_first[K, V](m: *mut Map[K, V]) -> usize
fn map_get_or[K, V](m: *mut Map[K, V], k: K, default: V) -> V
fn map_get_if[K, V](m: *mut Map[K, V], k: K, out: *mut V) -> bool
fn map_value_address[K, V](m: *mut Map[K, V], k: K) -> u64  // 0 = missing
fn map_set_if_new[K, V](m: *mut Map[K, V], k: K, v: V,
                        inserted: *mut bool) -> bool
fn map_increase[K, V](m: *mut Map[K, V], k: K, delta: V) -> V
fn map_take[K, V](m: *mut Map[K, V], k: K, out: *mut V) -> bool
fn map_cleanup[K, V](m: *mut Map[K, V]) -> bool
```

Iteration as a **cursor over the slots**, not as an array:

```firn
var i: usize = map_next[K, V](&m, 0)
while i < map_cap[K, V](&m) {
    let k: K = map_slot_key[K, V](&m, i)
    let w: V = map_slot_value[K, V](&m, i)
    i = map_next[K, V](&m, i + 1)
}
```

Reason: `lib/rt/map.fi` deliberately does not include `vec`. A second
import in a module the compiler itself uses is weight without a return, and
`firnc1` deduplicates import paths as strings (round 39) — whoever mixes
`std.map` and `rt.vec` otherwise loads the same file twice. Whoever does
want the keys as an array pushes them into a `Vec[K]` in this loop; then
the import is at the caller, where it belongs.

Documented warning: **do not insert during the iteration** (an insertion
can rehash). Deleting is harmless.

### 3.5 `std.math` (+633 lines)

```firn
// Constants and bit patterns
fn TAU() -> f64      fn SQRT2() -> f64   fn LN2() -> f64    fn LN10() -> f64
fn INF() -> f64      fn NAN() -> f64     fn EPSILON() -> f64
fn math_bits(x: f64) -> u64              fn math_f64(b: u64) -> f64
fn is_nan(x: f64) -> bool    fn is_infinite(x: f64) -> bool
fn is_finite(x: f64) -> bool

// Integers
fn sign(x: i64) -> i64
fn min_u(a: u64, b: u64) -> u64          fn max_u(a: u64, b: u64) -> u64
fn clamp_u(x: u64, lo: u64, hi: u64) -> u64
fn abs_diff(a: i64, b: i64) -> u64       fn gcd(a: u64, b: u64) -> u64
fn lcm(a: u64, b: u64) -> u64            fn ilog2(v: u64) -> usize
fn ilog10(v: u64) -> usize               fn is_pow_two(v: u64) -> bool
fn next_pow_two(v: u64) -> u64           fn pow_u(b: u64, e: u64) -> u64

// Floating point, EXACT
fn fabs(x: f64) -> f64    fn fmin(a: f64, b: f64) -> f64
fn fmax(a: f64, b: f64) -> f64            fn fclamp(x: f64, lo: f64, hi: f64) -> f64
fn trunc(x: f64) -> f64   fn floor(x: f64) -> f64   fn ceil(x: f64) -> f64
fn round(x: f64) -> f64   fn fmod(x: f64, y: f64) -> f64
fn hypot(x: f64, y: f64) -> f64
fn ldexp(x: f64, k: i64) -> f64           fn frexp(x: f64, e: *mut i64) -> f64

// Floating point, APPROXIMATE
fn exp(x: f64) -> f64     fn ln(x: f64) -> f64
fn log2(x: f64) -> f64    fn log10(x: f64) -> f64
fn powf(b: f64, e: f64) -> f64
fn sin(x: f64) -> f64     fn cos(x: f64) -> f64     fn tan(x: f64) -> f64
fn atan(x: f64) -> f64    fn atan2(y: f64, x: f64) -> f64
fn near(a: f64, b: f64, eps: f64) -> bool
```

* **`trunc/floor/ceil/round` do not compute, they cut off bits.** That way
  they stay right at 2^52 and above as well, where every formula with `+0.5`
  is wrong, and `round(0.49999999999999994)` is `0.0` instead of erroneously
  `1.0`.
* **`fmod` is exact**: the magnitude of the divisor is doubled until it lies
  above the remainder, then subtracted while halving — every intermediate
  number lies on the same bit grid, so every subtraction is exact.
* **`fmin`/`fmax` pass over NaN** instead of passing it on; otherwise a
  single NaN poisons every minimum over an array.
* **`exp/ln/sin/cos/tan/atan/atan2` are series with range reduction, without
  tables. They are NOT correctly rounded.** Measured bound in the
  checked range: **relative 1e-12**; for `sin`/`cos` with a large argument
  (|x| = 100) only **1e-10** — the reduction computes in double
  precision and loses digits there. That is the usual limit without
  Payne-Hanek reduction.
* **Naming rule, deliberately kept:** mathematical functions carry their
  international name (`sqrt`, `floor`, `exp`, `sin`) -- the module has been
  called that since round 39, and a second name next to the existing `sqrt`
  would be two names for one thing. Everything that is not an established
  function name is spelled out (`is_pow_two`, `next_pow_two`, `near`).

### 3.6 `std.io` (+285 lines)

```firn
fn println(p: u64, n: usize)             fn eprintln(p: u64, n: usize)
fn print_c(p: u64)                       fn eprint_c(p: u64)
fn print_line()                          fn print_fd(fd: i64, p: u64, n: usize) -> bool
fn read_stdin(out: *mut rt.Buf) -> bool
fn append_file(path: u64, p: u64, n: usize) -> bool      // O_APPEND
fn file_exists(path: u64) -> bool

struct LineReader { p: u64, n: usize, i: usize }
fn lines_new(p: u64, n: usize) -> LineReader
fn lines_of_buf(b: *mut rt.Buf) -> LineReader
fn lines_next(z: *mut LineReader, ap: *mut u64, an: *mut usize) -> bool
fn lines_number(p: u64, n: usize) -> usize

fn fmt_char(f: Fmt, c: u8) -> Fmt        fn fmt_bool(f: Fmt, w: bool) -> Fmt
fn fmt_u64(f: Fmt, v: u64) -> Fmt        fn fmt_hex(f: Fmt, v: u64, min: usize) -> Fmt
fn fmt_c(f: Fmt, p: u64) -> Fmt          fn fmt_repeat(f: Fmt, c: u8, times: usize) -> Fmt
fn fmt_line(f: Fmt) -> Fmt               fn fmt_wide(f: Fmt, v: i64, width: usize, filler: u8) -> Fmt
fn fmt_append(f: Fmt, h: Fmt) -> Fmt     // h is appended AND released
fn fmt_content(f: Fmt, out: *mut rt.Buf) // the content into a buffer, f released
fn fmt_print_line(f: Fmt)                fn fmt_eprint(f: Fmt)
fn fmt_eprint_line(f: Fmt)               fn fmt_in_file(f: Fmt, path: u64) -> bool
```

With that the **two gaps that `docs/ROUND39.md` explicitly named at the
end** are closed: `fmt_char` (a character as a *letter*, not
as a decimal number -- `f"{c}"` otherwise shows `65` instead of `A`) and
`fmt_content` (result into a buffer instead of onto stdout).

The `LineReader` is a cursor over an already read block: every
line comes as pointer+length **inside** the block, without a copy. The
line break does not belong to the line, a preceding `\r` falls away, and a
trailing `\n` produces **no** empty final line -- the rule that
`wc -l` and every editor use.

---

## 4. What was deliberately NOT built — and why

1. **No generic `Option[T]`/`Result[T]` layer.** Monomorphization does not
   substitute type arguments in the **payload of an error union**
   (`docs/RC.md`, deviation A4: `fn f[T](..) -> AllocError!Zaehlverweis[T]`
   reports „unbekannter typ"), and `enum` is not generic (SPEC §14.1.types
   T3). A real `Option[T]` needs a **core change** — and that was ruled out
   for this round. Instead of a fourth, half-finished way, the library uses
   the three forms the language *has* today throughout, and does so
   consistently:
   * **output pointer + `bool`** when both „there/not there" and the value
     matter (`map_hol_wenn`, `text_zu_u64`, `teiler_naechst`)
   * **sentinel value** when an index is sought (`npos()` in `std.str`,
     `vec_len` in `std.vec`, `map_kap` in `std.map` — in each case an index
     that can never be a hit)
   * **default** when the caller knows the substitute (`map_hol_oder`)
2. **No number parser in `std.str`.** Text → number lives exclusively in
   `std.num`. Two parsers would be two truths.
3. **Upper/lower case only ASCII.** Unicode case mapping needs the UCD
   tables; those are in the comptime branch (`tests/602_comptime_ucd.fi`)
   and do not belong in the core facade.
4. **`teiler_*` yields a sequence, not a list.** `lib/str` is an
   include library without module structure and cannot include
   `Vec[T]`. The cursor is the form that allocates nothing anyway.
5. **No sorting by a custom comparison.** Firn has no
   function pointers (`docs/SELF_HOSTING.md`, line 1576). `vec_sortiere`
   orders by `<` on `T`; anything else would need a language feature.
6. **No stable sorting.** See 3.3 — indistinguishable for scalars,
   for pairs it would be a promise that is missing here.
7. **No `Str` type (checked UTF-8) on top of `Spanne`.** `Spanne` is raw;
   `utf8_is_valid` says whether a sequence is text. A type of its own
   without a language means of enforcing it would be a promise without
   cover.
8. **No correctly rounded elementary functions.** That needs tables and
   is a round of its own. The actual bound is stated in 3.5 and is
   measured in `tests/805`.
9. **`read_stdin` is not called in the test** — the test runner gives the
   program no input, and a read would block. The function is a
   forwarding to `rt.lies_stdin` (in use by `bin/firnc1.fi` since round 29).
10. **`std.mem`, `std.rc`, `std.intern`, `std.rt` remained untouched.** They
    are complete for their purpose; added names would only have enlarged
    the surface.

---

## 5. Two findings from the build (named, not rebuilt)

**A. `f"..."` in the condition of an `if`.**

```firn
if !io.fmt_in_datei(f"zahl {255}", pfad) { ... }
//                  ^^^^^^^^^^^^^ error: unbekannter name '_fseg549'
```

The parser hoists the hidden text segments (`let _fsegN: [u8; N]`) in front
of the surrounding **statement**; with an `if`, however, the condition is
already part of that statement, and the name is not visible there. An
intermediate value solves it (`let inhalt: io.Fmt = f"..."`, see
`tests/806`). A fix would be a parser change and therefore does not belong
in this round.

**B. `firnc0` and `firnc1` do not round 17-digit floating point literals the
same way.** Measured with `num.f64_bits` on both compilers:

| Literal | `firnc0` | `firnc1` | correct (IEEE) |
|---|---|---|---|
| `0.30000000000000004` | 4599075939470750516 | 4599075939470750**517** | 4599075939470750516 |
| `9007199254740993.0` | 4845873199050653696 | 4845873199050653**697** | 4845873199050653696 |
| `0.49999999999999994` | 4602678819172646911 | 4602678819172646**909** | 4602678819172646911 |
| `2.718281828459045` | 4613303445314885481 | 4613303445314885481 | identical |
| `0.1` | 4591870180066957722 | 4591870180066957722 | identical |

`firnc0` agrees with the correct rounding, `firnc1` deviates by 1-2 ULP.
That is the **one** known deviation that `tools/lex_compare.sh`
has long been reporting as "DIFFERENT: 1 (known and named: 1) /
FLOATING POINT outside the fast path: 1" -- the new `std.num` only
makes it *visible* for the first time, because it can print bit patterns.
The tests of this round deliberately avoid such literals and **compute**
the values instead (`0.1 + 0.2`, `ldexp(1.0, 51) + 0.5`); the fix belongs
in a lexer round.

**C. A module-qualified call in the expression of an `f"..."` yields
different syntax trees.**

```text
firnc0 --emit=ast-kanon :  (ruf str.utf8_zaehle (id u))
./.astdump (firnc1)     :  (ruf str__utf8_zaehle (id u))
```

When re-lexing the expression segment, `firnc1` already inserts the
**internal** name (`modul__name`, SPEC §14.1.15), `firnc0` the written one.
The *behavior* is identical — `tools/self_compare.sh` reports `GLEICH` for
all affected files, and the programs print the same line.
`tools/parser_compare.sh`, however, compares the tree octet by octet, and
there it shows up. The tests therefore bind such values to a name before
the interpolation. A fix would be a parser change.

**D. `.astdump` hung in an endless loop on `da3b0d9` at EVERY `||` —
`test.sh` never got past section 12.** Found here, fixed by round 41
(`fe31d13`).

Symptom and narrowing down from this round:

```firn
fn f(a: bool, b: bool) -> bool { if a || b { return a } return b }
```

`./.astdump` on these six lines: runs forever, not a byte of output. With
that `tools/parser_compare.sh` hangs on the first source text with `||` —
and that is practically every one. Three measurements have shown that it is
**not** caused by this round:

1. with `lib/rt/vec.fi` and `lib/rt/map.fi` **reset** to `da3b0d9`
   and a freshly built `.astdump`: hangs just the same;
2. the same `bin/astdump.fi`, built with the `firnc0` from the working tree
   of round 41 (where only `compiler/src/regalloc.rs` differed): runs
   through;
3. `.firnc1` itself was never affected — the self-comparison and the
   fixpoint were green on `da3b0d9` as well.

The cause is described in `fe31d13`: the optimization „cell alias" (round
40, `regalloc.rs`) let a load read the cell register directly, although
between the load and the use another value wrote to exactly that register.
In `bin/print.fi`/`drucke_binop`, `43 - start` became
`43 - &tab[start]`, the length underflowed, and `rt.buf_wachse` spun
forever. It only came to light now because the dump binaries were reused in
a stale state before — the same trap that section 7 warns about.

For this branch nothing is open any more: it sits on `fe31d13`, and
the acceptance in section 7 is measured with its own, corrected `firnc0`.

---

## 6. Test coverage

Seven new programs in `tests/`, each of which runs **three times** in
`test.sh` (`opt` / `noopt` / `dev-fast`) and additionally in
`tools/self_compare.sh` against `firnc1`. Every single expectation is a
`return <code>` in the program — if one fails, the test ends with exactly
that code and `test.sh` names it; the printed line is additionally the
comparison point between the compilers. The column „error exits" counts
exactly these `return <code>` (without the final `return 0`); many of them
check several things at once with `||`, so the number of checked promises
is higher. Together: 310.

| Test | Content | Numbers |
|---|---|---|
| `800_std_str_core.fi` | trimming, splitting (fixed separator **and** whitespace), joining, searching (forward/backward/counting), replacing, upper/lower, padding, comparing, character classes, UTF-8 forward/backward/cut by character, invalid octet | 49 error exits |
| `801_std_num_core.fi` | base 2/8/10/16/36, padding, width, u64::MAX, **u64::MAX+1 as overflow**, i64::MIN, prefixes `0x`/`0b`/`0o`, partial reading with a rest, dtoa/strtod wrapper, `1e21`, round trip `0.1+0.2` | 43 error exits |
| `802_std_vec_core.fi` | two instantiations (`i32`, `u64`), searching, sorting, binary search, lower bound, inserting/removing (both forms), copying/appending/comparing, 200 elements descending and 200 identical ones | 41 error exits |
| `803_std_map_core.fi` | cursor over 50 pairs (sum of the keys and values), entry helper, value in place, taking out, **4000 insertions with every third one deleted** and a subsequent cleanup, second instantiation `Map[u32, i32]` | 36 error exits |
| `804_std_math_core.fi` | the **exact** part, everything with `==`: integer helpers, `fabs/fmin/fmax/fclamp`, `trunc/floor/ceil/round` including the largest double below 0,5 and the grid at 2⁵¹, `fmod`, `ldexp`, `frexp` (subnormal too), `hypot`, special values | 57 error exits |
| `805_std_math_f64.fi` | the **approximated** part against named bounds; plus two loops: sin²+cos²=1 at 41 places, `tan(atan(x)) == x` at 30 places | 54 error exits |
| `806_std_io_core.fi` | writing/appending/does-it-exist, lines with `\r\n` and without a final break, the whole `Fmt` extension; the functions that write a break **themselves** (`println`, `print_zeile`, `fmt_druck_zeile`) run with descriptor 1 redirected via `dup2` and are read back from the file — executed, not claimed | 30 error exits |

Not covered and named here: `read_stdin` (see 4.9), the
out-of-memory branches (`heap_alloc` returns 0) — those cannot be triggered
without injecting a failing `mmap`.

---

## 7. Acceptance

Measured on **`fe31d13`** (base of this branch), with a self-built
`firnc0` and freshly built helper binaries.

| Measurement | Value | Starting point `fe31d13` |
|---|---|---|
| `bash ./test.sh` | **PASS 673/673**, `RC=0` | 652/652 |
| `bash tools/self_compare.sh` | **GLEICHES VERHALTEN 196 · ABWEICHEND 0 · FEHLERHAFT 0 · CODEGEN FEHLT 0**, `RC=0` | 189 / 0 / 0 |
| `bash tools/fixpoint.sh` | **stage 2 == stage 3, character-identical (309468 lines of assembly)** · corpus: `.firnc2` behaves like `firnc0`, `RC=0` | character-identical, 309468 lines |

The 673 are 652 + 21: seven new programs × three runs
(`opt` / `noopt` / `dev-fast`). The 196 are 189 + 7. The **309468 lines are
unchanged** — the self-hosted compiler carries not a single byte of the 32
new generic `Vec`/`Map` functions, because it uses none of them
(monomorphization).

(Intermediate state on the old base `da3b0d9`, for the sake of
completeness: 670/670, 195/0/0, fixpoint character-identical at 289096
lines. The same statement, only before the rebase.)

The comparison tools in detail (from the same run):

| Tool | identical | differing |
|---|---|---|
| `lex_vergleich` | 361 | 1 (known: `tests/590_f64.fi`, literal `1e308`) |
| `parser_vergleich` | 235 | 1 (the same known one) |
| `typen_vergleich` | 185 | 0 |
| `sema_vergleich` | 144 | 1 (the same known one) |
| `fir_vergleich` | 143 | 1 (the same known one) |

No tool got a **new** exception: the list of known
deviations (`BEKANNT=` in the scripts) is untouched.

Before the measurement **all** helper binaries of the comparison tools were
deleted (`.astdump`, `.lexdump`, `.firdump`, `.semadump`, `.layoutdump`,
`.firnc1..3`). `lib/firnc1/vec.fi` is a *symlink* to `lib/rt/vec.fi`, and
`find -newer` does not see the change to the target file — a leftover
`.astdump` would have measured the state from before the extension and
would have been green without proving anything.

**One thing still belongs to the honesty of this measurement:**

1. **The measurement ran in its own mount namespace with a private `/tmp`**
   (`unshare --mount` + `tmpfs`). `tools/lex_compare.sh` and the other
   comparers use fixed paths like `/tmp/lexv_a.txt`; if the same suite runs
   at the same time in a second working tree (here: round 41), both write
   into the same files and the results are pure chance. Without a
   namespace `lex_vergleich` once reported 72 deviations, with a namespace
   exactly the one known deviation. Round 41 ran into the same trap
   (`/tmp/parv_a.txt`, „that looked like 148 real deviations"). That is a
   tooling deficiency, recorded here — the scripts should use `mktemp`; as
   long as they do not, only ONE suite may ever run at a time.

## 8. Lines

| File | before | after |
|---|---|---|
| `lib/std/str.fi` (generated) | 741 | 1604 |
| `lib/std/num.fi` (generated) | 1203 | 1625 |
| `lib/std/math.fi` | 116 | 749 |
| `lib/std/io.fi` | 92 | 377 |
| `lib/rt/vec.fi` (= `std.vec`) | 137 | 449 |
| `lib/rt/map.fi` (= `std.map`) | 298 | 437 |
| **Sum** | **2587** | **5241** |

Plus seven test programs with around 1400 lines together.
