# Game-logic target version: 1.20.1 vs 26.3

Question: which version should the Firn integrated server / worldgen / game logic follow (the client already speaks many protocols). Measured on 30.10.2026 from jars in `/root/.w5/jars` (1.20.1, 26.1, 26.3 downloaded now via piston-meta) and datagen `--reports` output (kept in `/root/.w5/rep-<ver>/`, not committed). "Estimate" = my rough guess, not measured.

## 1. Facts from version.json (jars)

| | 1.20.1 | 26.1 | 26.3 (latest release, 2026-09-15) |
|---|---|---|---|
| protocol | 763 | 775 | 777 (26.2 = 776 per minecraft-data) |
| world_version (DataVersion) | 3465 | 4786 | 5023 |
| Java | 17 | 25 | 25 (`java-runtime-epsilon`) |
| jar | obfuscated (needs `server_mappings`) | unobfuscated (inner jar in META-INF/versions/) | unobfuscated |
| `.class` files | 5,460 | 7,351 | 7,762 |
| datapack format | 15 | data 101.1 | data 121.0 |
| `--reports` | no `packets.json`, no `data_component_type` | yes | yes |

Datagen command (works for all three, JDK 25 for 26.x): `java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports --output <dir>`.

## 2. Content size (from reports / jar data)

| | 1.20.1 | 26.1 | 26.3 |
|---|---|---|---|
| blocks / block states | 1,003 / 24,135 | 1,168 / 29,873 | 1,286 / 35,723 |
| items | 1,255 | 1,506 | 1,658 |
| entity types | 124 | 157 | 161 |
| data component types | - | 110 | 122 |
| biome files | 64 | 65 | 67 |
| structure files | 33 | 34 | 52 (18 new `abandoned_camp_<biome>` variants) |
| density_function files | 35 | 35 | 55 |
| noise definitions | 60 | 62 | 64 |
| placed features | 231 | 258 | 273 |
| configured features | 194 | 221 | 0 as separate dir (registry shape changed; `feature_type` registry 58) |
| mob effects / menus | 33 / 24 | 40 / 25 | 40 / 25 |

- New biomes vs 1.20.1: pale_garden (1.21.4), dappled_forest and sulfur_caves (26.x). Cherry grove already in 1.20.
- New structure vs 1.20.1: trial_chambers (1.21), abandoned camps (26.x).
- New entities vs 1.20.1 (names from the reports): armadillo, bogged, breeze, wind charge, creaking, happy_ghast, copper_golem, nautilus, zombie_nautilus, camel_husk, parched, mannequin, cushion, sulfur_cube; boats/rafts split per wood (incl. poplar), potion split into splash/lingering.
- New blocks vs 1.20.1: 285 (pale oak, resin, copper family variants, shelves, tuff/cinnabar/sulfur sets, concrete/wool stairs+slabs, leaf litter, bushes, firefly bush, golden dandelion, crafter, creaking heart ...). Removed: `chain` and `grass` were renamed (`iron_chain`, `short_grass`).
- Items 26.3 vs 26.1: +152 (e.g. cushions, concrete/wool stairs/slabs, cinnabar/sulfur sets, map items per structure).

## 3. Protocol (client)

- 1.20.1 -> 1.20.2 (764): new **Configuration** phase between login and play; chunk-batching packets; 1.20.5 (766): **item data components** replace item NBT on the wire, new codecs (registry/data-driven), cookie/transfer packets; 1.21.x further changes (see 774/775).
- Packet counts from minecraft-data (what our generator reads): 1.20 play 111 clientbound / 51 serverbound, no configuration. 26.1 play 141/69, configuration 20/10. From datagen `packets.json` (per phase total both directions): 26.1: play 210, configuration 30, login 11, status 4, handshake 1. 26.3: play 213, configuration 31, login 11, status 4, handshake 1 (+3 play, +1 configuration vs 26.1).
- Our client: `tools/mcgen/versions.json` already generates 1.8.9 ... 1.20.1 (763), 1.20.2 (764), 1.21.1, 1.21.4, 1.21.5, 1.21.8, 1.21.11 (774), 26.1 (775). 26.3 (777) is not in the minecraft-data on disk (its newest protocol.json is 26.1; protocolVersions.json lists 26.2 = 776 but 26.3 not checked). Getting 777: take `packets.json` from the 26.3 datagen run, extend `tools/mcgen/gen_protocol.py` to accept it (it is currently fed by minecraft-data), and run the existing live test against a 26.3 server. Do NOT guess 777's field layouts: only the packet ids/names are in `packets.json`; field layouts come from the decompiled codecs (jar is unobfuscated, readable with javap) or from a newer minecraft-data.
- The client is already multi-version by design (Version = data), so protocol is not what decides the game-logic target.

## 4. Save format

