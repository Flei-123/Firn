## Round WINDOWS (2026-09-01) -- Firn builds Windows programs; branch windows
SPEC.md line 127 said "target binary format: ELF" and `syscall(nr, a1..a6)` was built into the
language with the LINUX numbers. Windows has no system call a program may use -- the numbers of
ntdll are deliberately unstable between versions -- so the round added a third value to the
second axis of the target model of round ARM-FREESTANDING: `Os::Windows`, `--target=x86_64-windows`.
FOUR PIECES. (1) PE/COFF through the COFF port of the same binutils (`x86_64-w64-mingw32-as`/`-ld`),
used as an assembler and a linker and never as a compiler -- and the IMPORT TABLE is written by the
compiler itself (`.idata$2`..`$7`, win.rs), so no import library and no C runtime object enters the
image. The only foreign names are `__CTOR_LIST__`/`__DTOR_LIST__` out of the linker script, and that
is said out loud rather than hidden. (2) The calling convention: Firn-to-Firn stays System V (SPEC 13),
and every call that LEAVES the program goes through a thunk the compiler emits -- rdi/rsi/rdx/rcx ->
rcx/rdx/r8/r9, 32 octets of shadow space, arguments five and up down to the stack FIRST because r8/r9
are argument registers on both sides and would otherwise be lost. The register allocator and both code
generators were not touched, which is exactly why the Linux side cannot get worse. (3) Stack probing:
every frame of a page or more walks down page by page, or Windows' guard page is stepped over.
(4) The seam: `syscall` becomes a call into `win_seam.rs`, ~620 lines of FIRN injected into the
compilation unit the way comptime and the test runner inject source text, mapping 35 canonical Linux
numbers onto 42 Win32 functions from kernel32/ws2_32/advapi32.
MEASURED. 299 of 304 comparable cases of tests/ behave identically on Linux and under Wine (98 %),
and the five that do not have exactly TWO causes: threads (4) and processes (1). hello.exe, tour.exe
and a TCP client over ws2_32 print character-identical to their Linux builds; a panic writes the same
message and exits 101. tools/windows/machine.sh: 25 of 25, including a scan of the whole corpus that
finds NO `syscall` instruction left in any of the 314 Windows builds.
THE FIND OF THE ROUND: the collector reads /proc/self/maps for its stack bounds. Windows has no /proc
-- and Wine makes it worse, because drive Z: is the host's root, so the file really opens and the
collector then scans from a Windows stack pointer to the end of a LINUX mapping and dies with a page
fault. 35 of the 46 failing cases were that one thing. The seam now answers the file itself out of
GetCurrentThreadStackLimits.
THE LINUX SIDE IS UNTOUCHED, checked three ways and measured before and after: 314 of 314 programs of
tests/+examples/ produce CHARACTER-IDENTICAL `--emit=asm` from the compiler before and after (0 differ,
run three times); `cargo test` 270 -> 281 with 0 failures; `tools/packages/run.sh` line for line the
same -- and that last one is 22 of 39 on the BASE commit as well, measured on an untouched worktree,
which is written down instead of blamed on this round.
NOT DONE, and named: `.pdata`/`.xdata` (so no usable crash report and no unwinding across a system
boundary), callbacks Win64 -> System V (a window procedure IS one, so this blocks a GDI window),
threads and processes, DWARF on the Windows target, and `lib/firnc1` -- the self-hosted compiler does
not know the target at all yet, which is written up point by point in docs/ROUND-WINDOWS.md 4.4.
FOR CERTUS: the engine calls only SEVEN system numbers and all seven work; the collector runs; DNS is
over TCP so the one socket call the seam cannot do (sendto with an address) is not needed; and Certus
is single-threaded, so the biggest gap of this round does not touch it. X11 sits in exactly TWO files
behind EIGHTEEN names -- a `lib/browser/gdi.fi` of 600-800 lines is the whole port, and the only thing
in front of it is the callback thunk.

