# Round 51 — realweb below 1,3× (measured in instructions)

Base: `cc1710f` (merge of rounds 46/47/48). Branch `r51-tempo`.
Territory: optimizer, register allocation, code generation, tokenizer
measuring run.

**Assignment.** Keep the tokenizer comparison against `html5ever` on the
corpus `realweb` safely below 2× and push further, target mark **≤ 1,3×**,
measured in **instructions with callgrind** — not with the wall clock.

**Result.** Seven changes, each measured individually, together
**−26,99 %** on `realweb` and **−23,43 %** on `html5lib`.

| Corpus   | instructions before | after         | change |
|----------|--------------------:|--------------:|----------:|
| realweb  |        957.989.680   |  699.459.494  | **−26,99 %** |
| html5lib |      2.149.257.366   | 1.645.729.694 | **−23,43 %** |

| Corpus   | factor before | factor after | goal |
|----------|--------------:|---------------:|-----:|
| realweb  |     **1,772×** |     **1,294×** | ≤ 1,30× ✅ |
| html5lib |     **1,034×** |     **0,791×** | ≤ 1,30× ✅ |

All seven changes sit in the **compiler**, none in the tokenizer. The
gain therefore holds for every Firn program, not only for the measuring
run.

---

## 0. What „factor" means here — and why it is a different value than in round 43

Round 43 determined the factor with `tools/tokenizer/throughput.sh`, i.e.
with the **wall clock** (1,54× realweb). This round computes it from
**instruction counts**:

```
valgrind --tool=callgrind --cache-sim=no --branch-sim=no  <binary>
```

* Firn:      `.tokenizer-work/tokenize_bench < korpus.<k>.auftrag`
* html5ever: `bench/tokenizer/target/release/html5ever_bench korpus.<k>.html`

Measurements of the other side (unchanged, same machine):

| Corpus   | html5ever, instructions |
|----------|-------------------------:|
| realweb  |              540.567.228 |
| html5lib |            2.079.365.558 |

The instruction factor is **stricter** than the wall clock factor: 1,772×
against 1,54× at the same state. So Firn executes more instructions than
html5ever and still needs less time per instruction. Both numbers are
honest, they only measure different things. This round prints the
instruction count because it is reproducible to the instruction — the wall
clock scattered on this machine between 2,58× and 2,85× **for the same
binary** (docs/ROUND43.md). During this round two further rounds ran in
parallel on the same machine; wall clock values are therefore **not** cited
as evidence at all.

**New tool: `tools/tokenizer/patterns.py`.** `profil.py` (round 43)
answers „which FUNCTION costs?". The new file answers the question
next to it — „which FORM of code costs?": it combines `objdump` with the
instruction-precise callgrind output (`--dump-instr=yes`) and weights
instruction **patterns** by their real execution counts. Invocation:

```sh
objdump -d --no-show-raw-insn .tokenizer-work/tokenize_bench > dis.txt
valgrind --tool=callgrind --dump-instr=yes --cache-sim=no --branch-sim=no \
         --callgrind-out-file=cg.out .tokenizer-work/tokenize_bench < auftrag
python3 tools/tokenizer/patterns.py dis.txt cg.out
```

Plus, in the working directory (not checked in), `.r51/messe.sh`, which
runs callgrind on both corpora and records the instruction count.

**The lesson from round 43 proved itself again**, this time in the other
direction: static frequency says nothing, but a *dynamically weighted
pattern* says a great deal. The three largest items of this round were on
the table after 20 minutes of tool building:

```
jmp direkt hinter jcc (Blocklayout)          29.258.200 Ir   3,05%    614 Stellen
setcc-Kette statt direktem Sprung           164.130.198 Ir  17,13%    345 Stellen
Store+Reload derselben Zelle                 98.393.560 Ir  10,27%    294 Stellen
```

---

## 1. The profile before (realweb, 957.989.680 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     526.801.688   54,99%      754.292.482  tokenizer__tokenize
     192.243.334   20,07%      192.243.334  dekodiere
     100.031.790   10,44%      100.144.102  tokens__tok_attr_value_push
      47.342.985    4,94%       47.469.496  tokens__sink_fehler_bei
      17.682.777    1,85%       17.683.869  tokens__tok_attr_name_push
      11.448.890    1,20%      957.989.676  main
