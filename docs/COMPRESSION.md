# Compression formats (`lib/compress/`)

zstd, xz/LZMA2, Brotli, bzip2 and LZ4 -- decoders for all of them, encoders
for zstd, xz/LZMA2/`.lzma` and LZ4 -- plus a streaming gzip/zlib/DEFLATE
inflater, one door for all of them (`compress.auto`), and the integrations
that were the reason to write them: `.tar.zst`, `.tar.xz`, `.tar.bz2`,
`.tar.lz4` in `std.tar`/`std.extract` (Adoptium, Modrinth packs and Linux
packages ship in them) and `Content-Encoding: br` and `zstd` in `net.http`.
Everything is Firn: no foreign library, no `extern fn`.

| module | file | decoder | encoder |
|---|---|---|---|
| `compress.common` | `common.fi` | -- | one error set `CompressError`, the input `Source` (memory or file descriptor), xxHash32/64, CRC-64/XZ, the window helpers |
| `compress.bitio` | `bitio.fi` | -- | LSB-first bit reader and canonical prefix code trees (shared by Brotli and gzip) |
| `compress.lz4` | `lz4.fi` | blocks, frames (linked/independent blocks, block and content checksums, content size, DictID with a dictionary, skippable frames, the legacy frame), streaming | the "fast" family: independent blocks 64 KiB..4 MiB, content checksum, streaming |
| `compress.zstd` | `zstd.fi` | RFC 8878 in full: raw/RLE/compressed blocks, Huffman (1 and 4 streams, FSE-compressed or direct weights) and FSE entropy coding, repeat tables, dictionaries, skippable frames, several frames, XXH64 checksum, streaming | `zstd_enc.fi`: levels 1-19 (fast / hash chain / lazy), Huffman literals, FSE tables with the cheapest of predefined/RLE/own/repeat, repeat offsets, dictionaries, streaming |
| `compress.lzma` | `lzma.fi` | LZMA, LZMA2 (every chunk type), the `.xz` container (CRC-32, CRC-64, SHA-256, none; several blocks and streams, stream padding, index and footer cross-checked), filters delta, x86, PowerPC, IA-64, ARM, ARM-Thumb, SPARC, ARM64, legacy `.lzma`, streaming | `lzma_enc.fi`: presets 0-9, real LZMA coding with all contexts and repeat distances, LZMA2 chunks with stored fall-back, `.xz` with any check, `.lzma`, streaming |
| `compress.bz2` | `bz2.fi` | one or more streams, the obsolete "randomised" blocks, streaming (bounded memory even for huge run-length expansion) | -- |
| `compress.brotli` | `brotli.fi` | RFC 7932 in full: windows 2^10..2^24, stored and metadata meta-blocks, block switching, context maps with all four context modes, simple and complex prefix codes, the distance ring and short codes, NPOSTFIX/NDIRECT, the 122,784 octet static dictionary with its 121 transforms, streaming | -- |
| `compress.gzip` | `gzip.fi` | streaming gzip (several members, header fields and CRC, zero padding), zlib, raw DEFLATE | (`std.deflate` writes them) |
| `compress.auto` | `auto.fi` | magic-octet detection: gzip, zstd, xz, bzip2, lz4, zip; whole buffer and streaming; `compress()` for gzip/zstd/xz/lz4 | |

```firn
import compress.auto

let kind: u32 = auto.detect(p, n)                              // auto.KIND_ZSTD ...
auto.decompress_limited(p, n, 1 << 30, &out) catch |e| ...      // any of them, with a size limit

var d: auto.AutoDec = auto.auto_dec_open_fd(fd) catch |e| ...   // streaming, format found in the first octets
let k: usize = auto.auto_dec_read(&d, buf, 65536) catch |e| ... // 0 = end
auto.auto_dec_close(&d)

auto.compress(auto.KIND_ZSTD, p, n, 3, &out) catch |e| ...      // gzip, zstd, xz, lz4
```

Each module also has its own API (`zstd.zstd_decompress_dict`,
`zstd_enc.zstd_compress_dict`, `lzma_enc.xz_compress_check(..., CHECK_SHA256, ...)`,
`lz4.lz4_frame_compress_opts`, `brotli.brotli_decompress_limited`, ...); each
file's head comment lists it.

## How it is built

**One error set.** Firn's `try` wants the same error set on both sides, and
error names are program-wide, so every module returns `CompressError`
(`Data`, `Format`, `Checksum`, `Truncated`, `TooLarge`, `OutOfMemory`,
`Unsupported`, `Dictionary`, `Io`) and the facade can pass them on with a
plain `try`.

