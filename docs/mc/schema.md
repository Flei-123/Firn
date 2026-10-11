# mc_schema -- generic packet walker

`lib/mc/mc_schema.fi`. Interprets the schema programs generated per version
(`lib/mc/vNNN/mc_proto_vNNN.fi`, bytecode documented at the top of `tools/mcgen/gen_protocol.py`).

```
walk(schema, &body, out) -> bool          consumed EXACTLY? (out != 0 gets `path=value` lines)
walk_prefix(schema, &body, out) -> bool   leftover octets allowed; cursor behind the packet
field_range(schema, name, &body, &off, &len)   where a top level field lies in the buffer
```

Hostile-input guarantees (bounds, counts against remaining octets, operation budget, NBT depth)
are listed in the file header. Measured: ~0.8-0.9 million packets/s on the reference fixtures
(`tools/mc/schema/run.sh`, release-fast, metrics printed per version).

Conformance: `bash tools/mc/run.sh schema` -- for every version in `tools/mcgen/versions.json`
the generator output equals the committed files and the reference serializer's example packets
(~4 per packet, 3056 in total for 5 versions) are consumed exactly; every truncation / extension
is rejected; mutations and random buffers never crash.
