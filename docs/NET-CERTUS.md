# Net APIs for Certus (note from Firn, 13.10.2026)

No Certus code was changed. These are the Firn pieces Certus should use instead of its own copies.
Everything below is tested on LOOPBACK / local fakes only (no real IPv6 route, no real network except what
`tools/dns/run.sh` does when online).

| need | use | file | proof |
|---|---|---|---|
| many sockets, timers, wake-up | `async.loop` (epoll, poll fallback; `EV_RDHUP`, `EV_ET` new) | lib/async/loop.fi | tests/2120, 2867, tools/net/epoll.sh |
| name -> addresses | `net.addrinfo.addrs_resolve` (v4/v6 literal, localhost, hosts, A+AAAA) | lib/net/addrinfo.fi | tools/netmc (1065 checks vs Python) |
| connect by name (Happy Eyeballs) | `addrs_connect(list, port, stagger_ms, total_ms, ..)` | lib/net/addrinfo.fi | tools/netmc |
| SRV (`_minecraft._tcp.`) | `resolve_service` | lib/net/addrinfo.fi | tools/netmc |
| own resolver, no libc | `net.dns` (UDP/TCP, CNAME, cache, hosts, resolv.conf, IPv6 servers new) | lib/net/dns.fi | tests/2090, 2091, 2868, tools/dns |
| v6 sockets | `std.net.connect6`, `listen_tcp6`, `peer_is_v6`; UDP `net.udp.udp6_*` | lib/std/net.fi, lib/net/udp.fi | tools/netmc, tools/dns/check6.py |
| stateful callbacks on the loop | capturing `gc fn` + `async.fnpin.pin/unpin` | lib/async/fnpin.fi | tests/2869 + tools/net/pin_mutation.sh |
| blocking work off the loop thread | `async.post` bridge (DNS is blocking!) | lib/async/post.fi | tests/2125 |

## Rules to keep in mind
* `net.dns` is BLOCKING: call it from a `post.bridge_call` worker when the loop must stay responsive.
* Loop callbacks are raw addresses (loop.fi A2): capture-free `fn`, or pin the closure with `fnpin`.
* `EV_ET` only on the epoll backend; the poll backend (and Windows) stays level.
* IPv6 DNS servers: Linux only; no zone ids; Windows seam untested.
* Push relay by hostname (Roadmap r74): building blocks exist (`addrs_resolve` + `addrs_connect`). The
  Android push thread (`lib/plat/android/push.fi`) is `#[no_gc]` raw syscalls with a dotted-IPv4 `ipv4()`;
  switching it to names means a worker-side resolve before the thread starts (it must not call GC code) and
  TLS (r74 part 2). Not done, needs the Android emulator.
