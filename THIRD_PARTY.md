# THIRD_PARTY.md -- Firn (language, compiler, standard library, Certus)

What in this repository is NOT written here. Round INVENTORY, 27 August 2026.

**Base of every number in this file:** branch `main`, commit `ce28104bf`
("Round B6: Certus ..."). Counted with `git ls-tree -r main` and
`git show main:<path> | wc -l`, so the numbers hold for that commit and not
for a working tree. Other branches (`speed`, `b2-layout`, `k6-merge`, ...)
are not covered.

**The one distinction that matters** is marked on every entry:

* **RUNTIME** -- the foreign part is inside what gets shipped and runs.
  These are the only entries that limit independence.
* **BUILD** -- needed to produce the artifact, gone afterwards.
* **TEST** -- needed only to measure or to cross-check. Never shipped.

An overview across all four repositories lives in the OrientOS repository,
`THIRD_PARTY_OVERVIEW.md` (branch `inventory`).

---

## 0. Summary in numbers

| | files | lines |
|---|---:|---:|
| Firn source of the shipped system (`lib/**.fi`) | 200 | **162,113** |
| all `.fi` outside `tests/`, `testdata/`, `demos/`, `examples/` | 373 | 189,174 |
| bootstrap compiler, Rust (`compiler/**.rs`) | 65 | 52,865 |
| **foreign SOURCE CODE anywhere in the shipped system** | **0** | **0** |

File counts are **regular files only**. `main` carries 43 symbolic links
(9 of them under `lib/`, the rest in `bin/`, e.g. `bin/ast.fi` ->
`lib/firnc1/ast.fi`); they are counted once, at their target.

There is **no foreign source code in `lib/`**. Checked two ways: a search for
`SPDX-License`, `Licensed under` and `Copyright (c)` over `lib/` and
`compiler/src/` returns nothing, and the Rust compiler declares no
dependency at all (`compiler/Cargo.toml`, section `[dependencies]` is empty;
no `extern crate` anywhere; every `use` line in `compiler/src` starts with
`crate`, `std` or `super`).

What IS foreign in the shipped system is **data**, not code, and it is
foreign data that has been rewritten into Firn tables at build time:

| generated file | lines | generated from |
|---|---:|---|
| `lib/generated/unicode_tables.fi` | 3,020 | Unicode Character Database 17.0.0 |
| `lib/html/entities_data.fi` | 4,677 | WHATWG named character references (2,231 entries) |
| `lib/browser/quirks_data.fi` | 70 | the DOCTYPE lists of WHATWG HTML 13.2.6.4.1 |

That is exactly what `orientos-browser/DECISION.md` section 7.1 declares
binding (interpretation B1): every executable line is Firn, normative data
tables from the standards are adopted and translated at build time.

**Foreign parts in the running system: none, with one qualification.** The
browser needs a trust store, and it does not carry one:
`lib/net/http.fi:301` (`client_load_trust`) reads a PEM file whose path is
passed in on the command line (`lib/browser/window_main.fi:450`). On a Linux
host that file is the machine's own `/usr/share/ca-certificates` set, which
is Mozilla's CA list. So the ROOT LIST is foreign data supplied by the host
and is not in this repository at all. See section 5.

### What to replace first, from this repository's side

1. **Nothing in `lib/`.** There is nothing foreign to replace.
2. **Missing licence copies for the test corpora** (section 4). Costs an
   afternoon, and it is the only real legal defect on this side.
3. **`as` and `ld`** (section 3). The only build tools without which the
   compiler produces no output at all. Not an independence problem today --
   named because it is the last hard build dependency, not because it should
   be replaced now.
4. Everything else -- rustc, qemu, node, numpy, fontTools -- is a measuring
   instrument. Replacing a measuring instrument with your own is the one
   thing that would make the measurement worth less, not more.

---

## 1. Rust crates

### 1.1 The bootstrap compiler -- no dependencies

* **What / where:** `compiler/`, 65 `.rs` files, 52,865 lines.
* **Foreign code:** none. `compiler/Cargo.toml` has an empty
  `[dependencies]` section; `compiler/Cargo.lock` is 149 octets and lists
  only `firnc` itself.
* **Stage:** BUILD.

### 1.2 `tools/testrunner` -- no dependencies

* **What / where:** `tools/testrunner/src/main.rs`, 341 lines.
* **Foreign code:** none. The file `tools/testrunner/Cargo.toml` says so in
  its header comment: "Ohne externe Crates (nur std), damit `cargo build`
  offline laeuft."
