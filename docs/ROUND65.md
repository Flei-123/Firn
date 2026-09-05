# Round 65 -- the number reader: one literal, one double

Round 63 left a note in `docs/ROUND63.md` (gap 8): the literal
`9007199254740991.0` becomes the bit pattern 4845873199050653695 in
`firnc0` and 4845873199050653694 in `firnc1`. Two compilers, one source
text, two different numbers. This round is about that one sentence.

A lexer that reads a literal differently from the lexer that bootstrapped
it breaks the fixpoint promise. It does not break it loudly -- the fixpoint
run stays character-identical, because the compiler source contains no
floating point literal that hurts -- it breaks it QUIETLY, in the programs
that the compiler translates.

## 1. What was really wrong

It was not one bug, it was three, and only the first one was known.

| literal | firnc0 (Rust) | firnc1 (Firn), before | what came out |
|---|---|---|---|
| `9007199254740991.0` | 4845873199050653695 | 4845873199050653694 | one ULP too small |
| `1e308` | 9214871658872686752 | 9214871658872686751 | one ULP too small |
| `2.2250738585072011e-308` | 4503599627370495 | 4503599627370499 | four ULP too large |
| `0.30000000000000004` | 4599075939470750516 | 4599075939470750517 | one ULP too large |
| `18446744073709551616.0` | 4895412794951729152 | 0 | **0.0** instead of 1.8e19 |
| `1844674407370955161.9` | 4880100556218669466 | 4599075939470750515 | **0.3** instead of 1.8e18 |
| `123456789012345678901234567890.0` | 5042042089369253694 | 14096529691208278065 | **-6.1e17**, a NEGATIVE number |

The last three are the serious ones. They are not a rounding question; the
literal became a different number, and one of them changed its sign.

## 2. Where it came from -- both sides looked at, not guessed

**The Rust side is correct and stays untouched.** `compiler/src/lexer.rs`
collects the literal text without the separators and hands it to
`str::parse::<f64>`, and that is correctly rounded (Eisel-Lemire with an
exact slow path). That is not taken on trust here: section 4 measures it
against C `strtod` and against Python over 4044 literals, and all three
agree everywhere.

**The Firn side had three defects**, all of them in `float_value` and
`lex_float_rest` of `lib/firnc1/lexer.fi`:

1. **The mantissa wrapped around.** The digits, those before AND those
   after the point, were collected into a `u64`. The guard read
   `if mant > 1844674407370955161`, that is `(2^64-1)/10` -- WITHOUT the
   digit that was about to be added. For `mant = 1844674407370955161` and
   a digit >= 6 the next `mant * 10 + digit` wrapped around silently, the
   overflow flag stayed false, and the fast path then computed with the
   wrapped remainder. That is where `18446744073709551616.0 -> 0.0` and
   the negative number in the table come from.

2. **Outside Clinger's fast path it multiplied step by step.** For
   `|exponent| > 22` the value was multiplied or divided by 10.0 once per
   step -- 308 divisions for `2.2250738585072011e-308`, each of them
   rounding once. The four ULP in the table are simply four of those
   roundings that did not cancel out.

3. **The fraction digits pushed the mantissa out of 2^53.**
   `9007199254740991.0` has the digits `90071992547409910` and the exponent
   -1. The digits alone are larger than 2^53, so `mant as f64` already
   rounds, and the division by ten rounds a SECOND time. Double rounding,
   one ULP. That is the case round 63 found; it is the mildest of the three.

The Rust side has none of the three: `parse::<f64>` sees the text, not a
`u64`.

## 3. What was built

`lib/firnc1/lexer.fi` gets an exact number reader, in integer arithmetic
only (+452 lines, no new module, no dependency on the standard library):

* `float_exact` holds the value as the fraction `D * 10^exp` of two big
  integers, scales it until the quotient has exactly 53 bit, and lets the
  REMAINDER decide the rounding -- at exactly half distance towards the
  even mantissa (IEEE-754 4.3.1). That is the same method as
  `lib/num/strtod.fi` (SPEC 8.4, requirement Z5).
* The big numbers are `u32` limbs in a block of 6 KB on the stack, 256
  limbs each, six of them. The widest intermediate value that can occur is
  under 4100 bit (781 significant digits shifted left by at most 1381), so
  the block is twice as wide as it has to be.
