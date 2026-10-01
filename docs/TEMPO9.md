# Round TEMPO 9 — the loop that only writes zeros

State 23.09.2026, branch `xmm-ra`. A small round with a big
result, and it stands here above all because of the measurement that it
triggered.

## The measurement: per address, not per function

Until TEMPO 8 counting was done per FUNCTION. That suffices as long as the effort
is evenly distributed — and hides everything else. This round counts
with `valgrind --tool=callgrind --dump-instr=yes` **per address** and assigns
the addresses via the **symbol table** (`nm`).

The assignment via the disassembly, which stood there first, left a quarter
of the instructions under `??` — and of all places the result lay there. The
reason: the positions in the callgrind file are mostly relative, a
single entry that is not understood shifts all following ones by a few
octets, and a comparison for equality then hits nothing any more. Via the
symbol table the function is unambiguous anyway; for the question "which
KIND of instruction" the next instruction before it suffices.

With that the MP3 decoder (8 s of sound, 160.6 M instructions) looked like this:

| Funktion | Befehle | Anteil |
|---|---|---|
| `synth` | 39,0 Mio | 24 % |
| `l3_huffman` | 23,1 Mio | 14 % |
| `l3_imdct36` | 19,7 Mio | 12 % |
| **`mp3_decode_frame`** | **17,2 Mio** | **11 %** |

`mp3_decode_frame` decides nothing and calculates nothing — it reads the
frame header and calls the others. Eleven percent could not be right there.

## Was dort stand

```text
2848362   cmp   $0x1200,%rdx
2848362   jae   done
2848362   mov   -0x8b0(%rbp),%r11      ; fetch the pointer anew EVERY TIME
2848362   movb  $0x0,(%r11,%rdx,1)
2847744   lea   0x1(%rdx),%rdx
          jmp   head
```

That is `rt.mem_set((&(*s).grbuf[0][0]) as u64, 0, 576 * 2 * 4)` — once per
granule, 4608 octets. `rt.mem_set` is written in Firn:

```firn
fn mem_set(target: u64, value: u8, n: usize) {
    var i: usize = 0
    while i < n {
        st8(target, i, value)
        i = i + 1
    }
}
```

Correct, usable everywhere — and **five instructions per octet**. 14.2 of
160.6 million instructions, only to set an array to zero.

## What was built

A new pass `memset` (`compiler/src/memset.rs`). It recognises in FIR

```text
P:    br H
H:    %i = phi [P %null, B %i2]
      %c = cmp.lt.uXX %i, %n
      brcond %c, B, X
B:    %a = add %base, %i
      store.u8 %wert, %a
      %i2 = add %i, %eins
      br H
```

and turns it into `secure_zero(%base, %n)` in the pre-header plus `br X`.

**Why `secure_zero` and no new instruction:** `Op::SecureZero` has existed since
`secure_zero(inout buf)` in all three generators — x86 `rep stosb`, aarch64
eight bytes at a time. It does exactly what is demanded, and "one question, one answer"
means here: do not invent a second instruction for the same thing. That
`secure_zero` additionally promises never to be optimised away is
stronger than necessary for this case and therefore harmless.

## The conditions — one of them is dangerous

All stand in the file head of `memset.rs`. The most important:

> **The comparison must be UNSIGNED.**

With `i64`, `n` can be negative. The loop then runs zero times.
`rep stosb` with `rcx = -1` fills half the address space. That is no
theoretical difference, but the one between "do nothing" and "machine
gone" — and a module test (`the_signed_comparison_stays_a_loop`) pins it
down.

The others: body with exactly three instructions (every further one would be an
effect that `rep stosb` does not have), value constant 0 and one octet wide,
start 0, step 1, `base` and `n` defined outside, nothing from the
loop is read outside. And: fixed lengths under sixteen octets
stay a loop, because `rep stosb` has a start-up time of a few dozen
cycles.

## The result

| | Instructions (8 s of sound) |
|---|---|
| after TEMPO 8 | 160.6 M |
| **after TEMPO 9** | **146.3 M** |

`mp3_decode_frame` falls from 17.2 to 3.0 M. The PCM output is
bit-identical. `tests/1700_memset_loop.fi` checks the edges: exactly `n`
octets zero, before and behind untouched, length 0 writes nothing, a
value not equal to zero stays a loop, and a length known only at run time
works just the same — the last point is the most important, because the
translator could also calculate a fixed length away entirely.

## Was daneben liegen blieb

`rt.mem_copy` is the same loop with a read added and stands at 3.5
M instructions. For that there is `Op::CopyMem` (`rep movsb`) — but its
size is a CONSTANT in the instruction, not a run-time size, and the question
of overlapping areas is a different one than with zeroing. That
would be a round of its own with a measurement of its own.
