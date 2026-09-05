# Round 66 -- generators, async/await, BigInt, private class elements

Everything in this round is written in Firn, character by character, without
a foreign library. Foreign code appears exclusively as a CHECKING INSTANCE:
the official suite `test262` of TC39. Nothing of it is in the production
path.

Touched are `lib/js/` and `tools/js/` only. **The compiler was not touched**
-- neither `compiler/src` nor `lib/firnc1`. What Firn itself lacks, seen
from a program of this size, is written down in section 6 instead of being
patched into the compiler; that is what made round 63 mergeable without a
conflict and it is what this round does again.

New: `lib/js/gen.fi` (the resumable evaluator, the promises, the async
functions), `lib/js/bigint.fi` (arbitrary precision integers),
`tools/js/round66.sh`, `tests/1130_js_generators.fi` to
`tests/1135_js_gen_gc.fi`.

## 1. The decision: an explicit frame stack in the GC heap

`lib/js/interp.fi` is a TREE WALKER over the Firn call stack. A generator
has to give its execution context up in the middle of an expression and
take it back later; a Firn stack frame cannot be given up. Three ways were
open:

* **A thread per generator.** Firn has threads since round 49, and the
  semantics would be exact: the body runs on its own stack, `next()` wakes
  it, `yield` puts it to sleep. It is out for one reason: a generator that
  is ABANDONED -- started and never finished -- can never be woken again,
  so its thread and its stack would stay forever. test262 abandons
  generators by the thousand. A construction whose normal case is a leak is
  not a construction.
* **A machine stack of its own per generator, switched with inline
  assembly.** That is how real coroutines are built, and Firn can do it
  (round 52 has inline assembly). It is out because of the COLLECTOR: it
  scans the stack CONSERVATIVELY (SPEC 3.5.3) and knows exactly one stack.
  A suspended generator stack would be invisible to it -- its objects would
  be collected while the generator still holds them. Making the collector
  aware of several stacks means changing `lib/gc`, and this round does not
  touch anything outside `lib/js/`.
* **An explicit stack of frames IN THE GC HEAP.** Chosen.

A frame (`val.GenFrame`) is one node of the syntax tree plus the state
inside that node: the environment, up to three saved values, a list, an
object, and four numbers. The frames form a linked stack; `val.GenState`
holds its top plus the status of the body. All of it is `gc class`, so:

* an abandoned generator is collected like every other object -- no second
  stack, no external root, no pinning, no finalizer,
* a suspended generator costs a handful of small objects instead of a
  megabyte of stack,
* the collector traces the frames through the type table the compiler
  generates. There is no special path anywhere.

`tests/1135_js_gen_gc.fi` and `tools/js/soak.sh` measure exactly that, with
a counter-check that must strike.

### What keeps the file from being three times as long

Only a subtree that REALLY CONTAINS a suspension point is carried by these
frames. `susp_has` marks every node that contains a `yield` or an `await`
(and a `for await`, which awaits in every round even when nothing inside it
does); the answer is cached in a bit table beside the syntax tree, outside
the GC heap. Everything else -- the overwhelming majority of every
generator body -- is handed to the ordinary `interp.eval_node` and runs on
the machine stack at full speed.

The analysis does NOT descend into a nested function: a `yield` in there
belongs to that function. It DOES descend into the parameter list of an
arrow function, because `function* g(){ const f = (a = yield) => a }` is
legal and that `yield` belongs to the generator.

### How the completion records fit

The machine uses the SAME completion record the interpreter already has:
`ctx.comp` (normal / throw / return / break / continue) and `realm.pending`
for the value. That is what makes the two halves fit together: a normal
call out of a generator body (`interp.call_function`) that throws lands in
the machine as an abrupt completion and unwinds the frame stack, and a
`try`/`finally` inside the generator sees it exactly as the tree walker
would. `AllocError!` stays the OTHER channel and means the heap is
exhausted.

Unwinding is: pop frames until one takes the completion. A `try` takes a
throw when it has a catch clause, and takes everything when it has a
finalizer; a loop takes `break`/`continue` with its own label; a `for ... of`
closes its iterator on the way out (7.4.9).

## 2. async/await and the promises

The same machine carries an async function: `await` is a suspension point
like `yield`. The difference is who wakes the body up.

* An async function creates its result promise, then runs the machine
  IMMEDIATELY -- an async function body runs synchronously up to its first
  `await`, and everything else would be wrong.
