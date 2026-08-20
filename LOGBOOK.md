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
