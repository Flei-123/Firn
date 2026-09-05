# LICENSING.md -- why this repository is under two licences

Decision of 27 August 2026. Justin (Flei123) is the sole author of Firn,
Osum, OrientOS and the browser requirements, so the change needed nobody
else's agreement.

Until this date the whole repository was MIT (`LICENSE.MIT.old`, kept
verbatim). It is now:

* **GPL-2.0-only** for the compiler and for the browser engine Certus,
* **MIT** for the runtime and the standard library.

The machine readable answer for any single file is its
`SPDX-License-Identifier:` line. This document says why the line runs where
it does.

---

## 1. Why GPL and not MIT

MIT lets anybody take the work, close it and sell it without giving
anything back. For a library that is often the right trade. For an
operating system and a browser it is not: the point of building them is
that they stay open, and the GPL is the only licence that enforces that.
Whoever ships a changed OrientOS has to ship the changes.

That is the whole of reason one, and it applies to the compiler and to
Certus. It does not apply to the runtime -- see section 3.

## 2. Why version 2 ONLY, and not version 3, and not "or later"

**GPLv3 section 6 requires "Installation Information" for User Products:**
if you ship GPLv3 software inside a consumer device, you must also ship
whatever a user needs to install their own modified version on that device
-- including the signing keys.

That would make one specific thing legally impossible: **binding firmware
to the machine it came with, as a theft deterrent.** A device that only
runs software signed for it is a device that is worthless to a thief. Under
GPLv3 section 6, shipping that device with GPLv3 software would oblige the
manufacturer to hand out the keys that defeat it.

This is not a theoretical worry and it is not a new one. **Linux is
GPLv2-only for exactly this reason**, and Linus Torvalds said so in public
when GPLv3 was drafted; Android carries GPLv2-only for the kernel and
permissive licences above it for the same practical reason. Firn, Osum and
OrientOS follow that path deliberately.

**"or any later version" is left out on purpose.** With that clause the
Free Software Foundation could effectively relicense this work later by
publishing a GPLv4 -- a decision that would be out of Justin's hands. Every
licence header here therefore says `GPL-2.0-only`, and every SPDX line says
`GPL-2.0-only`, never `GPL-2.0-or-later`.

**The cost, stated honestly:** GPL-2.0-only is **incompatible with
Apache-2.0** (Apache's patent-termination clause is an additional
restriction that GPLv2 clause 6 forbids), and it is incompatible with
GPLv3 code. That means this project can never absorb Apache-2.0 or GPLv3
source. Section 5 checks what that costs today. The answer is: nothing.

## 3. Why the runtime has to be MIT

Every program compiled by `firnc` gets runtime code linked into it:

* the standard library it imports (`lib/std`, and through it `lib/rt`,
  `lib/str`, `lib/num`, `lib/math`, `lib/mem`),
* the tracing collector, which the compiler injects on its own -- the
  program does not even ask for it. `lib/gc/gc.fi` says so at the top: "The
  compiler embeds this file with `include_str!` and pulls it in
  automatically as an additional module as soon as a `gc class` occurs
  inside the program", and `lib/gc/gcvec.fi` is appended to it,
* the arithmetic panic trampoline plus its message text, which
  `compiler/src/panic_rt.rs` writes into **every object file** that
  contains a checked operation (`codegen_x86.rs:365-375`,
  `codegen_a64.rs:567-570`).

If those were GPL, **every program ever compiled with `firnc` would be
GPL**. Nobody would write a program for this system, and the compiler would
be a compiler nobody can use for their own work. That is not a side effect
worth accepting; it is the difference between a usable toolchain and an
unusable one.

GCC has the same problem and solves it with the **GCC Runtime Library
Exception** -- a separate legal document bolted onto the GPL. The simpler
route is taken here: **the runtime and the standard library are simply
MIT**. No exception text to get wrong, no argument about what "eligible
compilation process" means, and the answer fits in one sentence:

> You may write a program in Firn, link it against the runtime and the
> standard library, and ship it under any licence you like, including a
> closed one.

`compiler/src/panic_rt.rs` and `panic_rt_a64.rs` are MIT for the same
reason even though they are compiler files: what they emit is copied
verbatim into the user's binary, so the emitted text has to be MIT, and the
cleanest way to guarantee that is to license the file that produces it.

---

## 4. The boundary, file by file

