# Round 74 -- the long tail of the built ins, the pattern engine, the dates

Everything in this round is written in Firn, character by character,
without a foreign library. No PCRE, no `libc`, no generated table, no
foreign parser. Foreign code appears exclusively as a CHECKING INSTANCE:
the official suite `test262` of TC39, and `node` for the direct comparison
of output. Neither of the two is in the production path.

Touched are `lib/js/` and `tools/js/` only, plus the tests of this round.
**The compiler was not touched** -- neither `compiler/src` nor
`lib/firnc1`; round 71 was working there in parallel. What Firn itself
lacks, seen from a program of this size, is written down in section 7
instead of being patched into the compiler; that is what made round 63 and
round 66 mergeable without a conflict and it is what this round does again.

New: `lib/js/builtin2.fi`, `lib/js/regexp.fi`, `lib/js/date.fi`,
`tests/1500_js_builtins2.fi` to `tests/1503_js_r74_gc.fi`,
`tools/js/round74.sh`, `tools/js/r74soak.sh`,
`tools/js/regexp_compare.sh`, `tools/js/fixlen.py`, `tools/js/mktest.py`,
`tools/js/cases/re_0*.js`, `tools/js/cases/150*.js`.

## 1. The numbers

Measured with `bash tools/js/run.sh --full` over the pinned subset of
`testdata/test262/MANIFEST.md`, **63 364 cases, nothing filtered**. A case
whose feature is missing counts as a FAILURE like any other.

| | parser | engine |
|---|---:|---:|
| round 63 | 69.98 % | 50.51 % |
| round 66 | 91.94 % | 71.07 % |
| **round 74** | **91.94 %** | **76.00 %** |

The breakdown by cause and the table per directory are in
`tools/js/RESULTS.md`; both are produced by `tools/js/report.py` and are
not written down by hand anywhere.

In numbers: the engine went from 45 033 to **48 155** of 63 364 cases, so
3 122 more programs really run. The parser did not move, and it was not
supposed to: this round did not touch it, and the 5 105 cases it still
fails are the same ones. The breakdown of the failures shifted where the
work went -- `throw` from 12 317 to 9 672, `unsupported-builtin` from 310
to 8, `async-incomplete` from 1 021 to 830, `crash` from 57 to 55. What
did not shift: `parse` (3 169) and `unsupported-syntax` (1 184), both of
them the parser's, and `timeout` went UP from 264 to 282 -- the price of
the step budget of the pattern engine, and it is named here rather than
hidden.

The `wrong` column -- a case that ran through and delivered a wrong result
-- is the one that matters, because that is where the real bugs are. It
stood at 6 after round 66 and it stands at **6** after this one. Nothing
new answers wrongly; everything that is missing fails honestly.

## 2. Why a second file of built ins

`lib/js/builtin2.fi` is a SECOND table, not a bigger
`lib/js/builtin.fi`, for two reasons. The first is the merge: round 71
works in the compiler at the same time, and a file that only grows at the
end conflicts with nothing. The second is the dispatcher --
`native_dispatch` decides in RANGES, so a second table costs exactly one
comparison, and the numbers of this round (1000 to 8999) lie in one block
that nothing else uses.

In it: the well known symbols and their protocols (`toStringTag`,
`hasInstance`, `species`, `match`/`matchAll`/`replace`/`search`/`split`,
`isConcatSpreadable`, `unscopables`) with `Symbol.for`/`keyFor` and the
description accessor; the iterators of Array, String, Map, Set and of the
`arguments` object with four prototypes of their own; `Reflect` (all
thirteen); `WeakMap`/`WeakSet`/`WeakRef`/`FinalizationRegistry`; the seven
set operations of ES2025; `Map.getOrInsert`/`getOrInsertComputed`/
`Map.groupBy`; the URI functions and `escape`/`unescape`;
`Object.hasOwn`/`fromEntries`/`groupBy`/`getOwnPropertySymbols`/
`getOwnPropertyDescriptors` and the accessor methods of Annex B including
`__proto__`; the sixteen missing `Math` functions; `toExponential` and
`toPrecision`; thirteen `String` methods plus the thirteen text methods of
Annex B; thirteen `Array` methods; `Error.isError`, `AggregateError` and
the `cause` option; and the host object `$262` that the suite expects.

