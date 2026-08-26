# Round B4 — the page comes alive: scripts, invalidation, HTTP

Rounds B1, B2 and B3 built a pipeline that runs **once**: markup in,
picture out. What came out of B3 was a rendering library, and it said so.
Two things were missing, and between them they are what makes a page a
page:

* a script could compute, and it could not touch the tree;
* nothing could fetch a document out of the network.

This round is both, and — as in B1, B2 and B3 — the step matters less than
the **measuring stick** that comes with it.

---

## 1. The numbers

### 1.1 The official DOM tests

The corpus is the `dom/nodes`, `dom/events`, `dom/traversal`, `dom/lists`,
`dom/ranges` and `dom/abort` areas of the **Web Platform Tests**: every
`*.html` directly in them that includes `/resources/testharness.js` —
**313 files**. What is in the corpus and by which rule is in
`tests/data/wpt-dom/PROVENANCE.md`; nothing was picked by hand and nothing
was left out because it looked hard.

They are run through the **original `testharness.js`**, 5207 lines of it,
unmodified, inside this engine. Only `testharnessreport.js` — which in a
browser talks to the test runner — is replaced, by four lines that call
`add_completion_callback` and print. That is the same interface
`wptrunner` uses.

| | files / subtests | quota |
|---|---|---|
| subtests the harness reported **PASS** | **390 / 1714** | **22.75 %** |
| subtests the harness reported FAIL | 1324 | |
| files in which **every** subtest passed | **16 / 313** | |
| files whose harness **never finished** | 169 | |

**The last row is the honest part of this table.** A DOM test that reports
nothing passes nothing, and a page whose harness dies before its first
assertion looks exactly like a page with no failures. `tools/liveb4/wpt.py`
therefore counts a file only if its harness really reached the completion
callback **and** produced at least one subtest; everything else is
reported as *could not run*, separately, and never as a pass. That is the
same rule as round B3's 32 empty reference pictures, and it costs the same
kind of flattering number.

Per area:

| area | passed | failed | quota |
|---|---|---|---|
| `dom/lists` | 141 | 48 | 74.60 % |
| `dom/nodes` | 234 | 1086 | 17.73 % |
| `dom/events` | 14 | 153 | 8.38 % |
| `dom/traversal` | 1 | 26 | 3.70 % |
| `dom/ranges` | 0 | 11 | 0.00 % |

### 1.2 What the narrowing buys, and what it costs

Same document, same 30 mutations, once with the narrowing on and once with
it off — `live_set_scoped` is the only difference:

| | narrowed | full | factor |
|---|---|---|---|
| elements whose style was computed again | **107** | 11070 | **103.5 x** |
| nodes the walk looked at | 1795 | 18420 | 10.3 x |
| boxes laid out | **3380** | 18390 | **5.4 x** |
| microseconds | 31302 | 291661 | 9.3 x |
| **boxes whose geometry differs from a full layout** | **0** | 0 | |

The document has 369 elements and 613 boxes; it contains 40 rows, each in
a box with a definite width and height, and the mutations set an inline
style or a class on elements inside them.

**The last row is the reason the first four may be believed.** After
*every single* mutation the same document is laid out from scratch and
every box is compared — x, y, w and h, as bit patterns, not as
"approximately". A narrowing that is fast and wrong is worth less than no
narrowing at all. See 4.3 for the two bugs that row found.

### 1.3 The HTTP client, against a server that is not this repository

`tools/liveb4/server.py` is Python's own `http.server`. It is started by
the acceptance run, driven over a real loopback socket and killed again.

| | |
|---|---|
| rules checked (framing, redirects, cookies, cache, TLS refusal) | **28 / 28** |
| of them counter-checks (the answer must be a refusal or a failure) | 6 |
| five fetches of one URL opened | **1 socket** |
| date formats against `email.utils.parsedate_to_datetime` | 9 / 9 |
| `Set-Cookie` rules of RFC 6265 (4 of them counter-checks) | 10 / 10 |
| `Cookie` header rules of RFC 6265 (2 of them counter-checks) | 5 / 5 |
| URL references against `urllib.parse.urljoin` (RFC 3986 5.4) | 48 / 48 |