## Round K5 (2026-08-25) -- four processors in Osum; branch k5-smp
The kernel of rounds 59/62/K1/K2 was an operating system on ONE core, and said so in
kstate.fi: "NOT atomic -- it does not have to be: the kernel runs on one processor". It now
reads the ACPI MADT, starts the application processors with INIT/SIPI over a 182-octet
real-mode trampoline copied to 0x8000, gives each one a stack, a GDT, a TSS and a local APIC
of its own, and puts spin locks around the run queue, the frame allocator and the file system.
NOTHING HAD TO BE ADDED TO THE LANGUAGE. `__atomic_add` (round 47, `lock xadd`) and
`__atomic_swap` (round 49, `lock cmpxchg`) were already there; round 47 wrote that the
difference "is NOT measurable today by a two-thread run" -- this is that run. An atomic load
and store are the MMIO forms of round 52, which is correct on x86-64 and is the one thing that
would have to change on a weaker memory model.
MEASUREMENTS (shared build host, load average ~10 on 12 cores, five sequential pairs): twelve
units of arithmetic 966/799/819/957/877 ms on one core against 321/322/435/272/373 ms on four,
median speed-up 2.48x, best 3.52x, earlier quiet run 4.24x. Eight kernel tasks through the
SCHEDULER: 656/560/555/655/556 ms against 158/200/218/176/204 ms, median 2.80x, all four cores
taking tasks. COUNTER-CHECKS: the same guest with `-accel tcg,thread=single` gets 1.04x -- four
cores in one host thread are not parallel and the number collapses. `nolock`: the shared
counter comes out 1630 of 6000 instead of 6000, and the frame allocator hands the SAME frame to
two cores five times out of 64. `nosmp`: four processors found, one online.
THE BENCHMARK HAD TO BE REWRITTEN ONCE: dealing every core a fixed share makes the total the
time of the SLOWEST core, and on a loaded host one emulated core out of four is regularly
starved. The units are CLAIMED out of one counter with `lock xadd` now; same total work, and the
number stops measuring the host. The eight scheduler tasks likewise: they used to spin on
`pause`, and QEMU's translator leaves the emulation loop at every `pause` -- that measured the
emulator, not the machine.
THREE BUGS, ALL FOUND BY MEASURING: (1) the per-core records were put at kdata+0x12000, which is
where pci.fi keeps the counters of round K2 -- the running task index landed on the address of
the local APIC and the machine died after EXACTLY ONE timer interrupt with no message, because
the end-of-interrupt went to address zero. The offset list at the head of kstate.fi stopped at
0x0F000 and did not mention that pci.fi and nvme.fi own 0x10000..0x1B000; it does now. (2) Task 0
is kernel_main itself and the scheduler migrated it, correctly and fatally: ring 3, KSTACK_CUR
and the syscall MSRs all belong to the boot processor. One run in six died. That is why affinity
exists in this round. (3) The file system lock deadlocked against itself at "fs: format " --
format -> dir_init -> write_at, and write_at is one of the six locked entry points; it is
re-entrant on the same core now.
THE RULE THAT KEEPS IT ALIVE: no lock is ever held while another is taken, every lock is taken
with interrupts already off, and the run queue lock is held ACROSS the context switch and given
back by whoever the processor switched TO -- releasing it earlier lets a second core pick up a
task whose registers are not saved yet.
NOT DONE, NAMED: ring 3 stays on the boot processor (one KSTACK_CUR for the machine), no IPIs
beyond INIT/STARTUP, no TLB shootdown, no load balancing beyond "whoever is free takes the next".
MEASUREMENTS: tools/smp/run.sh 58/58 (test.sh section 57), tools/kernel/run.sh 175/175,
english 0 0 0 0 0, firnfmt -c clean.

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

