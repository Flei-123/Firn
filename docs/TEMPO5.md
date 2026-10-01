# Runde TEMPO 5 — vier Spuren bis in die Umkehrwandlung

State 21.09.2026, branch `xmm-ra`. Continuation of TEMPO 4: there `v128`
became usable at all in the path with register allocation, here it is
applied — and the instruction set is supplemented by what was still missing for it.

## Was dazugekommen ist (Erzeuger)

| Name | x86 | aarch64 |
|---|---|---|
| `__v128_trunc_f32_i32` | `cvttps2dq` | `fcvtzs .4s` |
| `__v128_cvt_i32_f32` | `cvtdq2ps` | `scvtf .4s` |
| `__v128_cmplt_f32` | `cmpltps` | `fcmgt` (Operanden getauscht) |
| `__v128_cmple_f32` | `cmpleps` | `fcmge` (getauscht) |
| `__v128_cmpnlt_f32` | `cmpnltps` | `fcmgt` + `mvn` |
| `__v128_cmpgt_i32` | `pcmpgtd` | `cmgt .4s` |

In addition `and`, `andnot`, `or`, `xor`, `add32` and `sub32` may now also
go through the path with register allocation (before, a single one of them sent
the whole function to the basic path). `--cpu=avx` knows all the new instructions.

`cmpnlt` is **not** the negation of `cmplt`: with NaN both
comparisons are unordered, and `cmpnltps` then says TRUE. Exactly that is
needed to emulate the single version of the sample conversion —
`tests/1616_simd_cvt.fi` pins it down, so that the aarch64 version
(`fcmgt` + `mvn`) does not deviate from it.

## What was converted in the decoder with it

### `dct_ii` — vier Baender auf einmal (45 -> 19 Mio)

The inverse transform calculates per band ONE column of `grbuf`: the access is
`grbuf[k + 18*z]`, and for four adjacent bands the four values lie
SIDE BY SIDE. Thereby every row of the transform is a calculation on four
lanes. What remains of bands (`n` is not always divisible by four)
is still calculated by the old version — it stands unchanged next to it.

The coefficient table `sec` got four values of padding: a coefficient is fetched
as a 16-octet load and pulled onto all four lanes with `__v128_shuffle32`.

### `l3_imdct36` — die Schlussschleife (44 -> 27 Mio)

The nine steps at the end read seven rows, all of which run upwards with `i`
and lie side by side; only the second output goes backwards
(`17 - i`), and that is reversed by `__v128_shuffle32(.., 0x1B)`. Eight of the nine
steps now run as two groups of four, the ninth individually.

### `scale_pcm` — die Abtastwandlung (26 -> 15 Mio)

Four floating-point numbers become four integers: `+0.5`, truncate,
subtract one if the result is negative, clamp at both ends.

**Here lay the only bug of this round, and it is instructive.** The
single version writes

```firn
var s: i32 = (abtast + 0.5f) as i32
if s < 0 { s = s - 1 }
```

That is NOT `floor(x + 0.5)`. It subtracts one for **every** negative result
— even if the truncation discarded nothing, so the value
was already whole. The first version here calculated `floor` (truncate,
convert back, compare) and was the same for most values, but for
exactly whole negative intermediate results it was off by one. The
self-test reported it at once; a comparison program over 200,000
values showed which. Now there stands the mask `i < 0`
(`__v128_cmpgt_i32(0, i)`) — and that is shorter on top of it.

The two stops are expressly emulated, not left to saturation:
`cmpnlt(v, 32766.5)` is true for `v >= 32766.5` **and** for
NaN (the single version also ends up at 32767 for NaN through the overflow of the
conversion), `cmple(v, -32767.5)` is the lower stop.

### `synth` — pointers instead of index (small, but right)

The three addresses of the inner loop run in fixed steps (256
octets down, 256 up, 8 further). Calculated once and then
advanced.

## Die Zahlen

MP3 decoder, 60 s of sound, `release-fast`, smallest of nine runs, output
to `/dev/null`:

| | Instructions (8 s of sound) | Time |
|---|---|---|
| after TEMPO 4 | 258.6 M | 0.21 s |
| + `dct_ii` on four bands | 236.1 M | |
| + `l3_imdct36` | 222.1 M | |
| + `scale_pcm` | 211.2 M | |
| + pointers in `synth` | **209.6 M** | **0.19 s** |
| the same with `--cpu=avx` | **194.1 M** | **0.18 s** |
| `minimp3` in C, `gcc -O2` | 74.8 M | 0.06 s |

After **every** step: PCM bit-identical (`cmp` over 10.6 MB) and self-test
PASS 4/4, in `baseline` as in `avx`.

Since the beginning of the tempo rounds: **644 -> 210 M instructions, 0.88 -> 0.19 s.**

## What is still left in it

The measurement says it exactly (shares of the whole):

* `synth` 59 M (28 %) — now almost only address calculation and the eight
  individual accesses at the loop head.
* `l3_huffman` 30 M (14 %) — bit fiddling, nothing to vectorise.
* `l3_imdct36` 27 M (13 %) — what remains are the two calls of
  `l3_dct3_9` (12 M) with their dependencies.
* `mp3_decode_frame` 17 M, `scale_pcm4` 15 M, `dct_ii_4` 14 M.

The next big step would no longer be SIMD, but the
**integer side**: advance pointers instead of recalculating addresses (in the
generator, not by hand), and cut lifetimes at calls.
