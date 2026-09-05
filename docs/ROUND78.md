# Round 78 — the system is called Osum, and the acceptance stands on its own

Branch `r78-osum`, on top of `main` (`cbff66ca`).

Two subjects. They have nothing to do with each other except the same
idea: this repository should depend on as little outside itself as
possible — not on a name that has been abandoned, and not on a browser
that happens to be installed.

1. **The rename.** The operating system is called **Osum**. The previous
   name is gone from the repository — not aliased, not deprecated, gone.
2. **Chromium out of the mandatory acceptance.** The layout cross-check
   against a real browser stays, and it stays a hard pass criterion. What
   goes is the requirement that the browser be running while the suite is.

> **A note on how this document is written.** The old name does not appear
> in it, and not in any other file either. The exact mapping is in the
> message of commit `86392c79` — git objects are not part of the working
> tree, so the proof `grep -ric <old name> .` = 0 stays true. Repeating
> the table here would have broken the very thing it documents.

---

## 1. The rename

**47 occurrences in 9 files.** The inventory was taken BEFORE anything was
replaced, with

```
grep -rin '<old>' --exclude-dir=.git --exclude-dir=target \
     --exclude-dir=.test-work --exclude-dir=.layout-work .
grep -roih '<old>[a-z_]*' … | sort | uniq -c | sort -rn
```

so the mapping covers the spellings that are really there instead of the
ones somebody remembers. Nine variants existed: the capitalised system
name, its lower-case form, the bare stem, the panic symbol, that symbol in
capitals, and four assembler labels built from the stem.

| File | Occurrences |
| --- | --- |
| `ROADMAP.md` | 14 |
| `SPEC.md` | 13 |
| `DESIGN_GOALS.md` | 13 |
| `compiler/src/strings.rs` | 2 |
| `docs/ROUND52.md`, `docs/RC.md`, `PLAN.md`, `GAUNTLET.md`, `ACCEPTANCE.md` | 1 each |

Upper and lower case were replaced separately, and the longest names
first, so the panic symbol could not be turned into the new stem plus a
leftover `_panic`. No file NAME contained the word
(`find . -iname '*<old>*'` → 0), so nothing had to be moved with `git mv`.

The references to the neighbouring project — the browser repository next
door — are renamed in the TEXT here as well. **Nothing outside this
repository was touched**; renaming that directory is its owner's business.

### Proof

```
grep -ric <old> .   (without .git, target, .test-work, .layout-work, .self-work)
0
find . -iname '*<old>*'
0
python3 tools/english/check_lengths.py    ->  wrong length values: 0
cargo test --release                      ->  202 passed, 0 failed
```

`check_lengths.py` matters more than it looks: Firn writes fixed-length
byte fields as `var w: [u8; N] = "…"` and repeats `N` at the call site. A
rename that changes a string's length and leaves one of those numbers
behind does not crash — it silently truncates. On this branch no such
string was affected (the symbol lives only in prose here), and the checker
says so rather than the author.

---

## 2. Chromium leaves the mandatory acceptance

### What was wrong with it

`tools/layout/run.sh` started a real Chromium in sections 3, 3a and 4, and
`tools/layout/minquota_chrome.txt` made the result a **pass criterion**.
Three things follow, and all three are bad:

* **The suite measured the machine.** Without a 200 MB foreign program
  installed, the acceptance could not pass.
* **A deviation had two possible causes.** Debian updates Chromium every
  few weeks, so "three boxes off" might have been this engine or might
  have been Blink. Round 72 lost real time to exactly that (the 87 px
  viewport, below).
* **There was a hole.** When no browser was found, the section printed
  `NOT RUN: no Chromium found` — and the run still said **OK**. A
  cross-check that switches itself off when it is inconvenient is not a
  cross-check.

### What was built instead

The browser is asked **once**, and its answer is data in the repository:

| File | Content | Size |
| --- | --- | --- |
| `tools/layout/reference/cases.json` | 1087 boxes of the 146 cases in `tools/layout/cases/` | 69 KB |
| `tools/layout/reference/stack.json` | 5171 probe points of 15 cases, and the topmost element at each | 208 KB |
| `tools/layout/reference/realweb.json` | 60 611 boxes of the 8 real pages in `testdata/realweb/` | 4.2 MB |