The URL cases are the 33 reference examples of RFC 3986, section 5.4,
including the abnormal ones, plus fifteen of our own. Three of the 48
agree with a **browser** rather than with `urljoin`, and they are listed
by name in `tools/liveb4/url_check.py` rather than quietly excluded: an
empty path becomes `/`, and leading and trailing spaces and C0 controls
are stripped (WHATWG URL 4.4). `<a href=" x ">` is a link to `x` in every
browser there is.

### 1.4 The scripts, over a real socket

Seven scripts in one page — three parser-blocking (one of them inline),
two `async`, two `defer` — with the six external ones fetched over HTTP
from the same test server:

```
b1  inline  b2  a1  a2  d1  d2
```

and the counter-check: with the fetcher switched off, **all six external
scripts fail and only the inline one runs**. Without that number, "the
scripts ran in the right order" would also be true of an implementation
that ran none of them.

### 1.5 What did not get worse

| | before this round | after |
|---|---|---|
| html5lib tree construction (round B1) | 1837 / 1936 | **1837 / 1936** |
| WPT layout, corpus B2 | 59 / 186 | **59 / 186**, reflow 471 / 471 |
| WPT reference pictures, corpus B3 | 202 / 541 | **202 / 541**, 393 / 393 glyphs |
| test262 (rounds 63/66/74), parser | limit 3000 | **3049 / 3493** |
| test262, the engine really running the tests | limit 2250 | **2603 / 3493** |

The engine gained one field on `Ctx` (`host`) and one on `Node`
(`dirty`); nothing else in rounds B1 to B3 was touched, and the
`no-script-no-change` case in `tools/liveb4/cases.py` checks exactly that:
a document without a script has to come out of the new pipeline byte for
byte as it went in.

**And the dirty mark costs nothing that can be measured.** `Node` grows
from 48 octets to 52, and every class that extends it stays in the *same*
size class of the collector (32/48/64/96/128, `lib/gc/gc.fi`):

```
Elem      84 -> 88 octets, size class 96
Text      56 -> 60 octets, size class 64
Document  52 -> 56 octets, size class 64
Comment   56 -> 60 octets, size class 64
```

Bare `Node` is never allocated — the parser only ever makes the
subclasses.

---

## 2. What was built

### 2.1 The DOM bound into the engine — and how it is reached

`lib/js/builtin.fi` hands every native number from 30000 on to a function
**value** the driver installs (`Ctx.gen_native`). Round 66 put the
generators there. This round installs `domjs.dom_native` instead and
forwards everything below 40000 back to `gen.gen_native`:

```
builtin.native_dispatch  ->  domjs.dom_native  ->  gen.gen_native
```

So the chain grew a link and **neither of the two older files had to learn
about this one**. That is the same trick round 58 made possible and round
66 used, for the same reason: `lib/js/interp.fi` must not import a DOM, or
the engine could no longer be built without one. `lib/js/run_main.fi` still
runs test262 with no DOM in the binary at all.

The one change to the engine is a single field on `Ctx`:

```firn
host: *mut u8,      // the state of the host bindings, 0 when there is none
```

exactly as `gen_data` already was for round 66.

### 2.2 The wrapper, and why identity is not optional

A DOM node is a `Gc[Node]`; JavaScript wants an object with properties.
`DomObj` is a real subclass of `JsObj` with the node beside it, so the
collector traces the node through the compiler-generated type table and a
node that only a script still holds stays alive. `Host.wrappers` maps the
node to its wrapper, because

```js
document.getElementById("x") === document.getElementById("x")
```

has to be **true**. It is not a nicety: an event listener compares
`e.target` against an element it captured earlier, and with a fresh
wrapper per call every such comparison is false. `el.style === el.style`
and `el.classList === el.classList` hang on the same table, under a key
that cannot collide with a node (the node address plus 1 resp. 2).

### 2.3 The prototype chain is the DOM's, not a convenience

