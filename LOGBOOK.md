## Round 47 (2026-08-19) -- finalizers, Arc[T], weak references; branch r47-arc
The three memory-management items open since round 4 are done: S4 finalizers, Arc[T] with an
ATOMIC counter (new FIR primitive Op::AtomicAdd -> `lock xadd`, in BOTH compilers, FIR
octet-identical), S3 weak fields are now REALLY zeroed on collection (until now they only looked
empty, because the serial number no longer matched). On top of that, external root ranges, so that
a Gc[T] held in the value of an Arc is not collected.
DECISION on resurrection: there is none, and that is enforced -- Gc fields are zeroed before the
call, strong() returns 0, allocation/gc_collect/Gc writes abort visibly (71/72/73). The lock sits
in the S_INIT check that exists anyway: the common path costs nothing.
MEASURING GEAR REPAIRED FIRST: no program using `gc class` ran under valgrind (not even on the
baseline) -- __gc_stack_bottom read field 28 of /proc/self/stat, and under valgrind the client runs
on a different stack. It now consults /proc/self/maps first. That makes the collector measurable
with callgrind for the first time.
MEASUREMENTS: longest pause in CPU TIME median 460 us (baseline 469 us, 7 runs each), throughput
unchanged, 150 s run with 48.4 million finalizers RSS constant at 1372 KiB, 0 out of 253698 pauses
above 1 ms (CPU time). Instructions +4.6 % without weak fields, +12.1 % in the weak-heaviest case;
the first attempt was at +21.1 % -- four measured rollbacks in the sweep loop (docs/ROUND47.md 4.3).
Two blocks instead of one in __gc_alloc_raw cost 3.3 million instructions on their own, because
register allocation tipped over.
LESSON LEARNED: tests/520 and 535 retained 1 and 126 unreachable objects respectively after the
runtime grew -- they relied on the frame layout. Padding INSIDE the collector (3 KiB in gc_collect)
repaired one test and broke three others: the gap does not disappear, it moves. The right fix is to
place the pointer-holding frames deep (recursion plus padding), the way dom_observer_alive() has
done it since round 4.
Acceptance: test.sh 727/727, self_comparison 210/0/0, fixpoint character-identical (374454 lines).

## Round 35 (2026-08-16) -- comptime in firnc1, commit 5e16d8a
Parallelism experiment: separate git worktree (branch r35-comptime), merge fast-forward, zero conflicts.
Built: lib/firnc1/time.fi (689 lines, interpreter modelled on comptime.rs), the parser reads comptime
blocks, the driver bin/firnc1.fi runs them between the root parser and monomorphization, and the
generated text is parsed into the same tree as a module without an alias. Measurements: test.sh
634/634, self 166->169 (601 comptime_emit + 602 comptime_ucd + 760_core), 0 differing, fixpoint
210324 lines character-identical.
Limits stated honestly: comptime only in the root file; the constant case is separate.
Tooling fix: fixpunkt.sh rebuilds .firnc1 when the sources are newer (a stale binary would have
"confirmed" the merge with 166 instead of 169 -- only noticed because the worktree and the main
repository disagreed).
Lesson learned: worktree parallelism works for isolated blocks; a worker without a commit means the
work is lost (the round 34 worker hit its limit and committed nothing -- reassigned with a commit rule).

## Round 34 (2026-08-16) -- gc class / Gc[T] / #[no_gc] in firnc1, commits 6b406cf and following
The worker hit its limit without finishing, but left unfinished work in the tree (this time: 10 changed
files plus gc.fi 854 lines, gctext.fi 323 lines, nogc.fi 341 lines -- rescued and finished by hand).
FINDING on the corpus: the GC scan in the driver used intern_find -- numbers only exist if the ROOT
contains the words; when `gc class` appeared only in a module (560 -> modules/dom.fi) it scanned with
-1 and found nothing (a silent sema error). main.rs uses intern_number -- so this does too now. Tracked
down by bisection with mini modules plus instrumentation (exit codes 101+, then counter prints in err.fi).
Measurements: test.sh 637/637, self 169->180 (all 9 gc files + 770_core), 0 differing, fixpoint
279201 lines character-identical. 6 gc/nogc negative tests abort just like firnc0.
Remaining: constant time (4), errdefer (1), must_consume (1) -- round 36.

ROUND 36 (2026-08-16, commits 6ef2616 + 3144601 + 2e7d8a8): the last three core blocks.
#[must_consume] (modelled on attrs.rs, check_discard in sema.fi), errdefer (defer_until_error /
ret_term_error, union propagation rejected), ct intrinsics select and secure_zero (modelled on ct.rs;
core registration in the parser like barrier, cmov codegen, secure_zero not optimizable away).
Measurements: test.sh 640/640, self 180->185 identical / 0 differing / 0 faulty / NOT CORE 0,
fixpoint 284207 lines character-identical. The negative tests ct_select_*, ct_secure_zero_no_pointer,
errdefer_union_propagation, attr_must_consume_* abort just like firnc0 (rc=1).
Remaining, stated honestly: 600_comptime.fi (rc=4, COMPTIME 1) -- the core language is otherwise complete.

