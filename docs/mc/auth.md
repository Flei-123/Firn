# mc_auth -- identity of the login (`lib/mc/mc_auth.fi`)

Import: `import mc.mc_auth`.

## API
| Function | Purpose |
|---|---|
| `offline_uuid(name, n, out16) -> bool` | v3 UUID of MD5("OfflinePlayer:"+name); 1..114 octets |
| `uuid_to_text(u16, dashed, *Buf)` / `uuid_from_text(p, n, out16) -> bool` | 32 hex or 8-4-4-4-12, any case in |
| `server_hash(sid, sidn, secret, secn, pubkey, pkn, *Buf) -> bool` | SHA-1(serverId+secret+DER) as signed two's complement hex (Java `BigInteger.toString(16)`) |
| `digest_to_signed_hex(d20, *Buf)` | the conversion alone |
| `session_new / session_init / session_free` | client (uses `net.http`, TLS 1.2/1.3) |
| `session_set_base(url)` | default `https://sessionserver.mojang.com`; http only for 127.0.0.1/localhost |
| `session_add_trust_pem / session_set_system_trust / session_set_timeout` | test CA, timeout (default 10 s) |
| `session_join(c, accessToken, playerUuid, hash) -> i64` | POST `<base>/session/minecraft/join`, JSON `{accessToken, selectedProfile (32 hex), serverId}` |
| `session_status / session_error / session_error_message / session_net_error` | HTTP status, JSON `error` / `errorMessage`, net.http error number |

Result codes: `JOIN_OK` (204, 200 tolerated), `JOIN_NET`, `JOIN_FORBIDDEN` (403), `JOIN_UNAUTHORIZED` (401),
`JOIN_RATE` (429), `JOIN_HTTP` (other), `JOIN_ARG` (empty token, bad uuid or hash: nothing sent),
`JOIN_CONFIG` (base not https/loopback: nothing sent).

## Test vectors (verified with python hashlib)
`Notch` -> `4ed1f46bbe04bc756bcb17c0c7ce3e4632f06a48`, `simon` -> `88e16a1019277b15d58faf0541e11910eb756f6`
match the task text. **`jeb_` does NOT**: the task text had `-7c9d5b0044c130109a557e2b6f8e8e2b8e76b5b`, python
and Firn both give `-7c9d5b0044c130109a5d7b5fb5c317c02b4e28c1` (that is also the value in the public protocol
description). Offline UUID of `Notch` = `b50ad385-829d-3141-a216-7e7d7539ba7f` (confirmed).

## What is real, what is not
* Real: hash and UUID against python (400 random hashes incl. negative / short results, 200 names), JSON body
  and headers of the join request, status mapping, refused connection, no-send cases -- against a local python
  `http.server` mock on loopback.
* NOT done: the real sessionserver.mojang.com (needs a real account) -- **only tested against a mock**.
  https to a mock with a test CA is possible (`session_add_trust_pem`) but was not run; the TLS client is the
  one tested elsewhere (`tools/oauth`). Timeout behaviour is configurable but not asserted.

## Review of `lib/auth/msa.fi` (Device code -> XBL -> XSTS -> login_with_xbox -> entitlements -> profile)
Compared with the public flow description: endpoints (`login.microsoftonline.com/consumers/oauth2/v2.0/{devicecode,token}`,
`user.auth.xboxlive.com/user/authenticate`, `xsts.auth.xboxlive.com/xsts/authorize`,
`api.minecraftservices.com/{authentication/login_with_xbox, entitlements/mcstore, minecraft/profile}`), scope
`XboxLive.signin offline_access`, bodies (`RpsTicket "d="+token`, `RelyingParty http://auth.xboxlive.com`, XSTS
`rp://api.minecraftservices.com/`, `identityToken "XBL3.0 x=<uhs>;<xsts>"`), response fields (`Token`,
`DisplayClaims.xui[0].uhs`, `access_token`, `expires_in`, `items[].name`, `id`, `name`), error cases (XSTS 401 + `XErr`,
404 profile, 401/403, `authorization_pending/slow_down/expired_token/authorization_declined`). No defect found.
Minor gaps: no retry / `Retry-After` on 429; a 429 from login_with_xbox surfaces as `OA_HTTP`.
The existing stand-in (`tools/oauth/check_msa.py`, sections a-h, 60 checks) passes and is run by `tools/mc/auth/run.sh`.
**No real Microsoft account was used: the live chain is untested.**