* At most 780 significant digits are kept; everything below is folded into
  a STICKY digit 1. An exact halfway case has at most 752 significant
  digits (5^1075), so no tie can hide in the tail.
* The mantissa limit is now computed WITH the digit
  (`mant > (2^64-1 - digit) / 10`), in the integer collector as well as in
  the fraction. That kills defect 1 in both places.
* Clinger's fast path stays: for `mantissa <= 2^53` and `-22 <= e <= 22`
  ONE multiplication or division in binary64 is correctly rounded, so the
  result is exact to the bit. Its extension for `22 < e <= 37` (fold the
  excess into the mantissa while it stays exact) stays as well. Both are
  now MEASURED and no longer only argued.
* Everything else goes through the exact path. Outside the fast path the
  lexer does not compute in `f64` at all any more -- it builds the bit
  pattern with integers.

`tools/lex_compare.sh` had a list of KNOWN DEVIATIONS with one entry in it
(`tests/590_f64.fi`, "off by up to one ULP, correct would be Eisel-Lemire").
**The list is now empty.**

## 4. The proof -- `tools/lexnum/`

The corpus of the project contains a few dozen floating point literals, all
of them harmless. That is why the divergence only showed up in round 63,
and only by accident. A number reader is not proven by the numbers that
happen to be lying around.

`bash tools/lexnum/run.sh [COUNT] [SEED]` produces several thousand
literals that are MEANT to hurt and has four readers read them:

| reader | what it is |
|---|---|
| `firnc0` | the lexer in Rust, `firnc --emit=tokens` |
| `firnc1` | the lexer in Firn, `bin/lexdump.fi` |
| C `strtod` | glibc, `tools/lexnum/ref.c` |
| Python `float` | CPython, `tools/lexnum/check.py` |

Compared is the BIT PATTERN, not the printed value -- two doubles one ULP
apart print the same in almost every format, which is exactly why the
divergence survived so long. The last two readers are MEASURING
INSTRUMENTS, not dependencies: nothing in the compiler or in the standard
library uses them. They are there so that "both lexers agree" cannot
quietly become "both are wrong in the same way".

The default run (seed 65065) makes 4044 floating point literals in eight
groups: the shortest form of random doubles, random digit strings, forty to
eight hundred significant digits, the whole neighbourhood of 2^53,
subnormals, EXACT HALFWAY CASES written out to the last digit plus one step
to either side, the named hard cases from the literature, and all of that
again with `_` separators. On top of that 1250 integer literals (decimal,
hexadecimal, binary, with separators, at the 64 bit edge) and 16 literals
that have to be REFUSED -- for those, both streams are compared, the tokens
and the diagnostics with line and column.

**The counter-check, so that the test is not a test that always passes:**
run against the OLD lexer, the same 701 literals of a small run give

```
OLD firnc1 lexer differs from firnc0 in: 486 of 701 cases  (69 %)
NEW firnc1 lexer differs from firnc0 in:   0 of 701 cases
```

of which 314 were not a rounding question at all but a different number.
Even the everyday group ("shortest form of a random double") was wrong in
the old reader.

### The pinned cases: `tests/1100` to `tests/1104`

81 checks over the bit pattern, in the reserved range 1100-1129:

| test | checks | what it nails |
|---|---|---|
| `1100_float_literal_bits.fi` | 14 | exactly the literals from the table in section 1 |
| `1101_float_halfway.fi` | 27 | nine ties, each of them exactly in the middle, one step below, one step above |
| `1102_float_subnormal.fi` | 13 | the subnormal range, 2^-1075 as a tie towards zero |
| `1103_float_long_digits.fi` | 9 | up to 800 digits, the sticky digit |
| `1104_float_extremes.fi` | 18 | overflow, underflow, exponents beyond i64 |

These are not lexer unit tests, they are END TO END: `tools/self_compare.sh`
compiles every one of them WITH `firnc1` and runs the result. If the lexer
in Firn read a literal wrongly, the program built by it would return a
non-zero exit code. (That is exactly what happened during the work, with a
stale `.firnc1` still built from the old lexer -- the test struck.)

## 5. The other divergences that were looked for

The order named four more places to check. Result, measured, not assumed:

