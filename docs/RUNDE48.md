# Round 48 — packages, project manifest, visibility at module level

**State before this round:** there was `import a.b`, `export { … }` per file
and the environment variable `FIRNLIB`. Nothing more — no project manifest,
no dependencies, no build tool. `ABNAHME.md` item 5 (`W1`,
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

One statement per line: `schluessel wert [wert …]`. The separators are
space and tab, `#` starts a comment up to the end of the line,
and empty lines do not count. **No quotation marks, no
escapes** — a value therefore contains neither spaces nor `#`.

```text
paket        demo            # Pflicht, genau einmal
version      0.1.0           # Pflicht, genau einmal, zahl.zahl.zahl
start        src/main.fi     # höchstens einmal; eine Bibliothek hat keinen
quelle       src             # 0..n; ohne Angabe gilt das Manifestverzeichnis
oeffentlich  geo punkt       # 0..n; ohne Angabe ist alles öffentlich
brauche      geo ../geo      # 0..n; Name + lokaler Pfad
```

Rules that are really checked:

| Entry | Rule |
|---|---|
| `paket` | identifier: letter or `_` first, then letters, digits, `_` |
| `version` | exactly `zahl.zahl.zahl` |
| `start`, `quelle` | relative, without `..`, not empty (a package stays in its directory) |
| `brauche` | name like `paket`; the path **may** lead outside (`../geo`) |
| duplicate entries | error — also duplicate `quelle`, duplicate `oeffentlich` names, duplicate dependency names |
| dependency has the same name as the package itself | error |
| unknown key | **error**, not silently skipped — a mistyped `oeffentlih` would otherwise open an interface nobody wanted to open |
| name of the dependency ≠ `paket` line of the target | error |

`start` is **not** mandatory: a library package has no
entry point. Only `--paket` demands one.

## 3. Search order

For `import t1.t2…tn` in the file `F`, first hit wins:

```
1.  <verzeichnis von F>/t1/…/tn.fi          (wie bisher)
2.  <verzeichnis der Wurzeldatei>/t1/…/tn.fi (wie bisher)
3.  <paketwurzel>/<quelle>/t1/…/tn.fi        für jedes 'quelle' des Pakets,
                                             zu dem F gehört            NEU
4.  <abhängigkeit>/<quelle>/t2/…/tn.fi       wenn t1 der Name einer
                                             'brauche'-Abhängigkeit ist NEU
5.  $FIRNLIB/t1/…/tn.fi                      (wie bisher)
6.  <exe>/../lib/t1/…/tn.fi                  (wie bisher)
```

With `import geo` (only one part) and `geo` as a dependency,
`<geo>/<quelle>/geo.fi` is looked for — the module with the name of the
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

Paths are normalized **purely lexically** (`a/./b/../c` → `a/c`);
symbolic links are not resolved. That has to be so: `firnc1` has
no `realpath`, and without this rule `--paket-info` would be
machine-dependent.

## 4. Visibility at module level

`oeffentlich a b c` in `firn.paket` is the **interface of the package**.
If an import leads into a *different* package, the following applies:

* The target package must be a registered dependency
  (`paket 'x' ist keine abhaengigkeit von paket 'y'`).
* The module must stand in its `oeffentlich` list
  (`modul 'x' ist in paket 'p' nicht oeffentlich`).

**Inside** a package there is no barrier: `demos/packages/geo`
uses its private module `innen` and is allowed to.

**If `oeffentlich` is missing, everything is public.** That is deliberately
the same rule as with `export { … }` inside a file („if it is missing,
everything is visible", `modules.rs`). A stricter default (without a list
nothing is public) was under consideration: it catches forgotten
interfaces, but it turns every unfinished manifest into an
incomprehensible error and would be inconsistent with the existing
`export` rule. Whoever wants a real interface writes it down — `geo` does
it, `text` does not, and both cases are in the example project.

The two levels mesh: `oeffentlich` says **which modules**
a package shows, `export { … }` says **which names** a module shows.

## 5. Name conflicts

The module system internally renames names from non-root modules to
`modul__name`; `modul` is the file name without the extension. Two
**different** files with the same name therefore fell onto the same
renaming and would have silently shadowed each other. That is now an error:

```
error: namenskonflikt: modul 'hilfe' kommt aus zwei dateien
hinweis: '/…/anwendung/src/help.fi' und '/…/geo/src/help.fi'
```

The check goes over the absolute paths — two spellings of the same file
are not a conflict. The check runs **only with a manifest**; without a
manifest the behavior of round 47 remains (otherwise the change would not
be backwards compatible).

## 6. The build driver

```
firnc  --paket <verzeichnis> [-o ziel]     # Projekt übersetzen
firnc  --paket-info <verzeichnis>          # Manifest lesen und berichten
firnc1 --paket <verzeichnis> [-o ziel]     # dasselbe, in Firn
firnc1 --paket-info <verzeichnis>
```

`--paket` reads `<verzeichnis>/firn.paket`, loads all dependencies,
checks the graph for cycles and compiles `start`. Without `-o` the
result is named like the package:

```
$ firnc --paket demos/packages/app
$ ./demos/packages/app/anwendung
12 14 3
```

`--paket-info` prints a machine-readable report, computed purely
lexically from the given directory (no `getcwd`, no
symbolic links) — that is why it is the same on both compilers and on
every machine:

```
$ firnc --paket-info demos/packages/app
paket anwendung
version 0.1.0
wurzel demos/packages/app
start demos/packages/app/src/main.fi
quelle demos/packages/app/src
brauche geo demos/packages/geo
brauche text demos/packages/text
```

**It is not incremental.** The driver always compiles everything. That was
the deliberate choice from the round goal („correct beats fast"): a
wrong freshness comparison silently builds yesterday's state, and exactly
this trap has hit this project three times already, in rounds 35, 45 and
46.

## 7. The example project

`demos/packages/` — one program and two libraries:

```
anwendung/   firn.paket   brauche geo, brauche text; quelle src
             src/main.fi  import geo · import geo.punkt · import text · import hilfe
             src/help.fi eigenes Modul aus 'quelle src'
geo/         firn.paket   oeffentlich geo punkt   (KEIN start: Bibliothek)
             src/geo.fi   öffentlich, benutzt intern 'innen'
             src/dot.fi öffentlich
             src/inner.fi PRIVAT — von außen nicht einbindbar
text/        firn.paket   ohne 'oeffentlich' → alles öffentlich
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
  local paths. Reproducibility across two machines (`ABNAHME.md` item 5)
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
  but `datei:zeile: meldung`. The reason is equality: `firnc1` does not
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
