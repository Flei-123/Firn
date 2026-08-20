# Round 48 — packages, project manifest, visibility at module level

**State before this round:** there was `import a.b`, `export { … }` per file
and the environment variable `FIRNLIB`. Nothing more — no project manifest,
no dependencies, no build tool. `ACCEPTANCE.md` item 5 (`W1`,
„package management builds reproducibly") therefore stood at `[~]`.

**What is there now:** a project manifest `firn.paket`, a fixed and
deterministic module search order with error messages for cycles,
missing packages and name conflicts, visibility at **module level** as
a real package interface, and the build driver `--paket`. All of it in
**both** compilers — `firnc0` (Rust) and `firnc1` (Firn) — with
character-identical messages.

---

## 1. Why not TOML — the format decision

The choice was between `firn.toml` and a format of our own. The decision
went to a format of our own, deliberately tiny, a line format called
**`firn.paket`**. The reasons, in order:

1. **Everything has to exist twice.** Firn hosts itself. Every line of
   manifest logic exists in `compiler/src/package.rs` (Rust) *and* in
   `lib/firnc1/package.fi` (Firn, **without libc**, only buffers and
   `syscall`). A TOML reader would be several thousand lines in Firn:
   escaped and multi-line strings, arrays, inline tables,
   date values, number syntax with underscores, hex/octal/binary.
2. **Half a TOML is worse than none.** A file that is called
   `firn.toml` raises the expectation that every valid TOML is read.
   It is not — and then the file name lies. A name of our own with
   an extension of our own does not raise that expectation in the first
   place.
3. **No foreign libraries.** That is a basic decision of the project
   (`SPEC.md`); a `toml` crate in `firnc0` would have had no
   counterpart in `firnc1` and would have made the two compilers drift
   apart immediately.
4. **The format is supposed to be boring.** A manifest is read before
   anything else happens. It must not have surprising semantics.

The price has to be named honestly: **there are no ready-made tools** for
`firn.paket` (no editor highlighting, no library in other
languages). For a format of six keywords that is readable with `awk`,
that is acceptable.

## 2. The format

One statement per line: `key value [value ...]`. The separators are
space and tab, `#` starts a comment up to the end of the line,
and empty lines do not count. **No quotation marks, no
escapes** -- a value therefore contains neither spaces nor `#`.

```text
package  demo            # mandatory, exactly once
version  0.1.0           # mandatory, exactly once, number.number.number
start    src/main.fi     # at most once; a library does not have one
source   src             # 0..n; without one the manifest directory counts
public   geo dot         # 0..n; without one everything is public
needs    geo ../geo      # 0..n; name + local path
```

Rules that are really checked:

| Entry | Rule |
|---|---|
| `package` | identifier: letter or `_` first, then letters, digits, `_` |
| `version` | exactly `number.number.number` |
| `start`, `source` | relative, without `..`, not empty (a package stays in its directory) |
| `needs` | name like `package`; the path **may** lead outside (`../geo`) |
| duplicate entries | error -- duplicate `source`, duplicate `public` names and duplicate dependency names included |
| dependency has the same name as the package itself | error |
| unknown key | **error**, not silently skipped -- a mistyped `publci` would otherwise open an interface nobody wanted to open |
| name of the dependency != `package` line of the target | error |

`start` is **not** mandatory: a library package has no
entry point. Only `--package` demands one.

## 3. Search order

For `import t1.t2…tn` in the file `F`, first hit wins:

```
1.  <directory of F>/t1/.../tn.fi             (as before)
2.  <directory of the root file>/t1/.../tn.fi (as before)
3.  <package root>/<source>/t1/.../tn.fi      for every 'source' of the
                                              package F belongs to      NEW
4.  <dependency>/<source>/t2/.../tn.fi        if t1 is the name of a
                                              'needs' dependency        NEW
5.  $FIRNLIB/t1/.../tn.fi                     (as before)
6.  <exe>/../lib/t1/.../tn.fi                 (as before)
```

With `import geo` (only one part) and `geo` as a dependency,
`<geo>/<source>/geo.fi` is looked for -- the module with the name of the
package is its main module.

**Which package a file belongs to** is decided by its path: the package
with the longest matching root. That is why a package may lie in the
directory of another one. Files outside all package roots (typically:
everything from `$FIRNLIB`) belong to no package; for them steps 3 and 4
and the visibility check are skipped.

**The manifest itself** is, without `--paket`, looked for **upwards** from
the directory of the source file, at most 64 levels. If none is found, the
„package world" is empty, steps 3 and 4 are skipped, and the resolution is
character for character the one from round 47. **Without a manifest nothing
changes** — that is the reason why the 696 existing tests stay green
unchanged.

Paths are normalized **purely lexically** (`a/./b/../c` -> `a/c`);
symbolic links are not resolved. That has to be so: `firnc1` has
no `realpath`, and without this rule `--package-info` would be
machine-dependent.

## 4. Visibility at module level

`public a b c` in `firn.package` is the **interface of the package**.
If an import leads into a *different* package, the following applies:

* The target package must be a registered dependency
  (`package 'x' is not a dependency of package 'y'`).
* The module must stand in its `public` list
  (`module 'x' is not public in package 'p'`).

**Inside** a package there is no barrier: `demos/packages/geo`
uses its private module `inner` and is allowed to.

**If `public` is missing, everything is public.** That is deliberately
the same rule as with `export { ... }` inside a file ("if it is missing,
everything is visible", `modules.rs`). A stricter default (without a list
nothing is public) was under consideration: it catches forgotten
interfaces, but it turns every unfinished manifest into an
incomprehensible error and would be inconsistent with the existing
`export` rule. Whoever wants a real interface writes it down -- `geo` does
it, `text` does not, and both cases are in the example project.

The two levels mesh: `public` says **which modules**
a package shows, `export { ... }` says **which names** a module shows.

## 5. Name conflicts

The module system internally renames names from non-root modules to
`module__name`; `module` is the file name without the extension. Two
**different** files with the same name therefore fell onto the same
renaming and would have silently shadowed each other. That is now an error:

```
error: name conflict: module 'help' comes from two files
note: '/.../app/src/help.fi' and '/.../geo/src/help.fi'
```

The check goes over
are not a conflict. The check runs **only with a manifest**; without a
manifest the behavior of round 47 remains (otherwise the change would not
be backwards compatible).

## 6. The build driver

```
firnc  --package <directory> [-o target]      # compile the project
firnc  --package-info <directory>             # read the manifest and report
firnc1 --package <directory> [-o target]      # the same, in Firn
firnc1 --package-info <directory>
```

`--package` reads `<directory>/firn.package`, loads all dependencies,
checks the graph for cycles and compiles `start`. Without `-o` the
result is named like the package:

```
$ firnc --package demos/packages/app
$ ./demos/packages/app/app
12 14 3
```

`--package-info` prints a machine-readable report, computed purely
lexically from the given directory (no `getcwd`, no
symbolic links) -- that is why it is the same on both compilers and on
every machine:

```
$ firnc --package-info demos/packages/app
package app
version 0.1.0
root demos/packages/app
start demos/packages/app/src/main.fi
source demos/packages/app/src
needs geo demos/packages/geo
needs text demos/packages/text
```

**It is not incremental.** The driver always compiles everything. That was
the deliberate choice from the round goal ("correct beats fast"): a
wrong freshness comparison silently builds yesterday's state, and exactly
this trap has hit this project three times already, in rounds 35, 45 and
46.

## 7. The example project

`demos/packages/` -- one program and two libraries:

```
app/         firn.package  needs geo, needs text; source src
             src/main.fi   import geo * import geo.dot * import text * import help
             src/help.fi   own module out of 'source src'
geo/         firn.package  public geo dot   (NO start: a library)
             src/geo.fi    public, uses 'inner' internally
             src/dot.fi    public
             src/inner.fi  PRIVATE -- cannot be imported from outside
text/        firn.package  without 'public' -> everything is public
             src/text.fi
```

## 8. What is checked

`tools/packages/run.sh` (new, in `test.sh` as step 18): **21 cases**,
each through **both** compilers, error messages compared octet by
octet. Positive: build of the example project (firnc0 and firnc1), output
`12 14 3`, naming after the manifest, `--paket-info` equality, private
module in the own package, precedence of the project source, manifest
search upwards, second `quelle` directory, regression without a manifest.
Negative: private module of a dependency, package without `brauche`,
package cycle, dependency without a manifest, wrong package name, invalid
version, unknown key, missing `paket` line, name conflict, library without
`start`, directory without a manifest, `--paket` together with a source
file.

Plus **13 new Rust module tests** in `compiler/src/package.rs` (11) and
`compiler/src/package_world.rs` (2): format, mandatory entries, duplicate
entries, arity, path arithmetic, package membership, `--paket-info` text
and the fixed error texts.

## 9. Migration notes

* **Existing projects have to do nothing.** Without `firn.paket`
  everything is as before; `FIRNLIB` applies unchanged and is still
  searched as step 5. `test.sh`, `tools/self_compare.sh` and
  `tools/fixpoint.sh` set `FIRNLIB` themselves and run unchanged.
* **Converting a project:** put `firn.paket` into the root directory
  (`paket`, `version`, `start`, `quelle`), register dependencies with
  `brauche`, and write `oeffentlich` in every library. After that
  `firnc --paket <verzeichnis>` builds.
* **Careful when converting:** as soon as a manifest exists, the name
  conflict and visibility checks take effect as well. Two modules of the
  same name in one compilation are then an error instead of a silent
  shadowing — that is the purpose, but it may come to light on the first
  run.
* **No manifest in the root directory of this repo.** That is intentional:
  it would change the resolution of all test programs in the repo. The
  example project therefore lies under `demos/packages/`.

## 10. Open (honestly)

* **No network, no registry, no lock file.** `brauche` knows only
  local paths. Reproducibility across two machines (`ACCEPTANCE.md` item 5)
  is therefore **not yet** fulfilled; checksums and a
  `firn.sperre` are missing.
* **No `firn build --locked`, no version resolution.** `version` is
  checked but not *compared* — two packages cannot demand different
  versions of the same dependency.
* **Not incremental** (see 6). There are still no separate
  object files and no interface files; what is compiled is the
  whole program.
* **Symbolic links** are not resolved for package membership.
  A package reached through a symlink counts as lying at
  the symlink's place.
* **Errors in the manifest show no source line** with a marker,
  but `file:line: message`. The reason is equality: `firnc1` does not
  have the diagnostic machinery of `firnc0`, and for the new messages
  character equality was more important than the excerpt.
* **The visibility check only takes effect with a manifest.** Whoever
  builds without a manifest has no package boundaries — then there are
  none to violate either.

## 11. Acceptance (measured, 19.08.2026, branch `r48-pakete`)

Measurement was done after `rm -f .firnc1 .firnc2 .firnc3` — no binary from
an earlier run was involved.

| Check | Result |
|---|---|
| `bash ./test.sh` | **PASS 697/697**, exit 0 (base 696/696; +1 = step 18) |
| ⤷ step 18 `tools/packages/run.sh` | **21 passed, 0 failed** |
| `bash tools/self_compare.sh` | **201 identical behavior · 0 differing · 0 failing**, exit 0 |
| `bash tools/fixpoint.sh` | **stage 2 == stage 3, character-identical**, 2.070.856 octets, 364.765 lines of assembly; corpus: `.firnc2` behaves like `firnc0`, exit 0 |

For comparison the starting state of commit `a492d26`: `test.sh` 696/696,
`self_compare.sh` 201/0/0, `fixpunkt.sh` character-identical at 2.065.816
octets. The increase of 5.040 octets in the self-compiled compiler is
`lib/firnc1/package.fi` plus the changes in `bin/firnc1.fi`.
