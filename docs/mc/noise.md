# mc_noise -- deterministic random numbers and gradient noise

`lib/mc/mc_noise.fi`. Version independent, bit-reproducible on any machine (integer generators
use the wrapping operators, noise only adds / multiplies / floors f64 in a fixed order).

```
SplitMix64   splitmix_new(seed) / splitmix_next
Xoshiro      xoshiro_new(seed) / xoshiro_next              xoshiro256**, seeded via SplitMix64
JavaRandom   jrandom_new(seed) / _next_int / _next_int_bound / _next_long / _next_double / _next_bool
             the 48-bit LCG that java.util.Random is specified to be
Perlin       perlin_reference() | perlin_seeded(seed)      permutation table
             perlin3 / perlin2 (Ken Perlin's improved noise), simplex2 / simplex3 (Gustavson),
             perlin_octaves3 / simplex_octaves2 (lacunarity, persistence, normalised)
heightmap_16x16(&perlin, cx, cz, out)                      example heightfield (256 i32)
```

Tests: `bash tools/mc/run.sh noise` -- 37 lines of splitmix / xoshiro / noise values bit-identical to
an independent python implementation; 5 seeds x (6 nextInt, 8 nextInt(bound), nextLong, nextDouble,
nextBoolean) identical to the REAL `java.util.Random`; published vectors (splitmix64 seed 0,
improved noise (3.14, 42, 7) = 0.13691995878400012); counter-check. Three build stages.
`tests/2700_mc_noise_basic.fi`: vectors, determinism, ranges over 20000 samples, heightmap bounds.

Measured (release-fast, this server): improved noise 6.2 M samples/s, simplex 2D 7.3 M/s, simplex 3D
6.1 M/s, Java LCG 73 M ints/s, xoshiro256** 88 M u64/s (`tools/mc/noise/dump.fi bench`).
Not Minecraft's own world generator: the algorithms are the public ones, no attempt is made to
reproduce vanilla terrain.