- Anvil `.mca` unchanged in structure. Compression ids: 1.20.1 knows gzip(1)/zlib(2)/none(3); **LZ4 (4)** since 1.20.5 (lz4-java block framing, Horizon `compression.rs`). Custom ids (127) supported by newer versions.
- Chunk NBT: DataVersion 3465 -> 5023; 1.18 flattened section layout (`sections[].block_states/biomes`, no `Level` wrapper) is the same in both; heightmaps, tick lists and `block_entities` keep their shape; later versions add/rename fields (handled by DataFixer, 19.5k Rust lines in Horizon).
- **Item components** (1.20.5+): `ItemStack` NBT `{id, Count, tag}` becomes `{id, count, components}`; block entity item fields, player data, villager trades, loot results, tooltips all go through components (122 types in 26.3). This hits inventory/crafting/loot code most: with 1.20.1 we store `tag`, with 26.3 a component map.
- `level.dat`, `playerdata`, `entities/` region files (since 1.17): same containers; 1.21.x moves some fields (`Data.WorldGenSettings` -> dimension/data). Exact 26.3 schema: read from jar via `DataFixers`; not verified field by field here.

## 5. Worldgen differences (checked in jar data)

- Noise router and 1.18 terrain algorithm exist in both. **JSON format changed**, not just values: noise defs `{amplitudes, firstOctave}` (1.20.1) -> `{base_octave, base_amplitude, ...}` (26.3); density node names and fields renamed (`argument1/argument2` -> `left/right`, `y_clamped_gradient` -> `gradient` with `axis`, `flat_cache`/`cache_2d` -> `cache`, `shift_a` has `noise` field). In `server-26.1` vs `server-26.3` 38 of 50 density/noise_settings files differ, and all 60 shared noise files differ vs 1.20.1. Whether the resulting numbers are equal was NOT verified; do not assume terrain is identical.
- Biome source (multi-noise climate) unchanged in kind (4 biome_source types in both).
- 26.3 adds 20 density functions, 18 camp structures, trial chambers, pale garden, sulfur caves, dappled forest; carvers and structure type registry stay at 16 types.
- Chunk statuses: 26.3 has 10 (empty ... terrain, features, initialize_light, light, spawn, full); 1.20.1 had noise/surface/carvers as separate stages.
- Terrain exactness per version needs its own oracle: same Java `Random`/Xoroshiro/noise primitives for both, so the first ports (random, noise, NBT, region) are shared.

## 6. Tooling

- 1.20.1: obfuscated jar; names via `server_mappings` (Mojang mappings) from the version json; oracles must run against the obfuscated jar with remapped names or reflect by mapped names. Java 17 is enough.
- 26.x: unobfuscated; oracles call classes by real names (e.g. `net.minecraft.world.level.levelgen.XoroshiroRandomSource`); needs `/root/tools/jdk25`.
- Template: Horizon is a Rust port of exactly 26.3 (code, data version, 313 oracles). Porting it to Firn is a translation job with known results; for 1.20.1 there is no equivalent source, we would port from vanilla bytecode/decompiled code ourselves and Horizon would only be a hint.

## 7. Recommendation

**Target 26.3 for the game logic; keep the client multi-version (1.20.1 stays joinable).**

Why (facts above): (1) the Rust template and its parity method match 26.3 exactly, so each Firn port can be diffed against Horizon logic AND the unobfuscated jar; (2) 1.20.1 needs an obfuscated jar + mappings and has no matching template; (3) 26.1 and 26.3 are close (+3 play packets, +4 entity types, +118 blocks); (4) every later version gap grows cost, 26.3 is latest release (2026-09-15); (5) version-neutral parts (random, noise, NBT/region/lz4) are shared first, so the decision can still be changed after step 2.
Costs to accept: more content (1,286 blocks, 161 entities, components everywhere), Java 25 only for tooling, and the live Mojang server for tests must be 26.3.

Fallback: if the Boss wants 1.20.1 strictly (mods, smaller scope), do the same shared steps first; only worldgen data decode and item/inventory layer differ.

Effort (rough ESTIMATE in person-days, one experienced person + agents, not measured):
| Step | 26.3 | 1.20.1 |
|---|---|---|
| Protocol 777 via packets.json + codecs | 3-6 | 0 (exists) |
| random/noise/NBT/region parity harness | 8-12 | 8-12 |
| Density/router/aquifer/biome/surface/terrain | 40-60 | 50-80 (no template) |
| Features + structures | 60-100 | 70-120 |
| Block behaviour/fluids/redstone/tick | 40-60 | 40-70 |
| Items/components/inventory/crafting/loot | 30-50 | 25-40 (NBT tags simpler) |
| Entities + mobs + AI (first useful subset) | 80-150 | 60-120 |
| Commands | 15-25 | 12-20 |
| Integrated server + LAN | 10-15 | 10-15 |
| Total playable survival | ~300-500 | ~300-500 (more work, less content) |
Both end similar because 1.20.1 saves content but loses the template. These numbers are guesses (Horizon needed several hundred thousand Rust lines).
