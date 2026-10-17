# lib/fleitec_login -- Firn client of "Sign in with ..." (LOGIN-PLAN, roadmap r80 / L2)

Client/verifier side only. The broker/provider is built elsewhere (JARVIS `fleitec-id`, XOFFI builds its own OIDC).
Module files carry the prefix `fl_` because Firn module names are global (`jwt` would clash with `fleitec_id.jwt`).

| module | what |
|---|---|
| `fl_util` | base64url, S256, random, percent/query, JSON readers, endpoint rule (https; http only loopback) |
| `fl_transport` | `Transport{f}`: `fn(*mut HttpReq, *mut HttpResp) -> bool`; every HTTP goes through it (offline tests) |
| `fl_nethttp` | production transport on `net.http` (TLS, no redirects, 20 s; `net_set_timeout(ms)` shortens it; `net_add_trust_pem(p, n)` adds the operator's own root CAs to the system roots, the chain check is never off) |
| `fl_jwks` | JWKS (OKP/Ed25519), cache with expiry, rotation, pinned emergency key, refetch rate limit |
| `fl_cache` | discovery + JWKS cache for a server (own ttl per document, `cache_warm` before expiry, stale-while-error up to `stale_max`, failure brake `min_retry`, unknown-kid brake survives the request; text kept in raw heap, fits a `static mut`) |
| `fl_jwt` | `verify(token, cfg, now) -> Claims` (EdDSA, kid, iss/aud mandatory, ID section 4 rules); `verify_legacy_hs256` separate |
| `fl_oidc` | Code+PKCE: `discover`, `begin`, `finish`, `token_refresh`, `revoke`, `logout_url`; id_token EdDSA or (JOSE mode, foreign IdP) RS256/ES256 via `auth.jose` |
| `fl_device` | RFC 8628 `start`/`poll` (interval, slow_down +5 s, expired, denied) |
| `fl_link` | `link_begin` (prompt=login, max_age=0), `link_finish` (fresh `auth_time`, expected account), `unlink` (proposed broker contract) |
| `fl_session` | access+refresh, `ensure_fresh` before expiry, refresh rotation, wipe on `invalid_grant`, JSON (de)serialising |
| `fl_provider` | provider interface jarvis / xoffi (PLANNED) / password |

Tests
- `tests/3700_fleitec_login_jwt.fi`: 44 own EdDSA vectors + the SHARED Fleitec-ID vectors (`tests/data/fleitec-id-vectors_v2.json` =
  copy of JARVIS `docs/fleitec-id/vectors_v2.json`, same file the Node service passes; `tools/fleitec_login/sync_vectors_v2.sh [--check]`):
  jwt_eddsa (34), claims_v2 (7 legacy HS256 + 5 EdDSA), legacy HS256, JWKS rules, JOSE group (RS256/ES256 via oidc, 6).
  Reason names are the shared ones (`kid`, not `nokey`).
- `tests/3701_fleitec_login_flows.fi`: a MOCK IdP as `fl_transport` (EdDSA id_tokens via `ed25519_sign`): discovery, begin, code flow
  + negatives (state, redirect_uri, iss, PKCE, code twice, replay), id_token negatives (nonce, alg swap, expired, iss, aud, azp, kid,
  signature), JWKS rotation/refetch brake/expiry, fl_cache (hit = no network, ttl, warm-up lead, stale-while-error, failure brake, kid brake across requests, rotation absorbed), device flow (pending, slow_down, expired, denied), session (refresh rotation, reuse,
  wipe), link (prompt=login, auth_time, expected account, unlink), provider (jarvis, xoffi planned, password).
- `tests/manual/fleitec_login_interop.sh` (OPTIONAL, needs node + curl, not in test.sh): the real Node service `fleitec-id` on
  127.0.0.1:8470, curl plays the browser, the Firn library does code flow, refresh rotation, reuse detection and device flow.

HONEST
- Not thread safe (one Session/Client = one owner). Tokens are GC strings, cannot be wiped from memory.
- No HTTP loop/sleeping: `device.poll` and `session.ensure_fresh` are single steps; the caller schedules.
- JOSE mode is tested with vectors only (verify_id_token); a full JOSE login flow against a mock RS256/ES256 IdP is not.
- `provider_password.finish` discovers the issuer first (it needs the keys for the id_token).
