# Round 43 — the last speed goal: realweb below 2x

**Assignment.** The Firn tokenizer was to take at most twice as long as
`html5ever` on the corpus `realweb` (eight real pages, 4,70 MB). The state
at the start of the round: `html5lib` (pathological) had been accepted at
1,33x, `realweb` stood at 2,68x. The next lever noted from rounds 40/41 was
**interval splitting in the register allocator**.

**Result.** The goal has been reached — and at a completely different spot
than the noted lever suggested. Interval splitting was **not** touched in
this round; it did not become necessary. Measurement decided.

| Corpus   | instructions before | after         | change |
|----------|--------------------:|--------------:|----------:|
| realweb  |    1.297.226.150     |  957.989.680  | **-26,15 %** |
| html5lib |    2.481.675.238     | 2.149.257.366 | **-13,39 %** |

| Corpus   | factor before | factor after | goal |
|----------|--------------:|---------------:|-----:|
| realweb  |     **2,58x** |      **1,48x** | <= 2,00x |
| html5lib |     **1,31x** |      **0,99x** | <= 2,00x |

The factors come from `tools/tokenizer/throughput.sh` (best of seven
runs per page), **both states measured immediately one after the other on
the same machine**, so that the known fluctuation of around 30 % does not
enter the comparison. Over three such pairs the starting value lay between
2,58x and 2,85x and the final value between 1,47x and 1,54x — the value
2,68x from round 41 lies in the middle of the scatter band of the same
binary. What is dependable is the throughput: realweb 15,9-17,0 MB/s before
against 29,3-30,6 MB/s after, i.e. around **+80 %**. The instruction counts
above that are reproducible to the instruction and are the actual evidence.

On `html5lib` the Firn tokenizer is thereby **faster than html5ever**.

---

## 1. The tool that was missing

Firn binaries are statically linked and have no dynamic section.
`callgrind_annotate` finds no symbols in them and names every function only
by its start address:

    fn=(748) 0x000000000041d33b

A profile of 900 such lines is useless. But the names are very much present
in `.symtab` — `nm` reads them. New in this round:

**`tools/tokenizer/profile.py <binary> <callgrind-out> [anzahl]`** — reads
the callgrind output, resolves every `fn=` address via the symbol table and
prints self and inclusive costs per function.

Two traps that snapped shut while building the tool and that are named in
the header of the file:

* With `positions: line` the **first** number of a cost line is the
  line number, **not** the address. Whoever reads it as an address gets a
  plausible-looking, completely wrong profile.
* The line immediately after `calls=` is the **inclusive** cost line of the
  call, not the self cost of the calling function.

Without this tool the biggest item of this round would again have gone
undiscovered. That is the real lesson: **a profile that shows no names
is not a profile.**

## 2. The profile before (realweb, 1.297.226.150 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     526.900.669   40,62%      772.044.290  tokenizer__tokenize
     332.932.201   25,66%    1.297.226.146  main
     192.243.336   14,82%      192.243.336  dekodiere
     105.033.364    8,10%      105.787.926  tokens__tok_attr_value_push
      47.342.985    3,65%       47.693.872  tokens__sink_fehler_bei
      18.589.583    1,43%       18.590.675  tokens__tok_attr_name_push
      11.641.019    0,90%       16.293.464  tokens__tok_attr_finish
      10.700.120    0,82%       24.125.970  tokens__tok_emit
