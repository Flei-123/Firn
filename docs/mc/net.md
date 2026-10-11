# mc_net -- connecting: retry with back-off, server list ping

`lib/mc/mc_net.fi`, built on `mc_client` / `mc_codec`.

```
net_connect_retry(addr, port, attempts, backoff_ms) -> net.Conn     wait doubles (cap 8 s); closed conn if all failed
net_status(addr, port, host, &version, attempts, max_waits, &Status) -> NS_OK | NS_CONNECT | NS_SEND | NS_TIMEOUT | NS_BAD_REPLY | NS_CLOSED
Status { protocol, name[64], name_len, online, max, latency_ms, motd (plain text via mc_text) }
```

The protocol number in the answer selects the Version object (`mc_versions.version_by_protocol`) before the client logs in:
one binary, several versions (docs/MC-VERSIONS.md). The status exchange itself is identical in every version.
"Non-blocking" = no call blocks longer than the timeouts you pass (receive timeouts of the codec); an epoll based variant on
`lib/async` is not built.

Tests: `bash tools/mc/run.sh net` -- an independent python status server (own framing / JSON): string and component MOTD,
server closes, broken JSON, silent server (timeout within the wait budget), nothing listening (3 attempts with back-off,
750 ms), retry that reaches a server which starts listening 900 ms late.
