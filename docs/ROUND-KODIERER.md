# Round KODIERER -- Firn writes its own machine octets

Original German write-up (518 lines): commit `7943a9b1d` in the archive history
(`Flei-123/FirnOld`, branch `sammeln`, 05.09.2026). This is the English summary
that goes with the code on main; the numbers are the ones measured in that
round on the tree of that day.

## The answer first

Before this round the way of the compiler to the machine ended at a *string*:
`codegen_x86.rs` and `codegen_a64.rs` return assembler text, and no place in
the tree wrote an opcode octet. Now Firn can produce machine code as octets
for x86-64 **and** ARM64, and the result is checked against GNU `as`:

```
                              x86-64          ARM64
   translation units            1 023          1 008
   bit-identical to `as`        1 023          1 008     (100 %)
   octets compared        201 743 532    282 990 878
   relocations compared     2 177 621      2 702 497
   deviations                       0              0
   skipped                          0             15   (sources that do not build for aarch64-linux)
```

Plus the test of the whole: **314 of 314** test programs, built once through
`as` and once through the own encoder, run with the **same exit code and the
same output**. The study estimated 2 500-5 000 lines per architecture; it became
6 934 lines for both together, because the code generator uses only a small
part of the machine.

**Scope** (counted before anything was encoded): 4 632 266 instruction lines
from the whole tree, **86 mnemonics in 232 operand forms**; the first fifteen
forms cover three quarters of all instructions.

## Layout (three layers, deliberately separate)

* `x86enc.rs` / `a64enc.rs` -- the **pure encoders**: one instruction in, its
  octets out (opcode, REX, ModRM, SIB, displacement, immediate). No labels, no
  sections, no files, no I/O -- which is why a JIT could use them unchanged.
* `asm_x86.rs` / `asm_a64.rs` -- the **assembler**: parses exactly the text the
  code generators write, manages sections, labels and symbols, does the jump
  relaxation and collects the relocations.
* `elfobj.rs` -- the **ELF writer** (`ET_REL`, section and symbol order as the
  ABI demands, `.bss` as an empty `NOBITS` section).
* `asm_intern.rs` -- the process-wide switch `--asm-intern`. The default stays
  `as`; `--object-only` turns a finished `.s` file into an object file without
  the language front end (that is how `tools/kodierer/compare.py` sends the same
  text through both ways).

Why through the text and not straight from FIR: it gives a **counter check with
millions of real instructions**. Whoever lets both ways loose on the same input
can compare octet by octet; an encoder fed directly from FIR would have to be
believed.

## The traps that silently produce wrong code

1. **rsp/r12 as base** (`rm & 7 == 4`) cannot be written in ModRM -- a SIB with
   `index=100` (= none) must follow.
2. **rbp/r13 as base with displacement 0** needs `mod=01` with a displacement
   byte -- `mod=00` means RIP relative there.
3. **spl/bpl/sil/dil** need a REX prefix, even an empty `0x40`; without it the
   same numbers mean ah/ch/dh/bh.
4. **Immediates are cut to the operand width first** and only then is the short
   form chosen (`and r8d, 0xFFFFFFFC` is `-4` and fits `83 /4 fc`).
5. **What `as` does that cannot be guessed:** `jmp` to a *global* label in the same
   section is resolved directly, `call` to the same label always gets
   `R_X86_64_PLT32`; relocations against a *local* label in another section go
   against the **section symbol** with the label's offset as the addend.
6. **ARM64:** logical immediates are *patterns* (`N:immr:imms`), `#5` is not
   encodable; shifted immediates are 12 bits optionally `lsl #12`; load
   displacements are **scaled** (and `q` loads scale by sixteen although the
   instruction says `size=00`), otherwise the unscaled `ldur` form is needed.

All of them are handled explicitly and checked in `tools/kodierer/`.

## What it brings at once

Measured with `firnc --timings` on `lib/js/parse_main.fi`, best of several runs:
as + ld 255.1 ms -> 155.8 ms, the whole compile 757.3 -> 660.5 ms (-12.8 %). The
assembler step alone: `as` 246 ms against 162 ms for the own encoder (1.52 x).
`ld` costs 8 ms of 757 -- a linker of our own would be the most expensive item
with the smallest return. The text is still written, read and parsed; letting
`codegen_x86.rs` emit `x86enc::Inst` directly would save an estimated further
100-150 ms of 660 (an estimate, not a measurement).

The real gain is a dependency: with the encoder `as` is no longer needed. What
is still missing to drop it entirely is `.debug_line`; as long as a build wants
debug information the default path keeps `as`.

## What this round does NOT show

* Nothing about JIT -- no run-time code, no executable pages.
* Nothing about DWARF: only `.text`, `.data`, `.rodata`, `.bss` are compared.
* The order of the relocations is compared as a *set*; the order of sections and
  symbols differs from `as` (the *content* that counts when linking is compared).
* ARM64 was only checked against `as`, not run on real hardware (x86-64: 314/314
  run test).

## Tools

`tools/kodierer/run.sh` (the acceptance over the whole tree, `--a64`, `--quick`,
`--only <file>`), `compare.py` (one file, octet by octet, with the instruction
at the first deviation), `end_to_end.sh` (build both ways and run),
`default_unchanged.sh` (the default path is octet-identical to an untouched
compiler), `measure.sh` (compile time with and without `as`).

## Update 28.10.2026: integrated assembler is the x86-64 Linux default

* `--asm-intern` is now the default for `x86_64-linux`; `--asm-as` or `FIRN_USE_AS=1` selects `as`. Windows, aarch64 and wasm are unchanged.
* The assembler text is no longer written to a file and read back; it is assembled from memory (`--keep-asm` still writes it).
* `asm_dwarf.rs` writes `.debug_line`, `.debug_info`, `.debug_abbrev` and `.debug_aranges` byte-identical to `as` (DWARF 3 line program, same special-opcode choice); only `.debug_str` differs (producer name).
* `compare.py` checks the debug sections for x86-64; `FIRN_ASM_LENIENT=1` lists every unsupported line in one run; `FIRN_STAGE_TIMINGS=1` prints parse and layout times.
* Measured on bin/firnc1.fi (release-fast, host load 25-32): `as + ld` 2.9 s -> 1.1 s; assembler alone on its 16 MB text: `as` 1.9-2.5 s, own 1.0 s (parse+encode 0.84 s, layout 0.24 s).
