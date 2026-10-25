# mDNS / DNS-SD and SSDP in Firn

Pure cores over octets, thin socket layer, one CLI program per protocol.

| module | file | what |
|---|---|---|
| `net.mcast` | `lib/net/mcast.fi` (+ `.windows.fi`) | multicast UDP sockets: open with SO_REUSEADDR/REUSEPORT, join/leave, interface, TTL, loop |
| `mdns.mdns_wire` | `lib/mdns/mdns_wire.fi` | names (compression, escapes), header/question/RR parse, packet builder, TXT helpers |
| `mdns.mdns_cache` | `lib/mdns/mdns_cache.fi` | record cache: TTL, cache-flush, goodbye (TTL 0 expires after 1 s) |
| `mdns.mdns_resp` | `lib/mdns/mdns_resp.fi` | service registry, answers with known-answer suppression, legacy unicast, probe, announce, goodbye, conflict detection |
| `mdns.mdns_browse` | `lib/mdns/mdns_browse.fi` | query with known answers, ingest, resolve instance to host/port/TXT/A/AAAA |
| `mdns.mdns_sock` | `lib/mdns/mdns_sock.fi` | `browse_run`, `resolve_run`, `serve_open/start/loop/end` |
| program | `lib/mdns/mdns_main.fi` | `mdns_main browse <type> [--ms N] [--iface A] [--port P]`, `mdns_main serve <instance> <type> <host> <ipv4> <port> [--txt k=v] [--ms N]` |
| `ssdp.ssdp_msg` | `lib/ssdp/ssdp_msg.fi` | HTTPU parse, M-SEARCH / NOTIFY / response builders, ST matching |

## Tests

`tests/5200_mdns_core.fi`, `tests/5201_ssdp_msg.fi` and `tools/mdns/run.sh` (Python zeroconf both directions on 127.0.0.1: Firn browses a Python service, Python resolves and browses a Firn service, legacy unicast query, goodbye, name conflict).

## Honest

- IPv4 only on the wire (AAAA records are parsed, cached and answered; ff02::fb sockets are not implemented).
- One interface per socket (`--iface`, default kernel choice); no interface enumeration.
- One packet per response (1400 octets); no TC continuation for known answers; no NSEC negative answers; no response jitter.
- Windows option numbers are in `net/mcast.windows.fi` but untested; Android needs a WifiManager multicast lock from the app.
- SSDP: only the message layer exists (socket layer, device description XML are roadmap items).
