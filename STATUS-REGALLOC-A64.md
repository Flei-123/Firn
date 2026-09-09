# ROUND REGALLOC-A64 — the register allocation reaches the second machine

Branch `regalloc-a64`, from `phi`. Not merged.

Certus round TEMPO-3 (`/root/certus-layout/docs/RUNDE-TEMPO-3.md`) found
why Justin's phone hangs for four to five seconds where the x86 emulator
does not: `codegen_x86.rs` calls `regalloc::emit_func_ra`, and
`codegen_a64.rs` called it **zero times**. For aarch64 there was no
register allocation at all — every intermediate value went through its
frame slot, constants included.

This round gives aarch64 the allocation, and takes the second half of
that finding (the overflow check costing two branches, with the panic
arm sitting in the hot path) with it.

---

## 1. The numbers

Executed instructions — not seconds, not static size.

| what | x86 | aarch64 round 80 | aarch64 now | now / x86 | gain |
|---|---:|---:|---:|---:|---:|
| `misch_zeile` (the blend loop) | 1 093 312 | 1 903 520 | **1 210 512** | **1.11×** | 1.57× |
| `gc_alloc`, whole program | 13 332 393 | 38 473 940 | **13 857 131** | **1.04×** | 2.78× |

Before this round the same two ratios were **1.74×** and **2.89×**. The
brief asked for 1.2× or better; both are inside it.

Static shape of the same loop, which is where TEMPO-3 took its reading:

| | total | frame accesses |
|---|---:|---:|
| x86 | 264 | 22 (8 %) |
| aarch64, round 80 | 403 | 134 (33 %) |
| aarch64, now | 351 | **20 (5 %)** |

The stack traffic that was the whole finding of TEMPO-3 is gone; on this
loop aarch64 now touches the frame *less often than x86 does*.

### How it was measured

`x86` through `valgrind --tool=callgrind`, per function resolved through
`nm` (the binaries carry a symbol table but no DWARF ranges, so callgrind
prints raw addresses).

`aarch64` through `.ra/icount_a64.py`. This qemu (7.2, Debian) carries no
TCG instruction plugin and valgrind does not run aarch64 here, so the
count is assembled from two things qemu *will* say: the size of every
translation block (`-d in_asm`) times the number of times each block was
entered (`-d exec,nochain`). `nochain` is what makes that exact — without
it, chained blocks never report their entry.

`gc_alloc` is measured whole because at `--opt-level=release-fast`, the
level it is written for, its kernel is inlined into `main`. It mixes a
hash into a `u32` on purpose, which the checked levels rightly refuse.

---

## 2. What was built

### Step 1 — the overflow check costs one branch (`b701f606`)

Round 83 gave aarch64 the checked arithmetic in the shape x86 had
**before** round 90 moved the panic arms out of line:

```
adds w9, w9, w10
b.vs .Lchksite7
b    .Lchkok7
.Lchksite7:
<seven instructions of panic set-up>
.Lchkok7:
```

Two branches for the case that does not fire, and the arm that never runs
sitting between the operation and its successor — inside the loop. 15
such blocks with 190 instructions in the body of `misch_zeile` alone.

`Emitter` already had the cold buffer x86 uses (`cold_line`,
`flush_cold`); aarch64 simply was not using it. All four checked sites now
branch once, forward, into the cold half:

* `emit_checked_bin` — the ok label disappears, fallthrough **is** ok
* `emit_checked_idx` — `b.lo ok` becomes `b.hs bad` (exact negation)
* `emit_checked_cast` — `b.eq ok` becomes `b.ne bad`
* `emit_checked_div` — `cbnz past` becomes `cbz site`

and `codegen_a64::emit_func` calls `flush_cold`, without which the buffer
would be carried into the next function. 30 `.Lchkok` labels became 0.

### Step 2 — the register file becomes data (`1fa528bf`)

The analysis half of `regalloc.rs` was never x86 specific.
`compute_live`, `promotable_cells`, `immediate_consts`, `loop_depth`,
`loop_ranges`, `count_reads`, `exact_crossings` and `direct_frame_addrs`
do not contain a single register name between them — checked, not
assumed. Only the SUPPLY and the EMISSION were.

So the supply became a struct:

```rust
Machine { pool, callee_saved, temp, arg_spare, div_spare, m_call, m_memop }
```

`X86` is exactly the file this allocator always had. `A64` is AAPCS64:
**x19–x24** callee-saved plus **x0–x7** as the short-lived supply, and
nothing else — x8 and x9–x17 are the scratch of `codegen_a64.rs`,
`panic_rt_a64.rs` and `simd_a64.rs` and can be written between any two
instructions of a value's lifetime.

AAPCS64 offers ten callee-saved registers, not six. Section 3 says why
the last four are not handed out; it is the one finding of this round
that no test would have produced.

