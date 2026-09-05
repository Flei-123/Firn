# Round 94 — the line that was not the line, and a test that is a function

Branch `r94b-debug`. Two halves of `ACCEPTANCE.md` item 4: a **test runner
with machine-readable output** (`W2`, `TODO-FIRN.md` 0.3) and a **debugger
that shows source lines** (`W3`, 0.4).

Everything below was run. Every number has the command next to it.

**Machine:** AMD EPYC 7571, 8 vCPU, Debian 12, Linux x86_64.
**Toolchain:** `rustc` 1.99.0-nightly, GNU `as`/`ld`/`objdump`/`readelf` 2.40,
`gdb` 13.1.
**Base:** `main` at `2b55b9ef` (acceptance green).

The find of the round, up front:

| | before | after |
|---|---|---|
| `gdb`, optimized build, address of an addition that came out of an **embedded** function | `inline_probe.fi:24` — the line of `fn main` | `inline_probe.fi:23` — the line of the addition |
| what the program's **own panic message** says about the same instruction | `inline_probe.fi:23:9` | `inline_probe.fi:23:9` |
| source lines the line table covers (probe, `dev` / `dev-fast` / `release-safe` / `release-fast`) | 10 / 1 / 1 / 1 | 10 / 7 / 5 / 5 |

Two parts of the compiler said where one machine instruction came from, and
they disagreed. That is worse than saying nothing.

---

## 1. The bug: a side table that the optimizer invalidates

`compiler/src/dwarf.rs` kept the mapping *instruction → source line* in a
table keyed by **(function name, block number, index in the block)**, filled
during lowering. Its own header said what that costs:

```
//! Accuracy:
//!   * **always**: line of the `fn` declaration
//!   * **without the optimizer**: additionally instruction-exact lines. With
//!     the optimizer they are suppressed, because the optimizer removes and
//!     moves instructions and renumbers blocks — wrong lines would be worse
//!     than none.
```

The decision was right, the design was the problem: a key of *(block, index)*
survives exactly as long as nobody moves an instruction, and moving
instructions is what an optimizer does. So the whole line table was switched
off for every build except `--no-opt`, and what was left was one `.loc` per
function — the line of its `fn`.

That leftover is not merely coarse, it is **wrong**, and `inline.rs` makes it
wrong in the most visible way. Measured on `tools/dwarf/inline_probe.fi`
before the round (`--opt-level=release-safe`):

```console
$ gdb -batch -ex 'disassemble /s main' /tmp/probe
tools/dwarf/inline_probe.fi:
24	fn main() -> i32 {
   0x…199 <+11>:	mov    $0x7fffffff,%r8d
   0x…19f <+17>:	add    $0x1,%r8d          <- this is line 23, of another function
   0x…1a3 <+21>:	jo     0x…1ad <main+31>

$ /tmp/probe
panic: integer overflow in 'i32 + i32' at tools/dwarf/inline_probe.fi:23:9 (a=2147483647 b=1)
```

The panic message is built at lowering time out of the source position
(round 72, `lower.rs::overflow_msg`) and travels as text inside the binary,
so it is right. The line table said `24`. This is exactly the report that
opened the round: `gdb` pointed at `lib/gc/gc.fi:1993` while the program's own
message said `2000`.

---

## 2. The fix: the position travels on the instruction

`fir::Inst` grew a field:

```rust
pub struct Inst {
    pub dst: Option<Val>,
    pub ty: FTy,
    pub op: Op,
    pub loc: Loc,     // file, line, column; line == 0 = no position
}
```

`lower.rs` stamps it — per statement **and** per expression, so the column
agrees with the one the panic messages carry. From there on nobody has to do
anything: a pass that clones an instruction clones its position with it.
The passes that build something new say where it comes from:

| pass | what it does with the position |
|---|---|
| `inline.rs` | the copied body keeps the **callee's** position; the load of the result belongs to the **call** |
| `licm.rs` | the hoisted instruction keeps its own position (it moved, it did not change) |
| `peephole.rs` | the folded constant inherits the position of the instruction it replaces |
| `phi.rs` | the copy that resolves a phi carries the phi's position |
| `mem2reg.rs` | a phi node and the `undef` constant get **no** position — they are bookkeeping, not program text |
| `lower.rs::alloca` | frame bookkeeping, no position: otherwise a breakpoint on a line would stop in the prologue |

Both code generators emit `.loc` from it, at **every** build level:
`codegen_x86.rs` on the base path, `regalloc.rs` on the register-allocated
one. The register allocator used to hand a function back to the base path as
soon as debug lines were active (`debug_lines_active`), which would now have
thrown the register allocation away in every optimized build; the question it
asks is now the right one — *are VARIABLES being described* — because only
those need the frame.

Three details that decide whether the result can be believed:

* **`line == 0` emits nothing.** A `.loc` holds until the next one, so a gap
  inherits the previous line. Bookkeeping instructions therefore emit no
  `.loc` at all rather than a wrong one.
* **The panic arms carry their line.** Round 90 moved them behind the `ret`
  (`Emitter::cold`), where they used to inherit whatever line stood last.
  They are the code of the operation they belong to, and they now say so.
