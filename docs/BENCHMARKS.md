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
| `tools/self_compare.sh` | **321 the same, 0 differing, 0 faulty** |
| `tools/fixpoint.sh` | **stage 2 == stage 3, character-identical**, 649,720 lines of assembly |
| `cargo test --release` | 229 passed, 0 failed |

---

## Where Firn is honestly behind

* **DEFLATE at 1.88x of `gzip`** — the match search is the whole story, and
  it is scalar.
* **Register allocation spills more than half the values** of the compiler
  itself to the stack. That is the largest single lever left.
* **The optimizer is 61 % of the compile time** and does not earn all of it.
* **No vector instructions on aarch64** — round 82 built them for x86-64
  only; on ARM the scalar path runs.
* **JSON with floats collapses to 1.4 MiB/s**, ten times slower than with
  integers. The float parser is the reason, and it is known.