Rule: **anything that is linked into a program the user compiles is
runtime, and runtime is MIT.** Where a file could be argued either way, it
went to MIT -- a boundary drawn too tightly makes the system unusable, and
that failure is worse than the licence being one file too generous.

Counts are regular files on `main` (symbolic links are not separate files).

### MIT -- 92 files

| path | files | why |
|---|---:|---|
| `lib/std/**` | 29 | the standard library. A program imports it directly. Includes `lib/std/crypto/**` (15) -- SHA, AES, HMAC, X25519, ECDSA, RSA: general-purpose primitives, not browser code |
| `lib/rt/**` | 4 | `rt.fi` is the allocator and `Buf`; `vec.fi`, `map.fi`, `intern.fi` are the containers. `lib/std/{rt,vec,map,intern}.fi` are symlinks to these four |
| `lib/gc/**` | 3 | the collector. **Injected by the compiler without the program asking.** Would have been the single worst file to leave under GPL |
| `lib/rc/**` | 14 | `RefCount`/`WeakRef`. `lib/rc/rc.fi` is a symlink to `tests/modules/rc.fi`, which is therefore MIT as well |
| `lib/str/**` | 14 | text. Pulled in by `lib/std/str.fi` through `tools/strlib/expand.py` |
| `lib/num/**` | 8 | number formatting and parsing |
| `lib/math/**` | 1 | mathematics |
| `lib/mem/**` | 1 | memory |
| `lib/test/**` | 1 | the test harness; linked into every test binary a user writes |
| `lib/generated/**` | 1 | `unicode_tables.fi`. `lib/str/ucd.fi` imports it, so it has to travel with `lib/str` |
| `tests/modules/rc.fi` | 1 | the real file behind `lib/rc/rc.fi` |
| `compiler/src/panic_rt.rs`, `panic_rt_a64.rs` | 2 | their output is copied verbatim into the user's object file |
| `demos/**` | 13 | starter code. Meant to be copied into somebody's own program; GPL there would be a trap |
| `examples/**` | 5 | same |
| `bench/tokenizer/**` | 4 | a benchmark harness that links `html5ever`. MIT also sidesteps the GPLv2 / Apache-2.0 question entirely (section 5.2) |

### GPL-2.0-only -- everything else

| path | files | why |
|---|---:|---|
| `compiler/src/**` (except the two above) | 63 | the compiler. Not linked into anything it compiles |
| `lib/firnc1/**` | 22 | the compiler again, stage 1, written in Firn |
| `bin/**` | 6 regular | the driver programs of the compiler (`firnc1.fi`, `astdump.fi`, `firdump.fi`, `lexdump.fi`, `layoutdump.fi`, `semadump.fi`). The other `bin/*.fi` are symlinks into `lib/firnc1` and `lib/rt` |
| `lib/browser`, `lib/css`, `lib/dom`, `lib/font`, `lib/html`, `lib/js`, `lib/layout`, `lib/net`, `lib/paint`, `lib/tls` | 108 | **Certus.** A browser engine, not a runtime. No ordinary program links it, and it is exactly the kind of work reason 1 is about |
| `tools/**` (except `bench/tokenizer`) | ~430 | measurement and generation. Runs on the host, ships nowhere |
| `tests/**` (except `tests/modules/rc.fi`) | 2,297 | the test suite |
| `test.sh`, `test_opt.sh`, docs | | |

**Two judgement calls that are worth flagging**, because they could
reasonably go the other way:

1. **`lib/tls` and `lib/net` are GPL.** They are general-purpose libraries
   and a case can be made for MIT. They went GPL because they are Certus's
   network stack, they are not part of the standard library a plain program
   imports, and `lib/std/net.fi` (the syscall facade, MIT) is what a plain
   program actually uses. Flip them to MIT if you ever want a TLS client to
   be reusable by closed programs.
2. **`demos/` and `examples/` are MIT.** Nobody asked for that; it follows
   from the rule "in case of doubt, MIT". Example code that a beginner
   copies into their own program must not drag the GPL along with it.

---

## 5. Does GPL-2.0-only conflict with anything already in the tree?

Checked against every entry in `THIRD_PARTY.md`. **No blocking conflict was
found.** Details:

### 5.1 The one real rule: Apache-2.0 is out

GPL-2.0-only and Apache-2.0 cannot be combined. Apache-2.0 imposes a patent
retaliation condition; GPLv2 clause 6 forbids imposing "any further
restrictions". The FSF states this outright. Nothing in this repository is
Apache-2.0 today, and nothing may become Apache-2.0 in the future.

