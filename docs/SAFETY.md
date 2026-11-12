# Firn memory safety -- what is guaranteed and what is not

State: 12.11.2026, `firnc` at main `abe6d3aef` (guarantees G1-G24 unchanged, effort review of the open N items below). Every claim below points to a test.
Green tests: `tests/47xx_safety_*.fi`, `tests/neg/47xx_safety_*.fi` (4730-4746: r904/r905/r907, null guard 4733/4736-4739, constant zero divisor neg 4740/4741/4744, positive 4745, constant overflow neg 4746). Counts: 24 guarantees, 14 open non-guarantees, 32 green safety tests, 25 UB probes.
Documented undefined behaviour (not green tests): `tools/safety/ub/*.fi`, recorded per build level in
`tools/safety/expected.txt`, checked with `bash tools/safety/run.sh` (exit 1 on any difference). `test.sh` section 149 runs it (plus `tools/safety/pe.sh`), so a change of behaviour shows up in the suite.

Reading guide: **C** = the compiler rejects the program, **P** = run-time panic (exit 101), **T** = hardware trap
(SIGSEGV 139 / SIGFPE 136, no message), **-** = nothing catches it (UB or implementation defined).

## Build levels

| level | flag | `+ - * / %`, narrowing `as` | array / `str` / `Vec` index | notes |
|---|---|---|---|---|
| dev | `--no-opt` | checked, P | checked, P | |
| dev-fast (CLI default) | `--opt-level=dev-fast` | checked, P | checked, P | |
| release-safe | `--opt-level=release-safe` | checked, P | checked, P | all optimiser passes, checks stay |
| release-fast | `--opt-level=release-fast` | **unchecked**, wraps / truncates (SPEC 13 `L9`) | **unchecked** | `/ 0` is a hardware trap |

Same program, all four levels (tests `4701`-`4706` run in dev, dev-fast, release-safe and expect 101; they are
skipped in release-fast by `// not_mode: opt`). Measured behaviour in release-fast (`tools/safety/ub/`):

| case | dev-fast / release-safe | release-fast | probe |
|---|---|---|---|
| `i32::MAX + 1` | P | wraps (exit 7) | ub09 |
| `0usize - 1` | P | wraps | ub10 |
| `x / 0` | P | T (SIGFPE 136) | ub11 |
| `-i32::MIN` | P | wraps to MIN (exit 7) | ub14 |
| `1u32 << 40` | P | count modulo 32 (exit 8) | ub15 |
| `i32::MIN / -1` | P | wraps to MIN, `% -1` is 0 | tests `4815` |
| `300i64 as u8` | P | truncates (44) | ub12 |
| array index 4 of `[i32; 4]` | P | reads the stack, no check | ub06 |
| `str` index past the end | P | no check | ub07 |
| `Vec` index after `vec_free` | P (len is 0) | T (ptr is 0) | ub08 |

## Guarantees (a violation is C or P in every safe level)

