# Round 93 — the lock file, one version per name, and the three places where a build carried the name of its machine

**State before this round:** round 48 built the project system — the manifest
`firn.package`, a fixed search order, visibility at module level, package
cycles, module name clashes, the build driver `--package`, all of it in both
compilers. Its own closing section said what was missing: *„No network, no
registry, no lock file. […] Reproducibility across two machines
(`ACCEPTANCE.md` item 5) is therefore **not yet** fulfilled; checksums and a
`firn.sperre` are missing."* Item 5 stood at `[~]`.

**What is there now:** a lock file `firn.lock` with checksums over every
source file that took part in the build, `--lock` to write it and `--locked`
to insist on it, a version wish in `needs` with one resolution rule, and —
the part that actually decided the item — **three sources of
non-reproducibility in the compiler, found by measuring and removed.** All of
it in both compilers, with the same octets and the same sentences.

The measured result up front:

| | |
|---|---|
| `demos/packages/app` built by **firnc0** on machine A and machine B | `ba62c1fffe91a3d47fb8e91ba8710b074683da4cdf20eb384adb6db59ca4c7cc` — **identical** |
| the same, built by **firnc1** (the self hosted compiler) | `158436f67ecedd2736bad3ed3cc859271f96ca3e16bbf8a3c4be3ea22bbdcd17` — **identical** |
| `firn.lock` out of all four runs | `76b37bd79184bb508ec0912ac6f3bc6694fdeaff291f2634d0f2a033b296af8e` — **identical** |

Before this round, the same comparison over two directories alone gave
**3,562 of 6,840 octets different** in the artifact.

---

## 1. What „two machines" has to mean before it means anything

The criterion of item 5 is one sentence: *two different machines produce a
bit-identical artifact from the same source state*. Two halves hide in it,
and they fail for different reasons:

1. **The input has to be pinned.** Whoever says „same source state" has to
   be able to check it. That is the lock file.
2. **The output must not depend on the machine.** A compiler that writes its
   working directory into the binary cannot satisfy the criterion however
   well the input is pinned.

Round 48 built neither half. `tools/repro/run.sh` (also round 48) measured
the second one honestly and said out loud that it fails: two working
directories, four artifacts, two of them different. That script was the
starting point of this round, and it is the reason the round found the real
defect at all — a measurement that was already there and had been left red
on purpose.

## 2. The format of `firn.lock`

One line per statement, like the manifest. Six keywords, no nesting, no
quoting:

```text
lock 1
root app
package app 0.1.0 . 8db85d3efe06b22c794ef522eb51389c72de103e503ef251de2713bbd17de74d
package geo 0.2.0 ../geo 07c874fc081fca717f0e03918dd68eb627412075cac3f097442d0a0a1c9dc1f6
package text 0.1.0 ../text 092b111650885fb1f3f7f42372a2e2e879829c7c8eaf350e4ae2e868ebe31ff7
outside 0 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
total 71849bd30a572b41dab92e1126d98579068f3e52e3c17ac26857df42e3d44f4e
```

Every field is there for a reason, and every one of them is a statement
about the SOURCES, never about this machine:

* **the path is relative to the root package** (`.`, `../geo`) and computed
  purely lexically (`package::relative`). An absolute path would make the
  file useless on the second machine, which is the whole point of it.
* **the lines are sorted by package name.** After the resolution of
  section 4 one name means one package, so the order is total and does not
  depend on the order the manifests happened to be read in.
* **a package's checksum** runs over its manifest *and* every source file of
  it that took part in **this** build, each one as
  `relative path \n length in octets \n content \n`, sorted by that path.
  The path is in the stream so that renaming a file is a change; the length
  is in it so that no two different file sets can be glued into the same
  octet stream.
* **`outside`** covers the files that belong to no package — typically
  everything out of `$FIRNLIB`. They are keyed by their FILE NAME, not by
  their path: the place the standard library sits at is a property of the
  machine, its content is not. A lock file that ignored these inputs would
  be a lie, because they end up in the binary just as much.