The masks stay a `u16` and stay POSITIONAL (bit *k* = `pool[k]`), so
`inst_clobbers`, `exact_crossings` and `Iv::killed` keep working
unchanged. That is the reason for doing it with a table rather than a
register enum.

`inst_clobbers` gained an aarch64 arm, and it is short. That table is a
list of x86 *oddities*: `mul` into rdx:rax, `div` out of it, `cqo`, the
`cmov` sequences that park a bound in rdx, `rep movsb` in rdi/rsi. A64
has none of them. On that machine exactly one thing destroys a pool
register: a call.

**Proof that this step changes nothing:** every program in `tests/`
compiled to x86 assembly before and after — 316 files, 316 byte
identical.

### Step 3 — aarch64 gets the allocation (`73533fe9`)

`codegen_a64.rs` asks `regalloc::allocate_a64` and turns the answer into
either a register move or the memory access round 80 always emitted. The
instruction selection is unchanged; what changed is where its operands
live. Everything funnels through four functions, which is why this is a
small diff and not a second code generator: `load_full`, `load_ext`,
`store_dst_ty`, and `Op::Const`.

`canonicalise` is the invariant it rests on. Round 80 computes at 32 or
64 bits and lets the result keep whatever upper bits fall out, because
the SLOT is narrow and `load_ext` widens on the way back in. A register
has no width, so the WRITER has to leave the canonical form behind —
otherwise `add w19, w19, w20` on two `i32` leaves a zero extended sum
where a sign extended one belongs.

### Step 4 and 5 — A64 is a three address machine (`06875ef0`, `0bd743fe`)

Step 3 put the values in registers but still fed every operation through
the scratch pair: 167 of the 400 instructions of `misch_zeile` were plain
register to register moves. `operand`/`dest` let an operation read its
operands where they are and write its result where it belongs;
`imm12` and `logical_imm` let `add`/`sub`/`cmp` and `and`/`orr`/`eor`
carry their constants the way x86 does; `Op::Cmp`, `Op::Load`,
`Op::Store`, `Op::PtrAdd` and the shifts follow the same rule.

`logical_imm` implements the real rule from the Arm ARM (a block of set
bits inside a repeated element, rotated) rather than guessing, because an
unencodable value is a hard assembler error, not a slow path.

The checked `+`/`-` also lost the two `mov`s that saved its operands for
the panic message: since step 1 the arm is cold, and it re-loads the
originals itself through the caller's own loading code (`restore`) —
exactly the way round 90 does it on x86.

---

## 3. Three bugs the corpus found

All three are the same shape: **a sequential move sequence that is only
safe while values live in slots**. A slot is a source and never also a
target; a register can be both.

1. **The parameter prologue** (`tests/024_six_args.fi`). Parameter 3
   wanted x4 while parameter 4 still sat in x3:
   `mov x4, x3` / `mov x3, x4` destroyed parameter 4. 21 became 17.
2. **The argument list** (`tests/1400_core_span.fi`). Same shape at the
   call site: `mov x3, x5` / `mov x4, x3` gave argument 4 the new x3.
   0 became 5. Bisected out of 290 functions with `FIRN_RA_A64_ONLY`.
3. **The save slots** (`tests/331_stack_args.fi`). The callee-saved
   registers were parked after `fr.size` — which is exactly where the
   OUTGOING ARGUMENT AREA lives, at `sp+0`. `deep` wrote its ten
   arguments over the saved x27/x28. 42 became 4.

1 and 2 now go through `parallel_reg_moves`, the cycle breaking walk
`regalloc.rs` has used on x86 since round 43, with x9 as the scratch that
breaks a cycle. 3 grows the frame at the top, next to the value slots.

---

## 3b. The bug that no test would have found

`Op::GcAddr { regs: true }` spills the callee-saved registers into the
save area of the GC state block, and `lib/gc/gc.fi` scans it:

```
__gc_scan(regs + S_REGS, regs + S_REGS + 48)
```

**Forty-eight octets is six words.** x86 has exactly six callee-saved
registers (rbx, rbp, r12–r15) and `emit_gc_addr` writes all six, so
SPEC §3.5.3's conservative register scan holds there. AArch64 has ten,
and its `emit_gc_addr` writes six — which was correct as long as no value
lived in one of them at all. The function said so itself:

```
// On this path no value ever lives in a callee-saved register across a
// call — everything is in the frame, and the frame is scanned.
```

Step 3 of this round made that sentence false. Handing out x25–x28 would
have created exactly the bug the register scan exists to prevent: a Gc
object whose last reference sits in x27, a collection, and a freed object
still in use. It would surface only when a collection lands on the wrong
microsecond — in no test that does not collect there.