```

By mnemonic: **51,7 % of all executed instructions were
data movements** (`mov`/`movzx`/`movabs`; the plain `mov` alone 43,2 %),
among them **129.637.675 Ir of frame loads (13,53 %)** and **126.722.224 Ir
of frame stores (13,23 %)**. The tokenizer therefore spent more than half
of its instructions shoving values back and forth, and more than a quarter
of them between registers and the stack frame alone.

---

## 2. Measurement table — seven changes, seven measurements

| # | Change | realweb Ir | Δ | html5lib Ir | Δ |
|---|---|---:|---:|---:|---:|
| — | base `cc1710f` | 957.989.680 | — | 2.149.257.366 | — |
| H1 | jump threading through bool cells (`thread-bool`) | 790.898.007 | **−17,44 %** | 1.824.956.636 | **−15,09 %** |
| H2 | `switch` value from the register, no `mov eax, eax` | 775.569.867 | −1,94 % | 1.818.386.969 | −0,36 % |
| H3 | block layout along traces | 747.247.528 | **−3,65 %** | 1.758.320.039 | −3,30 % |
| H4 | no `xor eax, eax` on a `void` return | 743.411.513 | −0,51 % | 1.756.522.359 | −0,10 % |
| H5 | descriptor post-pass with zero-extension tracking | 733.566.422 | −1,32 % | 1.748.021.433 | −0,48 % |
| H6 | full x86 addressing `[basis+index*faktor+k]` | 712.941.773 | **−2,81 %** | 1.683.365.701 | −3,70 % |
| H7 | address folding with base/index from the frame as well | 699.459.494 | −1,89 % | 1.645.729.694 | −2,24 % |
| | **total** | | **−26,99 %** | | **−23,43 %** |

The output stayed the same at every step: realweb 187.473 tokens / 1 job,
html5lib 8.511 / 1.

---

## 3. H1 — jump threading through bool cells (`compiler/src/threading.rs`, new)

**Observation.** 17,13 % of all instructions sat in chains of the form

```
setb   %al                       ; make a bool out of it
movzbl %al,%r11d
mov    %r11b,-0xae1(%rbp)        ; write it into a cell
movzbl -0xae1(%rbp),%r11d        ; fetch it straight back out
test   %r11b,%r11b
je     ...
```

**Cause.** FIR has **no phi nodes** — an explicit invariant.
The short-circuit operators `&&` and `||` therefore have to merge their
result over an `alloca`. From `if c0 < 0x80 && c0 != 13` becomes:

```text
bbA: %1 = cmp.lt %c, 128 ; store.bool %1, %zelle ; brcond %1, bbB, bbJ
bbB: %2 = cmp.ne %c, 13  ; store.bool %2, %zelle ; br bbJ
bbJ: %3 = load.bool %zelle ; brcond %3, bbT, bbE
```

`mem2reg` cannot resolve this cell (two writes, no phi),
and the existing fusion of `cmp`+`jcc` in `regalloc.rs` does not apply,
because the `store` stands between the comparison and the terminator.

**Implementation.** A new pass `thread-bool` threads the edge past the
confluence. A **switch block** consists of exactly one instruction
`%v = load.bool %zelle` and ends with `brcond %v, T, E`. A predecessor that
executes `store.bool %x, %zelle` immediately before its terminator already
knows the content of the cell on this edge:

* `br J`            → `brcond %x, T, E`
* `brcond %x, A, J` → `brcond %x, A, E`  (on the J edge `%x` is false)
* `brcond %x, J, B` → `brcond %x, T, B`

After that the switch block is unreachable (`dce`), the cell is no longer
read anywhere (`mem2reg::remove_dead_stores` clears away the `store` and
the `alloca`), and the comparison stands immediately before the terminator
again — the existing fusion turns that into `cmp` + `jcc`. Seven
instructions become two.

**Why this is not the class of bug from rounds 40/41.** There, a lifetime
was stretched across a `call` boundary without the register allocator
knowing about it. Here **no new lifetime beyond a block** comes into being:
`%x` is already an operand of the `store` in the same block, and the
terminator reads it one instruction later — and `Term::BrCond` belongs to
the lifetime analysis of the allocator anyway. Additionally safeguarded:

* between the `store` and the terminator no instruction with a memory
  effect may stand (`store`, `call`, `syscall`, `copymem`, `atomicadd`,
  `securezero`),
* the cell has to be an `alloca` whose pointer does **not escape**,
* `secret` values and `#[constant_time]` functions remain untouched
  (SPEC §9.2),