| # | Guarantee | How | Proof |
|---|---|---|---|
| G1 | Constant array index out of range | C | neg `4720` |
| G2 | Index type must be `usize`, no signed index into arrays | C | neg `4726` |
| G3 | No uninitialised variable: `var x: T` without initialiser does not parse | C | neg `4727` |
| G4 | Run-time array / `str` / `Vec` index out of range | P | `4701`, `4702`, `4706`, `4101`, `4102` |
| G5 | Integer overflow of `+ - *`, unsigned underflow, `/ 0`, lossy narrowing cast | P | `4703`, `4704`, `4705`, `4444` |
| G6 | Use of a moved value (struct with `drop`), double move | C | neg `4725`, `move_use_after`, `2004`, `2005` |
| G7 | The address of a local leaves its frame (return, out pointer, struct field, via `as u64`) | C | neg `4721`, `4722`, `4723`, `1610`, `1611` |
| G8 | One `inout` place plus a second use of the same place in ONE call | C | neg `4724`, `inout_*_in_call` |
| G9 | A reference parameter cannot be copied, stored or returned; no reference fields in structs | C | neg `4728`, `4729`, `ref_*` |
| G10 | `gc class` objects reachable from a root are never freed (no dangling `Gc[T]`); weak refs are cleared on collection | GC | `4700`, `520`, `500` |
| G11 | `v[a:b:s]` on a `Vec` builds a new `Vec` (copy): no view that can dangle or alias the source | by design | `4707` |
| G12 | `vec_free` zeroes ptr/len/cap: freeing the same variable twice is a no-op | by design | `4708` |
| G13 | Drop runs once per owned value, in reverse order, moved values are dropped by the new owner only | C + codegen | `2005` |
| G14 | `rt.heap_free` of a block that is already free (cached block): message `firn: double free (heap_free)`, exit 101, in every build level. Detection is a mark word at offset 8 (`p ^ FREE_MARK`) set when the block is cached and cleared when it is handed out again: one load and one store per call | P | `4730`, `4731`, `4734` (reuse stays clean), ub16, ub18 |
| G15 | Stack overflow by unbounded recursion: `firn: stack overflow`, exit 101 (SIGSEGV handler on an alternate stack; it only claims faults within 1 MiB of the faulting `rsp`, any other SIGSEGV stays 139). Levels dev, dev-fast, release-safe; Linux x86-64 user programs, main thread. firnc1 installs the guard in the checked levels only, like firnc0 (before, it also did in release-fast: differential test `tests/diff/safety_null_deref.fi`) | P | `4732`, `4733` (null deref still 139), ub05 |
| G16 | An explicit `x.drop()` on a local with a `drop` is a move: no second drop at scope end, reading `x` afterwards is rejected | C + codegen | `4711`, neg `4735` |
| G17 | `-MIN` (`-i32::MIN`, `-i64::MIN`, any signed width) | P (release-fast wraps to `MIN`) | `4810`, `4811`, ub13, ub14 |
| G18 | Shift count outside `0 .. bits-1` (also negative, also `<<=`) | P (release-fast: count modulo the width) | `4812`, `4813`, `4814`, ub15 |
| G19 | `MIN / -1`, `MIN % -1` | P (release-fast: `MIN`, `0`, no SIGFPE) | `4815`, `div_*_min_m1_i32` |
| G20 | `x += y`, `-=`, `*=`, `/=`, `%=`, `x++`, `x--` are exactly `x = x op y` (SPEC 12.7): overflow, underflow and `/ 0` panic (r935; before, they wrapped in every level) | P (release-fast wraps; `+%` / `-%` / `*%` spell the opt-in) | `4850`-`4855`, `4856`, `4857` |
| G21 | Dereference of a null raw pointer or a null `Gc[T]` (read or write, also a small offset such as a field): the same SIGSEGV handler claims every fault with `si_addr < 4096` and ends with `firn: null pointer dereference`, exit 101. A wild address (>= 4096) stays a plain SIGSEGV 139. Levels dev, dev-fast, release-safe; Linux x86-64 user programs, main thread of any thread that runs on the main alternate stack. A fault from a non-canonical address also reports `si_addr 0`, so it is reported with the same text | P | `4733`, `4736`, `4737`, `4738`, `4739` (wild stays 139), diff `safety_null_deref`, `safety_null_write`, ub01-ub04 |
| G22 | `x / 0`, `x % 0`, `x /= 0`, `x %= 0` with an integer type and a literal (or `const`) zero divisor: `division by the constant 0` / `remainder by the constant 0`. Holds in EVERY build level, also release-fast, where a variable zero divisor is a SIGFPE. Literal shift counts are NOT checked at compile time: the differential corpus (`tests/diff/shift_*`) and G18 test the run time panic with literal counts | C | neg `4740`, `4741`, `4744`, ok `4745`, diff `neg_safety_*` |
| G23 | `a + b`, `a - b`, `a * b` (also nested, with unary minus and `const` operands) whose operands are all literals or `const` integers and whose exact result does not fit the integer type: `constant overflow: the result does not fit into the type T`. Holds in EVERY build level, also release-fast, where it would wrap. Types i8-i64, u8-u32 (u64 / usize are not folded), `+%` / `-%` / `*%` are not checked | C | neg `4746`, diff `neg_safety_const_overflow_*`, ok `safety_const_overflow_ok` |
| G24 | `return`, `break`, `continue` that leaves a `defer` / `errdefer` body: firnc0 and firnc1 both reject it (firnc1 used to crash with SIGSEGV, r1436) | C | neg `defer_return`, `defer_break`, diff `neg_safety_defer_*`, ok `safety_defer_loop_ok` |

## Non-guarantees (nothing catches these)

Green tests that PIN the current behaviour: `4709`-`4712`. Everything else is in `tools/safety/ub/` and recorded
(a result may change with the allocator; the table is a snapshot, not a promise).