* **The shared runtime trampoline gets `.loc 1 0 0`.** DWARF's line 0 means
  "no source line here". Before, the panic formatter — hand-written assembly
  — was attributed to the last line of whatever function stood in front of it.

---

## 3. What that is worth, measured

`bash tools/dwarf/run.sh`, sections 7 and 8. The measurement does not look at
anything by eye: `objdump` finds the address of the overflow check, `gdb` is
asked which line that address is, and the answer is held against the panic
message **the program prints itself**.

```
== 7. ROUND 94: the line table does not lie ==
   dev: the check at 0x401072 is line 23, the panic message says 23, 10 lines covered
   dev-fast: the check at 0x40102b is line 23, the panic message says 23, 7 lines covered
   release-safe: the check at 0x40102b is line 23, the panic message says 23, 5 lines covered
   release-fast: no check (unchecked arithmetic), but line 23 of the embedded callee stands in main, 5 lines covered
   the panic arm at 0x401035 is line 23 as well

== 8. ROUND 94: the four build levels, and what each of them still knows ==
   lines covered: dev=10 dev-fast=7 release-safe=5 release-fast=5

DWARF: 63 passed, 0 failed
```

`release-fast` has no overflow check at all — unchecked arithmetic is the
promise of that level (`SPEC.md` §13, `L9`) — so there is no message to
compare against. What is measured there instead is the attribution across
inlining: line 23 of the embedded `add_one` stands inside `main`.

Four counter-checks, because without them the section would prove nothing:

1. **stripped**: `strip --strip-debug`, then `info line` has to answer *No
   line number information available*.
2. **a wrong expectation** (line 999) has to strike.
3. **the `fn main` line is explicitly NOT the answer** — that is the state
   this round removed, so a return to it fails here.
4. **no line number of the table points at a blank or a comment line**, and
   none is larger than the file. This runs at all four levels.

And section 8 adds the shape of the degradation: a coarser level may cover
**fewer** lines, never a line that `--no-opt` does not have at all. That is
what "may be less exact, must not lie" means as a check.

The old sections 1–6 are unchanged and still green: `.debug_info` written by
the compiler itself, a `gdb` session over `docs/gdb_example.fi` and
`tools/dwarf/probe.fi` with `break`, `bt`, `info args`, `info locals`,
`next`, `print` of a struct, of a pointer and of an array, `finish`, `ptype`
— and the counter-check that an optimized build carries **no** variable
information.

---

## 4. `firnc --test`: a test is a function

There was a runner before this round, `tools/testrunner` in Rust. It runs the
test **programs** of `tests/`: one file is one case, and when the seventh
check inside that file fails it can say nothing beyond "the program returned
7". What was missing is the thing every language has.

```firn
#[test]
fn adds_up() {
    var a: i32 = 2
    var b: i32 = 3
    a = a + b
    if a != 5 as i32 {
        syscall(60, 1, 0, 0, 0, 0, 0)
    }
}
```

```console
$ compiler/target/release/firnc --test tools/testrunner/cases/mixed.fi
{"suite":"firn","total":4,"cases":[
  {"name":"adds_up","status":"pass","us":229,"file":"tools/testrunner/cases/mixed.fi","line":5,"col":1},
  {"name":"overflows","status":"fail","us":162,"file":"tools/testrunner/cases/mixed.fi","line":18,"col":9,"signal":0,"exit":101,"reason":"panic: integer overflow in 'i32 + i32' at tools/testrunner/cases/mixed.fi:18:9 (a=2147483647 b=1)"},
  {"name":"writes_to_null","status":"fail","us":185,"file":"tools/testrunner/cases/mixed.fi","line":22,"col":1,"signal":11,"exit":0,"reason":""},
  {"name":"prints_and_passes","status":"pass","us":129,"file":"tools/testrunner/cases/mixed.fi","line":28,"col":1}
],"passed":2,"failed":2,"rate":0.500}
$ echo $?
1
```

`--format=tap` gives the same run as TAP 13 with a YAML block per failure.

**How the two halves fit together.** `compiler/src/testrun.rs` finds the
functions marked `#[test]`, and writes one `main` that names each of them
with its function value and the position of its declaration. In front of it
goes `lib/test/runner.fi`, embedded with `include_str!` — the same way the
collector runtime has been embedded since round 38. Both are lexed and parsed
in the **same run** as the program itself, exactly like the source text a
`comptime` block produces (`SPEC.md` §6.4). No temporary file, no second
compiler run, no external tool.

**Every case runs in a child of its own.** A test that writes through a null
pointer does not return, it dies. A runner that calls its cases directly dies
with the first one:

* `fork` per case, the parent only ever looks at the exit status;
* everything the child writes goes through a **pipe** and is captured, so a
  case that prints cannot corrupt the report (measured: `mixed.fi` case four
  prints, and the JSON still parses);
* `alarm` gives every case a time limit — `--test-limit=1` on a case that
  loops forever: `"signal":14`, run over in one second;