* **Integer overflow when parsing** -- for pure INTEGER literals the limit
  was already computed with the digit, so there was no divergence there;
  only the float path had the wrong guard, and that is fixed. Measured over
  1250 integer literals in decimal, hexadecimal and binary, with and
  without separators, up to `18446744073709551615` and `0xFFFFFFFFFFFFFFFF`:
  **zero deviations.** Literals that are too large are refused by both with
  the same message.
* **The exponent form** -- `1e9223372036854775807` and
  `1e-9223372036854775807` are read by both as infinity and as zero. The
  Firn lexer stops accumulating the exponent at 100000, Rust reads the
  whole text; the result is the same, because everything beyond +-400
  decimal is decided before the value is computed. No change, named here so
  that nobody has to look for it again.
* **`-0`** -- not a question for the lexer in either compiler: a literal
  has no sign, the minus is a unary operator of the parser. The two lexers
  therefore cannot diverge on it. (`-x` on an `f64` is still missing in
  stage 0 -- round 63, gap 1, still open.)
* **Hexadecimal floating point literals** (`0x1.8p3`) do not exist in the
  language. `0x`/`0b` integer literals are part of the measurement above.

## 6. What is deliberately not done

* **Eisel-Lemire.** The exact path is a big number division and costs about
  a hundred microseconds for a long literal. Over the whole tree that is
  80 literals outside the fast path (section 7) -- not measurable. A
  128 bit fast path would be pure speed and would not make the two lexers
  agree any better than they do now. It stays open, and it stays optional.
* **The Rust side stays on `str::parse`.** Writing a second number reader
  by hand there would only create a second place that can be wrong. The
  point of two implementations is that they were written INDEPENDENTLY.
* **The duplication is named.** `lib/num/strtod.fi` does the same thing for
  the standard library. The compiler must not depend on the standard
  library, so the exact path exists twice. That is a price, not an
  accident; whoever changes one has to look at the other.

## 7. The measurements

Everything below was run in this worktree, on this branch. The numbers are
not written by hand anywhere -- they come out of the runs.

```
bash test.sh                    PASS 977/977, 0 failed, 785 s
                                (263 programs x 3 runs; before: 961/961)
bash tools/self_compare.sh      SAME BEHAVIOUR 264, DIFFERING 0, FAULTY 0,
                                CODEGEN MISSING 0
bash tools/fixpoint.sh          STAGE 2 == STAGE 3, character-identical,
                                3395416 octets, 577655 lines of assembly
bash tools/lex_compare.sh       SAME 567, DIFFERENT 0 (known and named: 0),
                                844435 tokens, 80 floating point literals
                                outside the fast path
                                (on its own, before a benchmark run has
                                 produced sources under bench/.work:
                                 SAME 491, DIFFERENT 0, 544156 tokens, 76)
bash tools/lexnum/run.sh        4044 floating point literals, 1250 integer
                                literals, 16 refused; firnc0 vs firnc1 0,
                                vs C strtod 0, vs Python 0 differing;
                                3749 of them through the exact path
bash tools/english/check.sh     0 0 0 0 0
bash tools/fmt/run.sh --fast    608 files formatted, 0 changed by the shape,
                                token stream 0, syntax tree 0, second run 0
```

The 977 come from 961 + 15 (five new tests, each in three optimisation
modes) + 1 (the new section 28).

## 8. A finding beside the point: 140 MB of work directory in the repository

`tools/english/check.sh` did not report `0 0 0 0 0` at the base commit
either -- it reported **828 German path names**, and it did so before a
single line of this round was written. The cause is not German:

`.js-work/` got into the repository in round 63 -- **32962 files, 140 MB**.
Everything in it is PRODUCED: `.js-work/t262` is unpacked by
`tools/js/run.sh` from `testdata/test262/test262-subset.tar.gz`, `cmp/`
comes from `tools/js/compare_node.sh`, the reports come from the harness,
`jsparse`/`jsrun` are binaries. `.gitignore` had no entry for it.
`tools/english/check_names.py` holds every path that git manages against
the morpheme table, and the test suite of TC39 is full of file names with
`fall`, `hole`, `element`, `primitive` in them.

Fixed the way it should be: `.js-work/` is now in `.gitignore` and the
directory is untracked (`git rm -r --cached`). Nothing is deleted from
disk, and `tools/js/run.sh` unpacks what it needs anyway -- section 9d of
the acceptance run above proves that. It is a commit of its own
(`.js-work: 32962 files out of the index`) so it can be dropped separately
if it is not wanted.