* **`total`** is the checksum over the text of all the lines above it. That
  is what catches an edit by hand — a lock file whose sources are untouched
  but whose text was patched is refused (`tools/packages/run.sh`, case 27).

The checksums are **not** taken on trust. `tools/packages/run.sh` recomputes
them with `sha256sum` out of coreutils, in shell, from the same stream — a
third implementation of the format. If the two compilers and coreutils
agree, the format is a format and not „whatever the compiler happens to do".

### Why not `sha256sum -c`, why not tar, why not JSON

The same reason as for the manifest in round 48: everything here has to
exist **twice** — `compiler/src/lock.rs` in Rust and `lib/firnc1/lock.fi` in
Firn, the latter without libc, with buffers and `syscall`. That includes
SHA-256 itself, written out in both. What that costs is one hour and about
two hundred lines per side; what it buys is a lock file that the
self-hosted compiler can write, which the fixpoint needs.

## 3. `--lock` and `--locked`

```
firnc --package <dir> --lock       # write <dir>/firn.lock
firnc --package <dir> --locked     # build ONLY if it fits
firnc1 --package <dir> --lock      # the same, in Firn
firnc1 --package <dir> --locked
```

Three decisions in there, and the third one is the interesting one:

**`--package` alone does not write a lock file.** `cargo build` updates
`Cargo.lock` as a side effect; here the file is only written when it is
asked for. The reason is not taste, it is the test suite: `test.sh` builds
`demos/packages/app` several times, and a build that writes into the source
tree as a side effect turns every test run into a change to the repository.
A build tool that modifies its input without being told to is a bad build
tool anyway.

**`--locked` refuses BEFORE the work.** The check sits between resolving the
modules and compiling them: at that moment the input is complete (every
module found and read) and not one instruction has been emitted. Both
compilers do it at that same place, so both refuse at the same moment —
`firnc0` right after `modules::resolve`, `firnc1` right after the first pass
over the module queue.

**A deviation is an error with a place, not a re-resolution.** The message
names the first line that differs and both sides of it:

```
error: /p/app/firn.lock: the lock file does not match the sources
note: line 3 of the file:  'package app 0.1.0 . 8db85d3e…'
note: line 3 of the build: 'package app 0.1.0 . 2e669552…'
```

A missing line, a superfluous line and a missing file each have their own
sentence. All of them come out of one function per side
(`lock::difference` / `lock_error_mismatch`) and are compared octet for
octet between the two compilers.

## 4. The version wish, and one rule instead of two

`needs` gained an optional fourth word:

```text
needs geo ../geo 0.2.0
```

**The rule:** a wish is met by the same **first number** and at least the
given version. `0.2.0` is met by `0.2.0` and by `0.9.1`, and never by
`0.1.9` or `1.0.0`.

This is deliberately **not** cargo's rule. There, `0.2.0` means `< 0.3.0`
but `1.2.0` means `< 2.0.0` — two rules, because in a registry world the
`0.x` range carries the convention „anything may change". With local path
dependencies there is no registry to negotiate with, and the first number is
the compatibility promise, always. One rule that fits on one line is worth
more than one that matches somebody else's tool. The choice is written down
here because it is a choice, not an oversight.

The old error message for a `needs` line that is too short is unchanged
(`'needs' expects a name and a path`); a fifth word gets its own sentence
(`'needs' expects at most one version behind the path`). Existing manifests
therefore do not see a single new character.

## 5. One version per package name

Round 48 keyed a package by its **root directory**: two directories that
both call themselves `geo` were two packages. They must not be — the module
system renames a module of a non-root file to `module__name`, so two `geo`
in one build collide, and `import geo` in two different packages has to mean
the same thing anyway.

So, after loading and **before** the cycle check:

1. per name the **highest version** wins (`version_higher`, purely on the
   three numbers);
2. every edge is **bent onto the winner**;
3. every wish is measured against what really got picked;
4. the lock file lists only the packages that are still **reachable** from
   the root — a superseded directory is not part of the build and has no
   business in the file.

