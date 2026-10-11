# Certus compiler merge -- map of `/root/firnc-gc` against main (r68, B1-B3)

State: 09.10.2026. The stages that are on main are marked with their commit in
section 6; this file is the map and the record.

## 1. What `/root/firnc-gc` is

* **Not a git tree.** A directory with `compiler/` and `lib/gc`, `lib/test` (the
  two library directories the compiler embeds with `include_str!`) plus
  `*.vor-*` backups. 12 of its 77 source files carry local patches, 65 are
  byte-identical to the base it was started from.
* **Origin.** The public MPL history of Firn ("A line", root `f1a371788`, tag
  `vor-mpl-umzug`, branch `windows-auf-main` = `4e64d7a54`). That line was
  started on 05.09.2026 as a squash of the archive branch `sammeln`:
  `compiler/src` of `ac7022096` ("Firn 0.3", the state of round SAMMELN) is
  byte-identical to the tip of `sammeln` apart from the licence header.
  `sammeln` forked from main at `ecec89f7f` (04.09.2026) and never came back.
  The fork is therefore **sammeln + four Windows commits + the Certus patch
  series** (`certus-ui/vendor/firn/firnc-gc/compiler-0009 .. 0021`,
  `vendor/firn/patches/compiler-0005, 0007`) applied by hand.

How the map was made (nothing guessed): the blob hash of every fork file was
searched in all 2 900 commits (65 exact hits in the A line; the 12 others are
the Certus patches); then, per file, a 3-way merge of the fork into main with
base `ecec89f7f` (`git merge-file`): 17 files conflict (108 hunks), 14 merge
clean; every conflict was read.

## 2. What is already on main (the list in LUECKEN B1-B3 is out of date)

| item | where on main |
|---|---|
| float register allocation `xmm-ra` (B3), TEMPO 1-15 | `regalloc.rs`, merged 24.09.2026 (`0d4e1a30`), docs/TEMPO1..15.md |
| branch `opt-allgemein` (r132): `promote.rs`, WASM back end + SIMD, `unroll.rs`, `ivsr.rs` | merged 24-25.09.2026; test 1722 (nbody, r133) |
| `x86_64-windows` target, PE/COFF, Win32 seam, threads, async | `win.rs`, `win_seam.rs` (`db66ac827`, `6cce68494`, `2ba55551c`); 213 imports against 130 in the fork |
| `#[win_callback]`, `#[arch(...)]`, `--pic`, `--win-subsystem` | `attrs.rs`, `extfn.rs`, `fnval.rs`, `archsel.rs` (`c8bf10fea`), `main.rs` |
| `poll -> ppoll`, `setrlimit` on aarch64 (fork name `PollPpoll`, main `PpollMs`) | `syscalls.rs` (C-059) |
| the `setcc`/`rcx` allocator fix (Certus patch 0017) | `67fb5219` |
| inline GC write barrier (B4) | `gc_lower.rs::emit_barrier` |

## 3. Patch groups, what they need, risk, order

| # | group | files | size | risk | status |
|---|---|---|---|---|---|
| E1 | **GC runtime set**: ANLEGEWEG fast path; Certus M1 chunk index, vDSO clock, block-of cache, phase counters, `gc_cycle_finish`, `gc_bottom_swap`, class census, class-name word in the type table | `lib/gc/gc.fi` (+1250), `gc.rs`, `gc_lower.rs`, `lib/firnc1/{gc,lower,codegen,gctext}.fi` | ~1 500 lines | medium: the collector; state-block offsets are a contract with `gc.rs` (module test) | see 6 |
| E2 | small tables: 5 Win32 imports, `mremap` (aarch64 + browser) | `win.rs`, `syscalls.rs` | 15 lines | low | see 6 |
| E3 | inliner of the fork: trivial leaves outside the budget (0009), size-ordered phase (0011), recursive frame budget + compact frame (0015, 0014) | `inline.rs`, `regalloc.rs`, `main.rs`, `codegen_x86.rs` | 900 lines | medium: changes code generation of everything | 0009 ported, rest measured |
| E4 | **BILLIG** register allocator for aarch64 | `regalloc_a64.rs` (new, 571), `codegen_a64.rs` (+450) | 1 000 lines | medium: aarch64 only; tests under qemu | see 6 |
| E5 | **KODIERER** integrated assembler (opt-in `--asm-intern`) | `x86enc.rs`, `a64enc.rs`, `asm_x86.rs`, `asm_a64.rs`, `asm_intern.rs`, `elfobj.rs` (new, 6 400), `main.rs` | 6 500 lines | low: opt-in, new files; large | see 6 |
| E6 | **ARM-FREESTANDING** rest: `x86_64-none` / `aarch64-none`, A64 inline-asm register tables, kernel profile defaults, panic runtime for A64 | `target.rs` (Arch/Os axes), `core.rs`, `prof.rs`, `panic_rt*.rs`, `codegen_a64.rs`, `lib/firnc1/{parser,syscalls}.fi`, demos | 1 900 lines | medium: conflicts in 5 files with main's own target design | open |