### 5.2 The 37 Rust crates behind `html5ever`

Queried against the crates.io API on 27 August 2026, all 37 resolved by
`bench/tokenizer/Cargo.lock`:

* **MIT OR Apache-2.0** (dual, so MIT can be chosen): `html5ever`,
  `markup5ever`, `tendril`, `string_cache`, `string_cache_codegen`,
  `serde`, `serde_core`, `serde_derive`, `parking_lot`,
  `parking_lot_core`, `lock_api`, `rand`, `rand_core`, `libc`, `syn`,
  `proc-macro2`, `quote`, `smallvec`, `log`, `bitflags`, `cfg-if`, `futf`,
  `mac`, `utf-8`, `siphasher`, `scopeguard`, `windows-link`.
* **MIT only:** `phf`, `phf_shared`, `phf_generator`, `phf_codegen`,
  `new_debug_unreachable`, `precomputed-hash`, `redox_syscall`.
* **(MIT OR Apache-2.0) AND Unicode-3.0:** `unicode-ident`. The MIT branch
  is available; the Unicode-3.0 part is a permissive notice requirement.

**No crate is Apache-2.0-only.** So even if `bench/tokenizer` were GPL,
the MIT branch of every dual licence could be taken and there would be no
conflict. It is MIT anyway, which removes the question.

### 5.3 The DejaVu glyphs -- compatible, but the notice is still missing

`tests/data/fonts/FirnSans.ttf` (14,396 octets, a DejaVu Sans subset) is
under the Bitstream Vera / DejaVu licence, which is permissive and
GPL-compatible. It is test data and is not shipped.

The compliance defect found in round INVENTORY stands and is **not** made
better or worse by this licence change: the licence text is not in the tree.
See `THIRD_PARTY.md` section 4.10. The equivalent problem in the *shipped*
system is in the Osum repository, not here.

### 5.4 Test data and Unicode

* **Web Platform Tests**, `test262` -- BSD-3-Clause. Compatible.
* **JSONTestSuite** -- MIT. Compatible.
* **css-parsing-tests** -- CC0. Compatible.
* **NIST CAVP vectors** -- no stated terms; almost certainly public domain
  US government work. Not code, not linked, not shipped.
* **Unicode Character Database** -- the Unicode licence, permissive and
  GPL-compatible. It is DATA, translated at build time into
  `lib/generated/unicode_tables.fi`, which is MIT here.
* **`testdata/realweb/`** -- four full Wikipedia articles under CC BY-SA
  4.0. **This was a defect under MIT and it is still a defect under GPLv2**,
  and the reason has not changed: they are redistributed without the
  attribution and share-alike notice CC BY-SA requires. Test data, never
  shipped, but it needs fixing. See `THIRD_PARTY.md` 4.8.
* **`Ahem.ttf`** -- public domain. Compatible with everything.

### 5.5 Build tools

`as`, `ld` (binutils, GPL-3.0), `gcc`, `qemu`, `xorriso` -- GPLv3 tools
running on GPLv2 source is not a licence question at all. The GPL binds
distribution of the tool, not of the tool's output. No conflict.

`rustc`/`cargo` (Apache-2.0 OR MIT): building a GPL-2.0-only program with an
Apache-2.0 compiler is fine for exactly the same reason -- nothing of the
compiler ends up in `firnc` except the Rust standard library, which is
available under MIT.

**Result: nothing has to be removed or replaced because of this licence
change.**

---

## 6. What this changes for somebody using Firn

| you want to ... | may you? |
|---|---|
| write a program in Firn and sell it closed | **yes.** Runtime and standard library are MIT |
| use `lib/std/crypto` in a closed product | **yes.** MIT |
| embed the Firn *compiler* in a closed product | no. GPL-2.0-only -- publish your changes |
| ship a changed Certus | no, not closed. GPL-2.0-only |
| ship a device that runs OrientOS and only boots signed firmware | **yes.** That is the entire reason for GPLv2 instead of GPLv3 |
| take Apache-2.0 code into the compiler or Certus | **no.** Incompatible. Look for an MIT, BSD or ISC alternative |


---

## 7. The SPDX headers -- what got one, and what could not

Applied as one separate commit, deliberately, so a rebase can treat it in one
go (it touches almost every file).

