# mc_chunk -- chunk columns as palette sections

`lib/mc/mc_chunk.fi`, GDD chapter 6. The decoded result is the **version neutral** world model;
the wire format is a decoding strategy chosen from `Version.chunk_format` (`chunk_decode_for`).

```
Chunk   x, z, nsec, secs (Section array), nbe (block entity count), nhm + hm_* (heightmaps),
        sky_n / block_n / light_hash (light arrays of the packet)
Section count (blockCount as sent), blocks (4096 entries), biomes (64 entries)
Container  palette (u16 global ids) + dense index array; 1 distinct value => palette only
```

* All three wire shapes (single value, indirect palette, direct/global ids) are normalised: the
  index array uses `ceil(log2(palette size))` bits per entry, entries may span two 64-bit words.
* `container_get(c, i)` -> global id. Block index `(y*16 + z)*16 + x`, biome index `(y*4 + z)*4 + x`.
* `chunk_decode(body, scratch, chunk)` decodes a 1.18+ `map_chunk` body (cursor behind the id);
  everything is bounds-checked (`BytesError`), the packet must be consumed exactly. Section count
  is "until the data is used up". Heightmaps: 9 bits per entry, 7 per long.
* `chunk_decode_for(version, ...)`: other `CF_*` formats return `Range` until a strategy exists.
* Version constants (largest indirect bit count of blocks / biomes) are at the top of the file.

Tests: `bash tools/mc/run.sh chunk` -- `demos/mcclient/selftest.fi` builds a chunk in all three wire
shapes (LCG content), decodes it, prints the canonical dump (format: `demos/mcclient/DESIGN.md`);
`tools/mc/chunk/expected.py` produces the same dump independently (own packing, FNV-1a, SHA-256);
byte-identical in three build stages, plus a counter-check.
