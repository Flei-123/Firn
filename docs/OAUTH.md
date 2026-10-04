# auth -- OAuth 2.0 / OpenID Connect, JWT, the Microsoft chain for Minecraft, "Sign in with Fleitec-ID"

Four modules for programs that sign a user in:

| module | file | what |
|---|---|---|
| `auth.jose` | `lib/auth/jose.fi` | read and check a JWT with the provider's JWKS: RS256/384/512, ES256/384, HS256; claims (`exp`, `nbf`, `iat`, `iss`, `aud`, `nonce`) |
| `auth.oauth` | `lib/auth/oauth.fi` | the OAuth/OIDC client: discovery, device code flow, authorization code + PKCE with a loopback redirect, refresh, id_token check, userinfo, revocation, token store in the keyring |
| `auth.msa` | `lib/auth/msa.fi` | Microsoft account -> Xbox Live -> XSTS -> Minecraft services -> ownership -> profile |
| `appkit.fleitec_login` | `lib/appkit/fleitec_login.fi` | "Sign in with Fleitec-ID": login / me / logout against the ID server, token in the keyring |

`lib/fleitec_id/jwt.fi` (the HS256 *server-side* verifier of the Fleitec-ID
session cookie) is reused where the work is the same: its strict base64url
decoder and its refusal codes (`JWT_OK` ... `JWT_NBF`) are what `auth.jose`
uses; it stays the one place that checks a Fleitec session token with the
shared secret.

## auth.oauth in 20 lines

```firn
import auth.oauth

var o: oauth.OAuth = oauth.oauth_new("my-client-id")
oauth.oauth_init(&o)
oauth.oauth_discover(&o, "https://accounts.example.com")  // or oauth_set_endpoints(...)
oauth.oauth_set_scope(&o, "openid profile offline_access")

var tok: oauth.Tokens = oauth.tokens_new()
// browser on this machine: opens the system browser, waits on 127.0.0.1
let r: i64 = oauth.oauth_login_loopback(&o, &tok, 300000)
// no browser: show da.user_code + da.verification_uri, poll
var da: oauth.DeviceAuth = oauth.deviceauth_new()
oauth.oauth_device_start(&o, &da)
let r2: i64 = oauth.oauth_device_wait(&o, &da, &tok)

oauth.tokens_save("myapp", "account-1", &tok)            // std.secret
oauth.tokens_load("myapp", "account-1", &tok)
oauth.oauth_ensure_fresh(&o, &tok, 60)                   // refresh when it runs out
```

