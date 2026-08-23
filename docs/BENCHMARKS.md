# BENCHMARKS.md — what Firn actually measures

Every number in this file **was run**, on the machine named below, with the
command printed next to it. Nothing here is estimated, and nothing is taken
over from somebody else's README. Where Firn loses, the number stands anyway.

**Machine:** AMD EPYC 7571, 8 vCPU, 12 GiB RAM, Debian 12, Linux x86_64.
**Toolchain:** `rustc` 1.99.0-nightly, GNU `as`/`ld` 2.40, `openssl` 3.0.20,
`gzip` 1.12, `python3` 3.11.2, `node` v18.20.8.
**Reproduce everything:** [RUN.md](../RUN.md) · **acceptance:** `bash test.sh`

> **A note on how to read the "behind by" column.** It is the reference
> divided by Firn. `1.42x` means the reference is 1.42 times as fast. A
> value below `1.00x` means Firn is ahead.

---

## 1. Cryptography and compression

The library is written **in Firn**, not bound to OpenSSL or zlib. Round 82
added the processor's own instructions (AES-NI, SHA-NI, SSE) behind a
`cpuid` check, and **the scalar path stays next to it** — on a processor
without those instructions the same program still computes the same result,
only slowly. Both paths are held against the official NIST vectors.

| workload | scalar path | with the processor's instructions | reference | behind by |
|---|---:|---:|---:|---:|
| **SHA-256** | 27.6 MiB/s | **968.3 MiB/s** | OpenSSL 1372.6 MiB/s | **1.42x** |
| **AES-128-CBC encrypt** | 7.4 MiB/s | **582.0 MiB/s** | OpenSSL 1056.4 MiB/s | **1.82x** |
| **AES-128-CBC decrypt** | 4.7 MiB/s | **691.8 MiB/s** | OpenSSL 1056.4 MiB/s | **1.53x** |
| **AES-128-CFB8** | 0.5 MiB/s | **26.9 MiB/s** | OpenSSL 37.1 MiB/s | **1.38x** |
| **DEFLATE level 6** | — | **11.2 MiB/s** | `gzip -6` 21.0 MiB/s | **1.88x** |
| inflate | — | 20.2 MiB/s | — | — |
| CRC-32 (table driven, scalar) | — | 188.4 MiB/s | — | — |
| SHA-1 | 60.5 MiB/s | — | — | — |

The gain of the instruction path over the scalar one is **35.1x** for
SHA-256, **78.6x** for AES-CBC encryption, **147.2x** for AES-CBC decryption
and **53.8x** for CFB8.

