# Round 81 — the four libraries every network program needs: hash maps with real keys, DEFLATE, JSON, crypto

**Branch** `r81-std` · **Date** 22.08.2026 · AMD EPYC 7571, 8 cores, 12 GiB,
Linux x86_64, Debian 12, `firnc0` release build, `python3` 3.11.2,
`openssl` 3.0.20, `gzip` 1.12, `node` v18.20.8.

Everything below **was run**. Where a number stands, it was measured on this
machine on this day; where something does not work, it says so.

---

## 1. What the round was for

Round 76 gave this language sockets, a byte buffer, NBT and a Minecraft
server that a real vanilla client walks into. It walks in only because that
server runs with `network-compression-threshold -1` and in offline mode. The
moment either of those changes, the language stops being able to speak:

* over the threshold **every** packet is a zlib stream — no DEFLATE, no
  connection past 256 octets;
* in online mode the login needs **SHA-1**, **AES-128/CFB8** and a JSON
  answer from `sessionserver.mojang.com`;
* and everything above it wants a map from a **name** to something, which
  until this round meant interning every string first, because `Map[K, V]`
  hashed scalars only.

So this round built the four things: `lib/std/hash.fi` + octet keys in
`lib/std/map.fi`, `lib/std/deflate.fi`, `lib/std/json.fi`,
`lib/std/crypto/`. **All of it in Firn.** Round 75 built `extern fn`, and
using it here would have answered a different question than the one this
round asks — the C library appears exactly once, as `openssl` on the far
side of a pipe, judging the output.

---

## 2. What was built

| | Lines | What |
|---|---|---|
| `lib/std/hash.fi` | 253 | FNV-1a (64 and 32 bit) and **xxHash64**, both written out, plus the splitmix finalizer for scalars and a seed from the monotonic clock. |
| `lib/std/map.fi` | +771 | `Set[K]`, **`BMap[V]`** (keys are octet sequences, copied into a blob the map owns, the full 64 bit hash cached per slot), `BSet`, `str.Span` and C-string spellings of every operation — and the **collision measurement** (`probe_max`, `probe_total`) for both map kinds. |
| `lib/std/deflate.fi` | 1,281 | DEFLATE (RFC 1951) **both ways**: stored/fixed/dynamic blocks, LZ77 with hash chains and lazy matching, length limited Huffman codes with a Kraft check, exact per-block bit costing. Frames: zlib (RFC 1950, Adler-32) and gzip (RFC 1952, CRC-32), also both ways. |
| `lib/std/json.fi` | 1,113 | RFC 8259 reader and writer: a flat node array, `\u` with surrogate pairs, the number grammar walked by hand, line and column on every refusal, `JsonWriter` for JSON without a document. |
| `lib/std/crypto/` | 1,059 | `sha1.fi`, `sha256.fi` (incremental), `hmac.fi` (RFC 2104 + a constant time comparison), `aes.fi` (AES-128, CBC and CFB8, S-box computed not transcribed), `random.fi` (`getrandom(2)` with a named fallback). |
| `tools/stdlib81/` | 1,542 | Four Firn programs and five Python/shell checkers — the proof. |
| `tests/1610`–`1613` | 1,004 | The same libraries inside `test.sh` section 3, in three build stages. |
| `testdata/json`, `testdata/crypto` | — | JSONTestSuite (340 files) and the NIST CAVP vectors (18 files), pinned per file with sha256. |

**Total: 8,344 lines added**, six commits.

---

## 3. The measurements

Everything here comes out of `tools/stdlib81/run.sh`, which is **section 40
of `test.sh`**. Not next to the acceptance — in it.

### 3.1 Hash and map

