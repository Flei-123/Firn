# DNS: Firn `net.dns` vs. the OrientOS (osum) resolver -- comparison (04.10.2026)

Question: should OrientOS use the new Firn resolver library instead of its own DNS code?
Result of a READ-ONLY check; nothing was changed in osum.

## 1. The Firn library (`net.dns`, main)

* Files: `lib/net/dns.fi` (1295 lines), `dnsconf.fi` / `dnsconf.windows.fi` (30 / 82), `udp.fi`, `dns_main.fi` (a CLI);
  doc `docs/DNS.md`. License **MPL-2.0** (SPDX in every file). It is on **main** (commit `090c1bcf3`), NOT on the branch `dns-pic`
  (that is the old line -- not touched, not merged).
* Features: A + AAAA records, UDP with doubling timeout (2 s, 2 attempts, cap 8 s) and every server in turn, **TCP fallback** on TC,
  CNAME chains (in one answer or across answers, at most 8 links, cycle = `CnameLoop`), **cache** (32 names, positive TTL 1..3600 s,
  negative 30 s), `/etc/hosts` (and the Windows hosts file), `resolv.conf` / `GetNetworkParams`, `localhost`, numeric IPv4 shortcut.
* Wire safety: name compression with hop limit 16, forward / out-of-range pointers refused, label types 01/10 refused, records off
  the chain of the question dropped. A reply counts only from the server asked, port 53, our ID and our question.
* Spoofing: 16 random bits transaction ID from the kernel (`std.crypto.random`); **the source port is chosen by the kernel** (the library
  does not bind a random port itself); no 0x20 case randomisation; no EDNS0; no DNSSEC/DoT/DoH; IPv4 transport only; no search list, no IDN.
* Built-in fallback servers 1.1.1.1 / 8.8.8.8 when nothing is configured.
* Dependencies: `std.rt`, `std.net`, `std.crypto.random`, `net.udp` -- the UDP layer calls **Linux system call numbers**
  (`socket/bind/sendto/recvfrom/setsockopt`, translated by the Windows seam). It is a user-space library on a libc-less Linux ABI.
* Proof: `tests/2090_dns_wire.fi` (790 lines, real answers of 1.1.1.1 against an independent Python decoder, hostile messages),
  `tests/2091_dns_loopback.fi` (579 lines, own DNS server over UDP+TCP in the test: cache, TC to TCP, NXDOMAIN, SERVFAIL, retry, forged
  answers with wrong ID / right ID from another port, TTL expiry, AAAA), `tools/dns/run.sh` (fake DNS, real network, https by name,
  Windows build under Wine). Mutations of `dns.fi` were checked to fail the tests.

## 2. What osum does today

* Files: `lib/libc/dnswire.fi` (502 lines, pure octet work, no system call) + `lib/libc/dns.fi` (543 lines, the socket / clock / random /
  retry half) = **1045 lines, MIT**. Used by `kernel/app/fetch.fi` (`dns.resolve_a`, certificate-checked fetch / ota), `kernel/user/host.fi`
  (the `host` tool), via `/etc/resolv.conf`. `lib/libc/dns.fi` is compiled for **both** profiles (kernel user space and `--profile=app`):
  one implementation.
* Features: A / AAAA / CNAME (count), up to 4 servers from `resolv.conf` (`options timeout/attempts`, plus a visible `fallback` line), **no
  built-in server address** (a test greps for `8.8.8.8`), R_TRUNC reported (**no TCP fallback**), **no cache**, **no `/etc/hosts`**, no EDNS0 / DNSSEC / DoT / DoH.
* Spoofing hardening is stronger than the Firn library: ID from `getrandom`; **source port rolled from `getrandom` and bound explicitly**
  (the osum kernel hands unbound UDP sockets `40000 + (counter & 4095)` in `kernel/inet.fi`, a counter -- so binding is a must); **0x20** case
  randomisation (`dnswire.q_build`); source address + port 53 checked; a mismatching reply is counted and dropped and the wait continues
  (the forger must displace the real answer, not just be faster); compression pointers must point strictly backwards, at most `MAX_JUMPS`,
  name < 255, label < 64.
