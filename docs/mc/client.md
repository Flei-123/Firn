# mc_client / mc_dimension -- the client connection state machine

`lib/mc/mc_client.fi`: Handshake -> Login -> (Configuration) -> Play for ANY generated version. It
names no packet id and no protocol number: ids come from the Version object (`mc_sem.SEM_*`),
behaviour from its flags, the few values a reply needs are located with the schema walker.

```
client_new(conn, ver) -> Client         client_free
client_handshake(&c, host, port, next_state)
client_login_offline(&c, name, max_waits) -> CL_OK | CL_* (Set Compression, plugin requests,
                                          configuration phase when FLAG_CONFIG_PHASE; Encryption
                                          Request -> CL_ONLINE_MODE)
client_login_online(&c, name, &acct, max_waits)   same, but answers the Encryption Request (below)
account_new(token, &session) -> OnlineAccount     { uuid[16], token (str view), session (mc_auth client) }
client_error_text(code) / client_detail(&c) / client_kick_text(&c, &buf)   messages without secrets
client_next(&c) -> packet id | CL_TIMEOUT | CL_CLOSED ...   body via client_body(&c) (a view)
client_is(&c, id, SEM_*)                the id means that semantic packet
client_auto_reply(&c, id)               Keep Alive (echo), Synchronize Position (Teleport Confirm +
                                        acknowledge, fields found by name in the schema), Chunk Batch Finished
client_send_settings(&c, view)
```

* Every client written packet is held against the version's generated schema (`mc_schema.walk`)
  before it is sent: a layout that does not fit the version is refused, never put on the wire.
* **Measured quirk (vanilla 1.20.1 server):** Client Information sent right after Login Success --
  before the first Play packet is read -- makes the server drop the connection with a
  `DecoderException ... Index 8 out of bounds for length 3`. The same bytes sent after the first Play
  packet (Join Game) are accepted; nmp and real clients behave that way. The integration client sends
  it on Join Game.
* **Measured quirk (chunk data):** the server pads `chunkData` with zero octets after the last
  section (16 octets after the 24 sections of an overworld column). The section count must therefore
  come from the dimension (`mc_dimension`), not from "until the buffer ends".
* Not proven: the configuration phase against a real 1.20.2+ vanilla server (the node server of tools/mc/online runs it).

## Online login (`client_login_online`)
Login Start with the account's real UUID -> S2C Encryption Begin (serverId, public key DER, verify token
[, `shouldAuthenticate` from 1.20.5]) -> `mc_crypto`: 16 random octets shared secret, RSA-PKCS1 of secret and
verify token -> `mc_auth.server_hash` + `session_join` (BEFORE the answer; skipped when `shouldAuthenticate` is
false) -> Encryption Response (`client_send_values`, hand written fallback) -> `wire_enable_encryption` -> Set
Compression / Login Success / configuration phase as offline. The secret is wiped from the stack, the token is a
view the owner wipes; no error text contains a token.
Failing step -> code: `CL_BAD_ENCRYPTION_REQUEST`, `CL_RSA` (detail = mc_crypto code), `CL_SECRET`,
`CL_SESSION` (detail = `JOIN_*`; HTTP status / Mojang error in the session client), `CL_SEND`,
`CL_ENC_REJECTED` (after the answer only noise or a close came: the server rejected secret / verify token --
it answers in plain text, which the encrypting reader cannot parse), `CL_KICKED` (`client_kick_text`).
Demo: `MC_ONLINE=1 MC_ACCOUNT_FILE=<file> [MC_SESSION_BASE=<url>] mcclient ...` (account file lines
`mc_token=`, `uuid=<32 hex>`, `name=`; chmod 600, outside any repo; the name argument is ignored).
Tests: `bash tools/mc/online/run.sh` (node-minecraft-protocol online server + mock session server, 1.20.1 with
chunks, negative cases, login-only on 1.8.9 / 1.12.2 / 1.16.5 / 1.20.2 / 1.21.4); `MC_ONLINE_VANILLA=1` adds the
real Vanilla 1.20.1 server in online mode with its session host redirected to the mock
(`tools/mc/online/vanilla.sh`). A fake token sent to the REAL sessionserver.mojang.com is answered 403
(TLS path proven); a real account was NOT used. Live setup: `tools/mcclient/online_server.sh`.

`lib/mc/mc_dimension.fi`: `dim_from_join(ver, id, join_body, &dim)` reads `min_y` and `height` of the
dimension type named by `worldType` out of the Join Game `dimensionCodec` NBT (1.19 - 1.20.1 layout) with
the pull reader, no allocation; `dim_sections(&dim)` = height / 16. Verified on a real 1.20.1 server
(min_y -64, height 384).