The order matters and is the same in both compilers: resolving first, cycles
second. The other way round the check would run on edges that the resolution
is about to change.

That resolution forced a change in `lib/firnc1/package.fi`. Up to round 92
an edge was **recomputed from its path** every time anybody asked
(`world_edge`, `cycle_dfs`), which is fine as long as an edge is a function
of the manifest. It is not any more, so the resolved target of every `needs`
entry is written down (`a_target`) and every reader uses that. The Rust side
already had `edges` and only needed the remapping.

Two conflicts have their own message:

```
error: package 'geo' comes from two directories with version 0.2.0
note: '/p/geo' and '/p/geo2'

error: /p/text/firn.package:7: dependency 'geo' is version 0.3.0, needed is 1.0.0 or higher with the same first number
```

The first one refuses to guess: same name, same version, two places, no
reason to prefer one. The second one is the real conflict case and names the
line to change.

## 6. The find of this round: the artifact knew where it was built

This is the part that decided item 5, and it was found by running a
measurement that had been sitting red in the repository since round 48.

`bash tools/repro/run.sh` before this round:

```
   stage1         DIFFERENT
   stage2         IDENTICAL
   stage2.s       IDENTICAL
   package_bin    DIFFERENT
        3,562 of 6,840 octets differ
```

Three separate causes, in growing order of how bad they are.

### 6.1 The assembler writes its working directory into the artifact

`as` builds a `.debug_line` out of the `.file`/`.loc` directives the
compiler emits, and puts **its own** working directory into it as
`DW_AT_comp_dir`. Two checkouts at different paths therefore produced
different binaries, and no amount of care inside the compiler would have
changed that.

Measured on the same `.s` file, assembled in two directories:

```
plain     b197755776f6574c…   cc2814ea1561c472…    DIFFERENT
mapped    9354ee47be6fda73…   9354ee47be6fda73…    identical
```

`mapped` is `as --debug-prefix-map <cwd>=.`, and that is what `firnc` passes
now (`main.rs::assemble`, both targets — the aarch64 assembler is the same
binutils). A counter-check that had to be made: the NAME of the `.s` file
does not matter, only the directory did.

### 6.2 The module search handed out absolute paths

`--package demos/packages/app`, the `.file` directives before the fix:

```
.file 1 "demos/packages/app/src/main.fi"
.file 2 "/root/jarvis/projects/u_DiS4in7esMF1/firn/demos/packages/geo/src/geo.fi"
.file 3 "/root/…/demos/packages/geo/src/dot.fi"
```