Every file carries a header, because a measurement without provenance is
an opinion:

```json
"_header": {
 "chromium": "Chromium 151.0.7922.137 built on Debian GNU/Linux 12 (bookworm)",
 "chromium_path": "/usr/bin/chromium",
 "created": "2026-08-22",
 "layout_viewport": [800, 600],
 "window_size": [800, 687],
 "refresh_with": "bash tools/layout/run.sh --refresh-reference"
}
```

`window_size` is 800×687 and not 800×600 on purpose. `--window-size` sets
the WINDOW, not the layout viewport, and this Chromium reserves 87 px of
browser interface even under `--headless`, so `--window-size=800,600` lays
out into 800×513. Branch `r72-arith` had already written the fix — a probe
that ASKS the browser which viewport it handed out and corrects the window
size once, instead of hard-coding 87 — but that branch was never merged.
`tools/layout/chrome.py::window_size_for` is taken over from it here,
because without it the frozen numbers would have carried the 87 px error
**for good**.

### The pieces

* **`tools/layout/reference.py`** (new) — `load` / `save` plus the header.
  `load()` **raises** when the file is missing. That is the fix for the
  old hole: a reference that is not there has to be a FAILURE, never a
  section that quietly skips itself.
* **`harness.py`, `stack.py`, `realweb.py`** gained `--reference` (read
  the frozen answer, start nothing) and `--write-reference` (ask a live
  browser and write the file). `--no-chrome` still means "do not compare
  against the browser at all" and is what section 2 uses.
* **`stack.py` freezes the PROBE POINTS along with the answers.** The
  points used to be derived from the CURRENT engine's boxes. Freezing only
  the answers would have meant that a later engine change moves the points
  and the frozen answers then belong to different questions. A probe point
  is a fixed place on the page — freezing the question together with the
  answer makes the comparison stricter, not looser.
* **`stack.py` measured live with a raw `--window-size`** and therefore
  carried the same 87 px error. It goes through `window_size_for()` now.
* **`chrome.py`** got `chromium_version()` and a record of what the last
  live measurement really used (`LAST_EXE`, `LAST_WINDOW`), so the header
  states facts instead of reconstructing them afterwards.
* **`tools/layout/run.sh`** takes `--live-chromium` (check against a newer
  browser without touching the frozen data) and `--refresh-reference`
  (rewrite all three files). Neither is reachable from `test.sh`, which
  calls `bash tools/layout/run.sh --fast` — the frozen path.
* **A missing measurement is a failure in every mode now.** The `NOT RUN`
  branch is gone from all three sections.

### Why this does not weaken the measurement