**A decoder is a state machine that decodes ONE UNIT per step** (a zstd block
<= 128 KiB, an LZMA2 chunk <= 2 MiB, a bzip2 block <= 900 KB, an LZ4 block <=
4 MiB, a Brotli meta-block slice, a DEFLATE slice of ~64 KiB) from a `Source`
into a growing buffer. The whole-buffer call loops over steps. The streaming
reader lets the caller drain the buffer between steps and `buf_compact` keeps
only the window the format may still reach back into -- so memory is
**window + one unit**, never the whole output, and the input is never held
whole when it comes from a descriptor (`src_take` hands out contiguous octets
from a 64 KiB refill buffer, or from a scratch copy when a unit straddles two
reads; `src_peek` lets `compress.auto` look at the first octets without
consuming them).

**Limits are real.** Every whole-buffer decoder has a `max` (0 = none) and
refuses with `TooLarge` BEFORE writing past it, per block/chunk/command -- a
frame that announces a huge content size, a bzip2 run-length bomb and an
LZMA2 chunk header that claims 2 MiB are all refused before the output grows.
The streaming decoders take the same limit (`auto_dec_set_limit`). For
`std.tar` the limit is `max_archive_size` and applies to the inflated tar of
every format.

## What was held against what

`tools/compress/check.py` (section 81 of `test.sh`, `tools/compress/run.sh`)
runs the library, compiled in the four build levels, the AArch64 build under
`qemu-aarch64` and the Windows build under Wine, against programs nobody here
wrote -- libzstd (the `zstd` command), liblzma (`xz` and Python `lzma`),
libbz2, libbrotli, liblz4 (Python's `lz4`), zlib/gzip:

| direction | what |
|---|---|
| reference -> library | a corpus (empty, 1..100 octets, text, source code, an ELF binary, random, zeros, long runs, sparse, skewed alphabet, 4-symbol DNA, machine-code-like data) compressed by the reference at every interesting setting (zstd 1/3/9/19/22 with `--long`, small windows, no checksum; xz presets 0/6/9e, every check, delta, every branch filter, lc/lp/pb extremes, `.lzma`; lz4 default/HC/linked/independent/every block size; brotli qualities 0-11, window 10-24, text mode; bzip2 1/5/9; gzip levels 0/1/6/9, multi-member, header CRC, FEXTRA, zero padding; zlib; raw DEFLATE with every strategy) decoded whole-buffer AND streaming from a descriptor in pieces of 1, 4093 and 65537 octets: octet for octet |
| library -> reference | what our encoders write (zstd levels 1/3/6, xz presets 1/3/6/9 with every check, lz4, `.lzma`; whole buffer AND streaming in pieces of 1, 4097, 1 MiB) read back by the reference decoder: octet for octet |
| dictionaries | zstd dictionaries trained by `zstd --train` and raw-content dictionaries, both directions, at levels 1/3/7/19; wrong and missing dictionary id; damaged dictionaries |
| hostile | every cut of a stream (must be an error), 120 flipped-bit copies and 60 junk inputs per format (an answer or an error, never a crash or a hang), a 64 MiB zeros bomb under limits of 1 MiB, exactly 64 MiB and one octet less |

Two more tools hold the claims that are about behaviour over time and size:

* `tools/compress/fuzz.py` mutates the fixtures (bit flips, overwrites with 0/0xFF/0x7FFFFFFF,
  deletions, insertions, duplicated stretches, splices of two streams, truncation) and runs the probe whole
  buffer and streaming on a **release-safe** build, where an arithmetic overflow or an index out of
  range traps: 240,000 probe runs (3 seeds x 40,000 mutations x 2 modes) gave an answer or an
  `ERROR` every time -- no crash, no hang, no limit passed.
* `tools/compress/memtest.py` decodes 300 MiB of output per format through the streaming API with
  the output thrown away and prints the peak resident set: **zstd 3 MiB, xz 6 MiB, gzip < 1 MiB,
  bzip2 4 MiB, lz4 8 MiB, Brotli 8 MiB** -- window + one unit, the output size does not matter
  (octet count and xxHash64 of the 300 MiB are checked against the generator's).
  `tests/2189` is the leak test: 150 rounds of every decoder and encoder, the error paths too,
  must not move the resident set or leave a descriptor open.

`tests/2180` .. `2189` run in every build level of `test.sh` (and on AArch64):
fixtures made by the reference tools (`tools/compress/gen_fixtures.py`,
deterministic, ~320 KB), refusals, the encoders' round trips at every level and
switch, the streaming readers from a descriptor, the integrations, and
`common.fi`'s input source.

## Honest notes

* **Encoders are not libzstd/liblzma.** Levels 1-2 of the zstd encoder are a
  one-table greedy matcher, 3-4 a hash chain, 5-19 a lazy chain whose depth
  grows (no binary-tree matcher, no optimal parsing, no long-distance
  matching); xz uses a hash-chain finder and liblzma's "fast" parser, not
  `bt4` + optimal parsing. Ratios and speeds are in the table below; at the
  same level the files are a little larger than libzstd's and 5-12 % larger
  than `xz -6`. LZ4 is the "fast" family, not LZ4HC.
* **No Brotli or bzip2 encoder.** `compress.auto.compress` says
  `Unsupported` for them. (`Content-Encoding: br` is for *receiving*.)
* **Speed.** The decoders run at a fraction of the C libraries (table below):
  the format code is correct and bounded in memory first. The measured numbers
  are what they are; nothing in the doc is a promise beyond them.
* **Window and dictionary caps.** zstd refuses windows above 128 MiB by default
  (`zstd_dec_set_window_max` changes it), the streaming xz reader refuses
  dictionaries above 256 MiB (`xz_dec_set_dict_max`): they must hold a window.
  Whole-buffer calls keep the output anyway and have no cap beyond `max`.
* **zstd** (Z1-Z3 in `zstd.fi`): a dictionary is parsed again at the start of
  every frame that uses it; the dictionary id is compared with the frame's, there
  is no digest of a dictionary in the format. **zstd_enc** (E1-E3): with a
  dictionary the content is history and its repeat offsets seed the encoder,
  its entropy tables are not reused.
* **xz** (X1-X4): unsupported check ids are skipped, not verified (xz does the
  same with a warning); RISC-V BCJ (xz 5.6) is `Unsupported`; legacy `.lzma` is
  whole-buffer only; trailing garbage after the last stream is `Format`.
* **bzip2** (B1-B3): bytes after the last stream that do not start another
  `BZh1`..`BZh9` are ignored (like Python's `bz2.decompress`); a first stream
  that is not bzip2 is `Format`.
* **Brotli** (R1-R3): bytes after the last meta-block are ignored; the "large
  window" extension is `Unsupported`; the 122 KB dictionary, the context lookup
  tables and the transform table are in `brotli_data.bin`, lifted from the
  reference implementation by `gen_brotli_data.py` and checked against the
  published SHA-256 of the dictionary (see `THIRD_PARTY.md`).
* **gzip** (G1-G3): trailing bytes after the last member that are neither zero
  padding nor another member are `Format`; zlib with a preset dictionary is
  refused; a gzip member has a window of its own (a distance into the previous
  member is `Data`, held by a fixture whose second member does exactly that).
* **tar.\*** (E1 of `std.tar` stands): the whole tar is inflated into memory
  before it is parsed; the limit makes that safe, it does not make it streaming.
* **net.http**: `Accept-Encoding: gzip, br, zstd` is sent when gzip is on
  (`client_set_gzip`); br and zstd bodies are refused above 256 MiB decoded
  (`HttpError::Encoding`).

## What it costs to use

No dead-code elimination in the linker: a program that imports `std.tar` or `std.extract` now
carries all decoders (**+420 KB**: the 608 KB test binary of `tests/2071` became 1.0 MB, compile
time +1.2 s); a program that imports `net.http` carries Brotli and zstd (**+245 KB** of the 581 KB
`http_main`, 125 KB of it the Brotli dictionary). A program that wants one format only imports
that module (`compress.zstd`) and pays for that one.

## Compiler findings (and what the library does about them)

* **A miscompilation in `--opt-level=release-fast`**, found by this work's
  test 2180 while xxHash's tail did the wrong thing there only: when a second
  `while i < (*s).used` follows a first loop `while i + 4 <= (*s).used` over
  the same index and both read the bound through a pointer, release-fast
  skips the second loop. The other three levels are right.
  `tools/compress/repro_release_fast_loop.fi` is the 40-line repro (prints
  `f441eb8` everywhere but release-fast, which prints `ba5aa115`); copying the
  bound into a local first (`let used = (*s).used`) makes it right again,
  which `lib/compress/common.fi` does. It is in the Firn roadmap.
* `static` initialisers, `#[allow_escape]` on functions that hand the address of
  a local array to a buffer helper, `size_of[T]()` not accepting a module-qualified
  type (the decoders measure themselves with a two-field struct instead) --
  all worked around, none needs a compiler change.

## Numbers

@@BENCH@@

## Files

```
lib/compress/common.fi bitio.fi lz4.fi zstd.fi zstd_enc.fi lzma.fi lzma_enc.fi bz2.fi brotli.fi brotli_data.bin gzip.fi auto.fi
tests/2180_compress_lz4.fi .. 2188_compress_common.fi   tests/data/compress/ (fixtures)
tools/compress/probe.fi          the program check.py drives (whole buffer and streaming, every format)
tools/compress/check.py          the comparison with the reference tools (both directions, hostile inputs)
tools/compress/http_check.py     net.http against a real server
tools/compress/bench.fi/.py      the throughput table
tools/compress/gen_fixtures.py   the fixtures; gen_brotli_data.py   brotli_data.bin
tools/compress/run.sh            all of the above for test.sh
```