| What | Measured |
|---|---|
| FNV-1a over 16 MiB | **247 MiB/s** (one multiplication per octet, on the critical path) |
| xxHash64 over 16 MiB | **5,591 MiB/s** — 22× faster, four independent accumulator chains |
| xxHash64 correctness | 11 inputs × 2 seeds against **python-xxhash** (the author's own C library): identical, and `""` → `ef46db3751d8e999` as published |
| FNV-1a correctness | against a second implementation in the checker and against the published `""`/`"a"`/`"foobar"` values |

The load test, **1,000,000 entries with string keys** (`key0` … `key999999`,
the digits written backwards so consecutive keys differ in their *first*
octets):

| | |
|---|---|
| insert | **0.56 s** = 1.8 M/s |
| look up all of them again | **0.29 s** = 3.4 M/s |
| iterate over all of them | **0.019 s** |
| delete 500,000 | **0.14 s** |
| table capacity | 2,097,152 slots, load **0.477** |
| **longest probe chain** | **41** |
| **average probe chain** | **0.458** |
| memory: table + key blob | **74.0 MiB** for **8.5 MiB** of key octets |
| process RSS | 66.6 MiB |

The correctness half of the same run, which is what makes the numbers worth
anything: every inserted key found with its own value (0 missing, 0 wrong),
**0 of 10,000 keys that were never inserted found**, iteration sees exactly
1,000,000 entries, after deleting every second key exactly 500,000 remain and
exactly the *other* half is still findable.

**The endurance run** (1.2 M insert+delete in 60 rounds):

| | RSS start | RSS end | growth |
|---|---|---|---|
| soak (insert **and** delete) | 1,236 KiB | 1,288 KiB | **+52 KiB** |
| counter-check (**no** deletions) | 1,232 KiB | 70,188 KiB | +68,956 KiB |

The counter-check is the point: without it a flat line proves nothing,
because a measurement that cannot see growth will report a flat line for a
leak too. It grows by 67 MiB, so it can see.

### 3.2 DEFLATE

Compression, against `zlib` at the same level, over the corpus (all of it
verified by `python3 zlib`, `gzip` and the `gunzip` binary):

| File | Raw | Firn (-6) | zlib (-6) | Firn / zlib |
|---|---|---|---|---|
| `empty.bin` | 0 | 8 | 8 | 100.0 % |
| `one.bin` | 1 | 9 | 9 | 100.0 % |
| `random.bin` (urandom) | 200,000 | 200,071 | 200,071 | **100.0 %** |
| `onechar.bin` (300 k × `a`) | 300,000 | 314 | 314 | 100.0 % |
| `libstd.txt` (the library sources) | 539,939 | 143,610 | 143,833 | **99.8 %** |
| `wikipedia_en_rust.html` | 1,009,516 | 150,363 | 151,862 | **99.0 %** |
| `hackernews.html` | 34,320 | 5,507 | 5,470 | 100.7 % |

Incompressible data costs **+71 octets on 200,000** — that is the stored
block doing its job; a compressor without one produces something larger than
its input here.

Throughput (`wikipedia_en_rust.html`, 1 MiB):

| Level | Pack | Result | vs zlib | Unpack |
|---|---|---|---|---|
| 1 | **24.0 MiB/s** | 168,715 | 87.7 % | 39.1 MiB/s |
| 6 | **16.9 MiB/s** | 150,363 | 99.0 % | 21.8 MiB/s |
| 9 | **14.8 MiB/s** | 150,220 | 100.3 % | 22.5 MiB/s |

The cross-check matrix, per file and per level (0, 1, 6, 9), **7 files ×
4 levels × 6 directions, 0 errors**:

1. Firn packs zlib → `python3 zlib.decompress` gets the original back;
2. Firn packs gzip → the **`gunzip` binary** gets the original back;
3. Firn packs raw → `zlib.decompress(..., -15)` gets the original back;
4. `zlib.compress` → Firn unpacks;
5. `gzip.compress` → Firn unpacks;
6. raw `compressobj(-15)` → Firn unpacks.

Counter-checks, **4 of 4 refused**: a truncated stream, a wrong Adler-32, a
header that is not zlib, an empty input. `tests/1611` adds a hand-built
stream with a **distance that reaches in front of the output** (`73 04 62` —
`python3 zlib` calls it "invalid distance too far back"), a lying gzip
`ISIZE` and a wrong gzip magic; all refused.

### 3.3 JSON

Against **JSONTestSuite** (Nicolas Seriot), 318 files, in the repository:

| Group | Result |
|---|---|
| `y_*` — must be accepted | **95 / 95** |
| `n_*` — must be refused | **188 / 188** |
| `i_*` — implementation defined | 30 accepted, 5 refused (see 6.J2–J4) |

Against `python3 -m json.tool` over all 95 `y_` files:

* **93 outputs octet for octet identical**,
* 2 differ, and both are `y_object_duplicated_key*`: RFC 8259 §4 declares the
  behaviour for duplicate names "unpredictable"; this parser **keeps both
  members**, Python keeps the last (J7).
* **95 of 95 semantically equal** under `json.load`, including those two.

The refusals carry a position: `3:8: a value was expected` for a file whose
error is on line 3, column 8.

Throughput: a 1.5 MB document of 20,000 objects parses and re-serialises at
**11.7 MiB/s** with integers — and at **1.4 MiB/s** when the same document
carries floats. That is not the JSON code (see 5, the float finding).

### 3.4 Crypto

| Vector set | Vectors | Result |
|---|---|---|
| `SHA1ShortMsg` / `SHA1LongMsg` | 129 | **all ok** |
| `SHA256ShortMsg` / `SHA256LongMsg` | 129 | **all ok** |
| `HMAC.rsp` (L=20 and L=32, truncated to each `Tlen`) | 525 | **all ok** |
| AES-128 CBC KAT (GFSbox, KeySbox, VarKey, VarTxt) | 568 | **all ok** |
| AES-128 CFB8 KAT (the same four) | 568 | **all ok** |
| **NIST total** | **1,919** | **1,919 ok, 0 wrong** |

Plus, because the KAT files are **single block** and therefore say nothing
about chaining:

* multi block **CBC and CFB8 over 4 KiB against the `openssl` binary**, both
  directions — 4 of 4 identical;
* FIPS 197 C.1 in both directions;
* 106 of 106 cases against `python3 hashlib`/`hmac` with random keys and
  messages of 0, 1, 55, 56, 63, 64, 65, 127, 128 and 1000 octets (the
  padding boundaries);
* `getrandom(2)`: two calls differ, no zero buffer, source reported as 1
  (the system call, not the `/dev/urandom` fallback).

Throughput:

| | Measured |
|---|---|
| SHA-1 | **60.5 MiB/s** |
| SHA-256 | **22.6 MiB/s** |
| AES-128-CBC | **5.50 MiB/s** |
| AES-128-CFB8 | **0.34 MiB/s** |

Two of those numbers moved during the round, and both moves came out of the
measurement rather than out of an opinion:

* **SHA-256: 9.8 → 22.6 MiB/s.** The 64 round constants were a local array
  literal inside the block function, so they were written out again for
  every 64 octets of input. They now live in the state.
* **AES: CBC 0.84 → 5.50, CFB8 0.04 → 0.34 MiB/s.** `MixColumns` called a
  GF(2⁸) multiplication routine per octet. Six 256 octet tables replaced it.

CFB8 costs **one full AES block encryption per octet** — that is the mode,
not the implementation, and it is why 0.34 MiB/s is sixteen times worse than
CBC. For a Minecraft connection (tens of KiB/s per player) it is enough; for
a bulk transfer it is not, and 7.A5 says what would fix it.

---

## 4. How to check it yourself

```sh
bash tools/stdlib81/run.sh          # all four areas, three build stages
STDLIB81_FAST=1 bash tools/stdlib81/run.sh   # the optimised stage only
bash test.sh                        # section 40 is the same thing
```

The test data is **in the repository** and pinned per file:
`testdata/json/files.sha256` (340 files) and
`testdata/crypto/files.sha256` (18 files) are checked before anything runs.
Nothing is fetched at test time.

---

## 5. What the round found on the way

Three of these are in other people's code, one is in the compiler, and all
four were found by measuring rather than by reading.

1. **The end-of-block symbol was not counted.** The first dynamic Huffman
   block this file ever wrote had no code for symbol 256, so the block could
   not be closed. Every decoder in the world says the same thing to that —
   `invalid code -- missing end-of-block` — which is exactly why the proof
   is built on other people's decoders.
2. **`rt.buf_reserve` takes a TOTAL capacity, not an additional amount.**
   Read as "reserve n more", it produced an `OutOfMemory` on every input
   larger than the initial buffer and worked perfectly on small ones.
3. **The lazy matcher did not terminate.** The first version kept a match
   back and stepped the input position *back* by one for it; on 300,000
   identical octets it never came out again. The loop now only ever moves
   forward, and the invariant is written down above it.
4. **`num.write_f64` costs 27.5 µs per double, `num.parse_f64` 21 µs**
   (measured over 20,000 conversions each, `release-fast`). Those are the
   correctly rounded shortest-round-trip routines of round 65, and they are
   about three orders of magnitude away from what a Ryu/Grisu style
   implementation does. Nothing in this round is affected beyond
   float-heavy JSON (1.4 MiB/s instead of 11.7), and nothing here was
   changed for it — it is recorded as a measured fact and a candidate for a
   round of its own. On the way it also showed that
   `num.bytes_new()` per number means an **`mmap` per number**; the JSON
   writer now hands one scratch buffer down instead (0.1 s instead of 1.4 s
   on 20,000 floats).

---

## 6. The language gaps this round ran into

Round 76 found one compiler bug; this round found three things, and the
second one is a real bug with a silent failure mode.

**G1 — `size_of[T]()` cannot see a type of an imported module.**

```firn
// lib/std/json.fi
struct Node { ... }
fn node_at(d: *mut Json, i: u32) -> *mut Node {
    return ((*d).nodes + i as u64 * size_of[Node]() as u64) as *mut Node
}
```

> `error: unknown type 'Node'`

The type resolution behind `size_of` only knows the types of the **root**
file. `lib/std/map.fi` gets away with `size_of[K]()` because a generic
parameter is substituted in the *caller's* module, where the type is known.
Exporting the struct does not help. **Workaround in this round:** the stride
is measured — `[Node; 2]` on the stack, the distance between element 0 and
element 1 — which is the truth including alignment, and costs two
instructions once per document. Reproduced minimally:
a module with `struct P` and `fn getsize() -> usize { return size_of[P]() }`
does not compile when it is imported.

**G2 — a non-generic struct with a field of a generic INSTANTIATION
disappears from the module, silently.**

```firn
// module m
struct GBox[V] { v: V, n: usize }
struct Holder { b: GBox[u8] }     // <- this
fn plain() -> Holder { ... }
```

The defining module compiles **without any error**. In the importing module
`Holder` and *every other name of that module* is then "unknown type" /
"unknown function". The silence is what makes it dangerous: the error
appears at the use site, names something unrelated, and points at the wrong
file. `BSet` in `lib/std/map.fi` is exactly this shape and is usable only
because it is addressed **qualified** (`map.BSet`, `map.bset_add`) — which
leads to:

**G3 — a generic type or call cannot be written qualified.**

```firn
var m: map.BMap[u64] = map.bmap_new[u64]()
```

> `error: expected '=' after the name in a 'var' statement, found '['`

Generic templates are visible **unqualified** after an import
(`BMap[u64]`, `bmap_new[u64]`), non-generic items **only qualified**
(`map.bset_new()`). So a file that uses both spellings for the same module
is not a style choice, it is the only thing that compiles. Every user of
`std.map` in this round is written that way, and it is the first thing that
will confuse the next reader.

None of the three was worked around silently: G1 is commented at the place
where the stride is measured, G2 and G3 are visible in the call style of
`tests/1610`.

---

## 7. Honest limits

The per-file lists live in the headers of the modules; these are the ones
that matter beyond one file.

* **D1 no streaming.** Everything in `deflate.fi` works on a whole input in
  memory. A 4 GiB file cannot be compressed by it.
* **D2 the decoder decodes bit by bit** (`puff`'s scheme), not through a
  multi-level table. Correct, and 4–6× slower than zlib's decoder.
* **D4 gzip: one member.** `cat a.gz b.gz` reads as `a`.
* **J1 object lookup is linear**; **J2 the nesting depth is limited to 200**;
  **J3 a lone surrogate becomes U+FFFD**; **J7 duplicate keys are both
  kept**.
* **A1 AES-128 only** — 192 and 256 need a different key schedule and are
  not built, so the vectors for them are not in the repository either.
* **A3 the cipher is not constant time.** S-box lookups indexed by secret
  data leak through the cache. SPEC 9 promises `secret` and
  `#[constant_time]` for exactly this and stage 0 does not implement them
  for tables. Written in the header of `aes.fi` as well, so that nobody
  finds out later.
* **A4 no GCM, no authenticated encryption**, and no RSA — the round did not
  reach the big-number arithmetic that Minecraft's online mode also needs
  (the 1024 bit RSA key exchange). SHA-1, AES/CFB8 and JSON are three of the
  four pieces; **RSA is the missing fourth** and is the obvious first item
  of the next round.
* **H1 neither hash is cryptographic.** A map fed with attacker-chosen keys
  wants a secret seed (`hash_seed_from_time`) or SipHash, which is not
  built.
* **M2 keys in `BMap` are limited to 4 GiB each**, and **M1** the key blob
  shrinks only at a rehash.

---

## 8. State of the acceptance

| | |
|---|---|
| `bash test.sh` | **PASS 501/501** — sections 1–40, `tests/1610`–`1613` in three build stages each |
| `tools/fixpoint.sh` | stage 2 == stage 3, **byte identical** |
| `tools/self_compare.sh` | **0 diverging, 0 broken** |
| `git status --porcelain` | empty |

The two failures that `docs/ROUND76.md` §4.6 recorded as inherited are
still recorded there; nothing in this round touches them.
