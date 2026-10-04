# Async IO (`lib/async`)

One thread waits for many sockets, timers and posted work, and calls a
function when something is ready. Callback API, no `async`/`await`. Built for
programs like FleiLauncher: a window that must stay responsive while it
downloads, talks to a server over TLS and keeps a WebSocket open.

| module | file | what it is |
|---|---|---|
| `async.loop` | `lib/async/loop.fi` | the event loop: descriptors (epoll or poll), timers, deferred calls, wake-up, hooks |
| `async.stream` | `lib/async/stream.fi` | non-blocking TCP and TLS-client streams, an acceptor for servers, timeouts, back pressure |
| `async.ahttp` | `lib/async/ahttp.fi` | HTTP/1.1 client on the loop, on top of `net.http`'s parser (http and https, redirects, cookies, gzip) |
| `async.wsc` | `lib/async/wsc.fi` | WebSocket client (ws and wss) with ping/pong, keep-alive, fragmentation, closing handshake, reconnect with backoff |
| `async.post` | `lib/async/post.fi` | hand work to the loop thread from other threads (`post`), run blocking work on `std.pool` workers (`bridge`) |
| `std.net` (additions) | `lib/std/net.fi` | `set_nonblocking`, `read_nb`/`write_nb`/`accept_nb`, `connect_nb` + `connect_finish`, `sock_error` |
| `tls.tls` (additions) | `lib/tls/tls.fi` | the handshake as a state machine (`tls_handshake_step`), non-blocking records, buffered output |
| `ws.ws` (additions) | `lib/ws/ws.fi` | `send_fragmented`, `backoff_ms`, `close_timeout`, `send_close_frame` for the blocking client |
| `window.wait_any_fd`, `fuiwin.window_wait_many_fd` | `lib/window/window.fi`, `lib/plat/fuiwin.fi` | wait for window events AND the loop's descriptor |

## The loop

```
var l: loop.Loop = loop.loop_new()
loop.loop_init(&l)                         // epoll if the kernel has it, else poll
let w = loop.watch_add(&l, fd, loop.EV_READ, on_readable, ud)   // fn(*mut Loop, u64, u32)
loop.timer_after(&l, 250, on_timer, ud)    // fn(*mut Loop, u64)
loop.timer_every(&l, 1000, on_tick, ud)
loop.defer_call(&l, f, ud)                 // next round, never inside the caller
loop.run(&l)                               // until loop.stop(&l) or nothing is left
```

* `watch_add/mod/del`: level triggered. A handle carries a generation, so a
  callback that deletes another watch in the same round never gets a stale
  event for it (tests/2120 F).
* Timers are a binary heap ordered by (due time, creation order); a timer set
  by a callback fires in the *next* round at the earliest; `timer_cancel` is
  idempotent.
* `wake(&l)` is the one call that may come from another thread (eventfd; a
  loopback socket pair where there is no eventfd, i.e. Windows).
* `run_once(&l, ms)` is one round: deferred calls, then descriptors (waiting at
  most `ms`, never longer than the next timer), then timers.
* `loop_fd(&l)` is the epoll descriptor: readable while a watched descriptor
  is ready. `embed_wait_ms(&l, cap)` is how long a foreign wait may sleep so
  that timers and deferred calls are not late.

### Backends

| backend | where | notes |
|---|---|---|
| epoll | Linux x86-64, AArch64 | `epoll_event` is 12 octets packed on x86-64 and 16 on AArch64; the loop asks `uname(2)` |
| poll | everywhere | `poll(2)`; `ppoll` on AArch64 (the compiler rewrites the shape); on Windows the seam answers it with `select`, at most 64 sockets per round |

Compiler tables added for this: AArch64 numbers for `epoll_create1`,
`epoll_ctl`, `epoll_pwait`, `eventfd2` (`compiler/src/syscalls.rs`, and the
browser table says "no files"); the Windows seam learned `getsockopt(SO_ERROR)`
(`win_seam.rs`, `win.rs`), which a non-blocking connect needs.

## Streams

