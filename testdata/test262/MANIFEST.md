# test262 -- the official ECMAScript test suite (the subset of round 63)

**Upstream:** https://github.com/tc39/test262
**Commit:**   `3655e7464de3d52643ecddd4b5f9f4f3e7f62398`
**Fetched:**  20.08.2026 with `git clone --depth 1`
**License:**  BSD-3-Clause (`LICENSE` inside the archive tree upstream); the
files are used **unchanged** and exclusively as a CHECKING INSTANCE. Nothing
of test262 is part of the engine -- `lib/js/` contains no foreign code.

## What lies here

* `test262-subset.tar.gz` -- the subset, as ONE deterministic archive
  (`tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="UTC
  2020-01-01"`, `gzip -9n`), 32,893 files, 4.3 MiB.
  sha256: `6550c1f6aefcfe1dfcbc746c4255a5a9039eae124de2cdaea523ec64eb5db938`
* `subset.sha256` -- the sha256 sum of **every single file** of the subset,
  relative to the root of the test262 repository. That is the actual
  pinning: it holds even if the archive is repacked.
* `tools/js/verify_testdata.sh` checks both -- the sum of the archive and
  every file sum after unpacking.

**Why an archive and not 32,893 loose files.** The other suites of this
project (`css-parsing-tests`, `html5lib-tokenizer`) have a dozen files and
lie loose. test262 has 98 MiB of source text in the language part alone;
loose in the repository that would be 140 MiB and 33,000 index entries in
every worktree of every parallel round. The archive is 4.3 MiB and pinned
exactly as strictly -- `subset.sha256` names every file individually.

## Which subset, and why exactly this one

The rule is: **whole directories, no case filtered inside them.** A case
that this engine does not support counts as a FAILURE, like every other
one. What is left out is left out as a WHOLE DIRECTORY and named here.

### `test/language/` -- taken (23 directories)

    arguments-object          identifiers               statements
    asi                       keywords                  statementList
    block-scope               line-terminators          types
    comments                  literals                  white-space
    computed-property-names   punctuators
    destructuring             reserved-words
    directive-prologue        rest-parameters
    expressions               source-text
    function-code             future-reserved-words
    global-code               identifier-resolution

### `test/language/` -- deliberately NOT taken, with the reason

* `eval-code/` (347 cases) -- this engine has no `eval`. Direct `eval` needs
  the whole parser at run time plus its own variable environment; it is
  named in `docs/ROUND63.md` as deliberately missing.
* `module-code/` (755), `import/` (191), `export/` (3) -- the parser reads
  modules (`import`/`export` produce ESTree nodes), but there is no module
  LOADER and no linking. Running those cases would measure a component that
  does not exist.

### `test/built-ins/` -- taken

    Array   Boolean   Error   Function   Infinity   isFinite   isNaN
    JSON    Map       Math    NaN        NativeErrors          Number
    Object  parseFloat        parseInt   Set        String     Symbol
    undefined

### `test/built-ins/` -- not taken

Everything else, because the engine does not have those objects at all:
`Temporal`, `RegExp`, `TypedArray*`, `Promise`, `Iterator`, `Date`,
`DataView`, `Atomics`, `Proxy`, `Reflect`, `ArrayBuffer`,
`SharedArrayBuffer`, `WeakMap`, `WeakSet`, `BigInt`, `Generator*`,
`AsyncFunction`, `FinalizationRegistry`, `WeakRef`, `decodeURI*`,
`encodeURI*`, `escape`, `unescape`, `eval`, `GeneratorFunction`.

### `harness/`

The 34 helper files of the suite (`assert.js`, `sta.js`,
`propertyHelper.js`, `compareArray.js`, ...) are part of the archive. They
are foreign JavaScript and are executed by the engine as ordinary test code
-- that is exactly their purpose.