* `await v` goes through `PromiseResolve` and hangs two native
  continuations on the promise. Then the machine gives control back.
* When the promise settles, the reaction becomes a JOB. The job calls the
  continuation, the continuation resumes the machine, and the machine runs
  to the next `await` or to the end.

The JOB QUEUE (9.5) lies in the realm and is drained after the script and
after every job that adds new ones. `Promise`, `Promise.resolve/reject/
all/race/allSettled/any`, `then`/`catch`/`finally` and the two resolving
functions of the executor are all in `lib/js/gen.fi`, in the same GC heap
as everything else.

An ASYNC GENERATOR is both at once: a generator whose `next()` returns a
promise. Its requests lie in a queue on the state; a `yield` resolves the
front request, an `await` suspends the whole thing until the promise
settles. `for await (... of ...)` works over a real async iterator and over
a synchronous iterable, whose values are awaited one by one.

### One native has to know which object it is

A resolve function, a reaction and the continuation of an `await` carry
state -- they are not one function but thousands. The native dispatcher of
round 63 gets `this` and the arguments, but not the function object. So
`Ctx` now carries `cur_fn`: `call_internal` puts the native function object
in there before it calls the handler, and the handler reads it as its first
act. That is one word of state in a struct that already lives on the stack,
and it removes the need for closures in the built ins entirely.

## 3. BigInt

`lib/js/bigint.fi`, from scratch. The magnitude lies as limbs of 32 bits in
a RAW vector of the GC heap (no pointers inside, so the collector walks it
without looking at the contents), the sign is a separate field, zero has no
limbs -- so there is no negative zero, exactly as the specification wants.

32 bit limbs and not 64: the product of two limbs plus two carries has to
fit into one machine word. With 32 bits it fits into a `u64`. No 128 bit
arithmetic, no assembly, no trick.

Addition, subtraction and multiplication are the schoolbook algorithms with
a carry; division is a binary long division (shift, compare, subtract, one
bit of quotient per step); exponentiation is square-and-multiply. The
bitwise operations go over the TWO'S COMPLEMENT of a width one limb wider
than the wider operand, which makes the sign extension correct without a
special case. Text in and out is repeated multiplication resp. division by
the radix.

The type errors the specification demands are there: mixing a BigInt with a
Number in an arithmetic operation is a `TypeError`, `>>>` on BigInts is a
`TypeError`, `BigInt(1.5)` is a `RangeError`, `BigInt("x")` is a
`SyntaxError`, `1n / 0n` is a `RangeError`. The comparisons `<`, `==` and
`===` against a Number are EXACT: the double is turned into a BigInt, so no
rounding decides the outcome.

## 4. The private class elements, the fields and the static blocks

The parser resolves the private names the way the specification does, in
ONE pass: a use may stand before its declaration, so every use is
remembered and held against the declarations of the class when the class
body closes; what stays open belongs to a class further out, and what is
still open when the outermost class closes is the early error of 13.3.2.1.
Duplicate private names, `#constructor`, a private name outside a class
body -- all early errors.

At run time a private name is a key of ITS OWN NAMESPACE: a symbol key. A
private field therefore never appears in `Object.keys`, in a `for ... in`,
in `Object.getOwnPropertyNames` or in `JSON.stringify` -- which is what the
specification asks for, and it comes out of the object model of round 63
for free. `#x in obj` is the brand check.

Instance fields run where the specification says: in a base class before
the constructor body, in a derived class directly after `super()` returns.
A static block runs in source order together with the static fields, with
`this` bound to the constructor.

## 5. The measurements

See `tools/js/RESULTS.md`; the numbers there are produced by
`bash tools/js/run.sh --full` and are not written down by hand anywhere.

## 6. The language gaps found

The point of the exercise: what does Firn itself lack, seen from a program
of this size? The gaps of round 63 are unchanged (`docs/ROUND63.md` 4); the
new ones are these.

**Gap 9 -- a raw pointer into a LOCAL survives its frame, silently.**

```firn
fn ascii_rejected() -> *mut u8 {
    var s: [u8; 8] = "rejected"
    return &s[0]          // compiles, and dangles
}
```