## Round B2 (2026-08-25, branch b2-layout) -- layout, measured against a suite nobody here wrote
Round B1 left a tree with a computed style on every element and said plainly what was missing: nobody
knew WHERE anything stood. The layout code itself was not new -- rounds 61 and 67 had built the box
model, the flow, floats, positioning and a flexbox and measured all of it against Chromium ON CASES
WRITTEN HERE. That proves the engine does what its author thought, not that the author was right. This
round brings the foreign measuring stick.
THE MEASURING STICK, and why it works without a rasteriser: most of the WPT css/ area are reftests
(pixels), but a large part is SELF-DESCRIBING through resources/check-layout-th.js -- the expectation
stands in the markup as `data-expected-width`, `data-offset-y` and their kin, and what is compared are
the CSSOM VIEW accessors: offsetWidth/Left/Top/Height, clientWidth/Height/Left/Top, scrollWidth/Height,
getBoundingClientRect, the computed display, the used margins and paddings. That is position and size
instead of pixels. 471 such files from css/css-flexbox, css/CSS2, css/css-box, css/css-sizing,
css/css-position and css/css-align lie in tests/data/wpt-css (PROVENANCE.md there), harvested by ONE
mechanical rule: every file in those directories that includes check-layout-th.js. Three groups are set
aside mechanically and counted separately: `script` (the test builds its own DOM in JavaScript, 92),
`grid` (next round, 22), `vertical` (writing modes and rtl, 171). The rest, 186 tests, is corpus B2.
The harness applies the tolerance of check-layout-th.js itself: a difference below one pixel passes,
because a browser reports these accessors rounded and a layout engine computes in sixty-fourths.
THE NUMBERS, same harness against three engines: main (rounds 61+67) 42/186 = 22.58 %, after this round
59/186 = 31.72 % (3535 of 4867 single checks), and CHROMIUM 141 through the same harness 138/186 =
74.19 %. The third number is what keeps the second honest -- 26 of the 186 are margin-trim tests that
Chromium fails too, 12 more need an image or a video decoded. tools/layout/run.sh (rounds 61/67) stays
1087/1087 against the frozen Chromium reference, unchanged, in all three build stages.
FIVE REAL DEFECTS, all invisible to the 146 own cases of rounds 61/67:
(1) THE ANONYMOUS BLOCK CARRIED THE PARENT'S STYLE. A box with padding, border or margin holding BOTH
text and a block gave the anonymous box the parent's edges a SECOND time -- 36 px too low in the case
that found it. cascade.style_anonymous: initial box properties, inherited properties from the parent.
(2) A FLEX CONTAINER WITHOUT A DEFINITE MAIN SIZE SHRANK ITS ITEMS TO NOTHING. `flex-direction: column`
without a height -- the most ordinary flex container on the web -- measured ZERO, because the missing
height arrived as 0 and 9.7 then found negative free space. Its used main size IS the sum of the
hypothetical item sizes (9.3.1), so the free space is exactly zero.
(3) INTRINSIC WIDTHS DID NOT SEE THE MARGINS OF THEIR CHILDREN: they are asked before the subtree is
laid out, when ml/pl are still zero.
(4) `start` IS NOT `flex-start`: under row-reverse the physical pair and the flex-relative pair are
mirror images. Round 67 folded them together; eighteen WPT tests ask exactly that.
(5) A WRAPPING CONTAINER WITH ONE LINE IS NOT A SINGLE-LINE CONTAINER -- the rule "definite cross size
goes to the line" removed all free space and made align-content do nothing.
NEW FEATURES the corpus demanded: the static position of abspos children of a flex container
(css-flexbox-1 4.1, aligned as the sole item, with the single-item fallbacks of css-align-3), and the
keyword sizes min-content / max-content / fit-content / stretch (css-sizing-3/4).
THE SPLIT FOR B3 AND FOR EVERY RESIZE: viewport-INDEPENDENT are the box tree, the styles and the
intrinsic contribution int_min/int_max of every box (style and text decide it, percentages count as
zero) -- they survive. Viewport-DEPENDENT is every used value, the line boxes, the fragments, the
collapsed margins and four flags -- box.reset_box_geometry throws them away and
flow.relayout_document computes them again. THE PROOF: 471 of 471 documents give, after 800 -> 400 ->
800, the same output as a single layout at 800. THE COUNTER-CHECK: calling layout_document twice
WITHOUT the reset, as main does, leaves only 269 of 471 unchanged -- line boxes appended twice, offsets
added a second time. A browser built on that drifts with every resize.
WHAT IS MISSING, named: replaced elements have no intrinsic size (canvas/video/svg/object 300x150 and
the HTML attributes are not read, an image is not decoded -- 24 tests), aspect-ratio (10), gap (6),
margin-trim (26, nobody passes them), writing modes and rtl (the whole 171-test group, reported at 0),
grid (next round), tables as a formatting context, form controls, scrollWidth without a scrolling box.
MEASUREMENTS: test.sh section 61 = tools/layoutb2/run.sh: three build stages reach the SAME 59, reflow
471/471, limits in tools/layoutb2/minquota.txt. tests/1185..1188 new (anonymous edges, flex base size,
reflow, keyword sizes), numbers computed by hand and held against Chromium 141 afterwards. english
0 0 0 0 0.


