# TEMPO 12 -- two attempts, both measured and NOT adopted

State before: TEMPO 11, MP3 decoder 8 s of sound = 128.5 M instructions.
The code lies on the branch `tempo12-versuch` (commit 67548aa4).

## Die Frage

`l3_huffman` needs 20.0 M instructions, C 12.1 M. The state of the
bit reader (`HuffLage { cache, sh, next }`) lies in the frame
the whole function, because the decoder passes a pointer to it to the helpers.
`FIRN_RA_STATS`: 958 values, 163 in registers, **maxlive = 86**, 92 values
cross a call.

## Versuch 1: SROA (`sroa.rs`)

An `alloca` whose address is used only as a `load`/`store` address
(directly or via `ptradd` with a constant offset) is split into one cell per
field; `mem2reg` promotes the fields afterwards.

It takes effect: the memory accesses in `l3_huffman` fall in FIR from 110 to 42.
And the program gets **slower: 128.5 -> 132.1 M (+2.8 %)**,
`l3_huffman` 20.0 -> 23.6 M. The three fields become values that live across
all loops; with 86 simultaneously live values on 14 registers
they get none, and instead of a memory operand there now stand
frame copies at every back edge. On the benchmark bank: no
change.

## Attempt 2: save caller-saved registers across a call

Until now: a value that crosses a call gets only one of the five
callee-saved registers, a floating-point number none at all. New: any register, and
at the call it is stored/restored if that is cheaper than the value in the
frame (cost = 2 x weight of the crossed calls).

| Variant | MP3 (M) |
|---|---|
| off | 128.5 |
| integer + floating point, factor 1..100 | 129.3 -- 134.0 |
| floating point only | +0.01 % |

Why it loses: the allocation is a linear pass by start.
Values that formerly got no register at all now take the callee-saved
registers first, and the values that had them before end up in
caller-saved registers and have to be saved at EVERY call
(`l3_imdct36`: five saves around two calls of `l3_dct3_9` per band).
`fib` gained 5.4 % in the first version, after the correction nothing more.

## What follows from this

Both attempts fail at the same place: the intervals have no
**gaps**. A value lives from the first to the last touch in one piece;
in `l3_huffman` that is 86 at the same time, although in the hot
count1 loop perhaps twelve are really needed. As long as that is so,
every change only shifts WHO ends up in the frame. The next
real step is an allocator with lifetime gaps (interval = list of
pieces, as with Wimmer/Franz) -- after that SROA and the
call save presumably pay off by themselves, and they lie ready on the branch.