* Proof: `tools/operation/run.sh` -- hostile fake server `dnsdienst.py --boese`, `kernel/user/dnswt.fi` (19 hand-built hostile messages
  against the wire parser, no packet), `host` / `ota` / `fetch` against it. The pinned Firn in `vendor/firn` is the OLD
  line (`c66c6bc`, pre-MPL); its `lib/net/dns.fi` there is the old TCP-only resolver of round B5 (370 lines) -- the new `net.dns` is NOT in the pin.
* Known gaps (osum's own words in the `dnswire.fi` header, list in `docs/RUNDE-BETRIEB.md` "was noch fehlt"): no DNSSEC, DoT, DoH, no cache, no IDNA;
  the answer is trusted as far as the nameserver is -- the real defence is the certificate check in `fetch`. No TCP retry and no hosts file were found in `dns.fi` (checked by grep, not by a run).

## 3. Comparison

| | Firn `net.dns` | osum `libc/dns` + `dnswire` |
|---|---|---|
| size | 1295 + conf (~1400) | 1045 |
| license | MPL-2.0 (compatible with GPL-2.0-only, MPL 3.3 secondary license; file-level copyleft; osum already vendors MPL Firn) | MIT |
| TCP fallback / cache / hosts / search | TCP yes, cache yes, hosts yes, search no | no / no / no / no |
| CNAME chains | yes, across answers, 8 | cname count in one answer |
| ID random / source port random / 0x20 | yes / **kernel's choice** / no | yes / **own random bind** / yes |
| forged-reply handling | dropped, retries | dropped, keeps waiting |
| built-in server | 1.1.1.1 / 8.8.8.8 | none on purpose |
| runs without libc | no: Linux syscall numbers (`std.net`, `net.udp`) | yes: osum libc, kernel + app profile |
| tests | 1.4k lines + mutation check, loopback server | hostile server + 19 wire cases + VM runs |

Weighed: osum's code is **safer on osum** (its kernel's port counter makes the Firn library's "kernel picks the port" a real weakness there)
and is the one that runs without libc. The Firn library is **richer** (TCP, cache, hosts, chains).

## 4. Recommendation: **no swap now.** Backport instead.

* The library cannot run in osum's kernel profile: it needs a Linux-ABI UDP layer and `std.crypto.random`; osum pins an old Firn without it. A
  switch needs (a) a bumped `vendor/firn` pin, (b) an osum back end for `net.udp`, (c) the library hardened with osum's tricks (explicit random bind,
  0x20, "no built-in server" option) -- otherwise osum LOSES security. Effort **1-2 weeks**, risk **high** (ota / fetch / certificate chain / install depend on name resolution).
* Cheap and useful instead (**1-2 days**, low risk, in osum): add to `lib/libc/dns.fi` what it lacks -- TCP retry on TC (osum has a TCP stack for fetch / ota),
  a small cache with TTL, an `/etc/hosts` read. Same tests (`tools/operation/run.sh`) plus a TC case in `dnsdienst.py`.
* And the other direction (**~1 day**, in Firn): give `net.dns` an optional "bind a random source port", 0x20 randomisation and a switch for "no built-in fallback server";
  then both resolvers have the same hardening and a later convergence (one shared wire parser `dnswire`) becomes realistic.

If convergence is wanted later, migration plan: 1. pick the shared part = the wire parser (pure octets, both sides have strict tests); 2. run osum's 19 hostile
messages (`dnswt.fi`) and Firn's `2090_dns_wire` against BOTH parsers; 3. make `net.dns` use the shared parser, osum keep its socket half; 4. only then the UDP
back end (osum syscalls) behind `net.udp`; 5. VM checks after each step: resolution, error cases (NXDOMAIN, SERVFAIL, timeout, TC, forged ID/port/0x20),
certificate chain, `fetch`, `ota` against `tools/operation/run.sh`.