So the supply is the six registers the collector really reads. That is
the same callee-saved count x86 has, so nothing is lost against the other
machine; it cost `misch_zeile` 1.07× → 1.11×. Widening it means changing
the runtime (the 48 in `gc.fi`, the save area in two code generators and
in `lib/firnc1/codegen.fi`) and belongs in a round that can test the
collector.

Verified afterwards that no function the allocator touches mentions
x25–x28: the only occurrences left in emitted assembly are in the
hand-written panic trampoline, which never returns.

---

## 4. The test table

| what | result |
|---|---|
| `tools/aarch64/run.sh` (dev-fast) | **307 SAME, 0 DIFFERENT** — identical to the baseline before this round |
| `tools/aarch64/run.sh --no-opt` | **307 SAME, 0 DIFFERENT** |
| `tests/` exit codes on aarch64 under qemu | **265 correct, 0 wrong** |
| `cargo test --release` | **269 passed, 0 failed** (two of them new: the aarch64 clobber answer, and SPEC 3.5.3 as an assertion) |
| `tools/checked/run.sh` | **150 checks passed, 0 failed** (compares panic messages octet for octet) |
| `tools/phi/run.sh` | **ok** — and its point 4 now reads *aarch64 loads/stores in `sum_to`: 25 without mem2reg, **0** with it* |
| x86 assembly vs. the base compiler | **316 files, 0 differences** |

The four programs carrying `only_mode: opt` (`028_cast_narrow`,
`030_wrap_u8`, `054_i16_ops`, `1334b_type_truncation`) are checked at
`--opt-level=release-fast`, the level they are written for, where they
give 57 / 44 / 1 / 0 on aarch64 — the same as x86. At any other level
they panic **on both machines**, which is what they are there to show.

### KERN-4 — what could and could not be done

`tools/kern/fasertest.fi` of the Certus tree **cannot be built on this
branch**, and not because of anything this round did. It needs
`#[arch(x86_64)]` / `#[arch(aarch64)]` to choose between two machine code
blobs; that attribute lives on branch `arm-freestanding`, which `phi`
does not contain (`git merge-base --is-ancestor arm-freestanding phi`
says no). The **unmodified base compiler fails on it in exactly the same
way**, with `function 'faser__code_worte' is already declared` — the two
`code_worte` definitions in `lib/kern/faser.fi` are guarded by that
attribute and without it both are visible at once. `/root/firn-dnspic`,
which does carry the branch, builds it fine.

What KERN-4 actually asks — after a deep chain of calls, does every frame
find its own locals again, and do both machines say the same numbers — is
answered by `.ra/kern4_ersatz.fi`: a twelve deep recursion carrying eight
live values across each call (more than x86's callee-saved supply, inside
A64's), mixed signed narrow widths through calls, and a checked site in a
loop. **x86 and aarch64 agree exactly, at all three build levels, with
the allocator on and off.** That is the same property, without the
inline machine code this branch cannot express.

---

## 5. Handles

* `FIRN_NO_RA_A64=1` — the whole machine back on the round 80 path.
* `FIRN_RA_A64_ONLY=<name,name>` / `FIRN_RA_A64_SKIP=<name,name>` —
  allocate only, or all but, the functions whose name contains one of
  these fragments. This is how bug 2 was found in a 290 function program.
* `FIRN_RA_STATS=1`, `FIRN_RA_WARN=1` — unchanged, as on x86.

## 6. What this round does not do

* **`f64`/`f32` and `v128` still go over the base path**, on both
  machines. `unsupported_basic` refuses them because the linear scan
  knows one register class; the `v`/`d` registers would need intervals of
  their own. That is the same restriction x86 has had since round 71/82,
  now shared rather than newly introduced.

  What that costs, counted with `FIRN_RA_WARN=1`:

  | program | functions | refused | allocated | reason |
  |---|---:|---:|---:|---|
  | `1400_core_span` | 290 | 73 | **74 %** | all `f64` |
  | `940_layout_box_model` | 1270 | 245 | **80 %** | all `f64` |
  | `1613_crypto` | 702 | 40 | **94 %** | 32 `f64`, 8 `v128` |

  So three quarters to nineteen twentieths of the code gets the
  allocation, and the hole is one shape: a second register class. That is
  the obvious next round, and it would help x86 by exactly as much.
* **Inline assembler and MMIO** stay on the base path (round 52's reason:
  they bind fixed registers and are `volatile`). On aarch64 that means
  `tests/850_asm_basic` and friends are refused by the code generator, as
  they were before.
* **The `.tempo3` measurement is a proxy for the phone.** It is the same
  Firn source and the same compiler, counted exactly; it is not a reading
  taken on Justin's device. The Certus side of that (`lib/android/spur.fi`)
  is where a real number would come from.