```

`main` with 25,66 % was the surprise. Striking: the value is the same to
within 1.000 instructions as on the other corpus
(332.931.511 against 332.932.201), although the inputs differ in size and
content. So **constant work that has nothing to do with the content**.

A second run with `--dump-instr=yes` showed exactly where: **100 % of the
self costs of `main` lie in a single block of 109
instructions**, executed 8.323.079 times, on average 40 instructions per
run. The disassembled block is a byte-wise copy loop.

8.323.079 is not a random number: `8.388.608 - 65.536`. That is exactly the
sum of all recopyings when a buffer grows from 64 KiB to 8 MiB by doubling.

## 3. Hypothesis 1 — reading the input, not the tokenizer

**Hypothesis.** `mem.read_all_stdin` reads the input in 64 KiB chunks and
lets the buffer double in the process. Every doubling calls `mem_copy`, and
`mem_copy` copies **byte-wise**. Together 8.323.072 bytes at 40
instructions each.

**Why 40 instructions for one byte?** Because `main` was compiled over the
base path without register allocation (see hypothesis 2). Every
intermediate value went through a stack slot:

```
mov  rcx, QWORD PTR [rbp-0xd50]     ; pointer to the cell of i
mov  rax, QWORD PTR [rcx]           ; i
mov  QWORD PTR [rbp-0xd68], rax     ; -> intermediate slot
mov  rax, QWORD PTR [rbp-0xd68]     ; <- straight back again
...
movzx eax, BYTE PTR [rcx]           ; read one byte
mov  BYTE PTR [rcx], al             ; write one byte
```

**Is that even a permissible target?** Yes, and the question has to be
asked. The reported factor measures the runtime of the whole program, and
the other side (`bench/tokenizer`, html5ever) reads its file with
`read_to_tendril` in one go. The previous factor was therefore, for close
to a third, a measurement of the Firn memory layer and not of the
tokenizer. **Honestly named: this part of the gain does not make the
tokenizer faster — it clears away a measurement error in favor of
html5ever.** The tokenizer itself becomes faster through hypothesis 2 and
the earlier rounds.

**Implementation.** `mem_copy` copies eight bytes per pass as long as a
full word fits into `n`; the rest byte-wise. Both areas are at least `n`
bytes large, so an access at `i + 8 <= n` lies entirely inside, and
x86-64 permits unaligned 8-byte accesses. Copying still goes
forward; overlapping areas with `dst > src` were not permitted before
either.

**Measurements.**

| Corpus   | before        | after         | change |
|----------|--------------:|--------------:|----------:|
| realweb  | 1.297.226.150 | 1.008.764.928 | -288.461.222 (-22,24 %) |
| html5lib | 2.481.675.238 | 2.185.519.179 | -296.156.059 (-11,93 %) |

`main` falls from 332.932.201 to 45.786.357 self costs. Output
unchanged (realweb 187473 tokens / 1 job, html5lib 8511 / 1).

**Conclusion.** Confirmed. The second-largest item of the measuring run was
a byte-wise `memcpy` while reading the input.

## 4. Hypothesis 2 — six functions without register allocation

**Observation.** The code in `main` did not look like the code in
`tokenize`: `tokenize` uses `rbx`, `r12`–`r15`, `main` only `rax`/`rcx`.
That is the handwriting of the base path in `codegen_x86.rs` — the
register allocator was not responsible for `main` at all.

**Why not?** Up to then `regalloc::supported()` only returned a mute
`false`. New: `unsupported_grund()` names the reason, visible with
`FIRN_RA_WARN=1`:

```
RA base path: tokens__out_word            -- 10 parameters
RA base path: tokens__tok_emit            -- a call with 10 arguments
RA base path: tokens__sink_flush_chars    -- a call with 10 arguments
RA base path: tokens__sink_end            -- a call with 10 arguments
RA base path: tokens__out_error_list      -- a call with 10 arguments
RA base path: main                        -- a call with 10 arguments
```

**A single signature** — `tokens.out_wort(s, a, b, c, d, e, f, g, h, i)` —
threw six functions out of register allocation, among them `main` with
its 25 % and the entire output chain of the sink. The register path could
only do System V up to the sixth argument; everything beyond that went to
the base path, which has long mastered it.

**Implementation** (`compiler/src/regalloc.rs`, word for word like the base
path):

* **Prologue.** Parameters from the seventh on come from the frame of the
  caller (`[rbp+16]`, `[rbp+24]`, …). They are fetched **only after** the
  parallel register moves: otherwise their target register could overwrite
  a source that is still needed. `rax` is never the home of a value and
  serves as intermediate storage.
* **Call.** Arguments from the seventh on are placed **first**
  (`sub rsp, raum` + `mov qword ptr [rsp+k*8], rax`), and only then are the
  argument registers filled — then the construction of the argument list
  can no longer destroy a value that is still needed. `raum` is rounded up
  to 16 so that `rsp` stays aligned at the `call` boundary; afterwards
  `add rsp, raum`. The sources are rbp-relative or registers and remain
  untouched by `sub rsp`.

**Measurements** (building on hypothesis 1).

| Corpus   | before        | after        | change |
|----------|--------------:|-------------:|----------:|
| realweb  | 1.008.764.928 |  965.887.079 | -42.877.849 (-4,25 %) |
| html5lib | 2.185.519.179 | 2.150.834.784 | -34.684.395 (-1,59 %) |

Self costs of the affected functions (realweb):
`main` 45.786.357 -> 11.448.891 (-75 %), `tok_emit` 10.700.120 -> 5.198.342
(-51 %), `sink_flush_chars` drops out of the top twelve.
`self_compare.sh` now reports „CODEGEN FEHLT: 0".

**Safeguarding.** This change touches the calling convention — the area in
which a mistake does not show up but surfaces three rounds later as a
miscompile. Therefore:

* `tests/331_stack_args.fi` (new): seven arguments (**one**
  stack word, i.e. with padding — the case that breaks the 16-byte
  alignment if you forget it), ten arguments (four stack words), one
  stack argument that itself comes from a call, and recursion with a
  **further call after** the stack call — a shifted `rsp` would otherwise
  only show up there. Every argument has its own decimal place in the
  result, so that a swapped order comes to light as well.
  `test.sh` compiles every file at three optimization levels; with
  `--no-opt` the function goes over the base path anyway because of the
  debug lines — the same test thereby checks **both** paths against each
  other.
* Two module tests converted (`zu_viele_parameter_gehen_an_den_grundpfad`
  becomes `viele_parameter_bleiben_im_registerpfad`, plus
  `aufruf_mit_acht_argumenten_legt_zwei_auf_den_stapel`).

**Conclusion.** Confirmed. The fallback to the base path was expensive and
was caused by a single signature.

## 5. Hypothesis 3 — the address offset belongs in the memory access

**Observation.** The generated assembly has everywhere

```
lea r8, [r8+160]
mov r8, qword ptr [r8]
```

although x86-64 can do the offset itself: `mov r8, qword ptr [r8+160]`.
Statically, 1.589 of the 2.478 `lea rX,[rY+k]` in the tokenizer are directly
followed by an access via `rX` — at first sight 3,6 % of all lines.

**Implementation.** `faltbare_versaetze()` collects the `ptradd` values
whose address is only needed in the immediately following access before the
emission; the `lea` is then dropped entirely, and load/store write
`[base+k]`. Can be switched off with `FIRN_NO_FALTUNG=1`.

**The conditions are deliberately narrow**, because every relaxation
lengthens the lifetime of the base — exactly the class from round 41.
Folding only happens if the offset is an immediate constant
`0 <= k <= i32::MAX`, the result is read **exactly once** (terminators
counted), this one reader is the **immediately following** instruction of
the same block, and the base lies in a register and is neither a frame
address nor a promoted cell nor a cell alias. The point in time at which
the base is read thereby shifts by exactly one instruction; nothing lies in
between. At the new spot only two registers are written: the target of the
access (which reads its address first — `mov r9, qword ptr [r9+8]` is
correct) and the home of the skipped `ptradd`, which is no longer written
at all.

**Measurements.**

| Corpus   | before        | after        | change |
|----------|--------------:|-------------:|----------:|
| realweb  |   965.887.079 |  957.989.680 | -7.897.399 (-0,82 %) |
| html5lib | 2.150.834.784 | 2.149.257.366 | -1.577.418 (-0,07 %) |

**Conclusion.** Confirmed, but small — and the static counter **lied**.
Of 1.589 supposed spots, 195 were folded
(`lea` 2.898 -> 2.703, 239 accesses with an offset). The rest fails on the
read-once condition, and the reason is structural: the most frequent form
is an address that serves **several fields of the same struct** —
`lea r13,[r14+8]`, then four times `[r13]`. To fold those would mean
lengthening the lifetime over several instructions. Without new
safeguarding that is exactly the bug from round 40. Noted as an open point.

Maxim for the next round: **static frequency is not an estimate of the
gain.** Here it was too optimistic by a factor of eight.

## 6. What was NOT done — and why

### Interval splitting (the noted lever)

Not touched. After hypotheses 1 and 2 the goal was reached; every further
change to the allocator would have meant risk without need. The warning
from round 41 (the cell alias that produced wrong code for months because a
lengthened lifetime remained unknown to the allocator) still applies: a
change that splits lifetimes has to be safeguarded against exactly that
class. That is work for a round that needs it.

### Peephole for narrow reloads — examined and deferred

`deskriptor_peephole` today only deletes 64-bit reloads from **value
slots**. An evaluation of the base binary with the realweb weights found
1.054 further spots of the form „store and immediately load back into the
same register" with 90.200.508 Ir (6,95 %) together. Of those, however,
around 50 million lay in exactly the copy loop that hypothesis 1 removed;
around 40 million (~4 %) remain.

Deferred, because the remaining cases **cannot be deleted safely**:
they concern narrow widths.

```
mov   BYTE PTR [rbp-0xae1], r11b
movzx r11d, BYTE PTR [rbp-0xae1]      ; superfluous ONLY when r11
                                      ; is zero-extended already