`instanceof` is a question about the prototype chain plus a global name.
An engine that puts every method on one object answers
`el instanceof HTMLElement` with `false` and fails a whole class of tests
for a reason that has nothing to do with the method. So the chain is the
real one:

```
EventTarget <- Node <- Element <- HTMLElement
                    <- CharacterData <- Text, Comment
                    <- Document
```

with `addEventListener` on `EventTarget`, the node type constants
(`Node.TEXT_NODE` and friends) on `Node` **and** on the interface object,
and an interface object per prototype that refuses to be called
(`Illegal constructor`, WebIDL 3.7.1) — except `Event`, which a test has
to be able to construct.

### 2.4 The window IS the global object

Not "a window object beside it". In a browser `window === globalThis`, and
`self.foo = bar` inside a library has to make `foo` a global variable.
`testharness.js` exposes its whole API that way; with a separate window
object **not one test in the suite can even start**, and that is how the
mistake was found. `parent`, `top` and `frames` point at the window itself
and `opener` is `null` — a top-level browsing context, which is the only
kind this round has. The harness walks `while (w != w.parent) w = w.parent`
to find the top window, and without those four names that loop
dereferences `undefined` on its second turn.

### 2.5 The event flow

`lib/browser/domjs.fi` implements DOM UI Events 3.1 in full for the part
that has no input layer under it: the path from the root to the target is
built **once**, before the first listener runs (DOM 2.9 — a listener that
moves the target must not change where the event goes), then walked
forwards for the capture phase, at the target for both kinds in
registration order, and backwards for the bubble phase.
`preventDefault`, `stopPropagation`, `stopImmediatePropagation`,
`{once: true}`, `{capture: true}` and the de-duplication of DOM 2.7 (the
same function twice for the same type and phase is registered once) are
all there and all measured.

### 2.6 `<script>`, and the order that cannot be seen afterwards

HTML 8.1.3.1 gives three groups. A classic script without `defer` and
without `async` blocks the parser and runs where it stands; `async` runs
as soon as it has arrived; `defer` runs after the document is parsed, in
document order, before `DOMContentLoaded`. And — the part that is easy to
get quietly wrong — **`defer` and `async` mean nothing at all on an inline
script**.

This browser fetches synchronously, so "as soon as it has arrived"
collapses to "in the order they were started". It therefore runs:
parser-blocking in document order, then `async` in document order, then
`defer` in document order, then `DOMContentLoaded`, then `load`, then the
timers. That is a **model** of the standard, not the standard, and the
difference is written down here rather than discovered later. What it does
keep exactly, and what section 1.4 measures: `defer` after everything
else, `async` never before a parser-blocking script, and both after every
parser-blocking one.

### 2.7 The dirty mark

Four bits on every node (`lib/browser/node.fi`):

```
D_SELF          this element's computed style is stale
D_TEXT          its text or its child list changed
D_LAYOUT        its box has to be laid out again
D_CHILD         somewhere BELOW here something is stale
D_CHILD_LAYOUT  ... and it needs layout
```

The first three are set **on** the node that changed. The last two are
carried **up** to the root and stop at the first ancestor that already
carries them — that is what makes a thousand `appendChild` calls under one
node cost one walk instead of a thousand. The walk downwards turns round
at every branch that carries neither bit.

**The two widenings, and they are the honest part.** A computed style does
not depend on its element alone:

* **inheritance.** Restyling an element means restyling its whole subtree.
  There is no cheaper rule that is also correct: `color` on a `div`
  reaches every descendant that did not set its own.
* **sibling and positional selectors.** `.a + .b` and `li:nth-child(2)`
  make an element's style depend on its neighbours. So a change to an
  element widens to its **following siblings**, and a change to a child
  *list* widens to all children — but **only if the stylesheet in force
  really contains such a selector**. `live_scan_rules` looks once and
  `has_sibling`/`has_positional` decide. A document whose sheet has
  neither pays nothing for the possibility.

### 2.8 The layout wall

