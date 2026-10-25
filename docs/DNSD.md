# An authoritative DNS server in Firn (`net.dnsd`)

`lib/net/dns.fi` asks questions. This is the other side: a server that ANSWERS them, for a zone
you describe in a few lines. Its reason to exist is the wildcard that ordinary DNS cannot do:
**one rule answers every name**, and the answer may contain the name that was asked.

The first consumer is the Minecraft gate (`_minecraft._tcp.<server>.mc.example.com` must answer
`SRV 0 5 443 <server>.mc.example.com`, because the client puts the SRV TARGET into its handshake
and the proxy routes on it). A plain wildcard SRV record has a fixed target, so every server
would look the same to the proxy; Aternos solves it with a DNS server of its own. This is that,
as a library.

## Modules

| module | file | what |
|---|---|---|
| `net.dnszone` | `lib/net/dnszone.fi` | the zone model: rules, owner patterns with `*` / `**`, `%1`, the text parser, the matcher |
| `net.dnsd` | `lib/net/dnsd.fi` | `dnsd_answer(zone, request, n, out, udp)`: a pure function from one datagram to one reply |
| `net.dnsd_serve` | `lib/net/dnsd_serve.fi` | UDP and TCP loops (`run_udp`, `run_tcp`), `open`, `stop`, counters, query log |
| program | `lib/net/dnsd_main.fi` | `dnsd_main <zone> [--port N] [--bind A.B.C.D] [--verbose] [--check]` |

## The zone file

```
zone mc.example.com
ttl 60
soa @ ns1.example.com hostmaster.example.com 1 3600 600 86400 30
ns  @ ns1.example.com
a   @ 203.0.113.7
a   * 203.0.113.7
srv _minecraft._tcp.* 0 5 443 %1.mc.example.com
```

* One rule per line: `<type> <owner> <data...>`. Types `a aaaa ns cname soa mx txt srv`.
* The owner is relative to the zone: `@` apex, `*` exactly one label, `**` one or more. One wildcard per pattern.
* `%1` in the data is what the wildcard matched; `@` as a whole name is the zone origin. Names in data are absolute.
* Exact beats `*` beats `**`; more literal labels beat fewer; the first rule of the file wins a tie.
  The best rule fixes the owner, every rule with that same owner is a record of the name.
* A wildcard only matches `a-z 0-9 - _` (case folded), so odd octets are never echoed into an answer.
* Comments are whole lines starting with `#`.

## Using it from Firn

```firn
var zone: dnszone.Zone = dnszone.zone_new()
let bad: usize = dnszone.zone_parse(&zone, text_ptr, text_len)   // 0 = ok, else the line number
var reply: rt.Buf = rt.buf_new()
if dnsd.dnsd_answer(&zone, request_ptr, request_len, &reply, true) { /* send reply */ }
```

For sockets see `dnsd_serve` (`open`, then `run_udp` and `run_tcp` in two threads, `stop`) and
`dnsd_main.fi` as the finished example (the thread hook `__thread_work` must be in the root file).

## What is proven

| what | how |
|---|---|
| the wire format, every field read back from the octets | `tests/5070_dnsd_wire.fi` (SRV/A/AAAA/NS/SOA/MX/TXT/CNAME/ANY, wildcard order, NXDOMAIN/NODATA/REFUSED, TC, EDNS, malformed input, 10 bad zone files) |
| real sockets, UDP and TCP, TC fallback, with the resolver of `net.dns` as the client | `tests/5071_dnsd_loopback.fi` |
| the finished program against raw octets and against `dig` | `tools/dnsd/run.sh` (24 checks x 3 build stages) |

## Honest limits

* No DNSSEC, no zone transfer, no NOTIFY/UPDATE, no rate limiting (do not run it as an open resolver; it is
  authoritative only and answers REFUSED outside its zone, but a public authoritative server can still be
  used for reflection like any other).
* CNAMEs are not chased. No empty non-terminals (`_tcp.foo` above an existing name is NXDOMAIN, not NODATA).
* UDP answers are cut to 512 octets (the client's EDNS payload, at most 1232) with TC set.
* TCP serves one connection at a time, 4 queries, 3 s timeouts. IPv4 sockets only. 64 rules per zone.
* To be reachable the zone has to be DELEGATED: one `NS` record (and an `A` record for that name server)
  in the parent zone, and UDP+TCP port 53 reaching the server.