## Round B3 -- painting: the browser becomes visible

THE STEP: out of the tree of rectangles of round B2 comes a PICTURE. Display list in the order of CSS
2.1 Appendix E, own scanline rasteriser, a TrueType reader, PPM and PNG.

THE NUMBER, against a suite nobody here wrote: 202 of 541 official WPT REFERENCE TESTS (37.34 %) --
`css/css-backgrounds` and `css/css-color`, every file with `<link rel=match>` plus its reference, taken
mechanically (tests/data/wpt-ref/PROVENANCE.md). A reference test compares PICTURES, which is what a
rasteriser is for and what round B2 could not do.

THE GUARD, and it is the whole reason that number can be believed: AN ENGINE THAT DRAWS NOTHING PASSES
EVERY REFERENCE TEST -- both sides come out white and white equals white. 32 pairs match here that way,
they are counted separately as `vacuous` and are NOT in the quota. With them the round would be
reporting 43.25 %.

TEXT WAS NOT CHECKED AGAINST AN AREA (the warning out of kernel round K7B, where a screen was 87 per
cent right and every letter was missing -- the 87 per cent were the background). Three checks instead:
(1) per GLYPH against a second rasteriser written with a different algorithm, on outlines fontTools
decoded -- 393 of 393 glyphs, mean coverage deviation 0.001, worst single pixel 0.049, 0 wrongly empty,
and 0 metric deviations over 408 characters and 469 kerning pairs; (2) per PAGE, every own case carries
the number of glyph pixels it set, so a regression to an empty page cannot be re-frozen away; (3) per
CORPUS, the vacuous guard.

THE YARDSTICK ITSELF WAS WRONG ONCE, and that is the finding worth keeping: the first reference
rasteriser used matplotlib's `Path.contains_points`, which unions the subpaths of a compound path.
It reported `O`, `D`, `Q`, `©` and `®` as broken at 0.45 overlap. The engine was right about all five.
A measurement has to be measured.

THE METRICS FLOW BACK, and it is proven with a counter-check that fails: the shrink-to-fit width the
LAYOUT computes for a text is within 2 px of the ink the PAINTER draws; with round B2's
one-em-per-character font behind the same painter it is 63 px too wide. 0 px of ink outside a fixed
width box, over 4 texts x 4 widths.

THE BUG THAT ONLY A PICTURE FINDS: `winner_clear` in the cascade cleared 53 property slots and round B3
has 65. A `linear-gradient` on one `div` therefore painted itself on the NEXT element as well. Every
layout test stayed green -- a gradient moves no box.

TIMES at 800x600, over 1082 renderings: layout 5.0 ms, display list 0.02 ms, raster 31 ms. The list is
a thousand times cheaper than drawing it, which is the argument for having one.

NOTHING GOT WORSE: html5lib 1837/1936, WPT layout 59/186, reflow 471/471, Chromium 1087/1087 boxes and
5171/5171 probe points -- all unchanged. The font metrics hang on a handle on the box tree that round
B2's programme does not pass, so its arithmetic is bit-identical.

THE DOUBLING WITH OSUM K10 IS REAL and is addressed rather than mentioned: `lib/font/raster.fi`,
`lib/font/ttf.fi` and `lib/font/metrics.fi` have NO imports beyond each other, are `#[no_gc]` throughout
and allocate nothing -- the caller hands them memory. They compile under `profile kernel` as they stand,
and K10 should import them instead of writing them a second time. docs/ROUNDB3.md section 5 has the
four steps. `lib/paint/` stays on the browser side; the boundary is one function call.

WHAT IS MISSING, named with the count of reference pairs each costs: bitmaps on the page (98 --
decode_png works and is checked against Pillow, but `<img>` is not a replaced element and JPEG is not
decoded at all), the modern colour spaces lab/lch/oklab/color() (78), background-repeat (55),
background-size (50), border-image (34), scripted reftests (29), table layout (24),
background-attachment (18), plus opacity as a multiplier instead of a layer, one shadow per property,
one font per page, no transform/filter/clip-path, no hinting, no CFF outlines. Eleven items, B3-1 to
B3-11 in docs/ROUNDB3.md section 4.