A box whose outer size cannot change from within is a wall: everything
outside it keeps its geometry. This round takes the narrow, provable
version — a definite `width` **and** a definite `height`, both in px, in
flow, not floated, not a flex item, and a block formatting context of its
own. From the changed element the walk goes **up** to the nearest such
**proper ancestor** and lays out only that subtree; if there is none, the
whole document is laid out.

`place_tree` adds an offset to the relative coordinates a layout produced,
so the offset that restores the old absolute position can be computed from
the numbers themselves — the box tree has no parent pointer (round 61 left
it out on purpose) and none is needed.

### 2.9 The HTTP client

`lib/net/http.fi`, `lib/net/httpstate.fi`, `lib/net/url.fi`. GET and POST,
the header block read case-insensitively with obsolete line folding
**refused** (RFC 9112 5.2 — a folded `Set-Cookie` is a known request
smuggling lever), `Transfer-Encoding: chunked` with chunk extensions and
the trailer section, `Content-Encoding: gzip` and `deflate` through
`lib/std/deflate.fi`, redirects 301/302/303/307/308 with the method rules
of RFC 9110 15.4, a hop limit and loop detection, `Content-Type` split
into media type and charset, persistent connections with one retry when
the server closed an idle socket, a response cache with `max-age`,
`no-store`, `no-cache` and revalidation through
`If-None-Match`/`If-Modified-Since`, and a cookie jar (RFC 6265) with
domain and path matching, `Secure`, `Max-Age`, `Expires` and the
creation-time rule of 5.3 step 11.

The three date formats of RFC 9110 5.6.7 are all accepted, with
`days_from_civil` — Howard Hinnant's shift of the year start to March,
which makes the leap day the last day of the year and turns the leap rule
into three divisions with no case distinction.

---

## 3. TLS, and what leaving it out means

**`https://` is refused**, with `HttpError::Tls`. It is not silently
downgraded to `http://`, not silently failed, and not faked with a
plaintext connection to port 443. The boundary is one function,
`scheme_ok`, and the acceptance run drives a real `https://` URL into it
**twice** — typed in, and reached through a `302 Location:
https://...` — and demands the refusal both times.

What it would have taken, so that nobody thinks it was almost there: TLS
1.3 is a record layer, X25519, AES-GCM or ChaCha20-Poly1305, HKDF,
SHA-256, ASN.1/DER, certificate chain building and a trust store. It is a
round of its own, and anything less than all of it is a lie in the address
bar.

What it costs, concretely: **the client cannot fetch the public web.**
Almost every site worth fetching is HTTPS-only, and the ones that answer
on port 80 answer with a redirect to `https://`. So the measurement in
section 1.3 is against a server on the loopback, and it is a *real*
server — Python's own `http.server`, which shares no line of code with
this client — rather than a mock. That is the honest ceiling of this
round, and it is drawn on purpose in the one place where it can be tested.

There is a second, smaller boundary of the same kind: **there is no name
resolver.** `lib/std/net.fi` takes a numeric address (its own limit N3),
so `addr_of_host` accepts a dotted quad and `localhost` and refuses
everything else with `HttpError::Resolve`. A resolver means
`/etc/resolv.conf`, UDP and a DNS message parser; without TLS it would buy
nothing anyway, since the names one would want to resolve all lead to
`https://`.

---

## 4. How it was checked, and why not the obvious way

### 4.1 The harness had to be somebody else's

The obvious way to measure a DOM is to write a few dozen assertions and
count them. That measures the author's idea of the DOM. So this round runs
the **official `testharness.js`**, unmodified — 5207 lines that use
generators, promises, `Object.defineProperty`, getters, `WeakMap`, regular
expressions and closures over closures, and that fail loudly on anything
the engine gets wrong. Getting it to *start* took four fixes, and every
one of them was a real defect:

1. `addEventListener` with no receiver (`this` = the global object) threw
   instead of registering on the window;
2. the window was a separate object, so `self.foo = bar` did not make a
   global — and the harness exposes its entire API that way;
3. `window.parent` did not exist, so `while (w != w.parent) w = w.parent`
   dereferenced `undefined` on its second turn;