Four of six entries carried the checkout path. The reason sits in
`package_world`: a package root is made absolute (it has to be — deciding
„does this file lie inside that package" is string work on absolute paths),
and steps 3 and 4 of the search build their candidates from it. The root
file came from the command line and stayed relative; every dependency
became absolute.

### 6.3 …and those paths ended up in the PANIC MESSAGE TABLE

This is the one that matters. The paths do not stay in the debug
information. The message table of the checked arithmetic (round 72) is built
from the same file names and lives in `.rodata` — it is **text the program
prints at runtime**:

```
$ strings app | head -1
panic: integer overflow in 'i32 * i32' at /root/jarvis/projects/u_DiS4in7esMF1/firn/demos/packages/geo/src/geo.fi:16:12
```

A user of that binary sees the directory layout of the build machine when a
program overflows. That is not only non-reproducible, it is a small
information leak, and it was in every package build.

**The fix, in one place:** since this round a source file is known
throughout the build under the spelling that does **not** name the machine —
relative to the working directory if it lies inside it, unchanged
otherwise (`package_world::build_path`, `package.build_path` in Firn). It is
applied where a file enters the build (`modules::resolve` in `firnc0`, the
import collection and the root file in `firnc1`), so diagnostics, `.file`
directives and the panic table all get the same name, and the two compilers
still print identical messages.

Afterwards:

```
$ strings app | grep -c 'firn-r93-lock'
0
$ strings app | grep -o "at [^ ]*geo.fi:16:12"
at demos/packages/geo/src/geo.fi:16:12
```

The artifact also got **smaller**: 6,840 → 6,312 octets, because the absolute
paths are gone.

`bash tools/repro/run.sh` after the fix:

```
   stage1         IDENTICAL  62ee950712933ad4  (1638944 octets)
   stage2         IDENTICAL  d51ac1c868eb5186  (4512072 octets)
   stage2.s       IDENTICAL  fc026e8a70c4fe6a  (22445664 octets)
   package_bin    IDENTICAL  f3dbc3e2470d08b2  (6312 octets)
   identical: 4   different: 0   missing: 0
```

**What did NOT have to be fixed:** `firnc1` writes no `.file`/`.loc`
directives at all, so `stage2` was reproducible before this round and stayed
so. The asymmetry is worth naming: the compiler in Firn was the honest one
here, and the one in Rust was not.

## 7. The second machine, and what „second machine" is worth

`tools/repro/run.sh` compares two directories. That is one difference. A
second machine differs in more, so `tools/repro/two_machines.sh` (new,
`test.sh` section 49) makes the second run differ in everything that costs
nothing to change:

another working directory (deeper, longer name) · another `$HOME`,
`$TMPDIR`, `$USER`, `$LOGNAME`, `$SHELL`, `$PATH` · another `$TZ`
(`Pacific/Kiritimati`) and `$LC_ALL` · another `umask` (077 against 022) ·
seconds between the two runs and source time stamps set to 2001 · the
sources **written in the opposite order**, so the directory order the file
system hands out differs · **the compiler binary at another path** — which
matters, because `firnc1` reads `/proc/self/exe` for its library search ·
and, since `qemu-x86_64` is installed here, machine B does not even run on
the same CPU implementation: the whole second build goes through the
emulator, with other CPU features and another address layout. Machine A runs
with ASLR switched off (`setarch -R`), machine B with it on.

Both compilers are measured, and the lock file with them. The numbers are in
section 9.

**What this is not.** It is not two pieces of hardware. The honest name for
it is: everything a second machine differs in except the hardware and the
kernel. Two things stay equal on purpose — the source state (that is the
premise) and the content of the compiler binary (a second machine would
build it from the same sources; `tools/repro/run.sh` measures that `firnc0`
and `stage1` come out identical from two directories, so it is not the
variable).

The one avenue that was tried and dropped: running the compiler itself on
**aarch64** under `qemu-aarch64` would have been a genuinely different
machine for the compiler. `firnc --target=aarch64-linux bin/firnc1.fi`
stops with `error: aarch64: system call 89 is not in the table
(syscalls.rs)` — `readlink` has no direct counterpart on aarch64 (only
`readlinkat`), and `firnc1` uses it for `/proc/self/exe`. Adding that
mapping is a change to the aarch64 syscall emulation and belongs to the
round that owns it, not to this one. Written down here so the next round
knows the price of it: one syscall.

## 8. SHA-256 twice, and the bug that cost an hour

Both implementations were checked against the four vectors of FIPS 180-4
(empty, `abc`, the 56 octet one, and a million `a`) before they were used
for anything. Both were right on all four — and the first lock file out of
`firnc1` still had three wrong checksums out of four.

The reason is worth writing down because it is the classic shape of a hash
bug: in the Firn version `hash_push` counted the octets, and `hash_byte`
(one octet, used for the `\n` between the pieces and for the padding) did
not. The padding length therefore came out too small for every stream
assembled from more than one piece. **All four standard vectors go through
one single push**, so every one of them was right. What found it was not the
vectors, it was a comparison of the exact octet stream between the two
implementations: the streams were identical, the hashes were not, so the bug
had to be in the state and not in the input.

`outside 0` was the one line that stayed correct, and even that was a hint
in hindsight: it is the checksum of the empty stream, the only one with no
separator in it.

## 9. Acceptance (measured, 24.08.2026, branch `r93-lock`)

| Check | Result |
|---|---|
| `cargo test --release --manifest-path compiler/Cargo.toml` | **247 passed, 0 failed** (+18 of this round: `lock.rs` 4, `package.rs` 3, `package_world.rs` 2, and the ones the two new functions brought) |
| `bash tools/packages/run.sh` | **39 passed, 0 failed** (21 of round 48 unchanged, 18 new) |
| `bash tools/repro/two_machines.sh` | **PASS** — firnc0 `ba62c1ff…c4c7cc` on both machines, firnc1 `158436f6…bdcd17` on both machines, `firn.lock` `76b37bd7…96af8e` out of all four runs, assembly text identical as well |
| `bash tools/repro/run.sh` | **4 of 4 artifacts identical** (`stage1`, `stage2`, `stage2.s`, `package_bin`), exit 0 — before the round 2 of 4 |
| `bash tools/fixpoint.sh` | **stage 2 == stage 3, character-identical**, 4,644,864 octets, 779,932 lines of assembly |
| `bash tools/self_compare.sh` | PLACEHOLDER_SELF |
| `bash test.sh` | PLACEHOLDER_TEST |

### The one red case, and why it is not this round's

`tools/self_compare.sh` reported **327 same behaviour, 1 differing**:
`tests/834_arc_thread.fi` (firnc0: 9, firnc1: 0). Exit 9 of that test is
this line:

```firn
    // The counter-check MUST have lost some.
    if af_ld(z, AF_RAW) >= AF_THREADS * AF_ROUNDS {
        return 9
    }
```

It is an assertion about four threads really running at the same time: an
UNLOCKED counter has to lose increments. On a machine with three other
rounds compiling at the same time (load average 11 to 25 during this
measurement) the four threads do not overlap, nothing is lost, and the
assertion fails. `tools/aarch64/run.sh` says the same thing about the same
two tests in its own comment.

That was not believed, it was measured. The compiler of the BASE commit
(`main`, `compiler/src` checked out fresh, nothing of this round in it) was
built and the same test run twelve times:

```
base  firnc0:   0 0 0 0 9 0 0 9 9 9 9 9      (6 of 12 fail)
r93   firnc0:   9 9 9 0 9 9 0 9              (6 of 8 fail)
both under SCHED_FIFO:  1 1 1 1 1 1          (identical, a third outcome)
```

Identical behaviour in every scheduling regime, before and after the round.
The deviation belongs to the load on the machine, not to the change — and it
is written down here instead of being left out, because a suite that is
green only when nobody else is working is worth knowing about.

## 10. Open (honestly)

* **No network, no registry.** `needs` still knows local paths only. That is
  enough for the criterion of item 5 — and it is the reason the resolution
  above is a decision and not a search.
* **A library package cannot be locked.** `--lock` hangs off `--package`,
  and `--package` needs a `start`. Locking a library on its own would need a
  build that produces nothing, which does not exist yet (there are no
  separate object files).
* **The lock file does not pin the COMPILER.** Two machines with different
  `firnc` binaries can satisfy the same lock file and still produce
  different artifacts. The lock file pins the input; `two_machines.sh`
  measures the output. Item 5 needs both halves, and it says so.
* **Files outside every package are locked by name and content, not by
  path.** A `$FIRNLIB` that has the same file names with different content
  is caught; one that has the same content at another place is not, on
  purpose.
* **Still not incremental** (round 48, section 6). Every build compiles
  everything, so „the lock file fits" and „the build is up to date" are
  different statements and only the first one is made here.
* **`--lock` overwrites without asking.** There is no `--lock --check-only`;
  `--locked` is that.
* **A build WITH debug information (`--no-opt`) is not reproducible yet.**
  `--debug-prefix-map` takes care of `DW_AT_comp_dir`, but our own
  `.debug_info` (round 64, `dwarf_info.rs`) writes the working directory as
  well, and that path was not touched here. Measured only for the optimised
  default build, which is the one a package build uses.
* **The time stamps in the archive, not in the artifact.** Nothing in a Firn
  binary carries a build time — there is no `__DATE__` and nothing that
  reads the clock during a build. That is why the two runs of section 7 can
  lie seconds apart at all, and it was worth checking rather than assuming.