* the position of a failure is read out of the child's **own panic message**
  (`file:line:column`, round 72). Only when there is none — a segmentation
  fault has no message — is the position of the `fn` declaration reported,
  which the compiler knew when it wrote the case list.

The runtime half has no import, no allocation and no collector in it: only
system calls and static storage. It must not be able to disturb what it
measures.

**Measured** (`bash tools/testrunner/run.sh`): **35 checks, 0 failed.** Among
them the ones that decide whether any of it is worth anything:

| check | why it is there |
|---|---|
| the case AFTER the crashing one is in the report | without a process per case the report would end at the crash |
| the JSON parses with `python3 -m json.tool` and its header numbers match the cases below it | "machine-readable" is not an opinion |
| `file:line:column` of the failing case is held against the **statement that really stands there** | a position that is merely present proves nothing |
| exit code 1 with a failure, 0 without | that is what makes it usable in CI |
| a file without a `#[test]` is refused (exit 2) | an empty run must not look green |
| a file with a `main` of its own is refused with a sentence that names the reason | two entry points would otherwise be a linker error far from the cause |
| a wrong signature is refused (`fn takes_something(x: i32) -> i32`) | a test says what it thinks by running, not by returning |
| the same file is core language for **both** compilers | `#[test]` is in `lib/firnc1/parser.fi` too |

---

## 5. Both compilers, and the fixpoint

`#[test]` is in the attribute register (`compiler/src/attrs.rs`, the single
truth, printed by `--list-attrs`) and in the parser of the self-hosted
compiler (`lib/firnc1/parser.fi`, prescan and `attr()`). firnc1 has no test
mode: it compiles a marked function as an ordinary one, which is why
`tools/testrunner/cases/with_main.fi` — a file with both a `#[test]` and a
`main` — is translated by **both** compilers and returns 3 in both.

```console
$ bash tools/fixpoint.sh
STAGE 2: 15260 ms   4649088 octets
STAGE 3: 46595 ms   4649088 octets
FIXPOINT:  stage 2 == stage 3, character-identical (780189 lines of assembly)
```

---

## 6. What is NOT there, and why it is not there half

**Variables in an optimized build.** They are still only in `--no-opt`, with
their types, their frame offsets and their real values — unchanged and still
proved by `tools/dwarf/run.sh` sections 2–5. In an optimized build there is
**no** variable information at all, and the counter-check in section 6a
insists on it.

Round 92 is the reason it stayed that way, and the size of the problem is
measurable. `tools/dwarf/probe.fi` has 8 declared names and 11 `alloca`s:

```console
$ firnc --opt-level=dev --emit=fir  tools/dwarf/probe.fi | grep -c alloca
11
$ firnc --opt-level=dev-fast --emit=fir tools/dwarf/probe.fi | grep -c alloca
2
$ firnc --opt-level=dev-fast --emit=fir docs/gdb_example.fi | grep -c alloca
0
```

**Nine of eleven** slots are gone after `mem2reg` at `dev-fast`, and in
`gdb_example.fi` **all** of them. A `DW_OP_fbreg <offset>` for those would
point at storage nothing writes to any more. On top of that the register
allocator promotes surviving `alloca` cells into registers itself
(`regalloc.rs`, `cells`), and `mem2reg::remove_dead_stores` removes a store
whose value is never read again — after which the frame slot holds a stale
value. Three independent reasons why a fixed frame offset would be a lie.

The honest way to describe a variable that lives in different places over its
lifetime is a **location list**: `DW_AT_location` pointing into `.debug_loc`,
one entry per address range, filled from the allocation the register
allocator decided on. It needs markers in the instruction stream that survive
every pass (LLVM calls them `dbg.value`), a label per range in the emitted
assembly, and a second section. That is a round of its own, and it is written
down here rather than half-built: **a wrong value in the debugger is worse
than none** — the same sentence round 64 used, and the reason this round did
not touch it.

What the round did do for optimized builds is the half that can be made
right: **the lines**. A backtrace in a `release-safe` build now names the
right file and the right line for every frame, which is what the bug hunt
that started this round actually needed.

---

## 7. Acceptance (measured, 25.08.2026, branch `r94b-debug`)

| command | result |
|---|---|
| `cargo test --release --manifest-path compiler/Cargo.toml` | **256 passed, 0 failed** |
| `bash tools/dwarf/run.sh` | **63 passed, 0 failed** (was 48 before the round) |
| `bash tools/testrunner/run.sh` | **35 passed, 0 failed** (new) |
| `bash tools/fixpoint.sh` | stage 2 == stage 3, character-identical |
| `bash test.sh` | see section 8 |

## 8. What the round changed in numbers

| | |
|---|---|
| `fir::Inst` | one field, `loc` — 28 construction sites adjusted, so nothing could be forgotten silently |
| lines covered in an optimized build (probe) | 1 → 5 … 7 |
| files that emit `.loc` | 1 backend → 2 (base path **and** register allocator) |
| new attribute | `#[test]`, in both compilers |
| new tool | `tools/testrunner/run.sh`, 35 checks, `test.sh` section 53 |
| new source | `lib/test/runner.fi` (the runtime half, in Firn), `compiler/src/testrun.rs` |
