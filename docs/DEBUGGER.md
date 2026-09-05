# Debugger: `.debug_info` of our own and a real `gdb` session

**Requirement:** `W3` · `ACCEPTANCE.md` item 4 criterion B · `TODO-FIRN.md` 0.4
**State (round 94):** lines, functions, parameters, local variables **and
their types** work. **New in round 94:** the line information is right at
**every** build level, not only under `--no-opt` — the position travels on
the instruction (`fir::Inst.loc`) instead of in a side table that the
optimizer invalidated. What is still open stands under „Limits", as before.

## How it is generated

Two halves, and the split is deliberate:

| Section | Who writes it | Why |
|---|---|---|
| `.debug_line` | the assembler, out of `.file`/`.loc` | it knows the addresses; the compiler would have to guess instruction lengths |
| `.debug_info`, `.debug_abbrev` | **the compiler**, `compiler/src/dwarf_info.rs` | names, types and frame offsets are known only to the compiler |

`compiler/src/dwarf.rs` collects during lowering: the mapping *instruction →
source line*, and — new in round 64 — **every declared name** with its FIR
value, its type, and the file and line of its declaration. The single hook
for that is `lower.rs::declare_ty`, the one place where a name out of the
source text is bound to storage. `codegen_x86.rs` adds the frame offset (it
is only known once `layout` has partitioned the frame) and writes the two
sections at the end of the assembly.

The shape of the information (DWARF 4):

```text
DW_TAG_compile_unit             producer, language, name, comp_dir,
                                low_pc/high_pc, stmt_list
  DW_TAG_base_type              i8 … u64, usize, bool, f64
  DW_TAG_pointer_type           *mut T / *const T
  DW_TAG_array_type             [T; N]  (with DW_TAG_subrange_type)
  DW_TAG_structure_type         struct with DW_TAG_member and offsets
  DW_TAG_subprogram             name, decl_file/line, low_pc/high_pc,
                                type, frame_base = DW_OP_reg6 (rbp)
    DW_TAG_formal_parameter     name, type, location = DW_OP_fbreg -off
    DW_TAG_variable             the same for local variables
```

**No** external tool and **no** C compiler is used — only the assembler,
which is in the build path anyway.

Precision (**round 94**):

| Build mode | Line information | Variables |
|---|---|---|
| `firnc --no-opt file.fi` | **statement-precise** | **yes**, with types |
| `--opt-level=dev-fast` | **statement-precise**, embedded code keeps the callee's line | **no** |
| `--opt-level=release-safe` | **statement-precise**, coarser (fewer lines survive) | **no** |
| `--opt-level=release-fast` | coarser still, but never a line the program does not have | **no** |

Measured on `tools/dwarf/inline_probe.fi`, distinct source lines covered:
**dev 10, dev-fast 7, release-safe 5, release-fast 5** — before round 94 the
three optimized levels covered exactly **one** line per function, the one of
its `fn`. `tools/dwarf/run.sh` sections 7 and 8 hold the table against the
panic message the program prints itself and insist that a coarser level never
invents a line that `--no-opt` does not have.

The **variables** are a different promise and they stay tied to `--no-opt`.
The reason is in `SPEC.md` §14.1 item 16 and in round 92: the optimizer pulls
an `alloca` into a register (`mem2reg` — 9 of 11 slots in
`tools/dwarf/probe.fi`, all of them in `docs/gdb_example.fi`), the register
allocator promotes surviving cells itself, and `remove_dead_stores` leaves a
frame slot stale. A frame offset recorded before all that would point at
storage that is no longer written to. **A wrong value in the debugger is
worse than none**, so with the optimizer the variable information is left out
entirely — and `tools/dwarf/run.sh` checks that as a counter-check. What it
would take is written down in `docs/ROUND94.md` section 6: DWARF location
lists.

## Proof: the session, copied verbatim

Program `docs/gdb_example.fi`:

```firn
// expect_exit: 55
fn summe(n: i32) -> i32 {
    var s: i32 = 0
    for i in 1 as i32..n + 1 as i32 {
        s = s + i
    }
    return s
}

fn main() -> i32 {
    let r: i32 = summe(10)
    return r
}
```

