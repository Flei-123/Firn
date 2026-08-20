# Round 63 -- the JavaScript engine: lexer, parser, interpreter

Everything in this round is written in Firn, character by character, without
a foreign library. No LLVM, no libc, no foreign parser, no regular
expression library. Foreign code appears exclusively as a CHECKING
INSTANCE: the official suite `test262` of TC39 and `node` for the direct
comparison of output. Neither of the two is in the production path.

New: `lib/js/` (nine files), `tools/js/`, `testdata/test262/`,
`tests/1000_js_lex.fi` to `tests/1005_js_builtins.fi`.
Nothing was changed at the compiler (`compiler/src`, `lib/firnc1`) -- three
other rounds were working there in parallel.

## 1. What is in there

```
lib/js/unicode_id.fi   generated: the ID_Start/ID_Continue ranges of Unicode
lib/js/lex.fi          the lexer of ECMA-262 clause 12
lib/js/ast.fi          the syntax tree in ESTree form, columnar
lib/js/parse.fi        the parser of clauses 13-16, recursive descent
lib/js/val.fi          the values and the object model, in the GC heap
lib/js/interp.fi       the interpreter of clauses 7, 9, 10, 13, 14
lib/js/builtin.fi      the built in objects of clauses 19-24
lib/js/parse_main.fi   the driver of the parser (root file)
lib/js/run_main.fi     the driver of the engine (root file)
```

### The lexer

Scans **on demand**, one token per call, and takes the GOAL SYMBOL as a
parameter. That is not a nicety: ECMAScript has no context free lexical
grammar. The same slash is a division operator or the start of a regular
expression depending on which goal the syntactic grammar is asking for
(12.1), and the closing brace of a template substitution is only a brace if
the parser is not inside a template. Everything that pretends otherwise --
a token array produced in advance plus a heuristic over the previous token
-- gets `a = b /hi/g.exec(c)` wrong. Here the parser re-scans a `/` exactly
where a primary expression may begin (`lex_regexp_from`), and calls
`lex_template_continue` on the brace. **No heuristic anywhere.**

Contained: white space and line terminators including the Unicode `Zs`
class, both comment forms, the LINE TERMINATOR BIT that automatic semicolon
insertion hangs on, identifiers over the real Unicode properties
(`lib/js/unicode_id.fi`, generated from `unicodedata`, Unicode 14.0.0, 647
`ID_Start` and 754 `ID_Continue` ranges) with `\uXXXX` and `\u{...}`
escapes, all numeric forms of 12.9.3 with numeric separators and both
legacy forms, string literals with every escape, template literals with
cooked AND raw text and the ES2018 rule for invalid escapes, and regular
expression literals as a token including the character class rule and the
flag check.

**The numeric values are exact.** A decimal literal goes through `strtod`
(`lib/num/strtod.fi`, correctly rounded); a literal in a power-of-two radix
is computed BIT EXACTLY here, with round to nearest ties to even, instead of
by repeated multiplication -- which is why `0x20000000000001` becomes 2^53
and `0x20000000000003` becomes 2^53+4 (`tests/1000_js_lex.fi`).

### The parser

Recursive descent to an ESTree shaped tree. The COVER GRAMMAR is solved the
way the specification describes it: `(a, b)` is read as an EXPRESSION and
turned into a parameter list at the `=>` (`to_pattern`); object and array
literals become destructuring patterns the same way.

**The tree lies OUTSIDE the GC heap** -- eight parallel `u32` columns, a
node is an index, 32 octets, no heap block. A closure refers to its function
by NUMBER. That is the separation the round asks for: the JS objects belong
to the collector, the program text does not. A large file makes hundreds of
thousands of nodes; as GC objects they would be the largest single graph in
the process and would be walked at every cycle although nothing in a syntax
tree ever changes after it has been built.

**Early errors.** A parser that only builds a tree is worth little for
test262: a large part of the suite consists of programs that MUST NOT parse.
Implemented are, among others: strict mode (octal literals and escapes,
`with`, assignment to `eval`/`arguments`, `delete` of a bare identifier,
duplicate parameters, the strict reserved words), the assignment target,
duplicate lexical declarations and the `var`/`let` clash up to the function
boundary, `new.target` outside a function, label resolution for
`break`/`continue`, `return` outside a function, rest elements, the arity of
getters and setters, two `__proto__` data properties in one object literal,
a declaration as the body of `if`/`while`/`for` (with the Annex B.3.3
exception for the sloppy function declaration), an escaped reserved word,
`??` mixed with `&&`/`||` without parentheses, and the restricted
productions of automatic semicolon insertion.