The compiler says nothing. This cost half an hour in this round; the
workaround is to declare the array at the use site, which is why the same
short text stands three times in `lib/js/gen.fi`. A raw pointer into a
local frame is exactly the case where a lifetime rule pays off, and stage 0
has none. SPEC 3.6 puts raw pointers into `unsafe`, but there is no
`unsafe` here and no warning either.

**Gap 10 -- a text literal has to know its own length.**

`var m: [u8; 40] = "yield in this position is not supported"` is an error
if the count is off by one, and every error message in this file costs a
manual count. There are about 200 of them in this round. A literal of type
`&str` (or `[u8; _]` with the length inferred) would remove all of it. It
also produces a class of bug that a reader cannot see: the text is padded
with blanks until the count fits, and the LENGTH passed on afterwards is a
second, independent number.

**Gap 11 -- no dispatch over a number.**

The machine decides over the kind of a syntax node: 25 comparisons in an
`if`/`else` chain, walked from the top for every step of every generator.
A `switch` over a dense integer (or the pattern matching of SPEC 6.3, which
stage 0 does not have) would be one jump. This is the hottest loop of the
whole engine.

**Gap 12 -- a function value in a struct field still cannot be called
directly.**

`(*c).gen_start(r, f, env)` does not compile; it has to be loaded into a
local first. Round 63 named this as gap 6; the round-66 hooks hit it three
more times, so it is worth naming again with a number.

**A pleasant surprise, for the record:** `gc class` inheritance works over
THREE levels (`JsPromise extends JsObj extends JsVal`), and the checked
downcast `v.as?[JsPromise]` gets it right in both directions. That is what
made it possible to hang the state of a generator and of a promise onto the
object without making the ORDINARY object -- of which a program makes
millions -- eight octets bigger.

## 7. What is deliberately NOT carried, and what is honestly wrong

Named here, not concealed. Every one of these counts as a FAILURE in the
test262 quota; nothing is filtered out of the run.

* **A suspension point in six rare positions.** `delete (yield x)`, an
  assignment TARGET that contains a yield, a `super(yield x)` call, the
  `extends` clause and the computed keys of a class, a tagged template, and
  an object literal that contains BOTH a method and a suspension point.
  The frame machine does not carry these; it throws a `TypeError` that
  says so instead of delivering a wrong value. An honest failure, not a
  wrong result.
* **A private name lives in the REALM, not in the class evaluation.** Two
  classes with the same `#x` share the key, so the `#x` of one class can
  reach the field of the other. Correct would be one key per class
  evaluation, which needs a private environment in the scope chain.
* **No species, no subclassing of `Promise`.** `class P extends Promise`
  produces promises whose `then` builds a plain `Promise`. There is no
  unhandled-rejection tracking either.
* **`Number(bigint)` is not correctly rounded in every case.** The value is
  accumulated limb by limb; for magnitudes far beyond 2^53 the last bit can
  be off. The COMPARISONS do not go through that path -- they are exact.
* **`built-ins/Promise` and `built-ins/BigInt` are not part of the pinned
  test262 subset** of round 63 (`testdata/test262/MANIFEST.md`), because
  neither existed then. The subset is pinned by the sha256 of every single
  file; extending it would break that pinning, so it stays as it is. The
  new built ins are therefore measured through the `language/*` directories
  that use them (async generators, `for await`, `await`) and through
  `tests/1131_js_async.fi` and `tests/1132_js_bigint.fi`.
* **A found bug in the test of round 63.** `tests/1002_js_interp.fi` read
  its result variable `fail` off the GLOBAL OBJECT, but a top level `var`
  in this engine lives in the declarative record of the global
  environment, not as a property of the global object. The test could
  therefore never fail -- all 71 assertions were dead. It now writes
  `globalThis.fail`, and the 71 assertions really pass. The tests of this
  round were written the same way from the start and were checked with a
  deliberately broken assertion.

## 8. The acceptance

Section 5 has the test262 quotas. Beyond them:

* `bash test.sh` -- the whole suite, with the six new programs
  `tests/1130` to `tests/1135` and the new section 27.
* `bash tools/self_compare.sh` -- 0 differing, 0 faulty.
* `bash tools/fixpoint.sh` -- the compiler compiled by itself is
  character-identical.
* `bash tools/english/check.sh` -- 0 0 0 0 0.
* `firnfmt -w` over every new and changed source; `firnfmt -c` finds
  nothing left to do.
* `bash tools/js/soak.sh` -- the endurance run with the counter-check.
