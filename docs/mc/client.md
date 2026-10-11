# mc_client / mc_dimension -- the client connection state machine

`lib/mc/mc_client.fi`: Handshake -> Login -> (Configuration) -> Play for ANY generated version. It
names no packet id and no protocol number: ids come from the Version object (`mc_sem.SEM_*`),
behaviour from its flags, the few values a reply needs are located with the schema walker.

```
client_new(conn, ver) -> Client         client_free
client_handshake(&c, host, port, next_state)
client_login_offline(&c, name, max_waits) -> CL_OK | CL_* (Set Compression, plugin requests,
                                          configuration phase when FLAG_CONFIG_PHASE; Encryption
                                          Request -> CL_ONLINE_MODE: online login is the integration step)
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
* Not proven: the configuration phase (1.20.2+) -- compiled and schema-checked only; online-mode login.

`lib/mc/mc_dimension.fi`: `dim_from_join(ver, id, join_body, &dim)` reads `min_y` and `height` of the
dimension type named by `worldType` out of the Join Game `dimensionCodec` NBT (1.19 - 1.20.1 layout) with
the pull reader, no allocation; `dim_sections(&dim)` = height / 16. Verified on a real 1.20.1 server
(min_y -64, height 384).
