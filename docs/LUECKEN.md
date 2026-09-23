# Firn gaps (LUECKEN) -- the complete list, checked against the compiler

Round GAPS, 23.09.2026, branch `runde-luecken` (base `main` 56bea7e9).

**How this list was made.** `grep` over `/root/certus-ui`, `/root/fb-osum`
and `/root/firn` for "Firn hat kein", "Firn kennt kein", "Firn kann nicht",
"Firn has no", "Firn cannot", "weil der Uebersetzer", "Umweg", patch files
in build scripts (`vendor/firn/patches/*`), the `--pic` side road, the
`docs/ROUND*.md` "what Firn cannot do yet" sections. **Every claimed gap
was then checked with a small program against the compiler of this branch**
(`/tmp/lk/*.fi` during the round; the results are in the "status" column).
A comment in a project is not evidence -- a third of them were stale.

Effect is estimated as T (speed), C (amount of code / workaround), F
(proneness to error), each 0-3. Sorted by effect on Certus and OrientOS.

## A. Closed in this round

| # | gap | where it forced a detour | what was done | test / measurement |
|---|---|---|---|---|
| A1 | **No square root instruction** | certus `lib/paint/painter.fi:175,2206` ("about ten f64 divisions PER PIXEL"), firn `lib/std/core.fi`, `std/math.fi`, `math/core_math.fi`, `paint/stroke.fi`, osum `kernel/app/wasm.fi:3178` | `__sqrt(x)` -> `sqrtsd`/`sqrtss`/`fsqrt`, FIR `Op::Un(UnOp::Sqrt)`, both compilers; std/math/painter/stroke use it | `tests/1653_sqrt_instruction.fi` (four levels + aarch64/qemu); `bench/firn/sqrt.fi`: 4 M roots **54-81 ms** vs Newton 1419-1688 ms vs painter fsqrt2 875-1019 ms (x12-x25). Old std sqrt looped forever on inf/NaN. |
| A2 | **Named constants were not valid `match` patterns** -- so every dispatch over named opcodes was an `if` chain, and a chain never becomes a jump table | certus `lib/js/bc.fi:13` ("kein computed goto"), `lib/js/interp.fi` bc_exec (44-arm chain), 26 chains of >= 6 arms in certus lib | bare `NAME` that is a const = the constant (Rust's rule), `module.NAME` qualified; resolved to literals in the type check; firnc1: bare form ported, qualified = not core | `tests/1650_match_const_module.fi`, `tests/1651_match_const.fi`, `tests/neg/1650_match_const_not_const.fi`; Certus bc_exec now ONE `jmp *(%rcx,%rax,8)` |
| A3 | **Miscompile: jump table overwrote `rdx`** (found while doing A2, not on any list) | every dense `match` on dev-fast/release-* whose arm reads a value the allocator put in `rdx`; Certus and firnc-gc have the same code | table base in `rcx` (pure scratch) | `tests/1652_switch_table_keeps_rdx.fi`: 254 instead of 42 before, on three of four levels |
| A4 | **`#[inline]` / `#[no_inline]` and the inliner's block placement** never reached main (branch `einbetten`, 14.09.) | JIT helpers in certus (`docs/RUNDE-JIT.md`), dev-fast builds get no inlining at all | the three commits taken over and translated; body is placed next to its call site (was: at the end, spilling across foreign loops) | 5 module tests in `inline.rs`; the round EINBETTEN measured 2.35 -> 1.08 ns per iteration |
| A5 | **No `A \| B` alternatives in a pattern** (was B7) | every chain with `op == A \|\| op == B` (certus `bc.fi:241-335`) | alternatives of literals, named constants, ranges, bool, enum variants; a dense match stays ONE jump table; alternatives must not bind; firnc1: not core (exit 3) | `tests/1654_match_or.fi`, `tests/neg/1654_match_or_binds.fi` |
| A6 | **No reinterpretation of the bit pattern** (was B11) | firn `std/core.fi:2238` + 20 detours in `std/math`, certus `js/builtin2.fi:1099`, `browser/netjs.fi:625` | `__bits(x)`, `__f64_from_bits(u)`, `__f32_from_bits(u)` -- one `movq`/`fmov`, no stack slot; both compilers; std/math switched over | `tests/1655_float_bits.fi` (four levels + aarch64), `tests/neg/1655_bits_wrong_type.fi` |
| A7 | **Function value -> number and back, no null `fn`** (was B6) | certus `android/a_main.fi:11224` (`code_of4`), osum `kernel/lib/ksym.fi:40`, `fs/ext4.fi:1215`, `fs/vfsops.fi:51` | `f as u64` / `n as fn(..)` (the record address both ways, `0 as fn(..)` = null function), `__code_of(f)` = the machine code address (word 0 of the record); both compilers | `tests/1656_fn_address.fi`, `tests/neg/1656_fn_from_i32.fi`, `tests/neg/1656_code_of_not_fn.fi` |
| A8 | **A `const` could not use another module's `const`** (was B8) | certus ROUND54 6.3 ("unknown name 'm__K'") | constants are checked in dependency order, not in merge order; both compilers | `tests/1657_const_from_module.fi` |
| A9 | **`Gc[module.Type]` did not parse** (was B9) | certus ROUND54 6.1 | `Gc[m.C]`, `GcWeak[m.C]`, `gc m.C { }`, `gc_null[m.C]()`, `weak_null[m.C]()`; both compilers | `tests/1658_gc_qualified_class.fi` |
| A10 | **A `str` literal returned from a function pointed into a dead frame** (was B18 "f-string prints garbage under `--no-opt`" -- the f-string was only where it showed) | every `fn f() -> str { return "x" }` on EVERY level (the optimised levels were lucky, not right); Certus' compiler `/root/firnc-gc` has the same code | the octets of a `str` literal now live in `.rodata`, one entry per distinct text (`statics::intern_text`); an ARRAY literal stays a writable copy in the frame; linking unchanged (`-n` stays unless there is a declared `static`); both compilers | `tests/1659_str_literal_outlives_frame.fi`: before: zeros and garbage under `--no-opt`, now the same text on all four levels, aarch64 and firnc1 |
| A11 | **No arithmetic on typed pointers** (was B13) | certus `anim/mix.fi:298`, `css/cascade.fi:2297` (`((p as usize) + 8) as *mut i32` -- the 8 is the element size done in the head) | `p + n`, `p - n` (n ELEMENTS, any concrete integer, signed goes back), `p - q` (i64, distance in elements), `p += n`, `p -= n`, `p++`, `p--`; C's rules, unchecked like every raw pointer; `Gc[T]` excluded; both compilers | `tests/1660_pointer_arithmetic.fi` (four levels, aarch64, firnc1), three `tests/neg/1660_*` |
| A12 | **No unchecked narrowing cast** (was B12) -- `u32 as i32` panics on dev-fast, only masks helped | certus `paint/ico.fi:99`, every hash/checksum/pixel packing | `x as% T`: integer to integer, never checked on any level (the `+% -% *%` promise for the conversion); an untyped literal inside takes the widest type, not the target; in a constant it is `as`; both compilers | `tests/1661_wrapping_cast.fi` (four levels, aarch64, firnc1), `tests/neg/1661_wrap_cast_float.fi` |
| A13 | **No `readdir` in std** (was B23) | certus `tools/android/bau.sh:144` (ships its own TLS root store because "Firn kann kein readdir") | `lib/std/dir.fi`: `dir.open(path)`, `dir.next(&d)` -> `d.name` (view into the block, valid until the next call), `d.kind` (`KIND_FILE`, `KIND_DIR`, `KIND_LINK`, ...), `dir.close`; one `getdents64` per 4 KiB, no allocation, "." and ".." left out; x86-64 and AArch64 through the syscall table | `tests/1662_std_dir.fi` (four levels, aarch64, firnc1) |
| A14 | **No `include_str`** (was B21) | `tools/gen_gctext.sh` (packs `lib/gc/gc.fi` into u64 words for firnc1) | `__include_str("path")`: the file's octets as a text literal at build time, relative to the source file, at most 1 MiB, `str` or `[u8; N]` by context; firnc1: not core (the prescan says so) | `tests/1663_include_str.fi` (four levels, aarch64, from another working directory), `tests/neg/1663_include_missing.fi` |

## B. Open, sorted by effect

| # | gap | status (checked) | where | T | C | F |
|---|---|---|---|---|---|---|
| B1 | **The compiler Certus builds with is not on main.** `/root/firnc-gc` (not a git tree) = branch `windows-auf-main` + local patches (ppoll, gc queries, dwmapi import). main lacks the Windows target, the integrated assembler (`asm_x86.rs`, `x86enc.rs`, `elfobj.rs`), `regalloc_a64.rs`, `#[arch]`, `#[win_callback]` -- 11 040 lines. firnc-gc lacks main's `if` expression (IFEXPR). Android builds from yet another tree (`/root/firn-c053apk`, `--pic`). Floating point register allocation (`xmm-ra`, 4 700 lines, TEMPO 1-3) sits unmerged on a branch. | confirmed (diff) | certus `bauen-win.sh:48`, `tools/android/bau.sh:26-33` | 3 | 2 | 3 |
| B2 | **Certus is built at dev-fast** (no `--opt-level`): no inliner, every add/mul/cast checked. release-fast was measured at ~1.01x on the JS benches (round NANBOX) -- the inliner alone is no lever, see B4 | confirmed | all `bauen-*.sh` | 1 | 0 | 0 |
| B3 | **No register allocation for floats on main** (functions with f64 go through the base path: every value through the frame) -- the painter's inner loops | confirmed (`regalloc.rs:2558`); done on branch `xmm-ra`, not merged | certus `lib/paint/*` | 3 | 0 | 0 |
| B4 | **GC write barrier is an out-of-line call per pointer store** (~19 ns measured by round EINBETTEN in the JIT helper, vs ~1.3 ns for a call) | confirmed (`gc_lower.rs::hook_assign`) | certus JIT, every `Gc` field store | 3 | 0 | 0 |
| B5 | **No tuples / multiple return values** | confirmed: `fn f() -> (i32, i32)` does not parse | certus `browser/tree.fi:776`, `browser/mediajs.fi:3493`, firnc1 `lexer.fi:1580` | 0 | 2 | 1 |
| B6 | closed, see A7 | | | | | |
| B7 | closed, see A5 | | | | | |
| B8 | closed, see A8 | | | | | |
| B9 | closed, see A9 | | | | | |
| B10 | **Tree walker dispatch is a sequence of `if k == ast.N_X { return }`** -- could now be a `match` (A2) | open (library work, not compiler) | certus `js/interp.fi:4548` eval_node | 2 | 0 | 0 |
| B11 | closed, see A6 | | | | | |
| B12 | closed, see A12 | | | | | |
| B13 | closed, see A11 | | | | | |
| B14 | **No type aliases** (`type Idx = u32`) | confirmed | firn `std/core.fi:2405`, `num/core_comfort.fi:14` | 0 | 1 | 0 |
| B15 | **Runtime names of the collector are a fixed list in the compiler** (`gc.rs` RUNTIME_QUERY) -- a new function in `lib/gc/gc.fi` needs a compiler patch | confirmed | certus `vendor/firn/patches/compiler-0005-*`, `0006-*` | 0 | 1 | 2 |
| B16 | **Windows import table is a list in `win.rs`** -- every new Win32 call is a compiler patch | confirmed (firnc-gc) | certus `vendor/firn/patches/compiler-0007-dwmapi-*` | 0 | 1 | 2 |
| B17 | **`lib/fui` patches that no longer apply** (0001-fui-painter-fontreq, 0002, 0004; uipaint<->painter rename) -- Certus freezes copies (`.fui-c073`) | confirmed (bauen-win.sh:17-24) | certus build scripts | 0 | 2 | 3 |
| B18 | closed, see A10 | | | | | |
| B19 | **No SIMD multiply** (vector blending) | not re-checked | certus `docs/RUNDE-RASTERN.md:43` | 2 | 0 | 0 |
| B20 | **No conditional compilation** | confirmed (by design: module choice per target) | certus `window/window.fi:20`, `tools/window/bau.sh:7` | 0 | 1 | 0 |
| B21 | closed, see A14 | | | | | |
| B22 | **No destructors** (by design, SPEC) | by design | `rt/vec.fi:14`, `rt/map.fi:30`, `std/json.fi:1353` | 0 | 1 | 2 |
| B23 | closed, see A13 | | | | | |
| B24 | **No alias information** (a store through one pointer invalidates everything) | by design today | certus `html/tokenize_main.fi:47` | 1 | 0 | 0 |

## C. Stale -- the comment says "Firn has no", the compiler has it

Checked with a program on this branch; the comments should go.

| claim | where | reality |
|---|---|---|
| "no function pointers" | osum `kernel/user/expdlg.fi:24`, `sched/sched.fi:394`, `docs/RUNDE-WAYLAND.md:109`, firn `lib/gc/gc.fi:2122` | `let f: fn(i32) -> i32 = add1; f(41)` works (round 58) |
| "no unary minus for f64" | certus `js/interp.fi:5498`, `builtin2.fi` | `-a` on f64 works |
| "no global variables" | osum `kernel/lib/kstate.fi:4`, `sched/ctr.fi:51`, `ipc/*.fi`, firn `std/core.fi:1295`, many | `static mut` exists (round 89) |
| "no `~`" | certus `browser/domjs.fi:6625`, ROUND68 | `~u` works |
| "no if expression" | osum `tools/fui-proto/tile_main.fi:51` | works on main (round IFEXPR) -- but NOT in firnc-gc (B1) |
| "no two-dimensional arrays" | osum `kernel/user/h264.fi:579` | `[[i32; 3]; 2]` works |
| "no array of strings" | certus `js/typed.fi:1599`, `browser/domjs.fi:5277` | `[str; 2]` is a type |
| "no sqrt in std" | osum `kernel/app/wasm.fi:3178` | `std.math.sqrt` existed; now it is one instruction (A1) |
| "no inliner" / "the JIT only knows the address" | certus `docs/RUNDE-JIT.md` | `inline.rs` exists since round 92, runs on release-*; `#[inline]` now on every level (A4). What the JIT really paid for is the write barrier (B4). |
| "no computed goto, so the dispatch is a comparison chain" | certus `js/bc.fi:13` | `match` had jump tables (`codegen_switch.rs`) all along; the missing piece was A2 |

## D. What the closed gaps bought in Certus (measured)

Certus built twice from the same tree (`/root/certus-luecken`), with the
Certus compiler copy plus this round's changes; JS benches with
`tools/nanbox/ab4.py` (twelve benches, interleaved, best of 3, 1 M
iterations, host under foreign load 8-10). Factor > 1 = faster.

| change | bench | result |
|---|---|---|
| A2: `bc_exec` as one `match` = ONE jump table instead of a 44-arm comparison chain | ab4, geometric mean | **0.991** (single benches 0.90-1.06: noise band). The chain was not the cost -- the branch predictor had it. github.com through `mess-ab.sh`, 5 interleaved pairs, `LAG_US` median 3.09 s (chain) vs 3.13 s (table): no difference beyond noise. |
| B4 experiment (NOT committed): the write barrier's fast path inline at every `Gc` field store instead of a call | ab4, geometric mean | **1.031** (zeichenketten 1.145, fibonacci 1.103, the rest in the noise). In firnc0 alone: 12.2 -> 2.9 ns per pointer store at dev-fast. Worth doing, but it touches the collector's contract -- a round of its own. |
| A1: `__sqrt` | `bench/firn/sqrt.fi`, 4 M roots | 54-81 ms vs Newton 1419-1688 ms vs the painter's `fsqrt2` 875-1019 ms (x12-x25) |

Honest summary: the gaps that were CODE gaps (A5-A12) buy shorter and
safer code, not speed. The one miscompile (A3) and the one dangling
pointer (A10) were the most valuable finds. The speed lever for Certus is
B1-B4, all of which are merges or collector work, not language features.

## E. The test suite at the end of the round

`./test.sh` on this branch: 1649 checks, 347 programs x 4 levels, all
negative tests, the self-compiling comparison (350 same behaviour, 0
differing), the fixpoint (stage 2 == stage 3, character-identical) -- green.
Three checks are red, and they are red on `main` (56bea7e9) too, for
reasons outside this round:

* `tools/js/run.sh`: `testdata/test262/subset.sha256` is not in the
  repository (only `MANIFEST.md` and the archive are).
* `tools/english/check.sh`: 512 German identifiers, all in `lib/fui`,
  `lib/svg`, `demos/fuidemo`, `tools/fui` (this round's one hit,
  `element_size`, was renamed).
* `tools/fmt/run.sh` step 3: files in `lib/fui`, `lib/svg`, `demos` not in
  canonical shape (87 on main, 67 here; none of them touched by the round).

Fixed on the way: `tools/fixpoint.sh` failed on main because the licence
line was added to `lib/firnc1/gctext.fi` by hand without regenerating it --
`tools/gen_gctext.sh` writes the line now.

## Counts

* 14 closed (A1-A14): two of them bugs that were on no list as such (A3 a
  miscompile, A10 a dangling pointer behind a "cosmetic" f-string entry)
* 14 open (B1-B5, B10, B14-B17, B19, B20, B22, B24), of which 3 by design
* 10 stale claims (C)
