# Round TON 1 -- the MP3 decoder (`lib/ton/mp3.fi`)

State 18.09.2026, branch `ton`. Everything here was **run**, not estimated;
the commands are below and can be repeated.

## Why this round

Firn has network, TLS and HTTP, but no sound: `lib/media/audio.fi` in Certus
has a back end for Win32 (`waveOut`) and silence otherwise. For everything
to do with sound -- radio, file, stream -- the first thing missing is the **decoder**.
Without it every audio layer is deaf, because practically everything that
comes over the network is MPEG-1/2 Layer III.

This round builds exactly that one building block: octets in, PCM out.
No system call, no heap, no device dependency.

## What was built

| File | Lines | Content |
|---|---|---|
| `lib/ton/mp3.fi` | ~1500 | the decoder: frame search, side info, scale factors, Huffman, stereo, IMDCT, synthesis filter bank |
| `lib/ton/mp3_tab.fi` | 415 | the tables, **generated** by `tools/mp3_tables.py` |
| `lib/ton/mp3_main.fi` | 90 | measuring driver: `.mp3` -> `.pcm` + one line of figures |
| `lib/ton/mp3_check_main.fi` | 150 | self-test against recorded checksums |
| `tools/mp3_tables.py` | 190 | table generator from the original |
| `tools/mp3_compare.py` | 60 | measures two PCM files against each other (max, RMS, SNR) |

Origin: the structure follows **minimp3** (lieff, CC0-1.0). The tables come
from the original through the generator, the flow is newly written -- Firn
has neither global variables nor C-style pointer arithmetic.

## The result: bit-exact

The comparison is against the same original, compiled as C (`gcc -O2
-DMINIMP3_NO_SIMD -DMINIMP3_ONLY_MP3`). "Bit-exact" means: every single
16-bit word identical.

| Sample | Format | Frames | Result |
|---|---|---|---|
| sine 440/3000 Hz | MPEG-1, 44.1 kHz, stereo, 128 kbit/s | 117 | **bit-exact** |
| pink noise | MPEG-1, 44.1 kHz, mono, 64 kbit/s | 79 | **bit-exact** |
| white noise | MPEG-1, 32 kHz, stereo, 192 kbit/s | 58 | **bit-exact** |
| sine | MPEG-2, 24 kHz, mono, 48 kbit/s | 87 | **bit-exact** |
| VBR material | MPEG-1, 44.1 kHz, stereo, VBR q2 | 194 | **bit-exact** |
| white noise | MPEG-1, 44.1 kHz, stereo, 320 kbit/s | 117 | **bit-exact** |
| sine | MPEG-1, 48 kHz, joint stereo, 96 kbit/s | 169 | **bit-exact** |
| impulses (short blocks) | MPEG-1, 44.1 kHz, mono, 128 kbit/s | 156 | **bit-exact** |
| sine | **MPEG-2.5**, 8 kHz, mono, 32 kbit/s | 45 | **bit-exact** |
| **real radio stream** (MangoRadio, 31 s) | MPEG-1, 44.1 kHz, stereo, 128 kbit/s | 1210 | **bit-exact** |

Against **ffmpeg** (a completely different implementation) on the same radio stream:
SNR 66.4 dB, largest deviation 310 of 32768, 0.47 % of the values unequal.
That is the normal distance between two permitted MP3 decoders -- the
standard prescribes an error margin, not an exact bit state.

### What was wrong on the way there

Three errors, all three found by measuring, not by reading:

1. **`MAX_SCFI` computed wrong** (232 instead of 44). Result: `1 << 58`
   in an `i32`, the gain became 0, the decoder delivered
   **complete silence** with formally correct frame counts. Lesson: an
   output with exactly the right LENGTH is not yet a proof.
2. **`s4 += s8 - s2`** was written as `s4 + s8 - s2`. In
   floating point that is not the same: one ULP of difference, which only becomes
   visible when a value lies close to a quantisation step.
   Effect: 0.02 % of the samples off by exactly 1.
3. **`b[j] += vz*w1 + vy*w0`** in the synthesis filter bank, likewise written as
   `b + vz*w1 + vy*w0` -- the same trap, the same effect.

After (2) and (3) the distance was **zero**. The intermediate state before that
(SNR 110 dB, at most 1 LSB) would have been audibly indistinguishable --
all the more reason not to let it pass as "correct".

## Robustness

* 200 kB of **random octets**: 0 frames, no crash, like the original.
* 5 kB of junk **before** a valid stream: bit-exact, the frame search finds it.
* **truncated** file (20 kB of 49 kB): 47 frames, bit-exact.
* **ID3v2** header in front: bit-exact (the tag block is run over).
* **150 randomly corrupted** streams (1-40 flipped octets per run):
  **0 crashes**, 147 of them bit-exact. The three deviations arise where
  the Huffman reader reads past the frame end on broken data --
  the original does that too, only that behind the buffer it has a different
  field. No access outside the own scratch space.

## Speed -- the honest part

60 s of audio (MPEG-1, 44.1 kHz, stereo, 192 kbit/s), AMD EPYC, same machine:

| | Time | Real-time factor |
|---|---|---|
| Firn (this decoder) | 3.3 s | ~18x |
| C original, scalar, `-O2` | 0.34 s | ~176x |

**About ten times slower than C.** That is enough for radio by a wide margin
(a stream needs 1x real time), but it is not a good value, and the reason is
known: every memory access goes through the helper functions `lf`/`sf`
with `adr4`, that is, through a real call including a branch for the
negative index. That is the place for round TON 2 -- measure first, then
inline.

## Known limits

* **Layer III only.** Layers I/II are recognised and skipped.
* **No SIMD.**
* **Output i16**, interleaved.
* No gapless trimming (the LAME/Xing header is not evaluated): at the start
  the encoder's lead-in values stay, as with the original.

## Repeating it yourself

```sh
FIRNC=/root/firn/compiler/target/release/firnc
FIRNLIB=$PWD/lib $FIRNC -o /tmp/mp3 lib/ton/mp3_main.fi
FIRNLIB=$PWD/lib $FIRNC -o /tmp/mp3check lib/ton/mp3_check_main.fi

# self-test (without foreign tools)
/tmp/mp3check testdata/ton/stereo44.mp3 testdata/ton/mono24.mp3 \
              testdata/ton/mono8.mp3 testdata/ton/shortblocks.mp3

# decode a file and listen to it
/tmp/mp3 something.mp3 /tmp/x.pcm
ffplay -f s16le -ar 44100 -ch_layout stereo /tmp/x.pcm
```

Regenerating the tables (only needed if the original changes):

```sh
curl -L -o /tmp/minimp3.h https://raw.githubusercontent.com/lieff/minimp3/master/minimp3.h
python3 tools/mp3_tables.py /tmp/minimp3.h > lib/ton/mp3_tab.fi
firnfmt -w lib/ton/mp3_tab.fi   # the repository keeps the canonical form
```

## What comes next

1. **TON 2 -- speed**: inline `lf`/`sf`, measurement against these figures.
2. **TON 3 -- output**: back end for Linux (ALSA) and Android (AAudio),
   so that PCM really becomes sound.
3. **TON 4 -- stream**: HTTP/Icecast connection, ring buffer, a refill path
   without dropouts.
4. Only then the mixer layer in the sense of Aulos (voices, buses, curves).