| | files |
|---|---:|
| `SPDX-License-Identifier: MIT` written into the file | **52** |
| `SPDX-License-Identifier: GPL-2.0-only` written into the file | **497** |
| **together** | **549** |

By kind: 260 `.fi`, 79 `.rs`, 106 `.py`, 91 `.sh`, 6 `.c`, 3 `.s`, 3 `.toml`,
1 `.ld`.

**Files that deliberately did NOT get a header, and why.** Every one of them
still has a licence -- assigned by path in `.reuse/dep5`, which is the
machine-readable fallback and is not breakable by a generator.

| what | count | why a header would break it |
|---|---:|---|
| everything under `tests/` except `tests/modules/rc.fi` | 2,297 | **line 1 is the test expectation.** `tools/testrunner/src/main.rs:14-19`: "The expectations stand in line 1 of the test program" -- `// expect_exit:`, `// expect_out:`, `// expect_error: L:C`. A header in line 1 fails every one of them |
| `examples/*.fi`, `demos/**`, `bench/firn/*.fi`, `tools/escape/**`, `tools/dwarf/*.fi`, `tools/core/soak.fi`, `tools/lsp/sample.fi`, `tools/phi/loops.fi`, `docs/gdb_example.fi` | 59 | same reason. `test.sh:319` runs `examples/*.fi` through the same runner. **This was found by a scan for "line 2 now starts with `expect_`" after the headers had been written, and reverted** |
| generated files | 20 | `lib/std/{core,math,num,str}.fi`, `lib/generated/unicode_tables.fi`, `lib/html/entities_data.fi`, `lib/html/error_codes.fi`, `lib/browser/{tag,quirks_data,foreign_data}.fi`, `lib/css/encoding_data.fi`, `lib/dom/ua_data.fi`, `lib/firnc1/gctext.fi`, `tools/strlib/src/std_*.fi`, `tools/dtoa_vectors/dtoa_stream.fi`, `tools/ucd/{gen_ucd,ucd_real}.fi`. Their generator would have to emit the line too |
| `lib/str/**`, `lib/num/**`, `lib/math/**`, `lib/mem/**` | 24 | **inlined verbatim** into the generated `lib/std/*.fi` by `tools/strlib/expand.py` (`//#include`). A header here makes `expand.py --check` report 16 files out of date -- measured, then reverted |
| `lib/rc/parts/**`, `lib/rc/arc.fi` | 15 | `lib/rc/gen_tests.sh` copies them into `tests/5xx`, `tests/8xx` and `tests/neg/` and takes **line 1 of the part file as line 1 of the generated test** |
| symbolic links | 43 | no content of their own (`bin/*.fi` -> `lib/firnc1/*.fi`, `lib/std/{rt,vec,map,intern}.fi` -> `lib/rt/*`, `lib/rc/rc.fi` -> `tests/modules/rc.fi`) |
| `testdata/**`, `tests/data/**`, `tools/ucd/*.txt`, `tools/mcserver/node_modules/**` | 1,796+ | foreign material. Not ours to mark |

**One generated file WAS regenerated on purpose:** `lib/firnc1/gctext.fi`.
It is `lib/gc/gc.fi` + `gcvec.fi` + `gcmap.fi` packed into u64 words, and
`tools/fixpoint.sh` compares it against what `tools/gen_gctext.sh` produces
today. Since `lib/gc/*.fi` now carry MIT headers, the packed copy was rebuilt
with `bash tools/gen_gctext.sh` so the invariant stays green.

### What was verified after the headers were written

| check | result |
|---|---|
| `cargo check` / `cargo build --release` on `compiler/` | **passes**, only the pre-existing dead-code warnings |
| `python3 tools/strlib/expand.py --check` | **0 files out of date** |
| `python3 tools/ucd/expand_tables.py --check` | up to date, sha256 `00e7741c7ecede89` |
| `python3 tools/ucd/expand.py --check` | up to date, sha256 `85598afb75cb4ae0` |
| `firnc examples/hello.fi` and run | prints "Hallo Welt aus Firn!", exit 0 |
| `firnc examples/{tour,structs,fib}.fi` and run | exit 0 / 42 / 89 -- their declared `expect_exit` |
| a program importing `std.io` and `std.core` (MIT headers) | compiles and runs |
| scan: any file whose line 2 now starts with `expect_` | **0** |

**Not run**, because it needs the full suite and a long wall clock:
`./test.sh`. Run it before merging this branch anywhere.
