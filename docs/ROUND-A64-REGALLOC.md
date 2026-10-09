# Round BILLIG -- the register allocator for aarch64

Original German write-up (507 lines): commits `77e4a5934` and `0ac99b2d5` in
the archive history (`Flei-123/FirnOld`, branch `sammeln`). English summary of
what is in `compiler/src/regalloc_a64.rs` and the hooks in `codegen_a64.rs`;
the numbers are the ones measured in that round (05.09.2026, qemu-user -- no
ARM machine hangs on this system, so wall clock is qemu and not a phone;
static and executed instruction counts are exact).

## The starting point

`codegen_a64.rs` said in its own header that every FIR value gets a stack
slot -- "that is not fast, and it is not meant to be: it is checkable". x86
got the linear-scan allocator in round 43; this is the "later".

Question asked first: how x86-wired is `regalloc.rs`? Of 5 005 lines about
1 580 were machine independent analysis (`compute_live`, `exact_crossings`,
`promotable_cells`, `loop_depth`, the linear scan), ~150 register tables and
~2 900 x86 ASSEMBLER TEXT. `regalloc.rs` returns no assignment -- `emit_func_ra`
IS the back half. For aarch64 the back half already exists and has exactly ONE
narrow point (`fn at()` in `codegen_a64.rs`, the only `.off(` user). So: a
separate allocator `regalloc_a64.rs` that reuses the three analyses and only
makes them `pub(crate)` in `regalloc.rs`.

## Design

* **Six registers, x19-x24.** The number comes from the collector, not the ABI
  (AAPCS64 would give ten): `emit_gc_addr` writes exactly x19-x24 into the
  state block at every `Op::GcAddr{regs:true}` (`gc::REG_SAVE_OFF`, six words
  -- the same six that x86 fills with rbx/rbp/r12-r15), because SPEC 3.5.3
  promises a conservative scan over stack AND registers. A pointer in x25-x28
  would be invisible and the collector would free a live object. Everything
  handed out is callee-saved, so no interval needs a crossing analysis.
* **Three locks, each with a reason:** `#[interrupt]` (the save area relies on
  x19-x28 never being handed out), `asm` blocks (the A64 inline assembler
  accepts any register name), secret values and floating point / `v128`.
* **Three steps, each measurable:** (1) a value lives in a register: `ldr/str`
  become `mov`; (2) constants that `imm_into` writes in one instruction are
  rebuilt at the use site instead of being held (the first run showed the
  constants 0,1,2,3 holding x20-x23 while the loop counter sat on the stack);
  (3) the register IS the operand where the whole computation is ONE
  instruction that reads all sources before it writes its target, including
  the immediate forms of `add/sub/cmp` -- explicitly not `and/orr/eor`, whose
  immediate field is the bit-mask encoding `N:immr:imms`.

```
   i = i + 1   was:  ldr x9,[sp,#48] / movz x10,#1 / add x9,x9,x10 / str
               is:   add x22, x22, #1
```

## Second finding: the allocator blinds the collector

`tools/aarch64/run.sh --no-opt` reported one difference,
`tests/901_dom_tree_gc.fi`: exit code 3, "too many objects alive after the
collection". Not wrong code -- a PERMANENT ROOT. A promoted cell lives its
whole function in a callee-saved register, those registers are written into the
state block at every save point, and a dead tree root in such a cell keeps its
whole tree alive (4 600 of 4 680 nodes held back). Two repairs were built and
measured (clearing dead registers with `xzr` at the save point and at the
point of death) and both stay in the tree because they are right on their own,
but neither was enough: `Gc[T]` is a NUMBER in FIR, and a number that is a
pointer cannot be told from a number -- the price of a conservative collector.

**Decision: promoted cells stay OFF on aarch64** (`FIRN_A64_CELLS=1` switches
them on for measuring). Everything else stays: values in registers, rebuilt
constants, the register as operand. What the decision costs, measured:

```
                              with cells   without cells
    b3_main static insns        -27.32 %      -24.66 %
    eleven benches static       -19.47 %      -17.11 %
    eleven benches throughput    1.358 x       1.384 x   (better without)
    Certus example               1.42 x        1.45 x
    Certus hackernews            1.18 x        1.29 x
```

## Measured in the archive

* Certus' painter (`lib/paint/b3_main.fi`), aarch64: static instructions
  572 399 -> 416 020 (-27.32 %), binary 2 646 224 -> 2 020 704 (-23.64 %), page
  build under qemu 1.18 x .. 1.42 x, picture BIT-IDENTICAL.
* Eleven benches (`bench/firn/`): static instructions 13 859 -> 11 161
  (-19.47 %), throughput under qemu 1.358 x (geometric mean), results
  identical in all eleven.
* Executed instructions (`qemu -singlestep -d cpu`): pure arithmetic loop
  37 490 -> 26 372 (-29.66 %). Round 43 had measured -26.15 % on x86.
* Correctness: `tools/aarch64/run.sh` (every `tests/*.fi` compiled twice, both
  run, output compared character by character): 304 of 304 comparable cases
  identical, 0 different. The assembly text of x86 builds is BYTE-IDENTICAL
  before and after (checked on four large programs in both stages).

## Files

`compiler/src/regalloc_a64.rs` (allocator), `codegen_a64.rs` (hooks:
`Frame.ra`, `emit_epilogue` restores the saved registers, `Op::Copy` as one
`mov`), `regalloc.rs` (visibility only), `tools/billig/` (benches and the
instruction counter).