## Rounds 37+38+39 (2026-08-16) -- three parallel tracks, merged
R37 optimizer (main repository, commits 977a2ad/ef4e530/bf13ed4): html5lib 1.94x->1.69x (target <=2x
reached), realweb 4.82x->4.34x. firnc1 deliberately has no optimizer -- the fir comparison uses
--emit=fir-raw, there is nothing to mirror. Next lever: interval splitting and coalescing (7391
reg->reg movs).
R38 GC (worktree r38-gc, f498740/5ab519f/47ab946): empty chunks of every class returned to the OS
(with hysteresis), growth cap 4 MiB (frag2 final RSS 24124->2112 KiB), hybrid incremental from 8 MiB on
(pauses ~0.5 ms independent of heap size, throughput -8.7 % within budget). SIDE FINDING: the optimizer
removes the last zeroing as dead code -- let unreachability die in a helper function (scrubber).
Finalizers and Arc[T] named as remaining work. The 30 minute soak test was made up later (running).
R39 std and interpolation (worktree r39-std, ebc3c1e/e4fc9bd/ed0faf7): search path $FIRNLIB plus
<exe>/../lib in both compilers, the lib/std facade (io/math/str/vec/map/num/mem), f"..." broken up into
an Fmt chain at compile time (no varargs), 790/791 core tests, 3 negative tests. Verified live from /tmp.
TOOLING FIX (the same trap again): lex/parser/types/fir/sema_comparison built their dump binaries only
"if missing" -- stale dumps after R39 made the comparisons fail. They are now rebuilt when the sources
are newer (like fixpunkt.sh since R35). fixpunkt.sh exports FIRNLIB now as well.

## Round 79 (2026-08-22, branch r79-life) -- a raw pointer into a local can no longer leave its frame
Gap 9 of docs/ROUND66.md, found by round 66 while writing a JavaScript engine: `return &x` on a local
compiled and dangled, and the compiler said nothing. Now an error at COMPILE TIME in BOTH compilers,
with three places in the message (where the address is taken, where the local dies, where the pointer
gets out). New: compiler/src/escape.rs (1195 lines), lib/firnc1/escape.fi (1723), tools/escape/ (36
cases), section 40 of test.sh.
THE MODEL: sources and sinks over the syntax tree. `&local` carries the frame it points into; casts,
pointer arithmetic, aggregate literals and assignments to locals carry it on, a LOAD out of memory does
NOT (that one line is what keeps `(*v).ptr` quiet and `vec_push` honest). What a PARAMETER does lands
in a SUMMARY of the function, driven to a fixed point over the whole program and used at every call
site -- so the check crosses function boundaries WITHOUT a single annotation. Return-through and
keeping are told apart; without that distinction every `fn f(s: *mut S) -> *mut T { return &(*s).x }`
in this tree would have blamed its callers.
NO LIFETIME SYSTEM. Where it cannot decide, it allows; nine gaps named in docs/ROUND79.md 4.
`#[allow_escape]` is the way out, stays visible in the source and empties the summary too.
FOUND IN A GREEN TREE: lib/std/nbt.fi::nbt_type_name handed out the address of a `var` array of its own
frame FOURTEEN times, under a comment claiming the names lay "in read only data" -- a real dangling
pointer in a shipped library, fixed by a signature change (tests/1610). And Parser::join dropped the
file number of every joined span, so a statement of a MODULE pointed into the root file; unnoticed for
79 rounds because no message used those spans.
GAP 10 CLOSED: `var m: [u8; _] = "hello"` -- the parser fills the length in from the literal, in both
compilers, so nothing downstream ever sees a `_`. Gap 12 turned out to be closed already by round 68
(tests/1613 measures it). Gap 11 (dispatch over a number) is a code generator feature, named and open.
MEASUREMENTS: tools/escape/run.sh 36/36, messages identical 22/22 (whole block via cmp). test.sh
1183/1184. fixpoint stage2 == stage3 character-identical, 648723 lines. self_compare 318 same / 0
differing / 0 faulty. types_compare 336 / 0. english 0 0 0 0 0. firnfmt -c clean.
THE ONE FAILURE IS INHERITED: section 23 (layout) 1082/1087 against a LIVE Chromium -- round 78 froze
the reference and took the browser out of the acceptance, and this branch starts before round 78. Main
with the frozen reference: 1087/1087. Round 79 touches nothing in lib/layout.
SIDE FINDING, worth knowing: section 34 (the JS promise soak of round 66) is NOT deterministic --
`jobs rc=-11` in 2 of 4 runs on main and 2 of 3 here, same binary. A real bug in lib/js/gen.fi waiting
for a round of its own.