4. `document.createElementNS` and `insertAdjacentText` were missing, and
   the harness writes its log with them.

None of those four would have been found by a hand-written test suite,
because a hand-written suite tests what its author remembered.

### 4.2 A better error message, because of it

`Cannot read a property of undefined` is true and useless. Finding fix 3
above meant bisecting 5207 lines of somebody else's JavaScript by hand. So
`lib/js/interp.fi` now names the property:

```
TypeError: Cannot read a property 'parent' of undefined or null
```

In a page with five thousand lines of somebody else's code that is the
difference between a minute and an afternoon.

### 4.3 The counter-check found two real bugs

`live_verify` lays the same document out from scratch after every mutation
and compares every box. It reported **476 wrong boxes** on the first run,
and the two causes were both invisible in the source:

1. **the boxes still pointed at the old `Style` object.** A restyle makes
   a *new* `Style` and puts it in the table; the full path never notices,
   because it builds new boxes anyway. The scoped path reuses them and
   laid out the old style — a margin that had just been taken away was
   still there, in exactly 14 boxes.
2. **the intrinsic cache survived a style change.** `int_min`/`int_max`
   survive a reflow on purpose (round B2: the width of the window cannot
   change what a box contributes). A *style* change can: one line of
   `padding` moves the min-content width of every ancestor up to the wall.

Both are the kind of bug that ships. Neither would have been found by
looking at a picture, because both give a picture that is plausible.

### 4.4 Every claim has a counter-check

| claim | the counter-check |
|---|---|
| the narrowing saves work | the same run with `live_set_scoped(false)`: the numbers have to become the whole document |
| the narrowing is correct | a full layout after **every** mutation, box for box |
| the cache serves from memory | with the cache off the second fetch must reach the server |
| `no-store` is honoured | two fetches, both must reach the server, with the cache **on** |
| keep-alive works | five fetches of one URL must open at most one further socket |
| chunked is parsed | a body whose last chunk never comes must **not** be a successful fetch |
| TLS is out of scope | a real `https://` URL must come back `Tls`, twice |
| the scripts ran in order | with the fetcher off, six of the seven must fail |
| a `Domain` cookie cannot be forged | `Domain=ex.com` from `other.com` must be refused |
| `Secure` means something | the cookie must not go out over `http://` |
| B1..B3 are untouched | a document with no script must come out byte for byte |

---

## 5. Where the host is touched — the road to an `.so`

The browser core is meant to be liftable out as a library, cross-compiled
to `aarch64-linux-android` and dropped into an APK. That only works if the
places that touch the operating system are few and **named**. They are:

| file | what it uses | why |
|---|---|---|
| `lib/html/mem.fi` | `mmap`/`munmap`, `read(0)`, `write(1)` | the allocator under everything |
| `lib/std/net.fi` | `socket`, `connect`, `sendto`, `recvfrom`, `setsockopt`, `close` | the only socket in the round |
| `lib/net/httpstate.fi` | `clock_gettime(CLOCK_REALTIME)`, once, in `now_unix` | cookie and cache lifetimes are calendar time |
| `lib/browser/live.fi` | `clock_gettime(CLOCK_MONOTONIC)`, only for the report | can be compiled out |
| `lib/paint/b3_main.fi` | the same, for its report | round B3 |
| the root files (`*_main.fi`) | standard input and output | the job protocol, not the engine |

**Everything else is arithmetic.** `lib/browser/`, `lib/dom/`, `lib/css/`,
`lib/layout/`, `lib/paint/`, `lib/font/` and `lib/js/` contain no system
call at all. `lib/net/url.fi` allocates nothing and imports only
`std.rt`.

Three deliberate shapes serve the same end and are worth naming:

* **no global variable anywhere.** `Live`, `Host`, `Client`, `Jar` and
  `Cache` are structs the caller owns, exactly like `net.Stack` in round
  K3. An embedder can run two documents in one process and nothing is
  shared between them by accident.
* **the state that holds managed pointers lives in a frame.** The
  collector scans the stack conservatively (SPEC 3.5.3), so `Live` and
  `Host` are locals of the driver, not heap objects with an external root.
