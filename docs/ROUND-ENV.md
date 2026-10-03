# Round FIRN-ENV — build-time environment variables in the language

*Vorgänger: FIRN-LUECKEN (508f6c796).*

## The occasion

In the OrientOS tree there has been a brand variable since commit `c3ecd95`, built after
the template `/root/projects/freeviewer/src/brand.rs`. In Rust that is one
line:

```rust
pub const NAME: &str = match option_env!("FV_BRAND_NAME") {
    Some(s) => s,
    None => "FreeViewer",
};
```

In Firn that was not possible. The worker had to make do with `tools/marke-einsetzen.py`:
a script that writes the values into a `/tmp` copy of the
source text before translating. Its comment says it literally — *"Firn has no
`option_env!`, so the build does it."* This round turns that into language.

## What there is now

| Notation | yields | if not set |
|---|---|---|
| `__env_or("FIRN_X", "default")` | `str` | the second argument |
| `__env_has("FIRN_X")` | `bool` | `false` |

The form follows what the language already uses for `__v128_*`
(`compiler/src/simd.rs`): an **intrinsic function** with a name with which
nothing can collide. No operator, nothing implicit. Both arguments
must be text literals.

In addition comes, because it is useless without:

```firn
const NAME: str = __env_or("FIRN_X", "FreeViewer")     // new: const with text
const VOLL: str = NAME + " " + "1.0"                    // computed at build time
```

Until this round `const` could do integers, `bool` and (since
FIRN-GAPS) floating-point numbers. Now `str` as well.

## Where this happens: in the parser

`__env_or(a, b)` becomes **in the parser** exactly the node that a hand-written
text literal produces (`ExprKind::Text` over the array literal
of its octets). Everything behind the parser sees no difference from a
literal. That answers three demands at once:

* the value works in `const`, in `static`, in an initialisation and
  in an interpolation, without any of these places learning a new case;
* a program that **runs** never asks the environment again — the octets
  stand in the binary, exactly like those of a literal. Verified with
  `env -i` (`tools/env/run.sh`, point 3);
* `firnc1` can do the same at the same place
  (`lib/firnc1/parser.fi::env_call`), so both translators print
  the same text for `--emit=ast-canon`
  (`tools/parser_compare.sh`).

## The limits, and why each one is there

A translator that writes **arbitrary** environment into the binary is a
way to get the secrets of a build machine into a delivered program.
Therefore:

1. **Positive list.** A name is read only if it begins with an allowed
   prefix. Without an option that is `FIRN_` alone; the build adds its
   own with `--env-allow=<prefix>` (several times or comma-separated).
   A name outside the list is an **error** — always, no matter whether the
   variable is set or not. That is important: an error that depends on the
   environment would be a second way in which two builds
   differ.
2. **Shape of the name.** `A-Z`, `0-9`, `_`, at most 64 octets. Not because
   lower case would be technically hard, but because `__env_or("path", …)`
   next to `PATH` is a trap.
3. **Value.** At most 4096 octets, valid UTF-8, no control characters. Too
   long or not UTF-8 is an error and **no silent truncation**: a
   halved brand name is worse than a build that stops.
4. **Log.** `--env-log` prints every reading with value and origin. Without
   the option nothing is printed.

```
$ FIRN_TEST_BRAND=OrientOS firnc --env-log tests/1640_env_const.fi -o x
env: FIRN_TEST_BRAND = "OrientOS" (environment)
env: FIRN_TEST_BRAND ? true
```

Both translators print these two lines character for character the same.

## The fixed point

`bin/firnc1.fi` uses neither of the two intrinsics and has no
positive list beyond the default — so the environment does not reach the
self-translation at all. Stage 2 and stage 3 stay character-identical,
whatever is set. That is not luck, but the reason why the
positive list is empty by default.

## What does NOT work (honestly)

* **`static NAME: str = "…"`** does not work. A `str` is a pointer and a
  length; a pointer in a data section needs a relocation, and stage 0
  does not have one. The message says so cleanly. What works:
  `static NAME: [u8; 10] = __env_or(…)` — as an array, with an **exactly** fitting
  length.
* **`[u8; _]` for `static`** does not work (round 79 enabled length inference
  only for `let`/`var`). That is why with an array `static`
  you have to know the length.
* **`__env_or` with a computed name** does not work and should not: the name is
  read while parsing. What has to be calculated belongs in `comptime`.
* **No `__env_int_or`.** A number from the environment would be the same folding with
  `ExprKind::Int` instead of `Text` — it is not built, because nobody
  needed it.
* **`comptime` still calculates only with `i128`.** The strings of this round
  live in the constant walks of `sema` (`const_octets`), not in the
  `comptime` interpreter. A `comptime` program therefore still cannot
  calculate with texts; `emit_raw` still takes only literals.

## For OrientOS: what the line looks like there

Proven, not rebuilt — the rebuild belongs to the OrientOS chat.

Today (`kernel/marke.fi` + `marke.conf` + `tools/marke-einsetzen.py`):

```firn
static mut s_produkt: [u8; 32] = "???????????????????????????????\0"
// … and a Python script replaces the question marks in a /tmp copy
```

With this round, **in the kernel profile, without a collector** — verified with
`firnc --profile=kernel -c`:

```firn
// The text is a pointer and a length. Every structure of this shape
// (round 88 calls it a "view" onto `str`) takes a text literal --
// and thereby also an `__env_or`.
struct Text { p: *mut u8, n: usize }

fn produkt() -> Text {
    let t: Text = __env_or("OSUM_MARKE_PRODUKT", "OrientOS")
    return t
}
```

In the app profile the shorter form works, the direct twin of `brand.rs`:

```firn
const PRODUKT: str = __env_or("OSUM_MARKE_PRODUKT", "OrientOS")
```

The build call gets an option added:

```sh
OSUM_MARKE_PRODUKT="Xoffi OS" firnc --env-allow=OSUM_MARKE_ kernel/marke.fi …
```

With that `tools/marke-einsetzen.py` (192 lines), the `/tmp` copy of the
kernel tree and the question-mark placeholders drop out. `marke.conf` can stay if the
defaults are to stand in a file — then the build reads it and sets it as
environment; the language no longer needs it. The length test that the script
does (`MAX`/`MAX_URL`) becomes superfluous: a `Text` carries its length itself.

## Files of this round

| File | what |
|---|---|
| `compiler/src/env.rs` | new — positive list, limits, log (6 module tests) |
| `compiler/src/parser.rs` | `env_call`, `literal_octets`, `text_from_octets` |
| `compiler/src/sema.rs` | `const` with `str`, `const_octets`, number lock |
| `compiler/src/lower.rs` | materialise text constant like a literal |
| `compiler/src/main.rs` | `--env-allow=`, `--env-log` |
| `lib/firnc1/parser.fi` | twin: `env_call` and the limits |
| `lib/firnc1/sema.fi` | twin: `const_octets`, `k_tdata` |
| `lib/firnc1/lower.fi` | twin: materialise text constant |
| `bin/firnc1.fi` | the two options, the log |
| `tests/1640_env_const.fi` | the default case in the corpus |
| `tests/neg/1641…1645` | five limits, one message each |
| `tools/env/run.sh` | both translators, both cases, `env -i` |
