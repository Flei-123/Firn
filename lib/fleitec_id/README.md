# lib/fleitec_id -- Firn client of the Fleitec-ID protocol

The protocol is specified in the JARVIS tree (`docs/FLEITEC-ID.md`); the shared
test vectors live next to it (`docs/fleitec-id/vectors.json`, copy in
`tests/data/fleitec-id-vectors.json`).

| module | what |
|---|---|
| `fleitec_id.jwt` | verify a session token (JWT HS256), strict rules, demanded `iss`/`aud`; copy of FirnChat `src/web/jwt.fi` |
| `fleitec_id.token` | device tokens `fkz1`/`fkm1`, API tokens `ckt_`, sha256 hex of a secret, pair codes |
| `fleitec_id.cookie` | `fleitec_session` Cookie header / Set-Cookie |
| `fleitec_id.relay` | FreeViewer relay URLs (`wss://host/fv/ws` -> account endpoints), token escaping |

Test: `tests/2012_fleitec_id.fi` (runs with `test.sh` in all four build levels).
Not in this library: the HTTP client (use `net.http`), the address-book merge, password hashing.