```

`mov eax, DWORD PTR [X]` zeroes the upper 32 bits of `rax`; deleting it
would only be correct with carried-along „from which bit on is the register
guaranteed to be zero" information. Feasible, but a textual post-pass with
half-knowledge about register contents is exactly the kind of construction
that produced the miscompile in round 40. Not without need.

Besides, the lion's share of these pairs sits at **block boundaries**
(`mov BYTE PTR [X], r11b` / label / `movzx r11d, BYTE PTR [X]`), i.e. at a
real confluence of two paths — that is a phi in memory form and
belongs in `mem2reg`, not in a post-pass. 1.022 of the 3.427 labels in the
tokenizer (29,8 %) are, however, **not a jump target at all**; they clear
the state of the post-pass without reason. That is a clean, small lever for
the next round.

### Range check per byte in `dekodiere`

`dekodiere` costs 192.243.336 Ir for 4.931.819 bytes = 39 instructions per
byte, although the ASCII fast path only has to load, compare and write.
Part of that is the range check in `byte_bei`, which the caller actually
knows already. Not touched: `dekodiere` stands word for word in
`tokenize_bench.fi` and `tokenize_main.fi`, a change has to hit both
and preserve their semantics on truncated input exactly (bytes beyond the
end read as 0). Worthwhile, but not needed for the goal.

## 7. Profile after (realweb, 957.989.680 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     526.801.688   54,99%      754.292.482  tokenizer__tokenize
     192.243.334   20,07%      192.243.334  dekodiere
     100.031.790   10,44%      100.144.102  tokens__tok_attr_value_push
      47.342.985    4,94%       47.469.496  tokens__sink_fehler_bei
      17.682.777    1,85%       17.683.869  tokens__tok_attr_name_push
      11.448.890    1,20%      957.989.676  main
      11.439.550    1,19%       16.091.995  tokens__tok_attr_finish
```

