# TEMPO 13 -- lifetimes with gaps

State before: TEMPO 11/12, MP3 decoder 8 s of sound = 128.5 M instructions
(callgrind). TEMPO 12 had shown that SROA and the save at the call
both fail at the same place: an interval was `[first touch,
last touch]` in one piece, `l3_huffman` had 86 "simultaneously live"
values with 14 registers.

## Was gebaut ist

1. **Pieces instead of an interval** (`regalloc.rs`, "LIFETIME WITH GAPS").
   Every value gets one piece per block from the data-flow liveness:
   from the block start if it lives in, to the block end if it lives out,
   otherwise first/last touch in the block. Two values interfere only
   if pieces overlap. The scan holds per register the
   intervals that it carries; it is free if no piece collides.
   Displacement is by density (weight per piece length). Both classes
   (integer and SSE). `l3_huffman`: maxlive 86 -> 40.
   Switch off: `FIRN_NO_HOLES=1`.
2. **Merge copies with several write sites.** The lock
   `defs != 1` in the coalescing is lifted; `interferes` checks every
   write site anyway (Chaitin). With that the phi variables of the loops are
   merged. Old: `FIRN_COAL_SINGLE=1`.
3. **SROA** (`sroa.rs`, from the branch `tempo12-versuch`), now narrowly positive.
   `FIRN_NO_SROA=1`.
4. **Save at the call for floating-point numbers** (from the branch `tempo12-versuch`),
   adapted to the gaps: saving happens only where the call lies in a
   piece of the value (`a <= p < z`). The first version checked `a < p`
   and thereby forgot calls that stand at the block start -- `l3_huffman`
   gave wrong values. The integer variant (`FIRN_CS_INT=1`) stays off:
   it still loses (124.3 instead of 123.7 M). `FIRN_NO_CALLSAVE=1`.

## Messung (MP3, 8 s Ton, Befehle)

| Stand | Mio |
|---|---|
| TEMPO 11 | 128,5 |
| + Luecken | 126,4 |
| + Coalescing mehrere Schreibstellen | 124,9 |
| + SROA | 124,7 |
| + Sicherung am Aufruf (Gleitzahl) | **123,7** |

-3,8 % insgesamt; `l3_huffman` 20,0 -> 16,8 Mio (C: 12,1).

Benchmark bank (`bench/firn`, instructions, output per program the same):
branchy -9.0 %, bytecount -14.2 %, jsonscan -13.0 %, statemachine -11.5 %,
bitmap -2.7 %, rest +-0. No deterioration.

Wall clock not measured meaningfully: the machine ran with load 10
(foreign test runs), both states fluctuated 0.28 -- 0.39 s.

## A test was wrong

`tests/822_gc_weak_zeroed.fi` expected `lives` to survive a
`gc_collect()`, although `lives` is never read afterwards. Dead
is dead -- the old allocator only happened to leave the pointer in a
register. The test now reads `lives` after the collection.

## Geprueft

All tests in `release-fast`, `release-safe`, `dev-fast` green (1018 runs),
new test `tests/1703_ra_holes.fi`. Fixed point stage 2 == stage 3
(793,453 lines), `self_compare` 342 of 342 same behaviour, 0 deviating. MP3 bit-identical (8 s and 60 s against `ref60.pcm`).

## What is worth doing next

`synth` (35.0 M, C 22.7) is the biggest gap. The inner loop
`k < 8` has a fixed length and branches per iteration on `k == 0` and
`k & 1` -- fully unrolling constant short loops would fold the
branches away (gcc does that). Estimated 5-6 M.