* **Stage:** TEST.

### 1.3 `html5ever` -- the only declared external crate in the tree

* **What / where:** `bench/tokenizer/Cargo.toml`, one line:
  `html5ever = "0.27"`. The program that uses it is
  `bench/tokenizer/src/main.rs`. `bench/tokenizer/Cargo.lock` (8,381 octets)
  resolves that one line into **37 crate entries** -- among them
  `html5ever 0.27.0`, `markup5ever`, `string_cache`, `tendril`, `phf`,
  `serde`, `parking_lot`, `rand`, `libc`, `syn`, `proc-macro2`.
* **Origin and licence:** fetched from `crates.io` at build time (every
  entry in the lock file has
  `source = "registry+https://github.com/rust-lang/crates.io-index"`).
  **There is NO licence file for any of these 37 crates in this
  repository**, and none of them is vendored. The licences are whatever the
  crates carry upstream; this tree does not record them and does not check
  them.
* **What it is for:** the throughput yardstick for the HTML tokenizer.
  `html5ever` is Servo's tokenizer and is the number Firn's own tokenizer is
  measured against.
* **Cost of replacing:** zero, and replacing it would be wrong. The whole
  point of the entry is that the comparison is against somebody else's
  engine.
* **Stage:** TEST. Nothing in `lib/` and nothing in `compiler/` touches it.
  A build of the compiler or of the browser never runs `cargo` in
  `bench/tokenizer/`.

---

## 2. Python packages on the host

All Python in this repository is a test harness or a generator. **Nothing
Python is shipped.** Sorted by whether the package is in the standard
library or not.

**Standard library only** -- the great majority: `sys`, `os`, `subprocess`,
`json`, `struct`, `re`, `glob`, `argparse`, `time`, `hashlib`, `threading`,
`tempfile`, `zlib`, `socket`, `shutil`, `random`, `bisect`,
`urllib.request`, `socketserver`, `math`, `io`, `gzip`, `statistics`,
`posixpath`, `collections`, `decimal`, `fractions`, `datetime`, `email`,
`hmac`, `http`, `unicodedata`, `xml`, `html`.

Note `html.entities.html5` from the standard library is not just used, it is
the SOURCE of a shipped table -- see section 5.2.

**Foreign packages** -- five, all optional or test-only:

| package | used in | what for | stage |
|---|---|---|---|
| `numpy` | `tools/paintb3/reftest.py`, `textfit.py`, `font_check.py`, `png_check.py` | pixel arithmetic when comparing rendered images | TEST |
| `fontTools` | `tools/paintb3/font_check.py`, `tools/layout/make_font.py` | reads the metrics of a font as a second opinion; builds `FirnMetric.ttf` | TEST |
| `Pillow` (`PIL`) | `tools/paintb3/png_check.py:26` (`from PIL import Image`) | decodes the PNG that `lib/paint/png.fi` wrote, to check it | TEST |
| `html5lib` | `tools/html/oracle.py:126` | the reference HTML parser the tree builder is diffed against | TEST |
| `cssselect2` | `tools/css/harness_select.py:37` | the reference selector engine `lib/css/sel.fi` is diffed against. Wrapped in `try`/`except`; if missing the script prints "cssselect2 IS MISSING -- no cross-check possible." | TEST |
| `xxhash` | `tools/stdlib81/hashvectors.py:42` | the author's own C xxHash, as a second opinion on the vectors. Wrapped in `try`/`except` (line 42-45); without it the script falls back to the published vectors and says so (line 91) | TEST |

**No licence file for any of these six is present in this repository.** They
are installed from PyPI on the host and are not vendored.

**Cost of replacing:** they should not be replaced. Every one of them exists
to be an INDEPENDENT second opinion. Writing your own numpy or your own
html5lib and then measuring against it proves nothing.

---

## 3. C libraries and the toolchain

### 3.1 GNU binutils -- `as` and `ld`. The hard one.

* **What / where:** `compiler/src/target.rs:41-60` names them --
  `Target::X86_64 => "as"` / `"ld"`, `Target::Aarch64 =>
  "aarch64-linux-gnu-as"` / `"aarch64-linux-gnu-ld"`. They are invoked in
  `compiler/src/main.rs:1039` (`Command::new(t.assembler())`) and
  `compiler/src/main.rs:1080` (`Command::new(t.linker())`). The error
  message the compiler prints when they are absent says it plainly:
  `"cannot run '...': ... (binutils installed?)"`.
