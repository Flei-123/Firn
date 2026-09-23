# Round WASM — Firn programs in the browser

Until this round a Firn program ran where an x86-64 or an AArch64 Linux
kernel ran. `firnc --target=wasm32-browser` adds a third target that is no
machine at all: a WebAssembly module for a web page. fUi (`lib/fui`) paints
into a `<canvas>` of an ordinary page -- the same surface that appears as a
PNG or in a window -- and the page is operated with mouse, wheel and
keyboard.

Everything below was run on 2026-09-23 on this tree; every number has the
command that produced it next to it.

---

## 1. The numbers

| What | Command | Result |
|---|---|---|
| **The same programs, native and as WebAssembly** | `bash tools/wasm/run.sh` | 335 programs (`tests/*.fi`, `tests/opt/*.fi`, `examples/*.fi`), each in four build levels: **314 identical octet for octet** (stdout, stderr, exit code) in every level, **21 refused at compile time with a reason**, **0 different** |
| The encoder against an independent assembler | same, section 2 | **314 of 314** modules: `wat2wasm(our text form) == our binary` |
| The fallback for irreducible graphs | same, section 3 | **314 of 314** programs identical to native with every function forced through it (`FIRN_WASM_DISPATCH=1`) |
| The collector without a stack scan | `bash tools/wasm/gc_soak.sh` | intact after **1096-1097 collections**, 36,000,000 nodes verified, in three build levels; **counter-check** without the spills: `CORRUPT after round 9` |
| The page of `gallery9_main.fi` in Chromium | `bash tools/wasm/webdemo.sh` | **0 differing pixels** in 4 of 4 pictures (dark/light, 1240/980 wide) against the PNGs of `tools/fui/run.sh --images`, from the optimised and the unoptimised build |
| Operating it | same | hover, click, wheel, Tab, arrows, space -- each changes the picture, each way back gives the reference again, **0 pixels**; no picture while idle |
| Device pixels | same | ratio 2: a 2480x1440 canvas for 1240x720 CSS; averaged down 2x2, **0.86 %** of the pixels off by more than 64/255 (glyph anti-aliasing) |

The 21 refusals, by reason:

| Reason | Programs |
|---|---|
| files (`open`, `close`, `dup`, `dup2`) | 610, 700, 790, 806, 823, 824, 1280, 1282, 1283, 1284 |
| threads (`arch_prctl`, `mprotect`, `__thread_start`) | 834, 860, 861, 862 |
| inline assembler | 850, 851, 852, 1242 |
| SIMD | 1613 (the crypto library's AES-NI path), 1614 |
| sockets | 1600 |

Each of them is refused by the compiler, with the name of the call and the
path from `main` to it, e.g.

```
error: wasm32-browser: 'rt__read_file' uses system call 'open' (2) -- a browser page has no files and no file descriptors
  note: reached through main -> rt__read_file
```

---

## 2. The translation (`compiler/src/codegen_wasm.rs`)

**Binary directly, not `.wat` + `wat2wasm`.** `as` and `ld` belong to every
Linux machine; `wat2wasm` belongs to one project (wabt) and would have been
the compiler's first foreign build dependency. The binary format is small
(`compiler/src/wasm_enc.rs`, LEB128 and eleven section kinds). The text form
is still written (`--emit=asm`), from the same instruction list, and
`tools/wasm/run.sh` hands it to `wat2wasm` wherever that exists: 314 of 314
modules come back octet for octet as our binary.

**Values.** `i8 i16 i32 u8 u16 u32 bool` live as `i32`, always normalised
(sign or zero extended, `bool` 0/1); `i64 u64 ptr` as `i64`; `f32`/`f64` as
themselves. A pointer stays **64 bits wide**: every layout and every
`size_of` is the x86-64 one, and the address is cut to the 32 bits of wasm32
only at the access (`i32.wrap_i64`).

**Control flow** (`compiler/src/wasm_cfg.rs`): the dominator tree after
Ramsey (*Beyond Relooper*, ICFP 2022) -- a loop header becomes a `loop`, a
node reached along two forward edges a `block` followed by it, everything
else is written in place. An irreducible graph (jump threading can make
one) takes the loop-plus-`br_table` dispatch; none of the 314 programs
needs it, which is why section 3 of `run.sh` forces every function through
it once.

**Memory.** `0..4096` stays empty (a null pointer hits nothing), then the
data -- the collector's type table and state block, the method tables, the
function records and the statics, READ out of the assembler texts that
`gc.rs`, `iface.rs`, `fnval.rs` and `statics.rs` already write, so that the
octets are the same on every target -- then an 8 MiB shadow stack for the
`alloca`s, then the heap. Function values are table indices; a function
record points at a shim that drops the record argument, because
WebAssembly, unlike System V, checks the argument count of an indirect call.

**Semantics chosen to match x86-64**, which is the reference of the
comparison: `cvttsd2si` answers `0x8000000000000000` for NaN and every
out-of-range value (WebAssembly's `trunc_sat` saturates -- corrected), the
checked arithmetic panics with the same two writes and exit code 101 as
`panic_rt.rs`, the saturating forms clamp the way `emit_wrap_sat` does.

---

## 3. System calls become imports

`syscalls.rs` has a second table, keyed by the same canonical x86-64
number:

| Linux | wasm32-browser |
|---|---|
| `write`, `read`, `exit`/`exit_group`, `clock_gettime`, `getrandom`, `nanosleep` | imports `firn.write`, `read`, `exit`, `clock_ns`, `random`, `sleep_ns` |
| `mmap`, `munmap` | functions of the module on `memory.grow` (anonymous memory only; a free list, first fit, merged, reused pages zeroed) |
| `futex` | the futex of ONE thread: WAKE wakes nobody, WAIT on a changed word is `EAGAIN`, WAIT on an unchanged one would block forever and ends the program with a message |
| `sched_yield`, `madvise` / `getpid`, `gettid` | 0 / 1 |
| files, sockets, processes, signals, `brk`, `mprotect`, `arch_prctl`, `clone` | **refused at compile time**, with the name |

Only what is reachable from `main` (and `gc_init`, the `#[panic_handler]`,
every `#[export_c]`) is translated, so a library function that opens a file
stands in the way of nobody who does not call it. `extern fn` becomes an
import from `env`, `#[export_c]` an export -- the two doors `lib/plat/web.fi`
uses.

---

## 4. The collector: the shadow stack as the root set

`lib/gc/gc.fi` scans the machine stack conservatively; a WebAssembly local
has no address and cannot be scanned. The collector is NOT changed (and with
it `lib/firnc1/gctext.fi`). Instead the code generator makes the roots
visible:

* every pointer sized value (`i64`, `u64`, `ptr`) that is **live across a
  call that may collect** is stored into a slot of the caller's shadow frame
  right before that call -- the liveness is a backward data flow per
  function, "may collect" comes from the call graph (`gc_init`, `gc_collect`,
  `__gc_alloc_raw`, `__gc_collect_now` and everything that reaches them;
  indirect calls and host calls always count);
* the collector's scan runs unchanged from `__gc_sp_below` -- the address of
  a local, i.e. an address in the shadow stack -- to the stack top, and sees
  exactly what it sees natively;
* the one thing that cannot stay as it is, the stack BOTTOM (`gc_init` reads
  it from `/proc/self/maps`), comes from two functions that get a body of one
  instruction on this target: the top of the shadow stack.

The collector does not move objects, so the value in the local stays valid.
A program without the collector writes not one slot.

`tools/wasm/gc_soak.fi` keeps four chains of 600 nodes alive ONLY in locals
while 15,000 rounds allocate, and checks every node after every round. With
the spills switched off (`FIRN_WASM_NO_SPILL=1`, nothing else) it reports
corruption after round 9 -- the test can see a missing root.

---

## 5. The web page

* `lib/plat/web.fi` -- the Firn half: owns canvas, painter, theme, ONE
  scene and sheet; hands the canvas to the host without a copy (its octets
  are already RGBA); takes pointer, wheel, key and size events and passes
  them to `control.fi` and `viewport.fi`; paints only when something is
  dirty (`anim.animator_dirty` included). `devicePixelRatio` becomes
  `theme.theme_set_scale`.
* `lib/fui/control.fi` -- a panel can serve widgets that stand INSIDE larger
  entries (`panel_attach_stride`): the widgets of a scene, without a copy.
* `lib/fui/scene.fi` -- the lengths of the tree (paddings, gaps, fixed and
  style sizes, basis, bounds) go through `render.len_of`, the one scale fUi
  has. At scale 1000 every conversion is the identity: the 26 pictures of
  `tools/fui/run.sh --images` stay octet for octet the same.
* `demos/webdemo/firn.js` -- the loader, 137 lines, no program logic: it is
  there because a browser starts WebAssembly only from JavaScript.
* `tools/fui/gallery9_web.fi` -- the page, built with the very functions of
  `gallery9_main.fi` (imported as a module) that paint the PNG.

---

## 6. What does NOT work

* **SIMD** (`v128` and its 42 intrinsics) and **threads** -- refused at
  compile time. `__cpu_features()` is 0, so a library that asks takes its
  plain path -- but the crypto library's AES-NI functions are still
  reachable and therefore refused (`tests/1613`).
* **Files, sockets, processes, signals** -- refused by name. A page gets its
  font from the loader, not from a file.
* **Inline assembler, `#[interrupt]`, the kernel profile** -- x86 text and
  bare metal by nature.
* **No debug information** in the module (a name section, so a trap names
  the Firn function; no DWARF).
* **The stack**: the shadow stack has 8 MiB like a Linux process, but the
  engine's OWN call stack is its business; `tools/wasm/run.mjs` is started
  with `--stack-size=7800`, a browser has what it has. No test of the series
  needed more.
* **`mmap` never gives memory back to the engine** -- WebAssembly memory
  cannot shrink; freed runs are reused.
* **CPU time**: `CLOCK_PROCESS/THREAD_CPUTIME` answer the monotonic clock --
  a page has no CPU time of its own.
* **The demo's device ratio is read once**: a ratio that changes while the
  page is open scales fonts and tree but not gallery9's scroll area numbers.
* **No WASI**, no component model; the target is a browser page.
