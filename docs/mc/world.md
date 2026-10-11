# mc_world -- chunk store and coordinate math

`lib/mc/mc_world.fi`. `World` owns heap-allocated `mc_chunk.Chunk`s by (x, z).

```
world_new / world_free / world_put(&w, chunk_ptr)   put replaces a chunk with the same (x, z)
world_find(&w, x, z) -> ptr | 0      world_chunk_at(&w, i)      world_count(&w)
floor_div / floor_mod                 correct for negative coordinates
block_to_chunk / block_in_chunk / section_index(y, min_y)
```

Planned: change lists (single / multi block change), Anvil import/export (`mc_anvil`).