### The interpreter

Execution contexts and scope chains with closures, `this` (an arrow function
has none of its own, a sloppy function gets the global object, a strict one
does not), prototype chains, property attributes
(writable/enumerable/configurable), getters and setters, the coercions of
clause 7 -- `==` in its full ugliness written out (7.2.15) -- and
exceptions.

Firn has error unions, not unwinding, so a completion record is carried
explicitly: `ctx.comp` says normal/throw/return/break/continue, the VALUE
lies in `realm.pending` -- in the GC heap, because a thrown object has to
stay alive. `AllocError!` stays the OTHER channel and means the heap is
exhausted, not that the program threw. Whoever mixes the two gets an out of
memory that a `catch` block swallows.

### The values -- and why every one of them is an object

**Every JavaScript value is a `Gc[JsVal]`.** No tagged word, no NaN boxing,
no immediate. The reason is the collector: it scans the stack
CONSERVATIVELY (SPEC 3.5.3), so it recognizes a pointer by its bit pattern.
A NaN boxed pointer, with its high bits set, would not be recognized -- the
object would be collected while a local variable still held it. A packed
small integer would be harmless in the other direction, but a
representation that is only half safe is worth nothing.

`gc class` with single inheritance gives the prefix layout and with it the
free upcast (SPEC 4.4), so a property value, an array element and a variable
binding are all simply `Gc[JsVal]`, and the collector traces them through
the compiler generated type table. No special path, no external root, no
pinning.

The price is named: a number costs a heap object of 24 octets instead of 8.
`realm_num` caches the integers 0..1024; everything else allocates. That is
what makes the collector work in an interpreter loop -- and it is exactly
what the endurance run of this round measures.

## 2. The measurements

See section 5 for the exact numbers of the acceptance run; they are produced
by `bash tools/js/run.sh` and are not written down by hand anywhere.

## 3. What is deliberately missing

Named here, not concealed. Every one of these counts as a FAILURE in the
test262 quota -- nothing is filtered out of the run.

* **Generators** (`function*`, `yield`) and **async/await**. Both need a
  suspendable execution context; this interpreter is a tree walker over the
  Firn call stack and cannot give one up in the middle. The parser reports
  them as "deliberately not supported" (`ERR_UNSUPPORTED`) instead of
  failing somewhere inside the body, so the report can tell the two apart.
* **`eval`** and the `Function` constructor. Direct `eval` needs the whole
  parser at run time plus a variable environment of its own; both drivers
  would have to become one program. `eval` is therefore not defined at all,
  which is why the `test/language/eval-code/` directory is not part of the
  subset either.
* **Regular expressions as an ENGINE.** The literal is a token, its flags
  are checked, `??`/`/` are distinguished exactly -- but there is no pattern
  matcher. `RegExp` does not exist as an object.
* **Proxy, Reflect, Promise, Date, typed arrays, `BigInt`, `WeakMap`,
  `WeakSet`, `Symbol.for`, tail calls, `with` inside strict code,
  `Intl`.**
* **Module LINKING.** `import`/`export` produce ESTree nodes and the early
  errors of a module are checked, but there is no loader and no linking.
* **Class private methods and static initialization blocks.** `#x` as a
  field and `#x in obj` are parsed; a private METHOD is reported as
  unsupported.
* **The full Unicode case tables.** `toUpperCase`/`toLowerCase` handle ASCII
  and Latin-1; a complete `SpecialCasing.txt` is a round of its own.
* **`Map`/`Set` search linearly** instead of hashing. A hash table over
  `SameValueZero` needs the hash of an OBJECT, and this engine has no object
  identity number yet.

## 4. The language gaps found

The point of the exercise: what does Firn itself lack, seen from a program
of this size?