`stream_open(l, addr, port, timeout_ms, cb, ud)` / `stream_open_tls(..., host,
store, now, ...)` / `stream_adopt(l, conn, ...)`; one callback
`cb(stream, ud, kind, arg)` with `SE_CONNECTED`, `SE_DATA`, `SE_DRAINED`,
`SE_CLOSED` (reason `CL_EOF/LOCAL/TIMEOUT/ERROR/CONNECT/TLS/MEMORY`).
Input stays in the stream until `stream_consume`; output is queued by
`stream_write` (always accepted, look at `stream_out_pending`). One timeout
number per stream covers connect, handshake and every later silence, with one
timer per stream re-armed lazily. The stream is freed by the library after
`SE_CLOSED` returns. `acceptor_new` listens and hands out non-blocking
connections.

### TLS without blocking

The blocking client (`tls.tls_handshake`) is now the same code run in a loop:
`tls_handshake_step` keeps its state in the struct (`hs_step`, the flags, the
certificate array) and answers `TlsError::WouldBlock` when it needs more
octets. A record that has arrived in part consumes nothing, so a stop in the
middle of a record loses nothing; output is encrypted into a buffer and sent as
far as the kernel takes it (`tls_out_pending`, `tls_flush`). `tests/2122` makes
the server dribble 1-7 octets per millisecond to prove it. The 4 TLS blocking
suites (`tools/tls/run.sh`: crypto, X.509, client against openssl/real hosts,
P-256, server) still pass.

## HTTP

`aclient_get/post/request(...)` with `cb(request, ud)`; the answer is an
ordinary `net.http` `Response` (`areq_response`), so `resp_header` etc. work.
Shared with the blocking client: request building (headers, cookie jar, gzip),
head parsing, `Content-Type`, `Content-Encoding`, cookies, redirect rules (303
and 301/302 of a POST become GET, 307/308 keep method and body, hop limit, loop
detection), TLS roots (`aclient_http(a)` is the embedded `net.http.Client`:
`client_add_trust_pem`, `client_set_max_redirects`, ...). New: the framing as a
push parser (Content-Length, chunked with extensions and trailers, until
close), a total deadline per request, errors as `net.http.err_num` numbers plus
`AE_TIMEOUT`/`AE_CANCELLED`.

## WebSocket

`wsc_new(l, cb, ud)`, `wsc_connect_str(c, "wss://host/path")`, then
`WE_OPEN`, `WE_MESSAGE`, `WE_PONG`, `WE_CLOSE`, `WE_ERROR`, `WE_RECONNECT`.
Pings are answered, `wsc_set_keepalive(interval, timeout)` detects a dead peer,
`wsc_close(code)` runs the closing handshake with a timeout,
`wsc_set_auto_reconnect(on, base, cap, max)` retries with `base * 2^n` (capped,
+-20 %), `wsc_set_fragment_size` splits outgoing messages, incoming fragments
are reassembled up to `wsc_set_max_message` (1009 above). The protocol checks of
the shared codec (reserved bits, unmasked/masked, UTF-8, control frame rules)
close with the right code (1002, 1007, 1009).

## Threads, and the UI thread

`async.post`: `post(&po, f, arg)` from any thread runs `f(loop, arg)` on the
loop thread, in order, in a later step. `bridge_call(&br, work, arg, done, ud)`
runs `work` on a `std.pool` worker and `done` on the loop thread. The loop
announces its sleep to the collector through hooks (`thread_blocking_an/out`)
that only `post.fi` sets, so programs without threads do not carry the thread
builtins.

A fUi program waits like this (tools/async/ui_probe.fi):

```
let ms  = loop.embed_wait_ms(&l, 1000)
let hit = window.wait_any_fd(&ws, n, loop.loop_fd(&l), ms)   // window index, -1 time, -2 loop
loop.run_once(&l, 0)                                          // network, timers, posted results
if hit >= 0 { /* the window's events */ }
```

A window with an event wins over the loop's descriptor, so a busy network
cannot starve the input. Measured on an Xvfb: network, worker and timer results
arrive in 0.3 s with no window event, an idle second costs 0 clock ticks, a
close event is seen in 0 ms, 9 loop rounds in 2 s.

## What is proved, and against what

