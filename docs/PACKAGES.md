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

## Diamond dependencies: one version per package

Two dependents may need the same package (`app -> a -> txt`, `app -> b -> txt`). A build contains ONE `txt`. The rules, the same in `firnc0` and `firnc1`:

1. Different versions: the highest wins (`docs/ROUND93.md`). A `needs NAME PATH VERSION` line is a wish: the winner has to be at least that version with the same first number, otherwise `dependency 'txt' is version 1.0.0, needed is 0.1.0 or higher with the same first number` (at the line of the `needs`).
2. Same version, same content: one package. `firn fetch` writes the tree hash of every git checkout to `<checkout>/.firn/pin` (not part of the tree hash). Two directories with the same name, the same version and the same non-empty pin are the same package; the smaller root path wins, whatever the order the edges were found in, so `firn.lock` is deterministic and lists the package once.
3. Same version, different content (different pin, a pin on one side only, or two path directories): `package 'txt' comes from two directories with version 0.1.0`.

`firn fetch` checks the same rules before the compiler runs (`conflict: dependency 'txt' 0.1.0 resolves to ... and to ...`: same version from two different git sources, or `is checked out twice`: two path directories). Different versions from different sources pass and are left to rule 1. `firn verify` also checks the pin marker (`pin marker missing or stale: run firn fetch`).

`firn tree` marks the outcome of these rules for every package that occurs in more than one directory: `[used]` (the copy the compiler uses), `[not used: 0.2.0 wins]` (a lower version), `[not used: same package as the used copy]` (same version and pin, larger root path) and `[conflict: same version, different content]` (rule 3; no copy is used). A package that occurs once has no mark.

Each dependent still keeps its own nested checkout in `.firn/deps` (cheap, from the cache); only one of them is compiled. The compiler trusts the pin file: it is as good as the last `firn fetch`/`firn verify`.

## Example

`examples/pkg_demo`: `app` needs `greet` by path.

```
cd examples/pkg_demo/app && firn build . -o /tmp/app && /tmp/app    # hello, world!
firn test                                                          # passed 1, failed 0
```

## Checks

`bash tools/firn/add_check.sh` (pins, transitive deps, diamonds, hash mismatch, offline, update, remove, conflict, example) and `bash tools/firn/test_firn.sh` (driver, 21 checks).

## Not there yet

Registry/index and semver ranges, publishing, docs generator, LSP, vendoring, patch overrides. See the ecosystem items in the roadmap.
