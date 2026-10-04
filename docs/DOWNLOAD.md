# net.download -- the download manager

`lib/net/download.fi`. Built on `net.http` (one client per worker slot,
keep-alive, TLS), `std.pool` (worker threads), `std.hashfile` (digests) and
`std.fs` (atomic rename). Asked for by FleiLauncher (3000 Minecraft assets,
libraries, a Java runtime), useful for any program that fetches a list of
files over a flaky link.

```firn
import std.pool
import net.download

fn __thread_work(kind: u64, arg: u64) -> u64 {          // the pool's one line
    if kind == pool.WORKER_KIND { return pool.worker_main(arg) }
    return 0
}

var d: download.Dl = download.dl_new(8)                  // 8 workers
download.dl_set_progress(&d, on_event, 0)                // optional callback
download.dl_add(&d, url, dest, hashfile.HASH_SHA1, hex, size)   // size 0 = unknown
download.dl_add_mirror(&d, 0, "https://mirror/...")      // fallback URL
download.dl_start(&d)                                    // returns at once
// poll: dl_bytes_done / dl_bytes_total / dl_files_done / dl_item_state ...
download.dl_wait(&d)                                     // or dl_run(&d) = start + wait
download.dl_free(&d)
```

## What it does

| feature | how | proof |
|---|---|---|
| parallel, N workers | `std.pool`, items taken in the order added | 16 files of 0.6 s on 8 workers: 916 ms (serial 9600) |
| keep-alive per host | one `net.http` client per worker slot; a job takes the free slot whose connection already goes to its host | 200 files: 8 sockets; 3000 files: 16 sockets; 40 Mojang assets over https: 8 sockets |
| resume | data goes to `<dest>.part`; next attempt/run sends `Range: bytes=N-` (+ `If-Range` with a strong ETag or the Last-Modified), the part is re-hashed first so the digest covers the whole file | cut at 300000 of 1000000: second request `bytes=300000-`, exactly 1000000 octets on the wire; Mojang `client.jar` resumed from 5 MB, SHA-1 right, only the missing 36483720 octets fetched |
| server ignores Range / sends a 206 that starts elsewhere / 416 | 200 -> start over; wrong `Content-Range` -> refused and retried from zero; 416 -> part dropped | `check.py` C7, C8 |
| no blind resume | without a digest a part is only resumed when a validator is known (sidecar or `dl_set_validators`) | C4, C5, C6 |
| ETag / Last-Modified | `DL_META` keeps `<dest>.dlmeta`; `If-None-Match` / `If-Modified-Since`; 304 = `DL_NOT_MODIFIED`, zero octets | E |
| already there | digest (or `DL_TRUST_SIZE`, or the declared size) right -> `DL_SKIPPED`, no request | 3000 assets: second run 1.1 s, 0 requests |
| retry + backoff | per URL `retries` attempts (default 4); delay `base * 2^(n-1)` capped, -25 %..+25 % jitter; `Retry-After` honoured up to the cap; transient: connect/TLS/read errors, 408, 429, 5xx, wrong digest or size | D: 200 ms then 400 ms (+-25 %), cap, Retry-After 1 s |
| mirrors | the next URL is tried at once (no delay); a 4xx retires that URL for this item | D6, D8 |
| rate limit | `dl_set_rate(bytes/s)`, one token bucket for all workers (100 ms of burst) | 600 kB at 300 kB/s: 1925 ms |
| progress | polled totals (`dl_bytes_done/total`, per item) or a callback (START, PROGRESS <= every 100 ms per file, RETRY, DONE) | A: counters equal the sum of sizes |
| verify | MD5/SHA-1/SHA-256/SHA-512 while the data arrives | A, B, D |
| atomic | fsync, rename over dest; any failure leaves the old dest or nothing, and removes a bad part | D4: old dest untouched after a digest that never matches |
| cancel | `dl_cancel`: queued items end `DL_CANCELLED` without a request, running ones after the next chunk; parts stay | G: cancelled at 100000 octets in 0.4 s, the next run resumes with `Range` |
| inline mode | `dl_run_inline`: one after the other on the caller's thread, no pool needed | I, tests/2140 H |