* **the host bindings are reached through a function value.** Swap
  `ctx.gen_native` and the same engine has a different DOM — or none.

---

## 6. What is open, and named

Openly, because a list of limits is worth more than a quota that hides
them. Of the files that could not run at all, the largest groups are:

| what is missing | files |
|---|---|
| syntax the parser refuses (optional chaining, class fields, `?.`, top-level `await`) | 91 |
| `document.implementation` — `createDocument`, `createHTMLDocument`, `hasFeature` | 5 |
| `Range`, `TreeWalker`, `NodeIterator` (all of `dom/ranges` and most of `dom/traversal`) | 48 |
| `MutationObserver` | 3 |
| `new Function` / `eval` | 7 |
| everything else: a second browsing context, `Attr` nodes, `AbortSignal`, and 29 files whose harness produced no result at all | 41 |

And of the things this round decided not to build:

* **no `DOMException`.** A wrong index throws a `RangeError` and a wrong
  node a `TypeError`, with the right *meaning* and the wrong *name*. Every
  WPT test that checks `e.name === "IndexSizeError"` fails on that alone,
  and there are many of them. It is a table of 30 names plus a class, and
  it belongs in the round that also brings `Range`.
* **no live `HTMLCollection`.** Every list this file hands out is a
  snapshot — which is what `querySelectorAll` returns anyway. A live
  collection has to hang on the tree and be invalidated on every change;
  the dirty mark of 2.7 is exactly the machinery it would use.
* **no `MutationObserver`, no shadow DOM, no custom elements.**
* **`style` is the inline declaration block, not the computed style.**
  `el.style.color` reads the `style` *attribute*; there is no
  `getComputedStyle`. That is the right shape for this round — the cascade
  parses the attribute anyway, so a second parsed representation would be
  a second truth to keep in step — and it is why `el.style.width` comes
  back empty for a width that came from a stylesheet.
* **no `document.write`.** Scripts run after the document is parsed, so a
  script that writes into the parser has nowhere to write. That is also
  why `document.getElementById` finds elements that stand *after* the
  script in the source; a real parser-blocking script would not see them.
  It is the one place where the model of 2.6 is visibly not the standard.
* **the layout wall is narrow.** Only a definite width *and* height count.
  A box with `height: auto` whose content grew by nothing measurable still
  forces a full layout. A real engine also walls at `overflow: hidden`
  with a fixed height, at a fixed-size flex item, and at a box whose
  intrinsic contribution provably did not change.
* **a structural change forces a full layout.** Rebuilding the box tree
  throws every position away, so `appendChild` is paid for at document
  scale even when the wall above it is small. The narrowing in 1.2 is
  therefore a *style* number in the structural case and a style *and*
  layout number otherwise.
* **HTTP: no TLS (section 3), no name resolver, no HTTP/2, no `fetch`,
  no `XMLHttpRequest`.** A script cannot make a request; only the document
  loader and `<script src>` can.
* **one thread.** Timers, microtasks and events run on the same thread as
  the layout, and the clock only moves when the driver moves it. That is
  what makes the whole round reproducible; it is not what a browser does.

---

## 7. Where the code lives

```
lib/browser/domjs.fi      the DOM as JavaScript sees it, and the events
lib/browser/live.fi       the dirty mark, the narrowed recomputation
lib/browser/b4_main.fi    the driver: parse, style, lay out, run, update
lib/browser/live_probe.fi the invalidation without a script engine
lib/net/url.fi            URLs and reference resolution, no allocator
lib/net/http.fi           the HTTP/1.1 client
lib/net/httpstate.fi      the cookie jar, the cache, the dates
lib/net/*_main.fi         the drivers for the three of them
tools/liveb4/             the acceptance run and the seven measurements
tests/data/wpt-dom/       the official corpus (PROVENANCE.md)
```

Changed elsewhere, and only this: one field on `browser.node.Node`
(`dirty`), one field on `js.interp.Ctx` (`host`), and the property name in
one error message in `js.interp`.