`tokenize` with 54,99 % is now clearly the only large item:
526.801.688 Ir over 4.917.779 code points are **107 instructions per
character**. The next lever is in there, and it is still the one from
round 40 — too few registers for a function with a frame of
43.104 bytes.

For comparison with the starting point: `main` has fallen from 332.932.201
to 11.448.890 (-96,6 %), and the entire output chain of the sink
(`tok_emit`, `sink_flush_chars`, `sink_end`) has roughly halved.

## 8. Acceptance

| Check | Result |
|---|---|
| `bash ./test.sh` | **PASS 676/676** (base 673/673; +3 from `tests/331_stack_args.fi` at three optimization levels) |
| `cargo test --release` (module tests) | 142/142 |
| `bash tools/self_compare.sh` | **197 identical behavior, 0 differing, 0 failing** (base 196; +1 from the new test file), CODEGEN FEHLT: 0 |
| `bash tools/fixpoint.sh` | **stage 2 == stage 3, character-identical, 309.468 lines** |
| `bash tools/tokenizer/run.sh` | **6810/6810 = 100,00 %** |
| lexer/parser/layout/sema/FIR comparison | unchanged (1 known and named deviation each, layout 0) |

Measuring tools of this round: `tools/tokenizer/profile.py` (new),
`.r43/messe.sh` (working directory, not checked in),
`tools/tokenizer/throughput.sh`, `valgrind --tool=callgrind`.

## 9. Open points

1. **`tokenize` with 107 instructions per character.** The frame is
   43.104 bytes large, there are nine allocatable registers. Interval
   splitting (rounds 40/41) is still right here — now with a
   clear share: 54,55 % of the measuring run.
2. **Delete labels without a jump target** (1.022 of 3.427). Costs
   nothing, lengthens the reach of every post-pass and is trivial to
   demonstrate.
3. **Narrow reloads** (~4 %) — needs zero-extension tracking in the
   post-pass or, better, a `mem2reg` that promotes bool cells at
   confluences.
4. **`dekodiere`** (19,90 %): range check per byte, to be changed in both
   drivers at the same time.
5. **`out_wort` with ten parameters** is no longer expensive now, but
   still a signature that passes characters through one by one. A
   string literal would be cheaper and more readable.
6. **Fold the address offset over several uses** (hypothesis 3): the
   most frequent unused form is a base address for several fields
   of the same struct. Needs safeguarding against the round 40 class —
   most likely by telling the allocator about the lengthened lifetime
   BEFORE allocation, instead of slipping it to it afterwards.