## What it needed from the neighbours (all additive)

* `net.http`: `client_set_headers/add_header/clear_headers` (extra request
  lines such as `Range`), `client_set_user_agent`, `client_set_sink` (bodies of
  status 200/206 go to a callback in pieces instead of memory: no 32 MiB limit,
  `HttpError::Aborted` when the sink says no), `client_close`,
  `http_get_num/http_post_num` (the error as a number: the error type of an
  imported module cannot be named in a `catch` handler), an `Accept` or
  `User-Agent` line in the extra headers replaces the default, and the read
  buffer no longer keeps a second copy of a large body.
* `std.fs`: `open_append`, `open_truncate`.

## Honest

* **D1** the program needs the `__thread_work` line (like `std.pool`), or uses `dl_run_inline`.
* **D2** since the Windows threads (`docs/WINDOWS_THREADS.md`) `std.pool` runs on real threads there too; before them (or on a target without threads) `dl_start` downloads everything before it returns.
* **D3** no proxy, no HTTP/2, no IPv6 (all `net.http`). The client's cookie jar and cache are not used.
* **D4** poll values are read while workers write: each is a whole word, the set is not a snapshot.
* **D5** the octets of a resumed part are trusted only through the final digest; without a digest a part someone edited is not caught.
* **D6** one manager per destination: two writers of the same `.part` corrupt each other, there is no lock file.
* **D7** a rename onto a file that is open (Windows) is retried for ~2 s, then the item fails with `DE_DISK` and the verified part stays.
* **D8** `Content-Encoding` is never asked for; if a server sends one anyway the body is decoded in memory (32 MiB limit) and the resume offset then means decoded octets.
* **D9** one client per worker (TLS buffers, a few hundred KiB each); URLs and paths are copied into the manager.
* **D10** only `<dest>`, `<dest>.part`, `<dest>.dlmeta` are written or removed; the directory is created.
* `fsync` per file is on by default (`DL_NOSYNC` turns it off for bulk lists); the directory is not fsynced.
* Not tested on a real Windows machine (FLEI-ONE cannot reach this server); under Wine the checks A, C, D, E, F, I pass.

## Platforms

| platform | status |
|---|---|
| Linux x86-64 | **tested**: tests/2140 in every build level, `tools/download/run.sh` (99 checks) |
| Linux aarch64 | tests/2140 compiles and **passes under qemu-aarch64** (threads, sockets, files); the Python checks were not run on it |
| Windows x86-64 | **tested under Wine** (`check.py` sections A, C, D, E, F, I); not on a real Windows machine. Paths should use `/` (`fs.path_parent` splits on `/` only; the Windows seam accepts it) |
| Wine, a caveat | across ~700 driver runs under Wine, three (two inside `check.py` section C, one in a loop of 40) ended with exit 0/-9 and NO output while other Wine or build jobs were running; 450 later runs, also under 8 busy-loop processes, did not reproduce it. Cause unknown (a trivial `W 2`/`X` run without downloads never failed in 300 runs). Treat a lone empty Wine run as a flake, a repeated one as a bug |
| macOS, OrientOS, Android | not built or run. The module only uses `net.http`, `std.pool`, `std.fs`, `std.hashfile`; it works wherever those do. Nothing in it is a stub, nothing was tried |

## Tests

* `tests/2140_download.fi` (every build level): backoff arithmetic, `dl_add` refusals, 40 files with SHA-1 on 4 workers against `lib/http/server.fi` in the same process, resume from a part (the server records the `Range` header), 404 + mirror, skip rules, `If-None-Match` + sidecar, inline mode. `tests/neg/2140_download_digest_type.fi`.
* `tools/download/run.sh`: `check.py` (99 checks incl. the real Mojang CDN: version manifest -> asset index -> 40 assets by SHA-1 and size, `client.jar` resumed) against `fake_server.py` (cuts connections, 503, `Retry-After`, wrong digests, ignores Range, bad `Content-Range`, chunked, gzip, redirects, stalls); three build stages; the Windows build under Wine.
* Throughput on this machine: 3000 files of 0.1-20 kB on 16 workers against a Python server: ~350 files/s.