Results are `OA_*` numbers (`oauth_error_text`): `OA_NET` (+ `oauth_net_error`),
`OA_HTTP`, `OA_PROTOCOL`, `OA_DENIED`, `OA_EXPIRED`, `OA_PENDING`,
`OA_SLOW_DOWN`, `OA_CANCELLED`, `OA_TIMEOUT`, `OA_INVALID_GRANT`, `OA_STATE`,
`OA_CONFIG`, `OA_TOKEN_INVALID` (`oauth_err_desc` = why), `OA_ERROR`
(`oauth_err_code`/`oauth_err_desc` = the server's), `OA_BROWSER`, `OA_NOTOKEN`.

### The rules it keeps

* **https only**, except `127.0.0.1`/`localhost` (tests, local servers); checked on every request, there is no switch.
* **PKCE S256** always (verifier = 256 random bits, 43 characters); `state` and `nonce` random; `state` compared in constant time; a redirect with another `state` is answered 400 and the wait goes on.
* The loopback server listens on **127.0.0.1 only**, serves the redirect path only (`GET`), takes the first request with the right `state`, and **closes the socket** afterwards (`http.server.close_server`, new).
* `id_token`: signature (JWKS fetched on first use, once more -- at most once a minute -- for an unknown `kid`), `iss` = the discovered issuer, `aud` = the client id, `exp`, `nonce`; `alg` must fit the key's kind; a key from a JWKS is **never** an HMAC secret (RS256-to-HS256 confusion refused by type); `none` is not in any list.
* Device flow: `authorization_pending`, `slow_down` (+5 s, as the RFC says), `expired_token`, `access_denied`; the poll interval is obeyed (the test server counts polls and answers `slow_down` to an early one); cancel flag checked every 50 ms.
* A refresh answer without `refresh_token`/`scope`/`id_token` keeps the old ones (RFC 6749 6); a refresh with no refresh token is `OA_NOTOKEN` without a request.
* Client secret in the body (percent-encoded) or in a `Basic` header (form-encoded first, RFC 6749 2.3.1).

### Honest limits (O1-O7 in the file head)

* **O2 TLS 1.2.** `net.http` speaks TLS 1.3 only on `main`: `login.live.com` (TLS 1.2 only) answers `HttpError::Tls`, measured. `login.microsoftonline.com`, `user.auth.xboxlive.com`, `xsts.auth.xboxlive.com`, `api.minecraftservices.com` and `accounts.google.com` work. The TLS 1.2 client is being built by another worker (branch with `lib/tls/prf12.fi`, not on main when this was written); when it lands the old endpoints become reachable through `msa_set_endpoints` -- **that live path is not tested here**.
* **O3** one loopback login at a time per process; the calling thread is busy while waiting (run it on a worker thread; `oauth_set_cancel` takes the address of a flag).
* **O4** not here: implicit flow, password grant, JWT/mTLS client authentication, DPoP, PAR, introspection, dynamic registration. A desktop program cannot keep a client secret; use public clients.
* **O5** tokens are `str` on the GC heap and cannot be wiped; the keyring (`std.secret`) holds them at rest: Windows Credential Manager, else an encrypted file (machine-bound unless a password vault is used, `std.secret` S1).
* JWT: **no PS256, ES512, EdDSA, HS384/512, no JWE** (`jwt_check` answers `JWT_ALG`).

## auth.msa -- the Minecraft login chain

```
Microsoft account (device code or browser)        login.microsoftonline.com/consumers/oauth2/v2.0/...
  -> Xbox Live user token   POST user.auth.xboxlive.com/user/authenticate   {"Properties":{"AuthMethod":"RPS","SiteName":"user.auth.xboxlive.com","RpsTicket":"d=<access token>"},"RelyingParty":"http://auth.xboxlive.com","TokenType":"JWT"}
  -> XSTS token             POST xsts.auth.xboxlive.com/xsts/authorize      {"Properties":{"SandboxId":"RETAIL","UserTokens":["<xbl>"]},"RelyingParty":"rp://api.minecraftservices.com/","TokenType":"JWT"}
  -> Minecraft token        POST api.minecraftservices.com/authentication/login_with_xbox   {"identityToken":"XBL3.0 x=<uhs>;<xsts>"}
  -> ownership              GET  .../entitlements/mcstore    (product_minecraft / game_minecraft)
  -> profile                GET  .../minecraft/profile       (id = uuid, name)
```

`msa_device_start` + `msa_device_wait` (or `msa_login_browser`) run it all;
`msa_save/load/delete` keep the session in the keyring; `msa_ensure(&m, &s, 300)`
redoes only what has run out (chain with the Microsoft token if that is still
good, else refresh first). XSTS refusals come as `MSA_XERR` with the number in
`msa_xerr` and words in `msa_xerr_text` (no Xbox account 2148916233, child
account 2148916238, country 2148916235, adult verification 2148916236/7,
banned 2148916227).

* **The client id.** Microsoft has no anonymous client ids. A program registers an application in the Azure portal (personal accounts, public client, device code and `http://localhost` redirect on, permission `XboxLive.signin`) and has it approved for Minecraft services by Mojang (https://aka.ms/AppRegInfo). **This library contains no client id and no credential.**
* **What was tested.** `tools/oauth/check_msa.py` against `fake_msa.py`, which checks every header and every field of every body and the order of the requests (64 checks); and, live, **without credentials**: a made-up client id gets `unauthorized_client` from login.microsoftonline.com (HTTP 400), a bogus RPS ticket gets 401 from Xbox Live, a bogus user token 400 from XSTS, a bogus identity token 401 from api.minecraftservices.com -- the TLS 1.3 / HTTP / JSON path to the real hosts works. **A complete real login has not been run.**
* **M4 ownership** is read from the entitlements; a Game Pass account may show none, then the profile decides (`owns` and `has_profile` report both).
* The Minecraft token lives 24 h; XBL/XSTS tokens are not kept beyond the chain. Skins/capes are not parsed.

## appkit.fleitec_login -- "Sign in with Fleitec-ID"

The five answers of docs/FLEITEC-ID.md section 7: `fid_login` -> `FID_OK |
FID_BAD_CREDENTIALS (401) | FID_RATE_LIMITED (429, Retry-After) | FID_FAILED |
FID_UNREACHABLE`; `fid_me` -> `FID_SIGNED_IN | FID_SIGNED_OUT | FID_ERROR`;
`fid_logout`; `fid_save/load/delete` (keyring); `fid_cookie_header` for calls to
other `*.fleitec.com` services; `fid_token_expiry` reads `exp` (it does **not**
check the token -- that needs the server's secret; `/api/me` is the authority).
A missing name falls back to the typed one, trimmed. Default server
`https://jarvis.fleitec.com`; https required except loopback.

Fleitec-ID is not OAuth/OIDC (no authorization endpoint, no refresh token --
docs/FLEITEC-ID.md section 12). **Not run against the real server** (a password
cannot be tested without a real account); `tests/2153` runs it against a
stand-in that follows the spec. When Fleitec-ID gets OIDC discovery, `auth.oauth`
is the client and this file shrinks to a configuration.

## Tests

| test | what |
|---|---|
| `tests/2150_oauth_jose.fi` | 54 tokens signed by Python `cryptography` (RS256/384/512, ES256/384, HS256) + hostile variants (alg none, RS256-to-HS256, wrong kid/kind, tampering, time, iss/aud/nonce, shapes); a JWKS of eight entries of which three are usable |
| `tests/2151_oauth.fi` | PKCE vector of RFC 7636 appendix B, percent coding, form, endpoint rules, authorize URL, token JSON, pasted-redirect `state`, `OA_CONFIG` without a request |
| `tests/2152_msa.fi` | the three request bodies parsed back, XErr texts, UUID, session JSON, default endpoints, claims reader |
| `tests/2153_fleitec_login.fi` | login/me/logout against an in-process ID server: status mapping, JSON of the request, the cookie, escaping, fallbacks |
| `tools/oauth/run.sh` | `check.py` (97 checks, `fake_idp.py`: strict PKCE, redirect URI, single-use codes, rotating refresh tokens, polling interval, JWKS rotation) and `check_msa.py` (64 checks, `fake_msa.py` + live section L); three build stages; the Windows builds under Wine |
| `tools/oauth/gen_vectors.py` | writes `tests/data/oauth-vectors.json` (run by hand; the file is committed) |