## Round B1 (2026-08-25, branch b1-dom) -- the clamp: tree, DOM, computed style
The browser parts were big and NOT connected: the tokeniser made tokens, the CSS cascade had no
elements to point at, and lib/dom/ was the GC experiment of round 53, not a DOM. This round is the
clamp -- one path from bytes of HTML to a tree with a computed style on every element.
THE NUMBER, measured and not claimed: the OFFICIAL html5lib tree-construction suite, complete and
unfiltered, 1936 cases in tests/data/html5lib/. Before 1323 (68.34 %), after 1837 (94.89 %). The suite
is not in html5lib/html5lib-tests any more -- commit 224991ec10 of 26 June 2026 deleted the directory
and points at web-platform-tests/wpt html/syntax/parsing/resources/. That is where these files come
from; PROVENANCE.md says so on the spot.
THE THREE GAPS ROUND 54 NAMED ARE CLOSED. Foreign content: the tree construction dispatcher of
13.2.6, the integration points, the four correction tables of 13.2.6.5 (37 SVG element names, 58 SVG
attributes, definitionURL, 11 foreign attributes with prefix and namespace), the breakout tags
including the END tags br and p, and CDATA -- the tokeniser knows no tree, so the tree computes the
position of the CDATA content out of the bogus comment and restarts the tokeniser there. `<template>`:
the 23rd insertion mode, the stack of template insertion modes, the content fragment, the redirection
of the insertion point, the template rule of foster parenting. Fragment parsing: the context element,
the tokeniser start state that belongs to it, and the fragment case of "after body". Round 54's own
known_gaps.dat goes from 0/10 to 9/10; the tenth is not a parser gap but a driver without a context
field. One expectation in that file was WRONG (`<template><td>` does not drop the td) and was
corrected against the official suite.
THE STANDARD MOVED WHILE WE WERE AWAY. "in select" and "in select in table" are GONE from WHATWG
(relaxed select parsing): select takes arbitrary content, joined the scope list and the end tag group,
and select/option/optgroup/hr/input got new rules in "in body". One case of round 54 was rewritten to
the new rule, with the reason in tools/html/LOG.md. Also new here: the scripting flag (it executes
nothing, it decides whether noscript holds raw text), and the COMPLETE quirks lists -- 55 public
identifier prefixes instead of the four round 54 built in and named.
THE DOM: lib/dom/api.fi. The node classes stay in lib/browser/node.fi, because the tree construction
builds them and a second node type would be a second DOM; they gained Attr.prefix (an attribute has
three name parts, and `xmlns` lies in the XMLNS namespace WITHOUT a prefix) and Elem.content. New is
the ACCESS: getElementById, getElementsByTagName/ClassName, querySelector and querySelectorAll over
the matcher of lib/css/sel.fi, textContent, innerHTML and outerHTML reading (the fragment
serialisation algorithm with its escaping rules, the void elements and the raw text elements). A
template behaves as in a browser: its content is a tree of its own, so getElementById does not find
anything in it and textContent is empty.
THE STYLE TREE: lib/dom/style.fi. What was missing was not the cascade but the two sources a test
harness hands in and a browser does not: the DEFAULT STYLESHEET (lib/dom/ua.css, built in) and the
`<style>` elements of the document itself, in document order, as origin AUTHOR. Plus the walk in
document order, so the parent's computed style is always finished first -- inheritance and `em` need
it. Eleven cases pin down inheritance, specificity, order of appearance, the three origins, the
reversal by !important, the style attribute, em/%, inherit/initial and the foreign namespace.
FOUND ON THE WAY, in lib/css/sel.fi: a type selector without a namespace prefix matched ONLY HTML
elements. selectors-4 5.1 says every namespace. Invisible as long as the tree carried no SVG. The
cross-check of the CSS round against cssselect2 on 14 real pages stays 840/840 after the fix.
WHAT IS MISSING, named: 99 cases. 88 of them are PROCESSING INSTRUCTIONS -- WHATWG made
`<?target data?>` a ProcessingInstruction node in June 2026 (PR 12118), this parser still makes the
old bogus comment. Doing it right means a tokeniser flag, because the frozen html5lib TOKENISER suite
of section 9 contains 38 `<?` cases that expect a comment; a shortcut that reinterprets the bogus
comment would pass most of them and be wrong on `<?t d > ?>`. 6 cases need a parser that EXECUTES
document.write, 4 need the selectedcontent element, 1 is an adoption agency case that was not chased.
MEASUREMENTS: tools/domb1/run.sh 1837/1936 (94.89 %), same in all three build stages, 17/17 DOM and
style cases. Section 9b 150/150 own cases, 8 real pages byte-identical across build stages, GC soak
20000 rounds at +4 KiB. Section 9c 305/305 + 109/109 + 840/840. english 0 0 0 0 0. firnfmt -c clean.