* the `store` and the `alloca` stay; only the existing pass for dead
  stores removes them. The pass is therefore debug-preserving.

Eight module tests in `threading.rs` cover that, among them „a call between
`store` and jump blocks it", „a foreign `store` in between blocks it",
„a cell whose pointer escapes", „`constant_time`", „a secret value" and
„a second run changes nothing more" (fixpoint).

**Effect in the binary:** the patterns „setcc chain" fell from 164.130.198
Ir to 847.126 Ir.

## 4. H2 — the `switch` value came via the frame

**Observation.** The state dispatch of the tokenizer — once per character,
5.109.380 iterations — looked like this:

```
mov %r12d,%r9d          ; the state out of the cell
mov %r9,%rax
mov %rax,-0x260(%rbp)   ; only so that emit_switch finds it
mov -0x260(%rbp),%eax   ; and straight back out again
cmp $0x48,%eax
```

`codegen_switch::emit_switch` could only read the value from the frame; the
register path therefore had to write it there first.

**Implementation.** `emit_switch` gets a `Wertquelle`: either `Rahmen(fr)`
(base path) or `Geladen(f)` — in which case the caller loads the value into
`rax` itself. In addition the `mov eax, eax` in front of the jump table is
dropped: on x86-64 **every** write to a 32-bit register zeroes the upper 32
bits, and every branch of `load_ext` writes `eax`. To be safe, the register
path checks that the value is not itself in `rax` (`rax` is never handed
out).

## 5. H3 — block layout along traces

**Observation.** 614 spots, 28.414.304 Ir (3,66 %), looked like this:

```
cmp  -0x18(%rbp),%r8
jae  40dbd4          ; then
jmp  40dbe0          ; else -- could have been a fallthrough
```

The blocks were emitted in their FIR numbering; if neither `then` nor
`else` happened to be the next block, every conditional jump cost a
second, unconditional one.

**Implementation.** `emissionsreihenfolge()` lays out greedy traces:
starting at `bb0`, follow the preferred successor as long as it is still
free; when the trace breaks off, continue at the smallest block not yet
placed. `else` is preferred — `emit_block` inverts the condition itself if
`then` follows instead.

That affects **the output only**. Liveness analysis,
intervals and register choice still work on the FIR order; every
block has an explicit terminator, and a jump is only dropped if
its target really follows immediately. Can be switched off with
`FIRN_NO_LAYOUT=1`.

**Effect:** the pattern fell from 28.414.304 Ir to 10.021 Ir; `jmp` in
total from 46.639.178 to 18.316.839 Ir.

## 6. H4 — `void` needs no `xor eax, eax`

Every function with return type `void` set `rax` to zero before the
epilogue. System V leaves `rax` undefined in this case, and in FIR nobody
reads the result of a void call (`Op::Call` without `dst`). With 4.229.623
calls in the measuring run that is one instruction per call for nothing.
Deleted in both paths (base path and register path).

## 7. H5 — the descriptor post-pass learns zero extension

Round 43 had explicitly **deferred** this item (§6): narrow
reloads are only superfluous if the register is „already zero-extended",
and without that information deleting them would be exactly the kind of
construction that produced the miscompile in round 40.