```console
$ compiler/target/release/firnc --no-opt -o /tmp/gdbdemo docs/gdb_example.fi
$ readelf -S /tmp/gdbdemo | grep debug
  [ 2] .debug_info       PROGBITS         0000000000000000  00000287
  [ 3] .debug_abbrev     PROGBITS         0000000000000000  000003b2
  [ 4] .debug_line       PROGBITS         0000000000000000  00000451

$ gdb -batch -ex "break summe" -ex run -ex bt -ex "info args" -ex "info locals" \
        -ex next -ex next -ex next \
        -ex "print s" -ex "print i" -ex "print n" -ex "ptype summe" \
        -ex continue /tmp/gdbdemo
Breakpoint 1 at 0x40010e: file docs/gdb_example.fi, line 3.

Breakpoint 1, summe (n=10) at docs/gdb_example.fi:3
3	    var s: i32 = 0
#0  summe (n=10) at docs/gdb_example.fi:3
#1  0x0000000000400257 in main () at docs/gdb_example.fi:11
n = 10
s = 0
i = 0
4	    for i in 1 as i32..n + 1 as i32 {
5	        s = s + i
5	        s = s + i
$1 = 1
$2 = 2
$3 = 10
type = i32 (i32)
[Inferior 1 (process 3538791) exited with code 067]
```

And with `tools/dwarf/probe.fi`, which contains a struct, a pointer and an
array:

```console
Breakpoint 1, shift (p=0x7fffffffeaa8, by=3) at tools/dwarf/probe.fi:12
#0  shift (p=0x7fffffffeaa8, by=3) at tools/dwarf/probe.fi:12
#1  0x0000000000400587 in main () at tools/dwarf/probe.fi:30
$1 = {x = 5, y = 7}          <- print *p
$2 = 5                       <- print p->x
type = struct Point {
    i32 x;
    i32 y;
}
Value returned is $3 = 18    <- finish
p = {x = 8, y = 10}          <- info locals in main
$5 = {1, 2, 3, 4}            <- print field
$6 = 3                       <- print field[2]
type = i32 [4]               <- ptype field
```

What the sessions establish:

* A breakpoint on a **Firn** function name hits and reports the file + line
  of the `.fi` file.
* `gdb` shows the **source text of the `.fi` file**, not assembly.
* `next` steps **line by line** through the Firn program.
* The **backtrace** names the caller with the right line, and the frame line
  carries the **parameter values**.
* `info args`, `info locals`, `print` and `ptype` work — for scalars, for
  **structs with their members**, through **pointers** and over **arrays**.
* `finish` yields the return value with the right type.
* Exit code `067` octal = 55 decimal — the expected result.

To reproduce: `bash tools/dwarf/run.sh`. The script runs the sessions above
and compares the output line by line against these expectations; the
addresses may change with the code generator, the file, the lines and the
values may not.

## Limits (honestly)

* **No variables in the optimized build.** Lines are there since round 94,
  variables are not. See above for the reason; it is a decision, not an
  omission.
* **No lexical blocks.** All variables of a function hang directly under the
  `DW_TAG_subprogram`, not under `DW_TAG_lexical_block`. If the same name is
  declared twice in nested scopes, both entries are there and `gdb` takes the
  later one. Right in most cases, but not by construction.
* **No CFI** (`.eh_frame`). The backtrace works because every function has an
  ordinary `rbp` prologue; in the middle of the prologue (before
  `mov rbp, rsp`) a backtrace would be wrong.
* **Function values and error unions are opaque.** They get a name and a size,
  no members. `print` shows the raw word. A struct-like breakdown would be a
  claim about a layout that the language does not promise.
* **The language number is `DW_LANG_C99`.** Firn is no C, but `gdb` only reads
  out of it how expressions and array indexing are written, and there Firn
  follows C. A number of its own would only make `gdb` fall back to its
  default.
* `ACCEPTANCE.md` item 4 criterion B additionally demands that **a real bug**
  has been found with the debugger. That is still not the case and is still
  listed as open there.