* **Origin and licence:** GNU binutils, **GPL-3.0-or-later**. Not in this
  repository; taken from the host distribution. **No licence file here.**
* **What it is for:** `firnc` emits assembly text and does not produce object
  code itself. `as` turns the `.s` into an `.o`, `ld` turns the `.o` into an
  executable. Without them the compiler produces nothing but a text file.
* **Does GPL-3.0 reach the output?** No. Running a GPL tool on your own input
  does not put the output under the GPL; only the tools themselves are GPL.
  This is the ordinary situation of every program compiled on Linux.
* **Cost of replacing:** an x86-64 and an AArch64 assembler plus an ELF
  linker. That is a real, bounded piece of work -- the instruction encodings
  are already known to the code generator -- but it buys nothing except the
  removal of the last hard build dependency. **Not worth doing now.**
* **Stage:** BUILD. Note the qualification: this is the only build dependency
  without which the project has no artifact at all.

Also from binutils, used only in test scripts: `readelf` (22 occurrences in
`.sh`), `objdump` (14), `objcopy` (10). **Stage: TEST.**

### 3.2 The Rust toolchain -- `rustc` / `cargo`

* **Origin and licence:** Apache-2.0 OR MIT (upstream). **No licence file
  here.**
* **What it is for:** builds `compiler/` -- stage 0 of Firn. Stage 1
  (`lib/firnc1/`, 22 files, 32,581 lines) is Firn compiled by stage 0.
* **Cost of replacing:** it disappears by itself the day stage 1 is
  self-hosting on its own. That is a project goal, not an inventory item.
* **Stage:** BUILD.

### 3.3 The AArch64 cross toolchain

* `aarch64-linux-gnu-as`, `-ld`, `-objcopy` -- see 3.1.
* `aarch64-linux-gnu-gcc` -- 8 occurrences in `.sh`, and it compiles
  `tools/aarch64/abi_host.c` (the AAPCS64 cross-check) and
  `tools/aarch64/qemu_mmap_probe.c`.
* **Stage:** BUILD for `as`/`ld` on the AArch64 target, TEST for `gcc`.

### 3.4 `gcc` on the host -- measuring instrument only

Compiles four C files, each of which says in its own header that it is an
instrument and not a dependency:

| file | what it measures |
|---|---|
| `tools/abi/host.c` | System V AMD64 ABI: "This file is a MEASURING INSTRUMENT, not a dependency: nothing in Firn uses it." |
| `tools/extfn/host.c` | a C program calling a Firn function (7 lines) |
| `tools/lexnum/ref.c` | glibc `strtod` as a third opinion on decimal parsing |
| `tools/lexnum/ref32.c` | glibc `strtof`, same for `f32` |

* **Licence:** GCC is GPL-3.0-or-later with the runtime library exception;
  glibc is LGPL-2.1-or-later. Neither is in this repository. **No licence
  file here.**
* **Stage:** TEST.

### 3.5 QEMU

* `qemu-system-x86_64` (5 occurrences in `.sh`), `qemu-aarch64` (user mode,
  for the AArch64 corpus).
* **Licence:** GPL-2.0 (upstream). Not in this repository.
* **What it is for:** running the freestanding and the AArch64 output.
* **Cost of replacing:** writing your own emulator to test your own kernel is
  the textbook case of a bad idea -- the bug then hides in the emulator.
  **Do not.**
* **Stage:** TEST.

### 3.6 Node.js