**Gap 1 -- there is no unary minus for `f64`.**
`-x` on a floating point value is a type error ("unary '-' expects an
integer type, found f64"), on a literal (`-3.0`) as well as on a variable.
SPEC 14.1.f64 names "the sign `-x`" as implemented; it is not. The
workaround in the engine is exact and even better for JavaScript: the sign
bit is flipped over the bit pattern
(`math_f64(math_bits(x) ^ 0x8000000000000000)`), which produces `-0`
correctly -- `0.0 - x` does not, because `0.0 - 0.0` is `+0`. The place:
`interp.eval_unary`, case `U_MINUS`.

**Gap 2 -- an error union over `f64` returns a WRONG value.**
`E!f64` compiles, but the value that comes out the other side is not the one
that went in. `E!u64` with the same shape is correct, and so is a plain
`f64` return -- so it is the error union over a floating point success type,
not floating point itself. Proof: `tests/1004_js_f64_union.fi`.

The suspicion, written down so that whoever fixes it has a starting point:
an error union is the struct `{ __err: u32, __val: T }`
(SPEC 14.1.error_unions). For `T = f64` the System V classification of that
struct puts the second eightbyte in the SSE class, while stage 0 passes
`f64` as a BIT PATTERN in the INTEGER registers (SPEC 14.1.f64, F2). The two
rules disagree exactly here. This bug is dangerous because it is SILENT.

The consequence for the engine: every conversion that can fail hands back
the BIT PATTERN as a `u64` (`interp.to_number`, `to_integer`,
`string_to_number`), exactly as `lib/num/strtod.fi` does.

**Gap 3 -- a binary operator may not begin a line.**
A condition that is broken across lines has to keep the `&&`/`||` at the END
of the line; a line that starts with `&&` is a syntax error. That follows
from "semicolons are optional" plus the missing continuation rule, and it
costs readability in exactly the places where a condition is long. It cost
this round four passes of a reformatting script over every file.

**Gap 4 -- `gc class` has no methods, and no generic form.**
`Gc[JsObj]` cannot carry `obj.get(key)`; everything is a free function with
the receiver as the first parameter. With an object model of nine classes
and about 120 operations that is a lot of noise. `impl` exists for `struct`
(round 45/46) but not for `gc class`.

**Gap 5 -- there are no global variables, not even immutable ones.**
The realm has to be threaded through EVERY function -- 1,400 parameters of
type `Gc[Realm]` in this round. `const` is restricted to scalars, so even a
constant table has to be built at run time by a generated function
(`lib/js/unicode_id.fi` pushes 2,802 range bounds into a buffer at start
up).

**Gap 6 -- a function value in a struct field cannot be called directly.**
`c.hook(a, b)` reports "type 'Ctx' has no method 'hook'"; it has to be
loaded into a local first (`let h = c.hook; h(a, b)`). That is consistent
with round 58 but surprising.

**Gap 7 -- `Gc[T]` does not upcast implicitly at `return`.**
`return derived` in a function that returns `AllocError!Gc[Base]` is
rejected although the upcast is free (SPEC 4.4). A local of the base type in
between fixes it. Roughly 200 places in this round.

**Gap 8 -- the two lexers read a decimal literal one ULP apart.**
`9007199254740991.0` (2^53-1) is lexed by `firnc0` (Rust) and by `firnc1`
(the lexer written in Firn) as two DIFFERENT doubles: the bit patterns
`4845873199050653695` and `4845873199050653694`, that is 9007199254740991
against 9007199254740990. Found by `tools/lex_compare.sh`, which exists for
exactly this purpose and struck the moment the literal appeared in
`lib/js/builtin.fi`. Interesting detail: the same literal in
`lib/js/interp.fi` does NOT diverge, so the deviation depends on the path
through the number reader ("floating point outside the fast path" in the
report of that tool), not on the digits alone.

The engine writes 2^53-1 as an INTEGER and converts (`n as f64`, exact up
to 2^53), so the value is the same in both compilers. The bug itself
remains and is worth a round of its own -- a lexer that reads a literal
differently from the one that bootstrapped it breaks the fixpoint promise
for programs that use such a constant.

**A pleasant surprise, for the record:** `dtoa` in `lib/num/` already
follows the ECMAScript rules for `Number::toString` EXACTLY -- the 21 digit
window, the exponential form and the treatment of `-0`. `Number(0.1+0.2)`
printed itself correctly on the first attempt. That is the payoff of round 2
having done the number output honestly.

**And one honest limit of the engine that came out of the same corner:**
the dense element storage of an array is materialized only up to
`DENSE_LIMIT` (4,194,304 slots). `a.length = 4294967295` is remembered but
not laid out, and an assignment to an index above the limit is DROPPED
instead of stored. Before that limit existed, `a.length = 2**32-1` laid out
four billion slots and the process died -- which is why it is in here; the
sparse fallback into the property table is the next step.

## 5. The acceptance

The numbers are in `tools/js/RESULTS.md`, produced by `bash tools/js/run.sh`.