The missing information is a property of x86-64: **every
write to a 32-bit register zeroes the upper 32 bits.** The post-pass
now carries `nullab[r] = k` („from bit k on, `r` is guaranteed to be
zero"):

* `movzx r32, byte ptr …` → 8, `movzx r32, word ptr …` → 16,
* every other write with a 32-bit target → 32,
* everything else, `call`, `syscall`, `div`, `setcc`, block boundaries →
  unknown.

In addition the post-pass remembers the **width** of every store. A reload
is deleted only if the target and source registers are the same, the stored
width suffices and `nullab` establishes the extension. `movsx`/`movsxd`
stay out of it.

In the hot path of `dekodiere` two of 29 instructions per byte disappear
because of this:

```
movzbl (%rcx),%eax
mov    %rax,-0x658(%rbp)
movzbl -0x658(%rbp),%eax   <- dropped (rax is <= 0xFF already)
mov    %rax,-0x660(%rbp)
mov    -0x660(%rbp),%eax   <- dropped (rax is zero-extended already)
```

## 8. H6/H7 — finally using x86 addressing in full

Round 43 had only folded the **constant offset** into the memory access
(`faltbare_versaetze`). The dynamic measurement showed what was left over
next to it:

| Pattern                                  |          Ir | Share |
|------------------------------------------|------------:|-------:|
| `shl k` + `lea (b,i,1)` + access         |  28.840.310 |  3,93 % |
| `lea (b,i,1)` + access                   |  16.231.553 |  2,21 % |
| `lea off(b)` + access                    |  14.432.184 |  1,97 % |

`faltbare_versaetze` became `faltbare_adressen`, which produces the full
x86 operand `[basis + index*faktor + versatz]`. From

```
mov  r8, qword ptr [rbp-416]
shl  r8, 2
lea  r8, [r9+r8]
mov  r8d, dword ptr [r8]
```

becomes

```
mov  r8, qword ptr [rbp-416]
mov  r8d, dword ptr [r9+r8*4]
```

**H7** additionally takes in the cases in which the base or the index lies
**in the frame**. Then all that remains of the address computation is the
filling of its target register, which is there anyway (`vorlader`) — one
instruction instead of two or three.

**The conditions are kept narrow**, because every relaxation lengthens the
lifetime of the base:

* address-forming is `ptradd` or a **64-bit** `add` — with 32 bits the
  addressing would not truncate the overflow that FIR demands;
* the result is read **exactly once**, and this reader is the
  **immediately following** `load`/`store` of the same block;
* scaling is a 64-bit `shl` with 0..3 resp. `mul` with 1/2/4/8, stands
  **immediately before it** and is likewise read exactly once;
* base, index and scaling are neither a frame address nor a promoted cell
  nor a cell alias nor `secret` (SPEC §9.2);
* if a register is preloaded, it must not at the same time be the index or
  the stored value — this case is checked explicitly, because the allocator
  may reuse registers of values with touching intervals.

With that, the point in time at which the base and the index are read
shifts by exactly the one or two instructions that are **dropped
entirely**; afterwards nothing lies in between any more, in particular no
`call`. Can be switched off with `FIRN_NO_FALTUNG=1`.

**New test `tests/332_adressierung.fi`** (in three build stages, return
77), built against exactly the warning from the round's brief:

* every scaling 1/2/4/8 (u8/u16/u32/u64), reading and writing,
* the same address read twice (must not be folded),
* **calls in the middle of the chain** base → index → access, with a
  function that writes all six argument registers,
* an index that itself comes from a call,
* a chain in which the result of an access is immediately the index again
  (target and index registers collide).

In the tokenizer this produces **419 scaled memory operands**; `lea`
falls from 78.473.907 to 55.831.640 Ir (−28,9 %) and the number of static
`lea` spots from 2.697 to 1.010.

---

## 9. Rejected — with numbers

### 9.1 Larger inlining limits: **refuted**

`tok_attr_value_push` costs 37 instructions per call, of which 13 are pure
frame management, and it is called 2.500.787 times. The obvious hypothesis:
the limits of the inliner (`MAX_CALLEE_INSTS = 40`, `MAX_CALLEE_BLOCKS = 8`)
lock out exactly the small hot functions, because they have themselves
grown through earlier inlining (`cp_push` moves into
`tok_attr_value_push`, which thereby slips over the limit).

Measured on the state after H3:

| Limits | realweb Ir | binary | compilation |
|---|---:|---:|---:|
| 40 / 8 (base) | 747.247.528 | 206.904 B | 1,4 s |
| 80 / 14 | 730.776.445 (−2,20 %) | 397.584 B (**+92 %**) | 6,6 s |
| 120 / 20 | **1.327.230.572 (+77,7 %)** | 531.680 B | 11,9 s |

At 120/20 the instruction count **explodes**: what gets inlined into
`tokenizer__tokenize` drives the register pressure of the largest function
anyway so high that everything is spilled into the frame. And 80/14 buys
2,2 % at almost double the binary size and five times the compilation time.
**Rejected; the limits stay at 40/8.** That is at the same time the
evidence that more inlining brings nothing without a better allocator.

### 9.2 `leave` instead of `mov rsp, rbp` + `pop rbp`: **deliberately not done**

`leave` does exactly the same as the two instructions and would push the
measured number down by 4.229.623 Ir (0,6 %) — without the program doing
less work. The instruction count is the measuring instrument for work here,
not the goal. Such a swap would make the metric prettier and the
statement broken. Explicitly refrained from and noted here, so that nobody
„forgets" to mention it later.

### 9.3 Not touched: `sink_fehler_bei`

41.444.572 Ir (5,93 %) at only 1.503 calls — the function computes the line
and column per parse error by rescanning the input stream.
`html5ever` does **not** do that in the measuring run. That is a real
asymmetry of the comparison to Firn's disadvantage, but it belongs to the
tokenizer, not to the compiler; optimizing it away here would mean changing
the measuring run instead of the compiler. **Named, not touched.**

---

## 10. firnc1

`lib/firnc1` has **no optimizer and no register allocation** — every
value lies in the frame there (that is what it says in the header of
`tools/self_compare.sh`). All seven changes of this round lie
exactly in those two parts and have no counterpart in firnc1; there is
nothing to mirror there. The demanded equality is therefore demonstrated
where it can be demonstrated in this setup:

* `tools/self_compare.sh` — **214 identical behavior, 0 differing,
  0 failing**: every test program, compiled by firnc1, yields the same
  return value and the same output as when compiled by firnc0.
* `tools/fixpoint.sh` — stage 2 == stage 3, **character-identical, 427.401
  lines**: the compiler compiled by firnc0 produces the same assembly as
  the one compiled by itself.
* `tools/fir_compare.sh` — 42.472 FIR instructions identical (1 known,
  named deviation as before the round).

New FIR opcodes were not necessary; the number range 40–49 remains free.

---

## 11. Profile after (realweb, 699.459.494 Ir)

```
          SELBST   ANTEIL         INKLUSIV  FUNKTION
     355.533.636   50,83%      559.683.789  tokenizer__tokenize
     128.322.175   18,35%      128.322.175  dekodiere
      92.529.491   13,23%       92.711.427  tokens__tok_attr_value_push
      41.444.572    5,93%       41.584.990  tokens__sink_fehler_bei
      16.322.580    2,33%       16.323.516  tokens__tok_attr_name_push
      11.448.594    1,64%      699.459.490  main
      10.234.104    1,46%       14.808.079  tokens__tok_attr_finish
```

| Metric | before | after |
|---|---:|---:|
| `tokenize` per character | 107 Ir | **72,3 Ir** |
| `dekodiere` per byte | 39 Ir | **26,0 Ir** |
| `tok_attr_value_push` per call | 40 Ir | **37,0 Ir** |
| data movements (`mov`/`movzx`/`movabs`) | 494.930.767 (51,7 %) | 371.999.741 (53,2 %) |
| frame loads | 129.637.675 (13,53 %) | 94.014.270 (13,44 %) |
| frame stores | 126.722.224 (13,23 %) | 80.695.439 (11,54 %) |

The **share** of data movements rises although their absolute number falls
by 24,8 %: everything else has shrunk more strongly. The frame stores go
down by 36,3 %, the frame loads only by 27,5 % — exactly the picture one
expects when superfluous intermediate stores disappear but the reason
for the spilling remains. The remaining bottleneck is thereby named
unchanged: **too few registers** (§13.2).

## 12. Acceptance

| Check | Result | Base |
|---|---|---|
| `bash ./test.sh` | **PASS 754/754** | 751/751 (+3 from `tests/332_adressierung.fi` in three build stages) |
| `cargo test --release` (module tests) | **169/169** | 155 (+8 `threading.rs`, +6 `regalloc.rs`) |
| `bash tools/self_compare.sh` | **214 identical, 0 differing, 0 failing**, CODEGEN FEHLT 0 | 213/0/0 (+1 new test file) |
| `bash tools/fixpoint.sh` | **stage 2 == stage 3, character-identical, 427.401 lines** | 427.401 |
| `bash tools/tokenizer/run.sh` | **6810/6810 = 100,00 %**, with errors **6809/6810** | unchanged |
| lexer/parser/layout/sema/FIR comparison | unchanged (1 known, named deviation each; layout 0) | unchanged |
| DOM endurance run, packages, atomic primitive | passed | unchanged |

Before every acceptance run, `.firnc1 .firnc2 .firnc3` were deleted (trap
(a) of the round's brief). All intermediate files lay under `.r51/` resp.
`.tokenizer-work/` in the own worktree — no `/tmp` (trap (b)). Wall clock
values are cited as evidence nowhere (trap (c)).

State of the measuring run at the end, with
`tools/tokenizer/patterns.py`:

```
Rahmenverwaltung (callee-saved sichern/holen)   36.414.390 Ir   5,21%    932 Stellen
Store+Reload derselben Zelle                    28.215.979 Ir   4,03%    118 Stellen
Rahmenverwaltung (push/pop/ret)                 12.688.869 Ir   1,81%   1177 Stellen
lea + Zugriff (Adressierungsmodus ungenutzt)     9.467.272 Ir   1,35%    215 Stellen
Rahmenverwaltung (rsp<->rbp)                     8.459.246 Ir   1,21%    664 Stellen
Rahmenverwaltung (call)                          4.229.623 Ir   0,60%   3949 Stellen
setcc-Kette statt direktem Sprung                  565.336 Ir   0,08%     16 Stellen
jmp direkt hinter jcc (Blocklayout)                 10.021 Ir   0,00%     39 Stellen
```

## 13. Open points

1. **Store/reload at block boundaries: 28.215.979 Ir (4,03 %), 118 spots.**
   That is the remainder of the phi-in-memory problem: a confluence at
   which every predecessor writes into a cell and the successor reads it
   immediately. H1 solves that for `bool` (because there the reader is a
   `brcond`); for values it only works via real phi nodes in FIR, tail
   duplication of the confluence or a cell promotion that would have enough
   registers.
2. **Register pressure is the bottleneck.** In `tokenize` (frame 43 KiB)
   and `dekodiere`, loop invariants such as `off`, `len`, `basis` are
   fetched from the frame on every iteration. The cause is the linear scan
   without interval splitting: whoever crosses a `call` gets only one of
   the five callee-saved registers — even when the call lies on a **cold
   branch** and the value is not live there at all. `crosses_call` is
   determined today as „some call lies between `start` and `end`"; a
   computation from the real liveness (`live_out` at the call site,
   without the result of the call, plus its arguments) would be more
   precise and can be had without splitting. That is the next biggest
   lever.
3. **Frame management: 66.021.751 Ir (9,44 %)** at 4.229.623 calls —
   prologue, epilogue, saving and restoring the callee-saved registers
   (36.414.390 Ir of that). `tok_attr_value_push` saves four registers
   although one of them is only needed on the cold branch; „shrink
   wrapping" would be the technical term. It is explicitly **not** more
   inlining (§9.1).
4. **Address folding: 16.188.606 Ir (2,3 %) were left standing**, because
   the result of the address computation is read more than once — typically
   several fields of the same struct. To fold that would mean lengthening
   the lifetime of the base over several instructions; for that the
   allocator would have to know about the lengthening **before**
   allocation. Unchanged open since round 43.
5. **`sink_fehler_bei`** (5,93 %) computes the line/column per parse error
   by rescanning the stream; a running counter would be cheaper. Belongs to
   the tokenizer, not to the compiler (§9.3).
6. **The base path** (`codegen_x86.rs`) has no block layout and no
   address folding. For `--no-opt` that is right; if `dev-fast`
   should ever fall back to it, it costs unnecessarily.