* 63 occurrences of `node` in `.sh`. The important users are
  `tools/js/compare_node.sh`, `tools/js/regexp_compare.sh`,
  `tools/js/round74.sh:111` ("the pattern engine against node, character for
  character"), and `tools/mcserver/run.sh`.
* **Licence:** MIT (upstream, plus V8 under BSD-3-Clause). Not here.
* **What it is for:** V8 is the second opinion for `lib/js/` (13 files,
  28,718 lines). Every deviation is then decided against ECMA-262.
* **Stage:** TEST. Explicitly optional: `tools/mcserver/run.sh:32` -- "`node`
  is optional. Without it point 4 is SKIPPED and said so".

### 3.7 `tools/mcserver/node_modules/` -- around 100 npm packages

* **What / where:** `tools/mcserver/node_modules/`, headed by
  `minecraft-protocol` (`node-minecraft-protocol`, PrismarineJS),
  `@azure/msal-node`, `axios`, `ajv`, `jsonwebtoken`, `protodef`, `uuid`,
  `lodash.*`, and their transitive dependencies.
* **Origin and licence:** npm. Each package carries its own `LICENSE` file
  inside `node_modules` (MIT for most, Microsoft MIT for `@azure/msal-node`)
  -- but **these are NOT part of the repository**: `.gitignore:116` excludes
  `tools/mcserver/node_modules/`, and `git ls-files | grep -c node_modules`
  returns **0**. They exist only in this working tree.
* **What it is for:** `tools/mcserver/run.sh:112-130` lets a real Minecraft
  client log into the server written in Firn.
* **Stage:** TEST, and optional at that.

---

## 4. Test data -- foreign material, foreign terms

`testdata/` and `tests/data/` together: **1,796 tracked files,
21,456,955 octets**. That is the largest block of foreign material in the
repository by far, and none of it is executed by anything that ships.
`testdata/README.md` states the rule: "fremde Testdaten (kein Fremdcode)".

**The general finding first: not one of these corpora carries a copy of its
upstream licence file, except `css-parsing-tests`.** The PROVENANCE and
MANIFEST files NAME the licence, which is more than most projects do, but
naming is not the same as shipping the notice, and for BSD-3-Clause and
CC BY-SA the notice is the condition.

### 4.1 Web Platform Tests -- 1,737 files

| directory | files | round |
|---|---:|---|
| `tests/data/wpt-ref/` | 854 | B3, reference rendering |
| `tests/data/wpt-css/` | 474 | B2, layout |
| `tests/data/wpt-dom/` | 346 | B4, DOM from JavaScript |
| `tests/data/html5lib/` | 63 | tree construction (`.dat`) |

* **Origin:** `https://github.com/web-platform-tests/wpt`, branch `master`,
  fetched 25 and 26 August 2026, path for path, by
  `tools/paintb3/harvest.py` and `tools/liveb4/harvest.py`. Documented in
  four `PROVENANCE.md` files.
* **Licence:** 3-clause BSD. Stated in
  `tests/data/html5lib/PROVENANCE.md` and
  `tests/data/wpt-dom/PROVENANCE.md`. **The WPT `LICENSE` file itself is NOT
  in this tree.** BSD-3-Clause requires the copyright notice and the
  conditions to travel with redistributions of the source; a repository that
  redistributes 1,737 WPT files without that file does not meet the
  condition.
* **What it is for:** the entire acceptance quota of Certus. Round B6 numbers
  come out of these directories.
* **Cost of replacing:** meaningless. A browser measured against tests it
  wrote itself is measured against nothing. **Keep. Add the `LICENSE`.**
* **Stage:** TEST.

The `tests/data/html5lib/` `.dat` files have their own wrinkle, recorded in
the PROVENANCE: they used to live in `html5lib/html5lib-tests` and were
deleted there with commit `224991ec...` ("Tree construction tests have moved
to WPT", 26 June 2026), so the files here come from the WPT location.

### 4.2 `testdata/html5lib-tokenizer/` -- the tokenizer suite

* 14 `.test` files, 6,810 cases, 1.7 MB.
* **Origin:** `https://github.com/html5lib/html5lib-tests`, commit
  `224991ec10db04f056a89eed8b0bd8695fd2950e`, fetched 13 August 2026
  (`testdata/README.md`).
* **Licence:** **not stated anywhere in this tree, and no LICENSE file
  present.** `testdata/README.md` argues only that they are "normative
  Testdaten, kein ausfuehrbarer Fremdcode". That is an argument about
  category, not about terms. **Must be checked and written down.**
* **Stage:** TEST.

### 4.3 `testdata/test262/` -- the ECMAScript suite

* `test262-subset.tar.gz`, 4.3 MiB, 32,893 files, plus `subset.sha256` with a
  sum per file.
* **Origin:** `https://github.com/tc39/test262`, commit
  `3655e7464de3d52643ecddd4b5f9f4f3e7f62398`, fetched 20 August 2026.
* **Licence:** BSD-3-Clause per `testdata/test262/MANIFEST.md`, which says
  the `LICENSE` is "inside the archive tree upstream". **It is not inside
  the archive here:** `tar tzf test262-subset.tar.gz | grep -i licen` returns
  nothing. Same defect as 4.1.
* **Stage:** TEST. The MANIFEST is explicit: "Nothing of test262 is part of
  the engine -- `lib/js/` contains no foreign code."

### 4.4 `testdata/json/` -- JSONTestSuite

* `JSONTestSuite.tar.gz`, 11 KiB, 340 files, plus `files.sha256`.
* **Origin:** `https://github.com/nst/JSONTestSuite` (Nicolas Seriot),
  fetched 22 August 2026.
* **Licence:** MIT per `MANIFEST.md`, which claims "the `LICENSE` file of the
  repository is inside the archive". **It is not:**
  `tar tzf JSONTestSuite.tar.gz | grep -i licen` returns nothing.
* **Stage:** TEST.

### 4.5 `testdata/css-parsing-tests/` -- the only one done right

* 76 KB, and it carries `testdata/css-parsing-tests/LICENSE`.
* **Licence:** **CC0 / public domain dedication**, "Written in 2013 by Simon
  Sapin". The file is present, verbatim, with the CC0 URL.
* **Stage:** TEST.

### 4.6 `testdata/crypto/nist-vectors.tar.gz` -- 960 KB

* Contents: `sha/SHA1LongMsg.rsp`, `sha/SHA256*.rsp`, `hmac/HMAC.rsp`,
  `aes/CBC*.rsp`, `aes/CFB8*.rsp`, `aes/ECB*.rsp`, plus `sha/Readme.txt`.
  Plus `files.sha256`.
* **Origin:** the NIST CAVP response files. Added with commit `daf2d9faa`
  ("Round 81, stage 4: lib/std/crypto ...").
* **Licence:** **no statement anywhere in this repository.** NIST CAVP
  material is a work of the US federal government and is generally treated as
  free of copyright, but that sentence is not written down here and the
  archive carries no terms. **Unclear, must be checked.**
* **What it is for:** the correctness proof for `lib/std/crypto/sha1.fi`,
  `sha256.fi`, `hmac.fi`, `aes.fi`.
* **Stage:** TEST.

### 4.7 `testdata/nbt/bigtest.nbt.gz` -- 507 octets

* **Origin:** `https://raw.githubusercontent.com/twoolie/NBT/master/tests/bigtest.nbt`
  (the `NBT` package by Thomas Woolford), fetched 21 August 2026,
  `testdata/nbt/MANIFEST.md`.
* **Licence:** **not stated, no LICENSE file. Unclear, must be checked.**
* **Stage:** TEST.

### 4.8 `testdata/realweb/` -- eight real web pages, 4,931,812 octets

This is the entry that needs the plainest sentence in the file.

| file | octets | source |
|---|---:|---|
| `wikipedia_en_rust.html` | 1,009,516 | en.wikipedia.org |
| `wikipedia_en_linux.html` | 978,810 | en.wikipedia.org |
| `rustdoc_vec.html` | 951,960 | doc.rust-lang.org |
| `whatwg_parsing.html` | 774,608 | html.spec.whatwg.org |
| `wikipedia_en_www.html` | 761,483 | en.wikipedia.org |
| `wikipedia_de_html.html` | 266,414 | de.wikipedia.org |
| `w3c_html52.html` | 154,701 | w3.org |
| `hackernews.html` | 34,320 | news.ycombinator.com |

* **Origin:** fetched with `curl -sSL` on 14 August 2026 and stored
  unchanged (`testdata/realweb/MANIFEST.md`).
* **Licence:** **the MANIFEST names the URL of every page and the licence of
  NONE of them.** Four of the eight are Wikipedia articles, which are
  **CC BY-SA 4.0 (and GFDL)** -- a share-alike licence with an attribution
  requirement. The WHATWG page is CC BY 4.0, the W3C page is under the W3C
  document licence, the rustdoc page is generated from MIT/Apache-2.0
  documentation, and the Hacker News page carries no free licence at all.
  This repository as a whole is MIT (`LICENSE`). Redistributing full
  CC BY-SA article HTML inside an MIT repository without the attribution and
  the licence notice is **the clearest licence defect on the Firn side.**
* **What it is for:** throughput measurement only
  (`tools/tokenizer/throughput.sh`, corpus `realweb`). The MANIFEST is
  explicit that these are not correctness data and do not touch the quota.
* **Cost of replacing:** low, and here replacement IS worth it. The corpus
  needs pages that are big, messy and real -- it does not need these
  particular pages. Swapping the four Wikipedia pages and the Hacker News
  page for pages under a licence that permits redistribution, or fetching
  them at measuring time instead of storing them, removes the problem
  entirely. Alternatively: keep them and add a `LICENSES.md` with the
  attribution CC BY-SA requires.
* **Stage:** TEST.

### 4.9 `tests/data/tls-chains/` -- six real certificate chains

* `example.com.pem`, `www.google.com.pem`, `www.rust-lang.org.pem`,
  `en.wikipedia.org.pem`, `www.cloudflare.com.pem`, `github.com.pem`.
* **Origin:** harvested once on 26 August 2026 with
  `openssl s_client -connect <host>:443 -servername <host> -showcerts`,
  stored exactly as they came off the wire
  (`tests/data/tls-chains/PROVENANCE.md`).
* **Licence:** none, and none is needed -- an X.509 certificate is a public
  credential a server hands to anybody who asks. Named here because it IS
  third-party material.
* **What it is for:** `tools/tlsb5/cert_check.py` verifies each one three
  times: as it stands (against the machine's own `/etc/ssl/certs`), under a
  wrong name (must be refused as `NAME`), and past `notAfter` (must be
  refused as `EXPIRED`).
* **Note:** these certificates will expire, and the PROVENANCE says so; the
  three real-chain rows then turn into `EXPIRED` failures on purpose.
* **Stage:** TEST.

### 4.10 Fonts in the test data

**`tests/data/fonts/Ahem.ttf` -- 21,768 octets. TEST.**

* **Origin:**
  `https://raw.githubusercontent.com/web-platform-tests/wpt/master/fonts/Ahem.ttf`,
  26 August 2026.
* **Licence:** **public domain** (Todd Fahrner, 1995), per
  `tests/data/fonts/PROVENANCE.md`. The WPT suite around it is BSD-3-Clause.
* **What it is for:** every glyph is a square of one em, ascent 0.8, descent
  0.2 -- the font in which `font: 10px/1 Ahem` makes a layout assertion mean
  something. Large parts of the WPT `css/` area depend on it.

**`tests/data/fonts/FirnSans.ttf` -- 14,396 octets. TEST. Foreign outlines.**

* **Origin:** a **subset of DejaVu Sans**, produced here with `fontTools`;
  the exact script is in `tests/data/fonts/PROVENANCE.md`. 136 glyphs of the
  original 6,253, with the legacy `kern` table copied over (469 pairs).
* **Licence:** the **Bitstream Vera / DejaVu licence** -- named in the
  PROVENANCE, which also says "free to use, modify and redistribute,
  including in subsetted form". **The licence text itself is not in this
  repository.** The Bitstream Vera licence requires the copyright notice to
  be distributed with the font.
* **What it is for:** the PROVENANCE gives the reason and it is a good one --
  a font drawn here would only contain the features this reader already
  implements. DejaVu has 13 composite glyphs, consecutive off-curve points,
  and a real multi-segment format 4 `cmap`. Every one of those is a place a
  TrueType reader can be wrong.
* **Cost of replacing:** replacing it would make the test weaker. **Keep.
  Add the licence text.**

**`tools/layout/FirnMetric.ttf` -- 4,428 octets. TEST. Own work.**

* **Origin:** built by `tools/layout/make_font.py` (fontTools). Not derived
  from any font: units per em 1000, advance width 1000 for every glyph,
  ascent 800, descent 200, and the outline is "a filled rectangle from -200
  to 800". The metrics are the point; the outlines are a placeholder.
* **Licence:** own work, covered by this repository's MIT `LICENSE`.
* **Stage:** TEST (`tools/layout/stack.py`, `chrome.py`, `realweb.py`).

---

## 5. Data tables that end up in the shipped system

These are the entries that reach RUNTIME. They are DATA, not code, and
adopting them is exactly what `orientos-browser/DECISION.md` 7.1 declares
allowed.

### 5.1 Unicode Character Database -> `lib/generated/unicode_tables.fi`

* **What / where:** the inputs are `tools/ucd/UnicodeData.txt`
  (2,198,209 octets, 40,575 lines) and
  `tools/ucd/DerivedCoreProperties.txt` (1,134,783 octets). Both are
  tracked. The output is `lib/generated/unicode_tables.fi`, **3,020 lines,
  100,826 octets**, and it IS shipped.
* **Origin:** `https://www.unicode.org/Public/UCD/latest/ucd/`,
  **Unicode 17.0.0**, fetched 23 August 2026
  (`tools/ucd/SOURCE.md`). Both files are pinned by sha256 in
  `tools/ucd/UCD.sha256`:
  `2e1efc1d...` for `UnicodeData.txt`, `24c7fed1...` for
  `DerivedCoreProperties.txt`, and `tools/ucd/build.sh` step 0 refuses to
  run if either differs.
* **Licence:** `tools/ucd/SOURCE.md` links
  `https://www.unicode.org/terms_of_use.html`. **No licence text is in this
  repository.** Unicode data is distributed under the Unicode licence, which
  requires the copyright notice and the permission notice to accompany
  copies -- and this repository redistributes 3.3 MB of it plus a derived
  table. **Add the notice.** The permission itself is not in doubt; the
  notice is missing.
* **What it is for:** general category, `ID_Start`, `ID_Continue` and the
  case mappings, in a three-stage table with deduplicated blocks -- three
  loads, no loop, no branch, the same cost for every code point.
* **Cost of replacing:** **there is nothing to replace.** Maintaining your
  own Unicode database means maintaining Unicode, which
  `orientos-browser/DECISION.md` 7.1 calls "Unsinnig" in as many words
  (interpretation B0). The table generated FROM it is own work.
* **Stage:** BUILD for the `.txt` files, **RUNTIME for the generated table**
  -- but the runtime part contains no foreign code, only foreign facts.

### 5.2 WHATWG named character references -> `lib/html/entities_data.fi`

* **What / where:** `lib/html/entities_data.fi`, **4,677 lines**, `COUNT`
  = **2,231** entries. Generated by `tools/tokenizer/gen_entities.py` from
  `html.entities.html5` in Python's standard library, which is Python's copy
  of the WHATWG list. Read by `lib/html/entities.fi` (WHATWG HTML
  13.2.5.72 ff.).
* **Licence:** the list is normative data from the WHATWG HTML standard
  (CC BY 4.0 upstream); the copy travelled through CPython (PSF licence).
  **Neither notice is in this repository. Unclear which of the two applies;
  must be checked.**
* **Cost of replacing:** typing 2,231 entity names by hand is not
  independence, it is transcription with more errors.
* **Stage:** **RUNTIME.**

### 5.3 WHATWG DOCTYPE quirks lists -> `lib/browser/quirks_data.fi`

* **What / where:** `lib/browser/quirks_data.fi`, 70 lines, generated by
  `tools/domb1/gen_quirks.py`. The payload is one 2,070-octet string of
  around 55 legacy DOCTYPE prefixes ("-//W3C//DTD HTML 4.0 Frameset//",
  "-//WebTechs//DTD Mozilla HTML//", ...) from WHATWG HTML 13.2.6.4.1.
* **Licence:** normative data from the WHATWG standard, CC BY 4.0 upstream.
  **No notice here.**
* **Stage:** **RUNTIME.**

### 5.4 The CA trust store -- foreign data, NOT in this repository

* **What / where:** nothing. That is the point. `lib/tls/x509.fi`
  (1,242 lines) implements `store_load_pem`, and `lib/net/http.fi:301`
  (`client_load_trust`) reads a PEM file from a path handed in at
  `lib/browser/window_main.fi:450`.
* **Origin in practice:** on a Linux host that path is the system CA bundle,
  which is Mozilla's CA root list as packaged by the distribution.
* **Licence:** MPL-2.0 for Mozilla's list.
* **Behaviour without it, and it is the right behaviour:**
  `lib/net/http.fi:297-300` -- "A client with no trust store cannot fetch
  `https://` at all -- every chain comes back UNKNOWN_ISSUER ... a browser
  that falls back to 'trust it anyway' when it has no roots has no security
  at all."
* **Cost of replacing:** a browser cannot have its own root list in any
  meaningful sense; the roots ARE the third parties. What a shipped product
  needs is a decision on WHICH list to bundle, and that decision has not been
  made yet.
* **Stage:** **RUNTIME**, supplied from outside. **Open item.**

---

## 6. Built here from somebody else's specification

**This is not foreign code.** Implementing a written standard is own work,
line for line. The section exists so that the difference between foreign CODE
and a foreign IDEA is visible at a glance -- everything below is Firn written
here, and every line of it counts as own.

| what | where | lines | specification |
|---|---|---:|---|
| SHA-1, SHA-256, SHA-512 | `lib/std/crypto/sha1.fi`, `sha256.fi`, `sha512.fi` | in `lib/std` | FIPS 180-4 |
| HMAC | `lib/std/crypto/hmac.fi` | | RFC 2104 |
| HKDF | `lib/std/crypto/hkdf.fi` | | RFC 5869 |
| AES, GCM | `lib/std/crypto/aes.fi`, `gcm.fi` | | FIPS 197, SP 800-38D |
| ChaCha20 | `lib/std/crypto/chacha.fi` | | RFC 8439 |
| X25519 | `lib/std/crypto/x25519.fi` | | RFC 7748 |
| ECDSA, RSA, bignum | `lib/std/crypto/ecdsa.fi`, `rsa.fi`, `big.fi` | | FIPS 186-4, RFC 8017 |
| DER / ASN.1 | `lib/tls/der.fi` | 201 | ITU-T X.690 |
| X.509 path validation | `lib/tls/x509.fi` | 1,242 | RFC 5280 |
| TLS 1.3 | `lib/tls/tls.fi`, `keys.fi` | 1,226 + 183 | RFC 8446 |
| Ethernet, ARP, IPv4, ICMP, UDP | `lib/net/wire.fi` | 492 | RFC 826, 791, 792, 768 |
| TCP | `lib/net/tcp.fi` | 1,555 | RFC 9293 (793) |
| IP stack | `lib/net/stack.fi` | 599 | |
| DNS | `lib/net/dns.fi` | 369 | RFC 1035 |
| HTTP/1.1, cookies, cache | `lib/net/http.fi`, `httpstate.fi` | 1,569 + 943 | RFC 9110, 9111, 9112, 6265 |
| URL | `lib/net/url.fi` | 575 | WHATWG URL |
| deflate / zlib / gzip | `lib/std/deflate.fi` | | RFC 1950, 1951, 1952 |
| PNG | `lib/paint/png.fi` | 499 | ISO/IEC 15948 |
| JPEG | `lib/paint/jpeg.fi` | 893 | ITU-T T.81 |
| TrueType reader, rasteriser | `lib/font/ttf.fi`, `raster.fi`, `metrics.fi` | 1,910 total | OpenType / TrueType reference |
| JSON | `lib/std/json.fi` | | RFC 8259 |
| NBT | `lib/std/nbt.fi` | | Notch's NBT format |
| MD5 | `lib/std/md5.fi` | | RFC 1321 |
| HTML tokenizer + tree construction | `lib/html/` | 9,353 | WHATWG HTML 13.2 |
| DOM | `lib/dom/` | 2,914 | WHATWG DOM |
| CSS parsing, cascade, selectors | `lib/css/` | 9,703 | CSS Syntax 3, Selectors 4, Cascade 5 |
| Layout | `lib/layout/` | 8,150 | CSS 2.1, Flexbox, Sizing, Position, Align |
| JavaScript engine | `lib/js/` | 28,718 | ECMA-262 |
| Browser shell (Certus) | `lib/browser/` | 15,291 | |
| System V AMD64 ABI, AAPCS64 | `compiler/src/` | in 52,865 | AMD64 psABI, ARM IHI 0055 |
| ELF | `compiler/src/`, `lib/` | | System V gABI |

Where the standard is normative DATA rather than a rule -- the UCD, the
entity list, the DOCTYPE lists -- see section 5.

---

## 7. This repository's own licence

* **`LICENSE`, 21 lines: MIT License, "Copyright (c) 2026 Justin
  (Flei123)".** Present and complete.
* **Caveat:** the MIT text covers the code. It does NOT cover
  `testdata/realweb/` (section 4.8), the WPT corpora (4.1), `FirnSans.ttf`
  (4.10) or the UCD files (5.1), all of which are under other people's
  terms and are shipped inside this MIT repository without their notices. An
  `EXCEPTIONS` paragraph in `LICENSE`, or a `THIRD_PARTY_LICENSES/`
  directory, would fix that. It is bookkeeping, not engineering.

---

## 8. Open items -- things this round could not settle

1. **`testdata/html5lib-tokenizer/`** -- licence not stated anywhere and no
   LICENSE file. Must be checked upstream.
2. **`testdata/nbt/bigtest.nbt.gz`** -- same.
3. **`testdata/crypto/nist-vectors.tar.gz`** -- no terms recorded. NIST CAVP
   material is very probably free of copyright, but "very probably" is not a
   finding.
4. **`lib/html/entities_data.fi`** -- the table came through CPython's
   `html.entities.html5`. Whether the applicable notice is WHATWG's or the
   PSF's is not decided here.
5. **The 37 crates behind `html5ever`** -- licences not recorded anywhere in
   this tree. Test-only, so the risk is small, but the list is unaudited.
6. **Which CA root list a shipped Certus bundles** -- not decided (5.4).