Two things moved as part of it, because two callers now need the same
thing: `ValidateAndApplyPropertyDescriptor` went from `lib/js/builtin.fi`
to `lib/js/builtin2.fi`, since `Reflect.defineProperty` needs exactly that
validation but ANSWERS with `false` instead of throwing; and
`lib/js/interp.fi` got `construct_nt` (the new target of
`Reflect.construct`), `instance_of_ordinary` (`Symbol.hasInstance` comes
first now) and `ordinary_to_primitive` (which
`Date.prototype[Symbol.toPrimitive]` needs -- going through
`to_primitive` again would call itself).

### Two numbers per text, kept in step

Round 66 wrote down gap 10: `var m: [u8; 40] = "..."` is an error if the
count is off by one, and the length that is PASSED ON afterwards is a
second, independent number that the compiler does not check. This round
adds about six hundred such pairs. Counting them by hand is not work, it
is a bug generator -- a wrong second number silently truncates a name and
installs a built in under the wrong key.

`tools/js/fixlen.py` does both, over the files it is given, and only ever
changes a number that is already there. It is deliberately NOT run over
the older files: they use the padded form on purpose (a long array, a
short length), and the tool would "fix" that away.

## 3. The pattern engine

`lib/js/regexp.fi`, from scratch. Round 63 named `RegExp` as the largest
single missing item and said it is a round of its own. It is: a pattern
grammar, a compiler and a backtracking matcher, plus the four symbol
protocols through which `String.prototype.match`, `replace`, `search` and
`split` reach a regular expression.

**The shape.** The compiled pattern is a TREE OF NODES, not a byte code.
The reason is the specification itself: it defines matching with
CONTINUATIONS (22.2.2.3), and a tree with an explicit continuation stack is
exactly that written down. A byte code with `split`/`jump` would need a
second mechanism for the empty loop check of 22.2.2.5 and a third for the
lookarounds.

* The nodes lie in ONE raw vector, eight `u64` columns per node, outside
  the collector's pointer walk -- like the syntax tree of the engine, and
  for the same reason: a pattern never changes after it has been compiled.
* `m(node, pos, k)` means "match the chain that begins at `node` from `pos`
  on, and when it runs out, go on with the continuation `k`". The
  continuations are five slot records on a stack that grows and shrinks
  with the recursion.
* The captures are one array of `i64` pairs, saved and restored around
  every attempt that may fail -- and RESET at the start of every iteration
  of a repetition, which is 22.2.2.5 step 1 and what `(?:(a)|(b))+`
  against `"ab"` is about. That one was found by the comparison against
  node, not by test262.