The comparison is still box-for-box against a **foreign engine**, written
by other people from the same specification, and every deviation still
counts. Unchanged: the same 146 cases, the same tolerance (1/64 px, a
browser's `LayoutUnit`), the same hard quotas —
`minquota_cases.txt` 1087, `minquota_chrome.txt` 1087,
`minquota_stack.txt` 5171.

What changed is only WHEN the browser was asked. Two things got better:

* A deviation now has exactly ONE possible cause. The other side cannot
  move any more.
* Adding a case without refreshing the reference does not silently pass:
  the case is reported as `NOT IN THE REFERENCE`, its boxes are missing
  from `chrome_total`, and the quota in section 6 catches the drop.

The honest cost: the frozen data is only as good as the browser that
produced it. That is why the version and the date are in the header and
why `--live-chromium` exists — a check against a newer Chromium is one
command; it is just not something the acceptance may depend on.

### The numbers

Frozen (the default, and what `test.sh` runs):

```
== 3. cross-check against Chromium -- the frozen reference ==
   frozen reference: Chromium 151.0.7922.137 …, 2026-08-22, layout viewport 800x600
   against Chromium: 1087 / 1087 boxes equal, deviation 0.00%
== 3a. the paint order against Chromium ==
   paint order against Chromium: 5171 / 5171 probe points equal in 15 cases, deviation 0.00%
LAYOUT OK: 1087 / 1087 own boxes, 1087 / 1087 equal to Chromium (deviation 0.00 %),
           paint order 5171 / 5171 -- the frozen reference
```

Live, the same day, for comparison — identical:

```
LAYOUT OK: 1087 / 1087 own boxes, 1087 / 1087 equal to Chromium (deviation 0.00 %),
           paint order 5171 / 5171 -- a LIVE Chromium (rewriting the reference)
```

`testdata/realweb` is reported, not required (it never was): **990 of
60 611 boxes to the bit, 98.37 % deviation**, median 1433.91 px, and 1.66 %
of the boxes within half a pixel. Those pages need tables, replaced
elements with an intrinsic size and presentational attributes, and none of
that exists yet. The number is frozen so the gap is a MEASUREMENT and not
an opinion — and so that the next round can watch it move.

### The proof that no browser is needed

A `PATH` was built containing everything the suite uses EXCEPT anything
named `chromium*`, `chrome*` or `*headless_shell*`, and `FIRN_CHROMIUM`
was unset:

```
PATH=/tmp/nochrome
no chromium in PATH
find_chromium() -> None
…
LAYOUT OK: 1087 / 1087 own boxes, 1087 / 1087 equal to Chromium (deviation 0.00 %),
           paint order 5171 / 5171 -- the frozen reference
RC=0
```

`chrome.py::find_chromium()` really returns `None` under that `PATH` — the
section is not passing because it found a browser somewhere else.

---

## 3. A side effect worth its own paragraph: the font was not reproducible

`tools/layout/make_font.py` builds `FirnMetric.ttf`, and `fontTools`
stamped `head.created` / `head.modified` with the CURRENT time. Twelve
octets of the file therefore changed on **every** layout run (the two
dates plus the checksums over them), and `git status` was dirty after
every run of the suite. That is how a project trains everybody to ignore a
dirty tree.

The date is pinned now (`FONT_EPOCH`), and the file is byte-identical
across runs — verified by building it twice and comparing. The reference
measurements were regenerated afterwards, so the frozen data belongs to
the font that is actually in the repository.

---

## 4. What the mandatory acceptance still needs from outside

The inventory asked for. **Nothing here was removed** — removing a check
to make a list shorter is cheating. This is the state and a proposal.

| Program | Where | Hard or soft |
| --- | --- | --- |
| **`node`** | `tools/js/compare_node.sh`, from `tools/js/run.sh` section 4 | **HARD.** The script prints `FAILED: node is not installed -- the cross check cannot run` and exits 1. |
| `node` + `npm` + `node-minecraft-protocol` | `tools/mcserver/run.sh` point 4 (round 76) | soft — prints `SKIPPED` and says why |
| `qemu-system-x86_64` | `tools/kernel/run.sh` | soft — prints `KERNEL: skipped`, but then the section proves nothing |
| `gcc` / `cc` | `tools/extfn/run.sh` (the C ABI, round 75) | soft — `SKIP: <tool> is missing` |
| `cargo` / `rustc` | `firnc0`, the bootstrap compiler | hard, and inherent — this is the dependency the fixpoint exists to make irrelevant |
| `as`, `ld` (binutils) | both compilers shell out to them | hard, and by design: Firn emits assembly (SPEC) |
| `python3` | every harness under `tools/` | hard; the harnesses are scaffolding, not the product |

### Proposal for the node case (`tools/js/compare_node.sh`)

The same recipe as this round. `compare_node.sh` runs a set of small
programs through `node` and through `lib/js/` and diffs the output.
`node`'s output for a fixed program does not change. So:

* `tools/js/reference/node.json` — program name → the exact stdout `node`
  produced, plus a header with `node --version` and the date;
* `--reference` by default, `--refresh-reference` to re-ask a live `node`;
* a program in the corpus with no entry in the reference is a FAILURE,
  not a skip.

Mechanical, and the smaller half of the work.

### Proposal for the Minecraft case (round 76, `tools/mcserver/`)

Harder, and more interesting, because there the foreign program is a
CLIENT talking to our server — a conversation, not a function. Freezing
answers is not enough; the questions have to be replayed.

* Record a full `node-minecraft-protocol` session against the current
  server as a **packet transcript**: every frame the client sent, in
  order, with length and direction, and every frame the server sent back —
  `tools/mcserver/recordings/nmp_login.bin` or similar.
* The replay harness (Python first, Firn later) opens a socket to the
  server under test, sends the recorded CLIENT frames in order and holds
  the server's replies against the recorded ones. Fields that legitimately
  differ per session — random values, timestamps, keep-alive IDs — are
  named in a small mask file rather than ignored wholesale.
* The recording carries a header: which client library, which version,
  which protocol number (765 / 1.20.4), and the date.
* `npm install` disappears from the acceptance; refreshing a recording
  stays a manual command, exactly like `--refresh-reference` here.

There is a precedent in the same round: the offline UUID for `Notch` was
captured once from a real `java -jar server.jar` 1.20.4 and is a
**constant** in `tools/mcserver/run.sh` today. Nobody thinks that weakened
the check. The transcript is that idea, one size up.

**Not this round's work.** Written down so the next round does not have to
rediscover it.

---

## 5. Round 72 is still not mergeable — and now it is known why

This round was told to wait for `r72-arith` (checked integer arithmetic,
`+% -% *%`, `+| -| *|`, the panic runtime) to reach `main`, because that
branch introduces the panic symbol and renaming it twice would be silly.
It never arrived: the round that was to merge it stopped. So the merge was
tried here — and the result is a finding worth keeping.

The merge itself is easy. Three conflicts, all "both sides added
something to the same place":

| File | What collided | Resolution |
| --- | --- | --- |
| `lib/firnc1/codegen.fi` | `struct Cg` gained `tr`/`ext_out` (round 75, `extern fn`) on one side and `pmsg`/`site` (round 72) on the other | keep both |
| `test.sh` | sections 36–39 (rounds 75/76) against section 40 (round 72) | keep both, 40 last |
| `tools/layout/FirnMetric.ttf` | both sides had rebuilt the font | `main`'s, and then §3 above |

And the self-hosting survives it untouched: **stage 2 == stage 3
character-identical, 692 907 lines of assembly, corpus 316 same
behaviour / 0 differing / 0 faulty**, both before and after the rename.

What does NOT survive is everything rounds 73–76 built in the meantime.
Round 72 makes `+ - * / %` and every narrowing or sign-flipping `as`
CHECKED, and the code written after it — the JS engine, the GC, `std.bytes`,
the NBT writer, the Minecraft server — was written when arithmetic
wrapped. `MC_FAST=1 ./test.sh` on the merged tree: **16 of 1166 failed.**

| Where | Panic |
| --- | --- |
| `tests/1600_net_echo.fi:51` | `u64 * u64` — an LCG, wrapping on purpose |
| `lib/std/bytes.fi:393` (`put_varint`) | `i32 as u32` on −1 — Java-edition VarInts are two's complement |
| `tests/1602_nbt_roundtrip.fi:42` | `i8 - i8` — `0 - (-128)` while building the NBT edge cases |
| `lib/js/interp.fi:636` | `i32 as u32` on `i32::MIN` — ECMAScript `ToUint32` is defined as wrapping |
| `lib/gc/gc.fi:1992` | `u64 - u64` (a=5 b=16) — a counter going below zero, and this one may well be a REAL bug |
| `tools/nbt/bigtest.fi:131` | `u32 as i32` on 2³¹ |
| `tools/mcserver/run.sh` | 7 errors, all downstream of `std.bytes` |
| `tools/kernel/run.sh` | 9 failures (QEMU 124, ring-3 faults 4 instead of 2) |
| `tools/fmt/run.sh` | 1 — the tree is not formatted after the merge |
| `tests/834_arc_thread.fi`, `tests/860_thread_basic.fi` | signals 9 / 14, under a machine running four suites at once — not established as merge damage |

**That is a round of work, not a side effect of a rename**, so round 78
does not carry it. The merged state is kept as the branch
**`r78-r72probe`** (merge commit `57f94ab1`) so nothing has to be redone.

The way through is known and was tried on the three `std.bytes` cases:

* Arithmetic that is MEANT to wrap gets round 72's own operators:
  `*%`, `+%`, `-%`. That is what they were built for.
* Casts have no unchecked spelling — round 72 deliberately left `as%` out
  (`docs/ROUND72.md`). So a protocol that wants the OCTETS instead of the
  value writes the reinterpretation out, through 64 bits, where the round
  trip is the identity:

  ```
  fn bits_u32(v: i32) -> u32 { return (((v as i64) & (4294967295 as i64)) as u32) }
  fn bits_i32(v: u32) -> i32 { return ((((v as i64) ^ (2147483648 as i64))
                                        - (2147483648 as i64)) as i32) }
  ```

  With `bits_u8`/`bits_i8`/`bits_u16`/`bits_i16` alongside and used in
  `put_i8`/`put_i16`/`put_i32`, `get_i8`/`get_i16`/`get_i32`,
  `varint_size`, `put_varint`, `get_varint` and `zigzag_encode`, all three
  failing `tests/160x` pass. That patch is on `r78-r72probe`'s working
  notes, not in this branch.
* `lib/gc/gc.fi:1992` should be looked at BEFORE it is silenced. A
  reference count that goes from 5 to −11 is the kind of thing a checked
  subtraction exists to find.

The honest summary: round 72 did not fail to merge because merging is
hard. It failed to merge because it is a language change that four later
rounds have not been held against yet — and now there is a list.

---

## 6. Acceptance of this round

Every number below was run, none was estimated.

| Check | Result |
| --- | --- |
| `grep -ric <old name> .` (without `.git`, `target`, work dirs) | **0** |
| `find . -iname '*<old name>*'` | **0** |
| `bash tools/english/check.sh` | 0 identifiers, 0 text sites, 0 lengths, 0 path names, 0 comment lines |
| `cargo test --release` | 202 passed, 0 failed |
| `tools/fixpoint.sh` | **stage 2 == stage 3, character-identical**, 617 667 lines of assembly, 3 621 184 octets each |
| `tools/self_compare.sh` | 315 same behaviour, **0 differing, 0 faulty**, 0 codegen missing |
| layout, frozen reference | **1087 / 1087** boxes, **5171 / 5171** probe points, deviation **0.00 %** |
| layout, live Chromium 151 | identical |
| layout with no browser in `PATH` | RC 0, `find_chromium() -> None` |
| `make_font.py` run twice | byte-identical |
| kernel (`tools/kernel/run.sh`) | 174 passed, 0 failed |
| freestanding (`tools/freestanding/run.sh`) | 41 passed, 0 failed |
| core (`tools/core/run.sh`) | 46 proofs, 0 failures |
| `MC_FAST=1 ./test.sh` | **1169 of 1170** — one failure, and it is not this branch's (below) |

### The one failure: `tools/js/round66.sh`, the promise endurance run

```
jobs   rc=-11    RSS first 10080 KiB  max 11628 KiB  growth +1548 KiB
```

`rc=-11` is SIGSEGV, and it is **intermittent**. `docs/ROUND76.md` §4.6
already recorded it as a failure of `main`. It was established again here,
because "inherited" is a claim:

* **The same soak, eight runs on each tree, same machine, same minute:**

  | Tree | SIGSEGV |
  | --- | --- |
  | `main` (`cbff66ca`) | **5 of 8** |
  | `r78-osum` | **2 of 8** |

  The branch crashes LESS often than `main`. Both crash. It is one bug,
  and it is older than this branch.

* **It cannot come from here.** Against `main` this branch changes exactly
  two files outside documentation and `tools/layout`:
  `compiler/src/strings.rs` (a string literal inside a `#[test]`) and
  `lib/firnc1/parser.fi` (the indentation of five continuation lines). The
  crashing program is `.js-work/jsrun.r66`, built by `firnc0` out of
  `lib/js/` — neither file is on that path.

The crash itself is worth a round of its own: an abandoned promise queue
under the collector, which is where a dangling reference would show.

### The layout section got BETTER, not just browser-free

`docs/ROUND76.md` §4.6 recorded section 23 on `main` as **1082 of 1087
boxes, 0.46 % off**, with five boxes wrong in four cases
(`a4_abs_icb`, `a2_fixed_bottom_right`, `a3_fixed_percent`,
`a7_sticky_bottom`). Those five are exactly the cases that ask where the
BOTTOM of the viewport is — the 87 px window/viewport confusion. Taking
`window_size_for()` over from `r72-arith` fixes them: **1087 of 1087,
0.00 %**, live as well as frozen. The frozen reference therefore records a
comparison that is more correct than the one `main` performs today.