| what | held against | where |
|---|---|---|
| loop, timers, defer, watches, wake, `loop_fd` | the kernel (both backends, 4 build levels, AArch64/qemu, Windows/Wine) | tests/2120 |
| streams: 120 echo clients, refused connect (errno 111), idle timeout, 4 MiB with a closed window, EOF, graceful close | itself, both backends, Wine, qemu | tests/2121 |
| non-blocking TLS 1.3: 100 KB, dribbled handshake, wrong name and unknown CA refused | `tls_server.fi` in-process; blocking suites unchanged | tests/2122, tools/tls |
| HTTP client: length, chunked (dribbled), until-close, 302/301/307/303, loop, hops, gzip, cookies, timeout, refused, 3 MiB, 100 at once | an in-process server | tests/2123 |
| WebSocket client: echo 70 KB fragmented, wss, server fragments with a ping inside, closing both ways, three protocol violations, keep-alive timeout, backoff 20/40/80/80, server that appears later, dropped connection, sub-protocol, wrong name | an in-process server (+ `tls_server.fi`) | tests/2124 |
| posting from 4 threads (2000 posts, per-thread order, loop thread ids), sleeping loop woken, bridge, queue-full refusal, GC on workers while the loop sleeps | itself (stress: 25 runs) | tests/2125 |
| 1000 connections open at once, both directions | **Python asyncio** (peak counted by the server) | tools/async/check_conn.py |
| window + loop in one thread | an Xvfb driven by python-xlib | tools/async/check_ui.py |
| wss against real services | echo.websocket.org (0.4 s), ws.ifelse.io (1.9 s): system roots, DNS, TLS 1.3, 70000 octets in fragments, ping/pong, close | tools/async/wss_main.fi |

Numbers (this server, loaded): 1000 clients open at once and echo in 190-270 ms
(ramp 64-220 ms); 10 000 in one process: epoll 4.2 s, poll 3.9 s (the ramp of
2.5 s dominates; **a mostly idle crowd, where epoll should win, was not
measured**).

## HONEST

* **Not tried on real Windows, macOS, OrientOS, Android.** Windows runs under
  Wine only (the whole suite 2120-2125 and the scaled-down 28 connections);
  `tools/async/winkit.sh` builds a kit for a real machine. Android: epoll and
  eventfd are in the compiler's tables for AArch64 and x86-64 but nothing was run
  on the emulator. macOS: no kqueue backend (poll would have to be used; not
  tried).
* **Windows limits:** the seam's `poll` is `select`: at most 64 sockets per
  round, and the seam's descriptor table holds 256. The loop says so
  (`loop.socket_limit`) and the tests scale themselves down there.
* **TLS 1.3 only**, as the rest of the library: `ws.postman-echo.com` offers
  TLS 1.2 and cannot be reached (also not by `openssl s_client -tls1_3`).
  The TLS handshake computes on the loop thread (a few ms each).
* **Name lookup blocks** (`net.dns`, UDP with a timeout; cached by TTL).
  Numeric addresses and `localhost` never block. `bridge_call` can move a
  lookup to a worker, but the HTTP and WebSocket clients do not do that for you.
* **No keep-alive pool** in the HTTP client (every request is one connection,
  `Connection: close`), no response cache, body held in memory (32 MiB).
* **No queue for WebSocket messages** sent while connecting or reconnecting
  (`wsc_send_*` answers false unless open): the program keeps its own.
* **Callbacks are plain function values with a `u64` argument**; the loop keeps
  them in memory the collector does not scan, so closures with captures must be
  kept alive by the program. The compiler's escape check refuses `&local`
  as `ud` unless the function is `#[allow_escape]`.
* Edge triggering, `EPOLLONESHOT`, `EPOLLRDHUP`, `sendfile`, UDP and
  unix sockets on the loop are not there. TLS servers on the loop are not there
  (`lib/http/server.fi` has its own poll loop with a TLS server; the tests drive
  `tls_server.fi` by hand on a stream).
* `ws.ws` (the blocking client) and the loop's client share the codec only.

## Running it

```
bash tools/async/run.sh                  # 1000 connections, UI probe, real wss, Windows under Wine
bash tools/async/winkit.sh [out.zip]     # the kit for a Windows machine (needs only Python 3)
```

`test.sh` section 77 runs the first; tests 2120-2125 are in section 3 (every
build level; 2122 takes 17 s of the `--no-opt` level).
