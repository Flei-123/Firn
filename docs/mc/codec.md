# mc_codec -- frames, compression, encryption of one connection

`lib/mc/mc_codec.fi`. Version independent (the framing is the same since 1.8).

```
wire_open(conn) -> Wire                 wraps a connected net.Conn
wire_timeout(&w, ms)                    receive timeout; a timeout is NOT an error (see below)
wire_set_threshold(&w, t)               after Set Compression; -1 = off
wire_enable_encryption(&w, secret16)    AES-128-CFB8 both directions, key = IV = secret
wire_read_packet(&w, &body) -> id       body = VIEW on the packet, cursor behind the id
wire_send_packet(&w, id, &body)         one write; compresses above the threshold
wire_view_rest(&body) -> Bytes          the body behind the cursor (for echo / forward)
wire_stat_*                             octets in, octets after inflation, packets, compressed packets
wire_close(&w)
```

* **Buffered reader**: one linear receive buffer, refilled with large `recv` calls; a packet
  is consumed only when complete. With a receive timeout set, `WireError::Timeout` leaves the
  partial packet in place and the next call continues (so "idle" is detectable without losing data).
* **Zero copy**: an uncompressed packet is returned as a view into the receive buffer, a
  compressed one as a view into the inflate buffer. Valid until the next `wire_read_packet`.
* **Limits** (hostile peer): frame > 2 MiB -> `TooBig`; announced inflated size > 8 MiB -> `TooBig`;
  wrong inflated size / bad zlib / bad Adler-32 -> `Inflate`; length <= 0 or a VarInt that never
  ends -> `Malformed`.
* **Encryption** is switched on between packets (after sending the Encryption Response); octets
  already in the buffer are not touched. The CFB8 shift registers live for the whole connection.

Tests: `bash tools/mc/run.sh codec` -- an independent python peer (own framing, zlib, `cryptography`
CFB8) sends 60 packets (0 B .. 1.9 MB, random/zero/text data, random write splits) for
{no compression, threshold 256, threshold 1} x {plain, encrypted}; Firn echoes them back
byte-identically. Seven hostile-frame cases end in a clean error. Counter-check: a wrong key
is detected.