Order used: E1 first (it is what Certus cannot build without: `unknown function
'gc_zaehle_klassen'`), E2 with it (the last link error of the Windows build was
`GetMessageExtraInfo`), E3 by measurement, then E4-E6 which Certus on x86 does
not need.

### What is deliberately NOT taken from the fork

| fork part | why not |
|---|---|
| `regalloc.rs` float path of Certus patches 0012 / 0014 / 0016 (floats through `rax`, direct xmm moves, compact frame) | main's `xmm-ra` allocator (TEMPO 1-15) does floats in registers properly; two float paths in one file cannot both be right |
| fork `win.rs` / `win_seam.rs` | main ported the same round with later fixes (threads, async, process); only the 5 missing import rows were taken |
| fork `archsel.rs`, `PollPpoll`, old inline barrier in `gc_lower.rs` | main has newer versions of all three |
| GROW default 2/1 of the fork's `gc.fi` | a policy for ALL programs (heap peaks near 3 x live; tests/771 goes red); main keeps 1/1 and gets `gc_set_growth(num, den)` -- Certus calls `gc_set_growth(2, 1)` |
| `/root/firnc-gc/lib/gc/gc.fi.base`, `*.vor-*` backups | backups of the fork itself |

### Traps found on the way

* **State-block offsets.** Two branches had put different words at the same
  offset 2152 (`S_THREAD_HI` of the thread table work on main, `S_HEAPLO` of
  ANLEGEWEG): the text merged clean, the numbers collided. The Certus words now
  start at 2160 (`S_HEAPLO` ... `S_VDSO` 2336, `S_GROWN` 2344, `S_GROWD` 2352).
  Rule: after every merge into `lib/gc/gc.fi` check that no offset occurs twice.
* **Conservative scan in tests.** A test that counts live objects must keep
  everything on ONE object and wipe the dead frames (`scrub`) before the count,
  otherwise a stale pointer to an outgrown GcVec buffer adds its slots in
  release-fast (tests/2420).

## 4. Certus side (not touched by this work)

* Pins: `/root/certus-ui` (main) `vendor/firn/COMMIT` =
  `7893ea2c8d745e89f5e16ee75997d4ec1bb428c5` (in no local repo any more),
  `/root/certus` (branch `speed`) = `52a13e75...`. The builds do not use the pin
  at all: `FIRNC=/root/firnc-gc/compiler/target/release/firnc` in
  `build-c073.sh`, `tools/certus/daily/run_all.sh`, ...
* **Renamed API.** The census functions had German names in the fork. They are
  English on main (the English guard counts German identifiers). Two Certus
  files call them (`lib/browser/certus_app.fi`, `lib/browser/web1_main.fi`):

  | fork name | main name |
  |---|---|
  | `gc_zaehle_klassen` | `gc_count_classes` |
  | `gc_klasse_anzahl` / `_oktette` / `_name` / `_groesse` | `gc_class_count` / `_bytes` / `_name` / `_size` |
  | `gc_klassen_n` | `gc_class_total` |
  | `gc_laengen` / `gc_laenge_fach` | `gc_len_histogram` / `gc_len_bucket` |
  | `gc_feld_zaehle` / `gc_feld_zaehle64` | `gc_field_count32` / `gc_field_count64` |
  | `gc_zeigt_auf` / `gc_slots_zeigt_auf` | `gc_refs_to` / `gc_slot_refs_to` |
  | `gc_zeit_ty` / `gc_anzahl_ty` / `gc_scan_worte` | `gc_time_by_phase` / `gc_slices_by_phase` / `gc_scan_words` |

  `bash tools/certus/gc_api_rename.sh <dir>` does exactly this.
* **Growth factor.** Add `gc_set_growth(2, 1)` after `gc_init()` in the entry
  points (`window_main.fi`, `web1_main.fi`, `run_main.fi`, `windows/certus_main.fi`)
  to keep the fork's tuning; without it the collector runs at 1/1.
* The Certus `.std-win` / `.rt-win` / `.fui-win` trees are frozen copies of
  Firn's `lib/std`, `lib/rt`, `lib/fui` from 16.09.2026 plus patches; they are
  Certus' business and were not changed.

## 5. The gate

Method: the SAME Certus sources (a throw-away worktree of `certus-ui` at
`add528d0` with the two renamed files and the four `gc_set_growth` lines) are
built with `/root/firnc-gc` and with the new main compiler, everything else
frozen (the Firn libraries come from a frozen copy of main's `lib/` at
`bea55fa33`, the Certus `.std-win`/`.rt-win`/`.fui-win` copies unchanged).
Instruction counts come from callgrind (`tools/ht/icount.py` of Certus, workload
N = 30 000), which does not depend on the load of the host.

Results are in section 6.

## 6. Results and commits

(filled in as the stages land)
