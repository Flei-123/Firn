# Debugger: `.debug_line` and a real `gdb` session

**Requirement:** `W3` · `ABNAHME.md` item 4 criterion B · `TODO-FIRN.md` 0.4
**State:** line numbers work, variables do not yet (see „Limits").

## How it is generated

`compiler/src/dwarf.rs` collects the mapping *instruction → source line*
during lowering; `compiler/src/codegen_x86.rs` writes `.file` and `.loc`
directives into the assembly from it. From those, `as` produces the
sections `.debug_line`, `.debug_info`, `.debug_abbrev`, `.debug_aranges`,
`.debug_str`. **No** external tool and **no** C compiler is used — only the
assembler, which is in the build path anyway.

Precision:

| Build mode | Line information |
|---|---|
| `firnc --no-opt datei.fi` | **statement-precise** — every statement has its source line |
| `firnc datei.fi` (with the optimizer) | the line of the `fn` declaration per function |

The reason for the restriction is in `SPEC.md` §14.1 item 16: the FIR
carries no source positions (`fir.rs` is frozen in this round), and the
optimizer removes instructions and renumbers blocks. A wrong line
would be worse than none.

## Proof: the session, copied verbatim

Program `docs/gdb_beispiel.fi`:

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

Commands (in the project directory, after `cargo build --release`):

```console
$ compiler/target/release/firnc --no-opt -o /tmp/gdbdemo docs/gdb_beispiel.fi
$ readelf -S /tmp/gdbdemo | grep debug
  [ 2] .debug_aranges    PROGBITS         0000000000000000  000000e0
  [ 3] .debug_info       PROGBITS         0000000000000000  00000110
  [ 4] .debug_abbrev     PROGBITS         0000000000000000  0000013e
  [ 5] .debug_line       PROGBITS         0000000000000000  00000152
  [ 6] .debug_str        PROGBITS         0000000000000000  00000192

$ gdb -batch -ex "break summe" -ex run -ex bt -ex "info line" \
        -ex next -ex next -ex next -ex "info line" -ex continue /tmp/gdbdemo
Breakpoint 1 at 0x4000c6: file docs/gdb_beispiel.fi, line 2.

Breakpoint 1, summe () at docs/gdb_beispiel.fi:2
2	fn summe(n: i32) -> i32 {
#0  summe () at docs/gdb_beispiel.fi:2
#1  0x0000000000400254 in main () at docs/gdb_beispiel.fi:11
Line 2 of "docs/gdb_beispiel.fi" starts at address 0x4000c6 <summe> and ends at 0x40010b <summe+69>.
3	    var s: i32 = 0
4	    for i in 1 as i32..n + 1 as i32 {
5	        s = s + i
Line 5 of "docs/gdb_beispiel.fi" starts at address 0x400190 <summe+202> and ends at 0x400202 <summe+316>.
[Inferior 1 (process 536651) exited with code 067]
```

What the session establishes:

* A breakpoint on a **Firn** function name hits and reports the
  file + line of the `.fi` file.
* `gdb` shows the **source text of the `.fi` file**, not assembly.
* `next` steps **line by line** through the Firn program (2 → 3 → 4 → 5).
* The **backtrace** (`bt`) names the caller `main` with the
  right line 11.
* Exit code `067` octal = 55 decimal — the expected result of `summe(10)`.

To reproduce: execute the three commands above one to one. The addresses
may change with the code generator, the file and the lines may not.

## Limits (honestly)

* **No variables.** `print s` does not work: there are no
  `DW_TAG_variable` entries and no type information in the `.debug_info`.
  For that the compiler would have to write the `.debug_info` itself
  instead of having `as` generate it.
* **No lines in the optimized build** apart from the function line.
* `ABNAHME.md` item 4 criterion B additionally demands that **a real
  bug** has been found with the debugger. That is not yet the case and
  is still listed as open there.