| # | Case | Outcome today (all safe levels) | Proof |
|---|---|---|---|
| N1 | (closed by G21, 08.11.2026) | | ub01, ub02 |
| N2 | (closed by G21, 08.11.2026) | | ub03, ub04 |
| N3 | Stack overflow by unbounded recursion in release-fast, in a thread other than the main thread, on aarch64 and on Windows | T (SIGSEGV 139), no message (the guard G15 is installed in `_start` of checked x86-64 Linux programs only) | ub05 |
| N4 | Raw pointer arithmetic past an object | reads / writes whatever is there | ub22 |
| N5 | `rt.heap_free` twice, when the first free already gave the block back to the kernel (more than 16 pages, or the size cache was full) | accepted (a second `munmap`); a later access faults. A free of a CACHED block is caught, see G14. A free of a block that was handed out again in between is a free of a live block and not detectable | ub16 |
| N6 | Read after `rt.heap_free` | returns 0 today for small blocks (wiped and cached); T for blocks that were `munmap`ed | ub17 |
| N7 | Copy of a `Vec` (`var w = v`): both are usable, `Vec[T]` is trivial. Freeing both is caught by G14 when the storage is cached; freeing one and using the other reads freed memory | P on the second free, else nothing | ub18, green pin `4712`, `4731` |
| N8 | `str` view from `rt.buf_str` after `buf_free` or after growth | dangling; today reads 0 / stale memory | ub19, ub20 |
| N9 | Pointer to a `Vec` element after the `Vec` grew | dangling; today reads stale memory | ub21 |
| N10 | Two raw pointers to one local; raw pointer plus `inout` of the same place in DIFFERENT statements | accepted, defined by execution order | ub23, green pin `4709` |
| N11 | `Vec[T]` is a trivial type (ptr/len/cap): copying and passing by value does not move it, no use-after-move | accepted | green pin `4712` |
| N13 | `Gc[T]` hidden as integer or stored in untyped memory (`rt.heap_alloc`) is NOT a root | object is collected while the integer still holds the address | ub24 |
| N14 | (closed by G17, r903) | | |
| N15 | (closed by G18, r914) | | |
| N16 | `unsafe { }` | NOT in stage 0 (parse error). Raw pointer deref and casts are allowed everywhere. SPEC 3.6 now says so (implementation status, r908): the rule is the target | ub25, green pin `4710` |
| N17 | Data races between threads, `Gc[T]` crossing threads | not analysed (no `Send` check) | not tested |
| N18 | Exclusivity of `inout` across statements and through callees: a callee can turn a reference into a raw pointer | not checked (SPEC 1560, ROADMAP r193) | ub23 |
| N19 | Lifetime analysis is not a proof: "where the analysis cannot decide, it allows" (SPEC 3.6.1) | escapes through unknown callee chains can pass | docs/ROUND79.md |

## Tools that watch the front-end and the unchecked corners (08.11.2026)

- `python3 -I tools/safety/fuzz_frontend.py --runs N --seed S [--keep DIR]`: mutates `tests/*.fi` (byte cut, line copy / swap, token insert, truncation) and compiles every mutant with firnc0 (`--emit=asm`) and firnc1. A signal, a timeout or a Rust `panicked at` is a **crash** (exit 1). Different accept / reject between the two compilers is counted as `disagree` (not a failure, only listed). `test.sh` section 149 runs 80 mutants.
- Measured 12.11.2026 (seed 2, 1200 mutants, firnc0 and firnc1 both built from main `abe6d3aef`): 0 crashes, 0 accept / reject disagreements. The 3 disagreements of 08.11. came from a stale `.firnc1` (built before G22): always rebuild it (`bin/firnc1.fi`) before fuzzing.
- `bash tools/safety/rawptr_audit.sh`: counts raw pointer casts and `(*p)` sites per `lib/*` directory. An approximate size of the N16 job (`unsafe { }` has to mark all of it first), not a proof.

## Segment rights (r898)

| Target | Artefact | Result |
|---|---|---|
| Linux x86-64 user ELF | `examples/tour.fi` (`readelf -lW`) | `R` / `R E` / `R` / `RW`, `GNU_STACK` RW. **No RWX** |
| Linux x86-64 user ELF, tiny | `examples/hello.fi`, release-safe | one `R E` segment (no data), `GNU_STACK` RW |
| Linux aarch64 ELF | `examples/tour.fi` | `R E` (text + rodata) / `RW`, `GNU_STACK` RW. **No RWX**; rodata is executable (page granularity) |
| Windows PE x86-64 | `fib`, `hello`, `tour` with `--target=x86_64-windows` | sections: `.text` RX, `.rodata` **R** (r902: `.section .rodata,"dr"` for the PE target; before it was RW 0xC0000040), `.data` RW, `.bss` RW, `.idata` RW, `.reloc` R. **No W+X section**. `DllCharacteristics 0x160` = NX_COMPAT + ASLR + HIGH_ENTROPY_VA |
| Osum user programs | `kernel/user/user.ld` (PHDRS) and `/root/vw/wg/ls.elf` | `R E` / `R` / `RW`: split correctly |
| **Osum kernel** | `osum.mb` built 08.09., 14.10., 18.10.2026 and `/k0.mb.elf` | **RWE**: LOAD 2 (`.utext .rodata .data .bss`) is `RWE`, LOAD 1 (`.text`) `R E`, `GNU_STACK` **RWE** |

