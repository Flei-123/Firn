# Packages: manifest, dependencies, pins, lock

One flow for Firn projects, built from three parts. The manifest format and the compiler side already existed (`docs/ROUND48.md`, `docs/ROUND93.md`); `tools/firn/firn` adds fetching and pinning of git dependencies on top. No registry yet.

| file | written by | says |
|---|---|---|
| `firn.package` | you / `firn new`, `firn add`, `firn remove` | name, version, `source`, `start`, `public`, `needs NAME PATH [VERSION]` (parsed by both compilers, unchanged) |
| `firn.deps` | `firn add`, `firn fetch`, `firn update` | one row per git dependency: `NAME URL REF COMMIT TREE-SHA256` |
| `firn.lock` | `firn build` (`firnc --package --lock`) | SHA-256 over every source file of every package that went into the build, one `package` line each, `total` over all |
| `.firn/deps/NAME` | `firn fetch` | the checkout of a git dependency (not committed, `.gitignore` has `/.firn/`) |

There is no `firn.toml`: `docs/ROUND48.md` section 1 explains why the manifest is a line format that both compilers parse.

`firn.lock` belongs to the compiler (format fixed by round 93), so the pins of git dependencies live in `firn.deps`. Together they cover the sources: `firn.deps` pins what is fetched, `firn.lock` pins what is compiled.

## Flow

```
firn new app                         # program: firn.package, src/main.fi, tests/
firn new geo --lib                   # library: no start, importable via needs
cd app
firn add geo ../geo                  # path dependency   -> needs geo ../geo 0.1.0
firn add txt https://host/txt.git v1 # git dependency    -> pinned commit + tree hash in firn.deps
firn build .  -o app                 # fetches what is missing, builds with --package, writes firn.lock
firn build . -o app --locked         # refuses to build when firn.lock does not match the sources
firn test                            # fetches, then runs tests/*.fi (TAP)
firn tree                            # dependency graph, transitive
firn update txt                      # re-resolve REF (branch/tag), re-pin
firn remove txt                      # needs line, firn.deps row and checkout
firn verify                          # re-hash all checkouts against firn.deps
```

Sources of a dependency: a directory with a `firn.package` (path) or a git URL (`https://`, `ssh`, `file://`, `git@`, `*.git`). `REF` is a branch, tag or commit; empty means the default branch.

## Pins and hashes

- `firn add` / `firn update` resolve `REF` to a full commit and write `NAME URL REF COMMIT TREE-SHA256`. `REF` is only used again by `firn update`.
- The tree hash is the SHA-256 of the sorted list `sha256(file)  ./path` over all files and symlinks of the checkout, without `.git/` and `.firn/`.
- `firn fetch`: a checkout whose hash matches is kept (`have`). Otherwise it is rebuilt from the cache at the pinned commit and compared. A wrong hash is an error (`checksum mismatch for NAME`, exit 125). A row without commit/hash gets pinned, unless `--locked` is given (then it is an error).
- Nothing is ever resolved from `REF` during `build`/`test`/`fetch` once a row is pinned, so the same `firn.deps` gives the same sources on every machine.

## Offline

Every git fetch goes through a bare mirror in `$FIRN_PKG_CACHE` (default `~/.cache/firn-pkg/git/<hash-of-url>.git`). After the first fetch `FIRN_OFFLINE=1` (or `firn fetch --offline`) works from the cache and from existing checkouts; an uncached dependency fails with `cannot fetch ... (offline, not in cache)`. A clean checkout of the project with `firn.deps` committed rebuilds without network when the cache is warm.

## Transitive dependencies

A dependency with its own `firn.deps` fetches its git dependencies into its own `.firn/deps` (nested; its `needs` lines point there). `firn fetch` walks the whole graph (path and git packages, every package once).

Conflicts are detected before the compiler runs:

- same name, different source or commit in two places: `conflict: dependency 'x' resolves to ... and to ...`.
- same name, same commit, but two checkouts: `dependency 'x' is checked out twice`. The compiler allows one directory per package (`package 'x' comes from two directories`). Workaround: point both `needs` at the same path, for example `needs txt .firn/deps/util/.firn/deps/txt` in the root. Two siblings that both depend on the same git package cannot be unified without a compiler change (roadmap).

## Example

`examples/pkg_demo`: `app` needs `greet` by path.

```
cd examples/pkg_demo/app && firn build . -o /tmp/app && /tmp/app    # hello, world!
firn test                                                          # passed 1, failed 0
```

## Checks

`bash tools/firn/add_check.sh` (31 checks: pins, transitive deps, hash mismatch, offline, update, remove, conflict, example) and `bash tools/firn/test_firn.sh` (driver, 21 checks).

## Not there yet

Registry/index and semver ranges, publishing, docs generator, LSP, vendoring, patch overrides, flat sharing of transitive git packages. See the ecosystem items in the roadmap.
