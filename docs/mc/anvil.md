# mc_anvil -- Anvil region files (.mca)

`lib/mc/mc_anvil.fi`: read and write the public region format (32 x 32 chunks, 4096 octet sectors,
location + timestamp tables, per chunk `length | compression | data`). Version independent; it does not
interpret the NBT payload (`std.nbt`, `mc_nbt`) and touches the file system only in `region_load` / `region_save`.

```
region_new() / region_free(&r)
region_from_bytes(ptr, n, &r) / region_load(path, &r) / region_save(&r, path)
region_check(&r) -> chunk count       header, lengths inside their sectors, NO shared sectors
region_has / region_count / region_sector_count / region_timestamp
region_read(&r, x, z, &out)           decompressed NBT bytes (gzip, zlib, none; 64 MiB bomb guard)
region_read_raw(&r, x, z, &ctype, &out)   the stored (compressed) payload
region_write(&r, x, z, ptr, n, ctype, stamp)  CT_ZLIB | CT_GZIP | CT_NONE; replaces in place when the new
                                      data fits the old sectors, else first free run or end of file
region_remove(&r, x, z)               frees the chunk's sectors
```

* Chunk coordinates are taken modulo 32 (negative coordinates work: slot = (x & 31) + (z & 31) * 32).
* A chunk may take at most 255 sectors (the count is one octet): larger -> `AnvilError::TooBig`.
* LZ4 (compression 4, newest releases) is recognised but reported as `Compression`.
* Hostile files: every location entry is validated before use; a damaged file gives an error.

Tests: `bash tools/mc/run.sh anvil` -- (A) Firn writes a region from an 80-step scenario (writes that grow and
shrink, removals, 3 compressions, chunk sizes at the sector boundaries, negative coordinates); an independent python
reader parses it STRICTLY and the chunk contents equal python's simulation; (B) python writes the end state with its own
allocator and Firn reads it identically; (C) 300 damaged copies (header-biased flips, cut-off files): 0 crashes, 0 hangs
(85 of them still passed the header validation, 85166 chunk reads were refused cleanly); counter-check included.
