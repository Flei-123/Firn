# Round TON 2 -- why Firn is ten times slower than C here

State 18.09.2026, branch `ton`. The question of the round was: **can Firn reach the speed
of C?** The answer up front: yes, but not through better Firn code --
the brake sits in the compiler, and it is named exactly.

## 1. Where the time goes (measured, not estimated)

60 s of audio (MPEG-1, 44.1 kHz, stereo, 192 kbit/s), `--opt-level=release-fast`.
Measured by leaving out single stages:

| Stage | Time | Share |
|---|---|---|
| everything | 2.34 s | 100 % |
| without synthesis filter bank | 0.80 s | -- |
| **hence: synthesis (DCT-II + windowing)** | **1.54 s** | **66 %** |
| without synthesis and without IMDCT | 0.70 s | -- |
| hence: IMDCT | 0.10 s | 4 % |
| rest (frames, scale factors, Huffman) | 0.70 s | 30 % |

The optimisation levels themselves:

| Level | Time |
|---|---|
| `dev` | 10.44 s |
| `dev-fast` | 3.32 s |
| `release-safe` | 2.97 s |
| `release-fast` | 2.46 s |

## 2. What helped in the source -- and what did not

**Helped (11 %):** the hot loop of the synthesis filter bank unrolled,
the four sums as single variables instead of an array, the address once per
`(i,k)` instead of four times per voice: **2.34 s -> 2.07 s**, output still
bit-identical.

**Did not help (worse!):** replacing the sign branch in `adr4` with
a branch-free bit-pattern trick (`i64` -> `u64` through memory):
**2.34 s -> 3.21 s**. The detour through memory costs more than the
well-predictable jump. Noted so that nobody tries it again.

## 3. The actual cause: the register allocator knows no floating point

`compiler/src/regalloc.rs` (linear scan, 4946 lines) says so itself:

```rust
// FLOATING POINT: this allocator knows only the integer registers.
if f.val_types.iter().any(|t| t.is_float()) {
    return Some("f64 in the value set".into());
}
```

**Every function in which even a single `f32`/`f64` occurs falls back to the
base path** -- and that gives every intermediate value its own stack slot.
The generated code then looks like this (from the innermost loop
of the measuring kernel):

```asm
movsxd rax, dword ptr [rbp-1168]
mov    qword ptr [rbp-240], rax
mov    rax, qword ptr [rbp-240]
mov    rcx, qword ptr [rbp-248]
imul   rax, rcx
mov    qword ptr [rbp-256], rax
```

Six memory accesses for one multiplication. No register holds anything
longer than one statement.

### The counter-test that proves it

The same loop structure twice, once in `f32` and once in `i64`
(`lib/ton/bench_main.fi`, `lib/ton/bench_int_main.fi` against
`bench/inner_loop.c`, `bench/inner_loop_int.c`, both `gcc -O2`), 2 million passes each,
identical result on both sides:

| Kind of computation | C | Firn | Factor |
|---|---|---|---|
| `f32` (without register allocation) | 0.03 s | 0.50 s | **~17x** |
| `i64` (**with** register allocation) | 0.03 s | 0.085 s | **~2.8x** |

That answers the question: Firn is **not** fundamentally ten times
slower. With register allocation it is at a factor of three -- and the rest
of that is gcc's vectorisation, not the language. Without register allocation
(that is, in every floating-point program) it is a factor of 17.

## 4. What follows from it

**The next sensible piece of work is not an audio round but a
compiler round: a second register class (`xmm0`-`xmm15`) in the linear
scan.** Outline:

1. Separate the intervals per register class (integer / floating point), the
   existing linear scan runs twice over the same numbering.
2. Output for `f32`/`f64` in registers: `movss`/`movsd`, `addss`, `mulss`,
   `comiss` instead of memory-to-memory traffic.
3. Spill slots of 4 or 8 octets, mind the calling convention (all `xmm`
   are **caller-saved** on System V, so across a `call` only
   after saving).
4. The base path stays as a fallback; the guard clause
   `supported()` releases floating point as soon as the path can really do it.

Benefit far beyond this decoder: everything that computes in Firn --
graphics, physics, layout in Certus, the species project -- runs today without
register allocation.

## 5. State of the decoder after this round

* 2.07 s for 60 s of audio = **29x real time**, still **bit-identical** on all
  test cases (`tools/ton_build.sh` -> PASS 4/4).
* The unrolled synthesis is in `lib/ton/mp3.fi`; the measuring kernels are in
  `lib/ton/bench_main.fi` and `lib/ton/bench_int_main.fi`.
