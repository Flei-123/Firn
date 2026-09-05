# testdata/json -- JSONTestSuite, the checking instance for `lib/std/json.fi`

**Upstream:** <https://github.com/nst/JSONTestSuite> (Nicolas Seriot, the
suite behind "Parsing JSON is a Minefield", 2016)
**Fetched:** 22.08.2026, branch `master`, with
`curl -L https://codeload.github.com/nst/JSONTestSuite/tar.gz/refs/heads/master`
**License:** MIT (the `LICENSE` file of the repository is inside the archive)
**Used:** unchanged, and exclusively as a CHECKING INSTANCE. Nothing of it is
part of `lib/std/json.fi`.

## What lies here

* `JSONTestSuite.tar.gz` -- the directories `test_parsing/` (318 files) and
  `test_transform/` (22 files) as ONE deterministic archive
  (`tar --sort=name --owner=0 --group=0 --numeric-owner
  --mtime="UTC 2020-01-01"`, `gzip -9n`), 11 KiB.
* `files.sha256` -- the sha256 sum of every single file, so that the pinning
  survives a repack of the archive.

## The rule the file names carry

The suite encodes the expected verdict in the first two characters:

* `y_...` -- **must be accepted.** 95 files.
* `n_...` -- **must be refused.** 188 files. This is the half that matters:
  a parser that accepts everything passes every `y_` case.
* `i_...` -- **implementation defined.** 35 files: huge exponents, lone
  surrogates, invalid UTF-8, 500 levels of nesting. RFC 8259 leaves those
  open, so neither answer is wrong -- but WHICH answer is given is recorded
  in `docs/ROUND81.md` and in the honest list of `lib/std/json.fi` (J2, J3,
  J4), instead of being left to chance.

`tools/stdlib81/run.sh` runs all three groups and prints the quota it
really reached.

## Why an archive and not 340 loose files

The same reason as `testdata/test262`: 340 files that no editor ever opens
are 340 index entries in every worktree. The archive is 11 KiB, and
`files.sha256` pins every single file individually.