**Compression ratio** (`lib/std/deflate.fi` against `zlib`, level 6, output
size as a percentage of zlib's — under 100 % means Firn packs tighter):

| input | size | Firn | zlib | ratio |
|---|---:|---:|---:|---:|
| `wikipedia_en_rust.html` | 1,009,516 | 150,363 | 151,862 | **99.0 %** |
| the library sources | 539,939 | 143,610 | 143,833 | **99.8 %** |
| `/dev/urandom` | 200,000 | 200,071 | 200,071 | **100.0 %** |
| 300 k times the same octet | 300,000 | 314 | 314 | **100.0 %** |

    bash tools/bench82/run.sh        # the speed table
    bash tools/stdlib81/run.sh       # correctness: NIST vectors, zlib both ways

## 2. Hash and hash map

| workload | result |
|---|---:|
| FNV-1a over 16 MiB | 247 MiB/s |
| **xxHash64 over 16 MiB** | **5,591 MiB/s** |
| xxHash64 correctness | 11 inputs x 2 seeds identical to `python-xxhash` |
| map, 1,000,000 entries with string keys | inserted, iterated and halved by deletion, longest probe chain measured |
| map, 1.2 M insert+delete in 60 rounds | RSS flat; the counter-check without deletion must grow, and does |

    bash tools/stdlib81/run.sh

## 3. JSON

| workload | result |
|---|---:|
| reading, integer documents | 11.7 MiB/s |
| reading, float-heavy documents | 1.4 MiB/s |
| JSONTestSuite | the `y_` cases accepted, the `n_` cases refused |
| against `python3 -m json.tool` | 93 outputs octet for octet identical |

## 4. The compiler on itself

`bin/firnc1.fi`, 30,643 lines of Firn, `--opt-level=release-fast`,
wall clock per phase (`firnc --timings`, new in round 82):

| phase | time | share |
|---|---:|---:|
| **optimizer** | **3120.3 ms** | **61.1 %** |
| codegen | 975.4 ms | 19.1 % |
| `as` + `ld` | 668.5 ms | 13.1 % |
| sema | 157.4 ms | 3.1 % |
| lex + parse | 106.7 ms | 2.1 % |
| lower | 73.1 ms | 1.4 % |
| mono | 5.0 ms | 0.1 % |
| write `.s` | 2.8 ms | 0.1 % |

Register allocation, share of values that end up on the stack instead of in
a register (`FIRN_RA_STATS`):

| program | functions | values | in registers | spilled |
|---|---:|---:|---:|---:|
| DEFLATE | 667 | 35,640 | 38.1 % | **45.2 %** |
| the JS engine | 1,561 | 148,127 | 29.3 % | **50.4 %** |
| the compiler itself | 1,308 | 174,459 | 19.0 % | **57.2 %** |

    firnc --timings --opt-level=release-fast -o /tmp/a bin/firnc1.fi

## 5. Layout engine against Chromium

The same 146 cases are laid out by Firn and by a real browser, and the
`getBoundingClientRect()` of every box is compared. Since round 78 the
browser's answer is **frozen in the repository**, so the acceptance needs no
browser installed; `--refresh-reference` regenerates it.

| measurement | result |
|---|---:|
| own frozen expectation | **1,087 / 1,087 boxes in 146 cases** |
| **against Chromium** | **1,087 / 1,087 boxes, deviation 0.00 %** |
| paint order (`elementFromPoint`) | **5,171 / 5,171 probe points** |
| throughput (callgrind) | 455,890 instructions per element |

    bash tools/layout/run.sh

## 6. HTML tokenizer against html5ever (Rust)

| measurement | Firn | html5ever | behind by |
|---|---:|---:|---:|
| corpus `realweb`, instructions | 957,989,680 | 540,567,228 | 1.77x |
| corpus `realweb`, wall clock | 29.25 MB/s | — | **1.54x** |
| corpus `html5lib` (pathological cases) | — | — | **0.95x — Firn is ahead** |
| html5lib-tests conformance | **6,810 / 6,810 (100.00 %)** | — | — |

    bash tools/tokenizer/run.sh

## 7. JavaScript against test262

The official TC39 suite, **63,364 cases, nothing filtered out**.

| | parser | engine |
|---|---:|---:|
| round 63 | 69.98 % | 50.51 % |
| round 66 | 91.94 % | 71.07 % |
| **round 74** | **91.94 %** | **76.00 %** |

    bash tools/js/run.sh

## 8. Garbage collector

| measurement | result |
|---|---:|
| longest pause, 120,000 live text nodes | **0.45 ms** (was 11.82 ms before round 44) |
| pure compute time in that pause | 0.62 ms |
| arena soak, 480,000 blocks | **exactly one system call for memory**, RSS drift **0 pages** |
| the leaking counter-check | +19,000 pages — the measurement can see a leak |

## 9. Network and the Minecraft server

| measurement | result |
|---|---:|
| 16 connections at once, 1 MiB each | **48.1 MiB/s** payload (96.3 MiB/s on the wire), 16 MiB in 0.33 s |
| NBT against Notch's `bigtest.nbt` | **1,543 octets identical** |
| a vanilla client | logs in and stands in the world; `node-minecraft-protocol` checks every field |

    bash tools/net/run.sh · bash tools/nbt/run.sh · bash tools/mcserver/run.sh

## 10. The second machine (aarch64)

The same Firn program compiled for x86-64 **and** for aarch64, both **run**,
the standard output compared character for character.

**Re-measured on 2026-08-23** (round 86). The corpus grew to 302 cases since
the table below was first written, and **one case now differs**: since the
r80/r82 merge, `tests/1613_crypto.fi` cannot be compiled for aarch64 at all --
`--target=aarch64-linux cannot emit the vector instruction CpuFeatures yet`.
Round 82 built the vector instructions for x86-64 only, and the aarch64
emitter says so instead of producing wrong code. It makes
`bash tools/aarch64/run.sh` **fail**, in both build stages.

| | optimised | unoptimised |
|---|---:|---:|
| **identical output** | **296 of 301 (98 %)** | **296 of 301 (98 %)** |
| **differing** | **1** (`tests/1613_crypto.fi`, see above) | **1** |
| not supported (inline x86 assembler) | 4 | 4 |
| environment (proven with a C probe) | 1 | 1 |

    bash tools/aarch64/run.sh

Earlier state, round 80: 290 of 294 identical, **0** differing -- at that point
the crypto case was not yet in the corpus.

## 11. The acceptance as a whole

| measurement | result |
|---|---:|
| `bash test.sh` | **PASS 1184 / 1184** (state after round 79) |
| the positive corpus in ALL FOUR build levels (round 90) | **320 / 320 in each** — before round 90 `release-safe` failed 117 of them and `dev-fast` 25 |
| `tools/optlevels/run.sh` (round 90) | the four levels agree, in both compilers |
| `tools/checked/run.sh` | **150 / 150** |
| `tools/self_compare.sh` | **328 the same, 0 differing, 0 faulty** (re-measured 23.08.2026) |
| `tools/fixpoint.sh` | **stage 2 == stage 3, character-identical**, 728,292 lines of assembly (re-measured 23.08.2026) |
| `cargo test --release` | **232 passed, 0 failed** |

---

## 12. Firn against Rust, with the build level named (round 90)

**Read this before the table.** Until round 90 `bench/bench.py` compiled the
Firn side with `firnc -o x y.fi` — no build level at all — and labelled the
column "Firn". The default level has been `dev-fast` since round 72, and
`dev-fast` **checks** integer arithmetic. Every number in `bench/RESULTS.md`
therefore holds a *checked* Firn build against an *unchecked* `rustc -O`
one. `sieve` stands there at 4.16x; at `release-fast` it is 1.30x.

So there are two questions, and they are asked separately:

* **the code generator** — `firnc --opt-level=release-fast` against
  `rustc -O -C overflow-checks=no`;
* **the price of safety** — `firnc --opt-level=release-safe` against
  `rustc -O -C overflow-checks=yes`, the same guarantee on both sides.

Every benchmark exists twice (`bench/firn/<n>.fi`, `bench/rust/<n>.rs`),
computes the same thing and **prints its result**; the harness stops if the
outputs are not identical, so nothing can be optimised away on either side.
Median of 9 runs, the four binaries measured in one alternating pass so that
machine drift cancels instead of landing on one of them.

    python3 tools/bench90/bench.py

| benchmark | what it stresses | Firn `release-fast` | `rustc -O` | behind by | Firn `release-safe` | `rustc -O` +checks | behind by |
|---|---|---:|---:|---:|---:|---:|---:|
| **fib** | recursion / call overhead | 0.044 s | 0.027 s | **1.62x** | 0.044 s | 0.027 s | **1.63x** |
| **sieve** | memory, byte writes in a loop | 0.036 s | 0.027 s | **1.30x** | 0.052 s | 0.025 s | **2.12x** |
| **matmul** | nested loops, index arithmetic | 0.060 s | 0.021 s | **2.84x** | 0.182 s | 0.070 s | **2.60x** |
| **bytecount** | memory, sequential read | 0.325 s | 0.173 s | **1.88x** | 0.324 s | 0.119 s | **2.72x** |
| **bubblesort** | memory + branch | 0.082 s | 0.035 s | **2.38x** | 0.104 s | 0.062 s | **1.69x** |
| **statemachine** | branches, table dispatch | 0.161 s | 0.079 s | **2.06x** | 0.157 s | 0.072 s | **2.20x** |
| **bitmap** | the osum frame allocator | 0.066 s | 0.032 s | **2.04x** | 0.080 s | 0.031 s | **2.63x** |
| **xxhash** | xxHash64 over 64 MiB | 0.186 s | 0.174 s | **1.07x** | 0.232 s | 0.174 s | **1.34x** |
| **jsonscan** | JSON scanner, generated document | 0.120 s | 0.067 s | **1.81x** | 0.144 s | 0.078 s | **1.84x** |
| **memstride** | memory bound, cache-hostile stride | 0.224 s | 0.197 s | **1.14x** | 0.234 s | 0.197 s | **1.19x** |
| **branchy** | unpredictable branches | 0.530 s | 0.465 s | **1.14x** | 0.523 s | 0.477 s | **1.10x** |

| | before round 90 | after round 90 |
|---|---:|---:|
| median, `release-fast` vs `rustc -O` | 1.82x | **1.81x** |
| median, `release-safe` vs `rustc -O` +checks | 3.18x | **1.84x** |
| median price of the checks inside Firn | 1.97x | **1.19x** |

"before" here is round 90 **stage 1** — the compiler with the wrong-code bug
already fixed. Against `main` itself there is no speed comparison to make:
**all eleven of these programs segfault** when `main`'s compiler builds them
with `--opt-level=release-safe`. That is the bug, and it is measured in
§1 of `docs/ROUND90.md`.

`release-fast` is not merely "about the same" — the emitted assembly of all
eleven programs is **character-identical** to what went into the round.
Round 90 changed only what the checked levels emit.

### What the checks cost, per program

| benchmark | `release-safe` before | `release-safe` after | the checks cost, before | after |
|---|---:|---:|---:|---:|
| fib | 0.051 s | **0.044 s** | 1.14x | **0.99x** |
| sieve | 0.094 s | **0.052 s** | 2.60x | **1.47x** |
| matmul | 0.416 s | **0.182 s** | 6.96x | **3.04x** |
| bytecount | 0.506 s | **0.324 s** | 1.55x | **1.00x** |
| bubblesort | 0.234 s | **0.104 s** | 2.89x | **1.27x** |
| statemachine | 0.227 s | **0.157 s** | 1.41x | **0.98x** |
| bitmap | 0.131 s | **0.080 s** | 1.97x | **1.22x** |
| xxhash | 0.363 s | **0.232 s** | 1.97x | **1.25x** |
| jsonscan | 0.243 s | **0.144 s** | 2.01x | **1.19x** |
| memstride | 0.283 s | **0.234 s** | 1.27x | **1.05x** |
| branchy | 0.605 s | **0.523 s** | 1.14x | **0.99x** |

### The same thing counted instead of timed

This machine is shared, and a wall clock median still moves by several
percent between two passes — enough to hide a real five percent and to
invent one that is not there. `valgrind --tool=callgrind` counts the
instructions the program really executed and is deterministic to the last
digit. It says nothing about cache misses; for "did the loop get shorter" it
is the honest answer.

    python3 tools/bench90/icount.py

| benchmark | `release-safe` before | `release-safe` after | change | `rustc -O` +checks |
|---|---:|---:|---:|---:|
| fib | 429,999,174 | **303,114,113** | **-29.5 %** | 204,721,288 |
| sieve | 1,095,032,655 | **526,329,353** | **-51.9 %** | 212,271,744 |
| matmul | 4,456,691,599 | **2,125,360,125** | **-52.3 %** | 670,142,765 |
| bytecount | 5,682,464,133 | **2,974,915,175** | **-47.6 %** | 1,278,372,613 |
| bubblesort | 1,885,661,378 | **857,315,966** | **-54.5 %** | 244,006,830 |
| statemachine | 1,548,537,886 | **971,401,368** | **-37.3 %** | 505,290,506 |
| bitmap | 1,494,353,935 | **1,004,558,579** | **-32.8 %** | 372,239,225 |
| jsonscan | 2,264,001,094 | **1,202,000,710** | **-46.9 %** | 382,796,317 |

Where it came from: every checked operation used to rescue its two operands
on the stack for a message that is almost never printed, and carried the
message-building arm inline in the hot instruction stream. It now reloads
the operands out of line and a checked `+`/`-` computes in the target
register (`docs/ROUND90.md` §2.2).

---

## Where Firn is honestly behind

Everything here is measured, and the command is next to it.

* **The code generator is 1.81x behind `rustc -O` in the median** of the
  eleven programs in §12 (range 1.07x – 2.84x). Three named
  causes, in the order of what they cost — the disassembly is in
  `docs/ROUND90.md` §4:
  1. **Loop counters live in memory in FIR.** `mem2reg` promotes only cells
     written once, FIR has no phi nodes, and the register allocator promotes
     cells at the very end — after the optimiser has already given up. So
     `licm` cannot hoist a loop-invariant `r * n` out of the inner loop (it
     depends on a `load`), and no induction-variable analysis can turn
     `k * n` into an addition. Largest item left, and an architectural one.
  2. **The allocator does not split intervals.** A value that crosses a call
     sits on the stack for its whole life, not just across the call.
     `matmul`'s `main`: 88 values in registers, 87 on the stack, worst
     overlap 15 against twelve registers (`FIRN_RA_STATS=1`).
  3. **No auto-vectorisation.** `rustc` turns `matmul`'s inner loop into
     SSE; Firn never vectorises on its own. `lib/std` uses the vector
     instructions by hand where it matters (round 82).
* **Checked arithmetic still costs 1.19x in the median** (`release-safe`
  against `release-fast`), worst `matmul` at 3.04x, where `rustc` pays
  almost nothing for the same guarantee. The reason is not the check any
  more — round 90 took it down from 1.97x — it is that **LLVM proves
  most of its checks away and Firn proves none of them away**. Firn has no
  range analysis; `i + 1` with `i < 240` known from the loop guard is still
  a full checked addition. That is the next step, and it is the one that
  makes "fast AND safe" true rather than nearly true.
* **DEFLATE at 1.88x of `gzip`** — the match search is the whole story, and
  it is scalar.
* **The optimizer is 61 % of the compile time** and does not earn all of it.
* **No vector instructions on aarch64** — round 82 built them for x86-64
  only; on ARM the scalar path runs.
* **JSON with floats collapses to 1.4 MiB/s**, ten times slower than with
  integers. The float parser is the reason, and it is known.

### Where Firn is level or ahead

* **xxHash64, `release-fast`: 1.07x** — a hash written with the wrapping
  operators is the code generator with nothing in the way.
* **unpredictable branches 1.14x, memory bound stride 1.14x** — where
  the processor and not the compiler decides, Firn is level. At
  `release-safe` those two are 1.10x and 1.19x: the safety is
  nearly free where the machine is the bottleneck.
* **`fib`, `bytecount`, `statemachine` and `branchy` cost NOTHING for being
  checked** (price 0.99x, 1.00x, 0.98x, 0.99x) — the
  checked build is as fast as the unchecked one there.
* **HTML tokenizer on the html5lib corpus: 0.95x** — faster than html5ever.