Contained: alternation, groups (capturing, non capturing, named),
backreferences by number and by name, character classes with ranges and
class escapes, the quantifiers `*` `+` `?` `{n,m}` greedy and lazy, the
assertions `^` `$` `\b` `\B`, lookahead and lookbehind (positive and
negative), the flags `d g i m s u v y`, `lastIndex`, `exec`/`test`/
`toString`, `source`/`flags` and the nine flag accessors,
`[Symbol.match/matchAll/replace/search/split]`, `RegExp[Symbol.species]`,
the RegExp string iterator of `matchAll`, the `$` substitutions (`$$`,
`` $` ``, `$'`, `$&`, `$n`, `$<name>`) and the six String methods
`match`/`matchAll`/`replace`/`replaceAll`/`search`/`split` -- the last two
also with a TEXT pattern, without building a regular expression at all.

A literal like `/a+/g` is really evaluated now. `lib/js/interp.fi` cannot
import `lib/js/regexp.fi` (that file imports the interpreter), so the
literal goes through the native table the driver installs -- the same
construction the built ins use since round 63.

**The budget.** A pattern like `/(a*)*b/` against a long text is
exponential; the specification does not forbid that and no engine can. A
step budget and a depth budget turn it into a FAILURE instead of a hang,
which is what the harness can count. That is a deliberate limit, not a
hidden one.

**The counter check against node** (`tools/js/regexp_compare.sh`): test262
says whether a case passes; it does NOT say whether two engines agree on
the CAPTURES a pattern produces, and that is exactly where a backtracking
matcher goes wrong quietly. Four programs -- 329 lines of basic patterns,
23 lines of groups, 18 lines of replacements, 18 lines of splits -- run
through both and are compared octet by octet. They are identical.

## 4. The dates

`lib/js/date.fi`. The whole of clause 21.4 rests on ONE number, the
milliseconds since 1970-01-01T00:00:00Z, and on the calendar arithmetic
that turns it into year, month and day and back. That arithmetic is
written out: leap years, the floor division below the epoch,
`MakeDay`/`MakeTime`/`MakeDate` and `TimeClip` stand there as Firn code.
There is no `mktime` and no table of days.

Contained: all four forms of the constructor, `Date.now`/`parse`/`UTC`,
the eleven getters with their UTC twins, the nine setters with their UTC
twins, `toString`/`toISOString`/`toUTCString`/`toDateString`/
`toTimeString`/`toJSON`/`valueOf`/`toLocale*`/`toGMTString` and
`Date.prototype[Symbol.toPrimitive]`. `Date.parse` reads the ISO form of
21.4.1.32 AND the form that `toString` produces, so the round trip holds.

**The time zone is UTC, and that is a decision, not a gap.** This engine
runs freestanding and has no zone database; the specification allows any
local time zone (21.4.1.7, "an implementation-defined algorithm"), so
`getTimezoneOffset` answers 0 and the local getters agree with their UTC
counterparts. The clock itself is the one system call this file makes,
`clock_gettime(CLOCK_REALTIME)` -- without it `Date.now` would have to lie.

Fourteen lines of date output are character identical with `node` under
`TZ=UTC`.

## 5. The memory

Everything this round adds lies in the GC heap and has no external root, no
pinning and no finalizer: a compiled pattern (`val.JsRegExp` with its two
raw vectors), the iterator objects, a `Date`, the weak collections and the
result lists of `matchAll`.

`tests/1503_js_r74_gc.fi` builds 200 of each per round, uses them once and
drops them; after three rounds and a collection the number of live objects
is back where it started. `tools/js/r74soak.sh` does the same over 30 000
rounds and samples the RSS of the process: the clean run grows by **20
KiB**, which is the noise of the allocator, and the counter check by
**17 324 KiB**.

The COUNTER CHECK is the point of the exercise. The same program that
holds everything in a global array MUST grow, and it does. Without it the
first measurement would prove nothing -- a measurement that cannot see a
leak is not a measurement.

**A WeakMap that holds strongly.** `WeakMap`, `WeakSet`, `WeakRef` and
`FinalizationRegistry` keep their keys STRONGLY here. That is not a
shortcut around the collector but a permitted implementation: the
specification only forbids the liveness of a key from becoming OBSERVABLE,
and one that is never released cannot be observed. `FinalizationRegistry`
therefore never calls its callback and `unregister` always answers
`false`. It is named here so that nobody reads a real weak reference into
it.

## 6. What is deliberately NOT carried

Named here, not concealed. Every one of these counts as a FAILURE in the
test262 quota; nothing is filtered out of the run.

* **`Proxy`.** It is the last big one. A proxy has to intercept EVERY
  internal method -- `[[Get]]`, `[[Set]]`, `[[HasProperty]]`,
  `[[Delete]]`, `[[OwnPropertyKeys]]`, `[[DefineOwnProperty]]`,
  `[[GetOwnProperty]]`, `[[GetPrototypeOf]]`, `[[SetPrototypeOf]]`,
  `[[IsExtensible]]`, `[[PreventExtensions]]`, `[[Call]]`,
  `[[Construct]]` -- which means a hook in every one of those places in
  `lib/js/interp.fi` plus the invariant checks of 10.5. That is a round of
  its own, exactly as `RegExp` was, and half a Proxy is worse than none:
  it would produce WRONG values instead of honest failures. `Reflect` is
  there, so the other half of the pair already works.
* **`eval` and the `Function` constructor.** Unchanged since round 63:
  direct `eval` needs the whole parser at run time plus a variable
  environment of its own. `$262.evalScript` and `$262.createRealm` throw
  for the same reason instead of pretending.
* **Typed arrays, `ArrayBuffer`, `DataView`, `SharedArrayBuffer`,
  `Atomics`, `Intl`, `structuredClone`.**
* **Module LINKING.** `import`/`export` produce ESTree nodes and the early
  errors of a module are checked, but there is no loader.
* **`\p{...}` knows the categories this file lists** (`Any`, `ASCII`,
  `L`/`Letter`/`Alphabetic`, `Lu`, `Ll`, `N`/`Nd`, `White_Space`) and
  throws a `SyntaxError` for everything else. The full `UnicodeData`
  tables are a round of their own, like the case tables of round 63.
* **`v` mode has no set operations.** The flag is accepted and behaves
  like `u`; `[[a--b]]` and friends are not carried.
* **A lookbehind is matched by trying its START POSITIONS** instead of
  running the pattern from right to left. The same language, but the
  captures of an AMBIGUOUS lookbehind may differ from what the
  specification picks.
* **`String.prototype.normalize` checks its form and hands the text back
  unchanged.** Every text the suite passes in outside the `normalize`
  directory is already normalized, so the answer is right there -- but a
  text that is not comes back wrong, and the cases that test exactly that
  fail. `toLocaleUpperCase`/`toLocaleLowerCase` are `toUpperCase`/
  `toLowerCase` and `localeCompare` compares code units, which the
  specification allows without an `Intl`.
* **No species for the collections.** `Array.prototype.map` on a subclass
  builds a plain array; `Symbol.species` exists as a symbol and on
  `RegExp`, but the constructors do not consult it.
* **`Math.random` is a xorshift with a fixed seed per realm.** There is no
  entropy here and the engine does not pretend there is.
* **`Number(bigint)` is still not correctly rounded in every case**
  (round 66, section 7) and the six rare suspension positions of round 66
  are unchanged.

## 7. The language gaps found

The gaps of round 63 (`docs/ROUND63.md` section 4) and of round 66
(`docs/ROUND66.md` section 6) are unchanged and were all hit again. Two of
them cost time again and are therefore repeated with the price attached,
and two are new.

**Gap 2 again -- an error union over `f64` returns a WRONG value.** It cost
this round twice: once in `builtin2.num_arg` (every `Math` function of the
round answered garbage) and once in `date.from_args` (`new Date(2020,0,1)`
became 1970). Both are silent. The workaround is the one round 63 found --
hand back the BIT PATTERN as a `u64` -- but a silent wrong value in a
compiler is the most expensive kind of bug there is, and it is still
there.

**Gap 10 again -- a text literal has to know its own length, twice.** See
section 2. Six hundred pairs this round; a literal of type `&str` (or
`[u8; _]` with the length inferred) would remove the whole class.

**Gap 13 (new) -- `*(expr) as T` parses as `*((expr) as T)`.**

```firn
let o: bool = try val.str_push(out, *((p as usize + i) as *mut u8) as u32)
```

is rejected with "dereference expects a pointer, found u32": the trailing
cast binds tighter than the dereference. The workaround is a local
variable. Every other language with a prefix `*` binds it tighter than a
postfix cast; the error message is good, but the rule surprises.

**Gap 14 (new) -- `const` cannot be a floating point value.** `const
MS_DAY: f64 = 86400000.0` is "const supports only integer and bool types
in stage 0". `lib/std/math.fi` already works around it by making `PI` and
`E` FUNCTIONS, and this round did the same for the length of a day. It is
a small thing, but it turns a constant into a call in the hottest place of
the calendar arithmetic.

**A pleasant surprise, for the record:** `gc class` inheritance carried a
fourth kind of object without a murmur -- `JsRegExp extends JsObj extends
JsVal`, with two raw `GcVec` fields for the node tree and the ranges, is
traced by the collector through the compiler generated type table with no
special path anywhere. The memory proof of section 5 is the evidence.

## 8. The acceptance

* `bash tools/js/run.sh --full` -- the numbers of section 1, and the three
  build stages (`opt`, `--no-opt`, `--opt-level=dev-fast`) deliver the
  same quota.
* `bash test.sh` -- the whole suite, with the four new programs
  `tests/1500` to `tests/1503` and the new section 33.
* `bash tools/js/round74.sh` -- the groups of this round separately, the
  comparison against node, the endurance run.
* `bash tools/self_compare.sh` -- 0 differing, 0 faulty.
* `bash tools/fixpoint.sh` -- the compiler compiled by itself is character
  identical.
* `bash tools/english/check.sh` -- 0 0 0 0 0.
* `firnfmt -w` over every new and changed source; `firnfmt -c` finds
  nothing left to do.