MEASUREMENTS: test.sh section 62 = tools/paintb3/run.sh -- three build stages of three root files, the
font against fontTools and against the second rasteriser, PNG both ways against Pillow, seven own cases
byte-identical in all three stages against a frozen picture, the text-fit check with its counter-check,
the 541 reference pairs, limits in tools/paintb3/minquota.txt. english 0 0 0 0 0.

## Round ARM-FREESTANDING -- a machine with nothing underneath it

THE ROUND IN ONE LINE: `--target=aarch64-none` exists, and a Firn program built with it BOOTS in
`qemu-system-aarch64 -M virt` and prints over the serial line. Round 80 built the second instruction
set; this one built the second SITUATION -- no operating system.

`target.rs` got a second axis. Arch (x86_64 / aarch64) was round 80's question; Os (linux / none) is
this one's, and the two do not fold into each other. Four names, and `none` is the word the GNU and
LLVM triples already use for bare metal. A `-none` target TURNS ON the kernel profile of round 52
rather than duplicating it -- which is why the x86 claim can be made to the octet:
`--target=x86_64-none` and the plain build of a `profile kernel` source produce the same 24,138
octets, and 305 of 305 programs in tests/ produce character-identical x86 assembly before and after.

INLINE ASSEMBLER ON A64, which is where round 80 stopped (4 NOT SUPPORTED, all of them this). The
first thing that had to move was not in the code generator: register names are checked in the TYPE
CHECKER, so `core.rs::stem` had to become target-dependent, or an A64 build would have swallowed
`out("rax")`. Operands do not travel on the stack here -- `sp` is set once in the prologue and every
slot is addressed relative to it -- so an asm block parks its operands in the outgoing argument area,
which is what makes a template that names x12 or x13 (this backend's own scratch) safe. MRS/MSR need
no form of their own and that was checked before it was written down: the system register name is
assembler TEXT and GNU as owns that table.

`#[interrupt]` on A64: x0-x18 and x30 saved by hand (A64 saves NOTHING by itself; the return address
is in ELR_EL1, a system register, not on the stack) and `eret` instead of `ret`.

NEW IN THE LANGUAGE: `#[arch(x86_64)]` / `#[arch(aarch64)]` in front of a function. An x86 assembler
template is not a Firn expression that has not been ported, it is a line for another assembler, and
the language had no way to say which machine a definition belongs to. One attribute, one `retain`,
run BEFORE the type checker. On the function and not on the statement, because two definitions of one
name then resolve themselves and a block has no value. firnc1 learned it too.

MEASURED: tools/aarch64/run.sh 304 of 304 comparable cases identical on both machines, 0 DIFFERENT,
0 NOT SUPPORTED, in both build stages (before: 300 SAME, 4 NOT SUPPORTED). machine.sh 16/16.
tools/freestanding/none.sh 27/27 (new, test.sh section 65). tools/aarch64/syscall_table.sh 6/6 (new,
section 66). tools/freestanding/run.sh 41/41. cargo test 262/262. The fixpoint holds: stage 2 ==
stage 3, character-identical, 23,278,384 octets. Compilation time -1.4 % on two workloads, i.e. no
measurable change.

NOT MEASURED, and said out loud: the full 66-section test.sh could not be run to completion, before
or after. Four to eight other rounds were running their own suites on the same twelve cores and the
long sections (16 self_compare, 17 fixpoint) were killed twice. Sections 1-15 ran green with 0 FAIL,
and the fixpoint was re-run on its own and holds.

WHAT IS STILL MISSING: firnc1 cannot generate aarch64 and says so instead of quietly producing x86.
Its share of this round is real but partial -- `#[arch]`, `lib/firnc1/syscalls.fi` (compared entry for
entry against the Rust table on every run, read out of a RUNNING program built by both compilers) and
`--target=` on its command line, including `x86_64-none`, so one flag builds a freestanding object
with either compiler. The A64 code generator in Firn is a round of its own.

TRAPS worth the next reader's time: `.align 2048` for a vector table is not an error on AArch64 but a
WARNING ("alignment too large: 63 assumed") and the table is then misaligned -- the silent form of
round 80's `.align` trap. A64 has no move-64-bit-immediate and no store-immediate-to-memory, both of
which bite inside asm templates where the compiler cannot help. And the freestanding check "contains
no syscall instruction" does not translate literally: `svc` is also how a kernel is ENTERED, so the
A64 check counts them instead of forbidding them.
