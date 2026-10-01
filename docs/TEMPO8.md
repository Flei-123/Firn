# Runde TEMPO 8 — Kopien verschmelzen

State 22.09.2026, branch `xmm-ra`. Continuation of TEMPO 7, which ended with a
diagnosis: the distance to C no longer sits in the calculation, but
in the **register pressure** — `synth` has 44 simultaneously live values,
`l3_huffman` 63, with fourteen registers.

This round takes on the allocator. But it begins not with an
idea, but with a count.

## Die Messung, die die Richtung vorgab

`valgrind --tool=callgrind --dump-instr=yes` counts the instructions per ADDRESS.
Together with the disassembly this turns the question "what does the time in
this function go on" into numbers instead of guesses. For `synth` (37.3 of
174.2 M instructions, so 21 % of the whole program):

| Kind | Instructions |
|---|---|
| **register-to-register copies** | **9.73 M** |
| storing to the frame | 4.03 M |
| fetching from the frame | 3.24 M |
| `mulps` | 2.67 M |
| `movdqu` | 2.42 M |

So the biggest item of the hottest function was not calculating,
but **copying**. A look into the hot loop showed what from:

```text
667440  movdqu (%r10),%xmm13
667440  mov    %rbx,%rax            # <-- four running pointers, four instructions each
667440  add    $0x2,%rax
667440  mov    %rax,-0xa18(%rbp)
667440  mov    %r10,%rax
667440  add    $0x8,%rax
667440  mov    %rax,-0xa30(%rbp)
        ...
250290  movaps %xmm0,-0xd60(%rbp)   # <-- two accumulators, two instructions each
250290  movaps -0xd60(%rbp),%xmm14
250290  mov    -0xa18(%rbp),%rbx    # <-- and on the back edge
250290  mov    -0xaa8(%rbp),%r11       everything back again
250290  mov    -0xac0(%rbp),%r9
250290  mov    -0xa30(%rbp),%r10
```

Four instructions for what C does with `add $0x2,%rbx` — times four pointers,
times 667,440 iterations.

## Where this comes from

`phi.rs` resolves every phi node into a **copy at the end of the predecessor**.
`fold_into_definitions` gives this copy back where the incoming value
is calculated IN THE SAME BLOCK — and that is exactly not the case in a loop with
a branch:

```text
bb_head:   i' = i + 2          <- advanced here
           if ... -> bb_a else bb_b
bb_a:      ...
bb_b:      ...
bb_end:    i <- i'             <- the back edge leaves here
           jmp bb_head
```

The copy stands in `bb_end`, the calculation was done in `bb_head`. The four
conditions of `fold_into_definitions` do not apply, and `i'` gets a
slot of its own — in the frame, because all registers are handed out.

The comment head of `phi.rs` has predicted this since round 92: *"When a later
round teaches the allocator to coalesce copies, splitting has to come with
it."* That is this round.

## What was built

For a copy `p = copy t`, **`t` and `p` are put together into ONE interval
before the linear scan runs**. Both thereby get
the same slot; `emit_bin` calculates right there (`load_full` writes nothing
if the value already stands there) and the copy no longer emits an instruction — all
three copy paths in `emit_inst` already check that.

Three conditions, each individually necessary:

1. **`t` is written exactly once and read exactly once**, namely
   by this copy. A second reader would otherwise see a register that the
   back edge has long since turned further.
2. **Same type, no special slot.** An immediate value carries
   `Slot(0)` as a placeholder, a pool value (TEMPO 6) stands in `.rodata`,
   a cell and an `alloca` lie elsewhere. Whoever gives `t` one of these
   "slots" writes into nothing.
3. **No interference (Chaitin):** at no place at which the one is
   written does the other still live. The copy itself is excepted —
   it is the reason for merging.

Several sources per `p` are allowed as long as they do not interfere with one another.
An `if` in the loop body writes the loop variable once in each
branch, and the branches exclude each other — in `synth` that is
three sources per accumulator.

### Die drei Fehlversuche, die die Bedingungen erklaeren

The round needed three attempts, and each taught something:

**First attempt: after the scan, with the condition "lifetime of `t` entirely
within that of `p`".** Sounded right, was too strict: the blocks of an `if` in the
loop body often stand in the linear numbering BEHIND the block with
the back edge, and `t` lives through them — `p` does not, because that is
exactly the precondition. Of the four running pointers all four failed.
Result: 174.2 -> 173.9 M, so nothing.

**Second attempt: state the condition directly** (no other value with the
same register overlaps with `t`, and nothing destroys it) and
replace the coarse destruction mask by the exact one from `exact_crossings` —
`rough` sums over the linear numbering and therefore sees calls that
do not lie on the path of `t` at all. 174.2 -> 162.1 M.

**Third attempt: before the scan.** With the `fp_taugt` barrier (see below)
the accumulators suddenly got an `xmm` themselves — and were thereby withdrawn from
the after-the-fact merging, because a `t` with a register of its own
cannot be moved any more without taking the neighbour's away. The copy
stayed as a `movaps`. Only when the merging happens BEFORE the scan
does `p` carry from the start the lifetime, the weight and the
destroyed registers of both values. Measured: four merged values
after the fact, **twenty-five** before.

