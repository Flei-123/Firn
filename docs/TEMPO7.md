# Round TEMPO 7 — four causes, fetched from the count

State 21.09.2026, branch `xmm-ra`. Continuation of TEMPO 6, where the question
was answered WHY Firn needs more instructions. This round clears away the
next four items — each from the measurement, none from a feeling.

## The generator: two new vector instructions

| Name | x86 | aarch64 |
|---|---|---|
| `__v128_store64` | `movlps` | `str d<n>` |
| `__v128_unpacklo32` / `hi32` (now in the allocator path) | `punpckldq` / `punpckhdq` | `zip1` / `zip2` |

`store64` writes only the lower half. Storing two adjacent `f32` used to be possible
only as a full sixteen-octet write — and that destroys the
two values next to them. `punpck*` already existed as an intrinsic, but it sent
the whole function to the basic path; now the path with
register allocation emits it itself. `tests/1617_simd_pack.fi` checks both
lane by lane.

## Die vier Posten im Dekoder

### 1. Acht Einzelzugriffe am Schleifenkopf -> zwei Ladungen (197,3 -> 194,6 Mio)

`synth` fills at the start of every iteration eight values in `zlin`, in pairs from
`xl` and `xr`, and from ADJACENT places:

```text
xl: [A, A+1, .., ..]   xr: [A, A+1, .., ..]
punpckldq  ->  [xl[A], xr[A], xl[A+1], xr[A+1]]
```

That is exactly the order of the four assignments. Instead of eight accesses with
eight index calculations: one load, one interleave, one write. The second
half lands in two separate places — for that `store64` and once
`shuffle32(.., 0x0E)`.

### 2. Coefficients on the stack (194.6 -> 188.5 M)

`scale_pcm4` began with

```firn
var konst: [f32; 12] = [0.5f, 32766.5f, ...]
var grenzen: [i32; 8] = [32767, ...]
```

— twenty values, each with a load and a write, **at 1.3 million
calls**. In the generated code that was forty of the roughly hundred instructions of the
function. Now they stand as a `static` once in the program. The same in
`dct_ii_4`.

### 3. Zeroing for nothing (188.4 -> 184.0 M)

`l3_imdct36` created its two helper arrays (`co`, `si`, nine values each)
INSIDE the band loop — eighteen write instructions per band, although both
are completely written before the first read. Now they stand before
the loop.

### 4. The hot `mem_copy` (184.0 -> 180.9 M)

The state of the synthesis filter bank is 3840 octets, copied twice per granule.
`rt.mem_copy` manages eight octets per iteration — correct and
usable everywhere, also in the kernel profile without SSE. For this one case
there now stands a `kopiere16` in the decoder that takes sixteen. **`rt.mem_copy`
itself stays untouched**: it must also run where there are no
SSE registers.

## Die Zahlen

| | Instructions (8 s of sound) |
|---|---|
| after TEMPO 6 | 197.3 M |
| after TEMPO 7 | **180.9 M** |
| the same with `--cpu=avx` | **163.6 M** |
| `minimp3` in C, `gcc -O2` | 74.8 M |

Distance to C: **2.4x** (with AVX 2.2x). At the start of the tempo rounds it was
8.6x. After every single step the PCM output is bit-identical and the
self-test gives PASS 4/4.

## Wo die restlichen 2,4x sitzen

Function by function, Firn against C:

| | Firn | C |
|---|---|---|
| `synth` | 52,6 Mio | 22,7 Mio |
| `l3_huffman` | 30,1 Mio | 12,1 Mio |
| `l3_imdct36` | 19,7 Mio | 12,9 Mio |
| `mp3_decode_frame` | 17,1 Mio | ~6 Mio |
| `dct_ii` | 17,6 Mio | 11,1 Mio |
| `l3_dct3_9` | 9,7 Mio | 8,0 Mio |

The last two are practically level — there calculation happens, and Firn
can do that now. The first two hang on the same thing, and the
measurement names it unambiguously: **register pressure**. `FIRN_RA_STATS=1` says for
`synth` `maxlive=44`, for `l3_huffman` `maxlive=63` — with **fourteen**
registers. Every value above that lies in the frame and is
fetched anew before every use; in the generated code there therefore stand chains like

```text
mov  -0xfd8(%rbp),%rcx     ; fetch the pointer anew
mov  (%rcx),%eax           ; read the value
mov  %rax,-0xfe0(%rbp)     ; store the intermediate value
movslq -0xfe0(%rbp),%rax   ; and fetch it again
```

What helps against that is no further vector work, but work on the
allocator:

1. **Restoring instead of spilling** (rematerialisation): a value like
   `15 - i` or a sign extension is cheaper to calculate again than to
   fetch from the frame.
2. **Cut lifetimes at calls**, so that a pointer that survives a
   call is not loaded anew at every access.
3. **Smaller loop bodies** — also a task of the generator
   (splitting), not only of the programmer.