**Kernel RWE: not changed here, reason.**
- Cause: `kernel/kernel.ld` has no `PHDRS`; the executable `.utext` (ring-3 pages, 2 MiB aligned) sits in the same segment as `.rodata/.data/.bss`, and `ld -n` is used. `tools/build-kernel.sh` filters the linker warning `LOAD segment with RWX` on purpose.
- Fixing the ELF flags alone changes nothing at run time: the multiboot loader and `boot.s` build their own identity-mapped page tables and ignore the segment flags; only stack pages carry NX (round K1). A real fix is W^X in the kernel page tables (`.text` RX, `.rodata` R+NX, `.data/.bss` RW+NX, `.utext` user+RX) plus `PHDRS` and `-z noexecstack`.
- That touches the Osum boot path and needs the QEMU gates of the Osum repo (other workers are active there), so it is a roadmap item, not a drive-by edit.

## Gaps (roadmap)

- Kernel W^X (page tables + PHDRS + `noexecstack`).
- Stack overflow guard for release-fast, threads (own `sigaltstack` per thread), aarch64 (`panic_rt_a64`/startup, fUi owns it) and Windows (N3).
- Use after `heap_free` (reads of cached blocks return 0, no detection) and double free of uncached blocks (N5, N6, N8, N9).
- Make `Vec` / `Buf` non-trivial (move semantics) or add a `drop` that is checked (N7, N11).
- Add `unsafe { }` and enforce it for raw pointer dereference (N16); the whole library has to be marked first (`rawptr_audit.sh` measures the size).
- Null guard on aarch64 and Windows (the SIGSEGV handler is x86-64 Linux only; same gap as N3).
- Roots for `Gc[T]` stored in raw memory: documented only (N13).

## Open items by effort (review 12.11.2026)

Cheapest first. None of the remaining N items can be closed by a one-line compiler fix: each needs a language rule, a runtime mechanism or a whole-library migration.

| order | items | what closing needs | size | why not done in a drive-by |
|---|---|---|---|---|
| 1 | N3 (threads, aarch64, Windows) | child sequence of `Op::ThreadSpawn` maps a 64 KiB alternate stack and calls `sigaltstack` (x86-64 first), munmap on exit; same text in `lib/firnc1/codegen.fi`; the aarch64 and Windows startups get their own handler | medium | changes the asm of every checked program that starts a thread: firnc0 and firnc1 must stay identical (fixpoint, `tests/diff`), plus a full cargo test run |
| 2 | N13 | reject `Gc[T] as <int>` outside `lib/gc` and add an explicit opt-in cast for identity use (hashing, printing) | medium | breaks existing library code that uses the address as an identity; needs a migration of every use first |
| 3 | N5, N6, N8, N9 | debug allocator mode: quarantine of freed blocks with `mprotect(PROT_NONE)`, generation tags on `Vec` / `Buf` views | medium to large | costs memory and speed, must be opt-in; the safe fix is ownership of `Vec` / `Buf` (item 4) |
| 4 | N7, N11 | `Vec[T]` and `Buf` as move types with a checked `drop` | large | every library signature and ~all tests that copy a `Vec` change |
| 5 | N10, N18 | exclusivity analysis across statements and through callees (SPEC 1560, ROADMAP r193) | large | a borrow checker; "where the analysis cannot decide, it allows" (N19) stays until then |
| 6 | N4, N16 | `unsafe { }` block, enforced for raw pointer deref and pointer casts | large | `rawptr_audit.sh`: 1928 files / 7505 casts in `lib/` have to be marked first |
| 7 | N17 | `Send` / `Sync` style check for values handed to threads | large | needs item 4 and 5 first (what may cross a thread boundary) |
| - | N19 | inherent: the lifetime analysis is not a proof | - | a stronger analysis only shrinks it |