## The bug that the round made itself

`tests/1182_layout_float_probe.fi` returned 2 instead of 0 — only with
register allocation, so in three of the four build stages. Guilty was a
single merge in `flow__layout_inline`, and the cause is
instructive enough to stand here:

Merged were `%311 = load.f64 ...` and the phi value `%851`. `%851`
got **no register** (its lifetime crosses calls), so both shared
one FRAME SLOT. That is right in itself — but the
floating-point handover `fp_handover` (round TEMPO 2) sees a value without a
register with exactly one reader and puts it into `xmm2` instead of storing it.
The copy thereupon saw "source and destination are the same slot" and emitted nothing
— while the value stood in `xmm2` and nobody ever wrote it into the frame. The reader fetched from the slot whatever happened to lie in it.

The answer is no longer a special case, but a narrower rule:
**merging happens only if the destination has really got a register.**
Two values on one frame slot bring nothing anyway — measured again: zero
instructions of difference — and every further path that after the fact gives a value without a register
a slot after all (cell alias, cell advance,
handover register) would be the same trap once more.

## A bug that the round uncovered

`fp_taugt` says which value may see an SSE register at all. There
stood

```rust
Op::Bin(..) | Op::Copy { .. } => inst.ty.is_float(),
```

`is_float()` is **wrong** for `FTy::V128` — and `emit_inst` has a branch of its own
for the vector copy that very much does stand in the SSE path. Every value
that a vector copy read thereby lost its register. Since TEMPO 4 that concerned
exactly the two accumulators of the synthesis filter bank: they ran through the frame,
although twelve `xmm` were free.

## Die Zahlen

MP3-Dekoder, 8 s Ton, `release-fast`, `valgrind --tool=callgrind`:

| | Instructions |
|---|---|
| after TEMPO 7 | 174.2 M |
| **after TEMPO 8** | **160.6 M** |
| the same with `--cpu=avx` | **144.7 M** |
| `minimp3` in C, `gcc -O2` | 74.8 M |

And the wall clock, 60 s of sound, smallest of eleven runs, output to
`/dev/null`:

| | Time |
|---|---|
| Firn after TEMPO 7 | 0.18 s |
| **Firn now** | **0.15 s** |
| Firn with `--cpu=avx` | 0.17 s |
| C, `gcc -O2` | 0.07 s |
| the same C without auto-vectorisation | 0.10 s |
| the same C with `-O0` | 0.39 s |

So **2.1x behind `gcc -O2`** and **1.5x** behind the same C if you take away from gcc
the auto-vectorisation. The PCM output over 60 s of sound is
octet-identical with that of `minimp3`.

The hot loop of `synth` now looks like this:

```text
667440  movdqu (%r10),%xmm13
667440  pshufd $0x0,%xmm13,%xmm12
667440  pshufd $0x55,%xmm13,%xmm11
667440  lea    0x2(%rbx),%rbx        # one instruction per running pointer
667440  lea    0x8(%r10),%r10
667440  movdqu (%r11),%xmm13
667440  movdqu (%r9),%xmm10
667440  lea    -0x100(%r11),%r11
667440  lea    0x100(%r9),%r9
        ...
584010  addps  %xmm10,%xmm15         # the accumulator stays in the register
```

After every single step the PCM output is **bit-identical**.

## Geprueft

The full test series (`test.sh`) runs through without a new error: all
test programs in four build stages, 338 of 338 same behaviour in the
self-comparison, and the **fixed point** -- Firn translates itself, stage 2
and stage 3 are character-identical (793,453 lines of assembler). The three red
points of the run (`tools/js/run.sh` fails on a missing
`testdata/test262/subset.sha256`, `tools/fmt/run.sh` on unformatted files
in `lib/fui/`, `tools/english/check.sh` on `nur_hier` from TEMPO 6) are
older than this round and have nothing to do with it.

## Was es kostet

The translation of `bin/firnc1.fi` takes 3.92 instead of 3.76 seconds, so
a good four percent more. The first version was at twelve percent, because
`stoerung` walked the whole function per candidate; now the
write sites stand once in `defsites` and checking happens only there — at every
other place two values cannot become live at the same time at all.

`FIRN_NO_COALESCE=1` switches the round off, `FIRN_COAL_DBG=<name>` says for
every copy of the affected functions why it was merged or
not.

## What would come next

The count of the rejected candidates in the decoder:

| Reason | Count |
|---|---|
| `t` has more than one reader or writer | 217 |
| **merged** | **25** |
| `t` is already assigned elsewhere | 18 |
| special slot | 11 |

The 217 cannot be fetched by merging — there what is needed is what
TEMPO 7 wrote down as point 2: **cutting lifetimes at calls and at
blocks**, so that a value may have a register in one section
and not in the next. That is the next chunk.
