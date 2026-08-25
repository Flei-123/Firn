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
