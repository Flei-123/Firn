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

## B. Open, sorted by effect

| # | gap | status (checked) | where | T | C | F |
|---|---|---|---|---|---|---|
| B1 | **The compiler Certus builds with is not on main.** `/root/firnc-gc` (not a git tree) = branch `windows-auf-main` + local patches (ppoll, gc queries, dwmapi import). main lacks the Windows target, the integrated assembler (`asm_x86.rs`, `x86enc.rs`, `elfobj.rs`), `regalloc_a64.rs`, `#[arch]`, `#[win_callback]` -- 11 040 lines. firnc-gc lacks main's `if` expression (IFEXPR). Android builds from yet another tree (`/root/firn-c053apk`, `--pic`). Floating point register allocation (`xmm-ra`, 4 700 lines, TEMPO 1-3) sits unmerged on a branch. | confirmed (diff) | certus `bauen-win.sh:48`, `tools/android/bau.sh:26-33` | 3 | 2 | 3 |
| B2 | **Certus is built at dev-fast** (no `--opt-level`): no inliner, every add/mul/cast checked. release-fast was measured at ~1.01x on the JS benches (round NANBOX) -- the inliner alone is no lever, see B4 | confirmed | all `bauen-*.sh` | 1 | 0 | 0 |
| B3 | **No register allocation for floats on main** (functions with f64 go through the base path: every value through the frame) -- the painter's inner loops | confirmed (`regalloc.rs:2558`); done on branch `xmm-ra`, not merged | certus `lib/paint/*` | 3 | 0 | 0 |
| B4 | **GC write barrier is an out-of-line call per pointer store** (~19 ns measured by round EINBETTEN in the JIT helper, vs ~1.3 ns for a call) | confirmed (`gc_lower.rs::hook_assign`) | certus JIT, every `Gc` field store | 3 | 0 | 0 |
| B5 | **No tuples / multiple return values** | confirmed: `fn f() -> (i32, i32)` does not parse | certus `browser/tree.fi:776`, `browser/mediajs.fi:3493`, firnc1 `lexer.fi:1580` | 0 | 2 | 1 |
| B6 | **Function value -> address (`f as u64`) and no null `fn`** | confirmed: "conversion from fn(...) to u64 is not allowed" | certus `android/a_main.fi:11224` (`code_of4` punning through memory), osum `kernel/lib/ksym.fi:40`, `fs/ext4.fi:1215`, `fs/vfsops.fi:51` | 0 | 1 | 2 |
| B7 | **No `A \| B` alternatives in a pattern** | confirmed: "expected '=>' after the pattern, found '\|'" | every chain with `op == A \|\| op == B` (certus `bc.fi:241-335`) | 1 | 1 | 0 |
| B8 | **A `const` cannot use another module's `const`** | confirmed: "unknown name 'm__K'" | certus ROUND54 6.3 | 0 | 1 | 1 |
| B9 | **`Gc[module.Type]` does not parse** (qualified name in a type argument; works only because `gc class` names are global) | confirmed | certus ROUND54 6.1 | 0 | 1 | 2 |
| B10 | **Tree walker dispatch is a sequence of `if k == ast.N_X { return }`** -- could now be a `match` (A2) | open (library work, not compiler) | certus `js/interp.fi:4548` eval_node | 2 | 0 | 0 |
| B11 | **No reinterpretation operator** (`f64` <-> `u64` bit pattern) -- done through a stack slot | confirmed | firn `std/core.fi:2238`, certus `js/builtin2.fi:1099`, `browser/netjs.fi:625` | 1 | 1 | 0 |
| B12 | **No unchecked narrowing cast** (`u32 as i32` panics on dev-fast; `+% -% *%` exist, `as%` does not) | confirmed | certus `paint/ico.fi:99` | 0 | 1 | 1 |
| B13 | **No pointer arithmetic on typed pointers** (`p + 2` on `*mut i32`) -- written as `((p as usize) + 8) as *mut i32` | confirmed | certus `anim/mix.fi:298`, `css/cascade.fi:2297` | 0 | 2 | 2 |
| B14 | **No type aliases** (`type Idx = u32`) | confirmed | firn `std/core.fi:2405`, `num/core_comfort.fi:14` | 0 | 1 | 0 |
| B15 | **Runtime names of the collector are a fixed list in the compiler** (`gc.rs` RUNTIME_QUERY) -- a new function in `lib/gc/gc.fi` needs a compiler patch | confirmed | certus `vendor/firn/patches/compiler-0005-*`, `0006-*` | 0 | 1 | 2 |
| B16 | **Windows import table is a list in `win.rs`** -- every new Win32 call is a compiler patch | confirmed (firnc-gc) | certus `vendor/firn/patches/compiler-0007-dwmapi-*` | 0 | 1 | 2 |
| B17 | **`lib/fui` patches that no longer apply** (0001-fui-painter-fontreq, 0002, 0004; uipaint<->painter rename) -- Certus freezes copies (`.fui-c073`) | confirmed (bauen-win.sh:17-24) | certus build scripts | 0 | 2 | 3 |
| B18 | **f-string with a `str` returned by a function prints garbage under `--no-opt`** (bug, round FIRN-LUECKEN) | confirmed today | `docs/ROUND-GAPS.md` "not achieved" | 0 | 0 | 2 |
| B19 | **No SIMD multiply** (vector blending) | not re-checked | certus `docs/RUNDE-RASTERN.md:43` | 2 | 0 | 0 |
| B20 | **No conditional compilation** | confirmed (by design: module choice per target) | certus `window/window.fi:20`, `tools/window/bau.sh:7` | 0 | 1 | 0 |
| B21 | **No `include_str`** | confirmed | `tools/gen_gctext.sh` | 0 | 1 | 1 |
| B22 | **No destructors** (by design, SPEC) | by design | `rt/vec.fi:14`, `rt/map.fi:30`, `std/json.fi:1353` | 0 | 1 | 2 |
| B23 | **`readdir` missing in std** | not re-checked | certus `tools/android/bau.sh:144` | 0 | 1 | 0 |
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

## Counts

* 4 closed (A1-A4, one of them a miscompile that was on no list)
* 24 open (B1-B24), of which 3 by design
* 10 stale claims (C)
