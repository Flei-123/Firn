# Horizon analysis (port template for the full game in Firn)

Source: /root/projects/_archive-clones/horizon @ 0ad879b (Rust rewrite of Java 26.3, protocol 777, DataVersion 5023, Java 25, MIT, "Copyright (c) 2026 Ludovic Marie and the Horizon contributors"). Facts come from reading its docs and tree; line counts are `.rs` lines measured with `wc -l`. Horizon itself says full vanilla parity is NOT reached.

## 1. Crates

| Crate | Lines | Role | Depends on |
|---|---|---|---|
| horizon-core | 19.9k | Java randoms, `jmath` (HotSpot-exact Math), positions, noise, density graph (6.5k), paletted storage, scheduled ticks, `version.rs` | libm |
| horizon-world | 168.8k | NBT, region/Anvil, chunk, `datafix/` 19.5k, `worldgen/` 53.9k, `level/` 23.3k (block behaviour, ticks, redstone), entity physics, items, effects, food, damage | core |
| horizon-protocol | 12.9k | codecs, framing, crypto, `packets/{handshake,status,login,configuration,play}` 4.7k, entity data | core, world |
| horizon-client-core | 7.5k | text, lang, SNBT, NBT path, scoreboard, bossbar, gamerules, block-entity NBT | world |
| horizon-items | 6.6k | block facts, items, item components, enchant, bundle | client-core, core, world |
| horizon-command | 10.6k | Brigadier-style parser: tree, argument types, selectors, suggestions, network tree | client-core, protocol, world |
| horizon-client | 357k | the game: `game.rs` loop + `game/` 143k, `mobs/` 68.6k, `lan/` 16k, dedicated server (`--dedicated`) | all above |
| horizon-client-render | 96.6k | chunks, meshing, light, streaming, wgpu, HUD, menus | no gameplay |
| horizon-prepare | 6k | builds `.local/horizon-client/26.3` from the player's own Minecraft install via Java exporters | - |
| others | small | assets 5.9k, server/server-files, friends/relay, mods, shaders, telemetry, launcher | - |

Layering: core -> world -> protocol/items/client-core -> command -> client. Gameplay lives in the client crate (flat ~100 modules plus `game/`), not in a server crate; `horizon-server` is only a supervisor/config wrapper around the headless game.

## 2. Tick loop and threads

- One `Game` struct holds client and server state. `fn tick()` (game.rs:5747) -> `fn tick_world(running)` (game.rs:6152).
- Single player: the world runs on its own "Server thread" (`game/integrated.rs`); the window joins it as a player **through memory** (`horizon_protocol::connection::Pipe`, mpsc channels), not TCP. A world tick never blocks a frame. `--in-frame` (tick in render thread) is deprecated.
- LAN guests and dedicated server use real TCP (`lan/conn.rs`, `lan/hub.rs`), same code path as the integrated server.
- `tick()` order: debug/tutorial -> remote client? (`tick_remote`, return) -> loading screen -> `ticks += 1` -> `/tick freeze` check (`clock.world_runs()`) -> game_time++, weather timers -> autosave when `ticks % 6000 == 0` -> clock -> `tick_world`.
- `tick_world` order: chunk tickets -> effects -> item use/cooldowns -> local player digging (packets arrive before the level ticks) -> postprocess chunks -> `tick_blocks` (scheduled block/fluid ticks, then random ticks; before any entity, as `ServerLevel.tick`) -> paintings, frames, armor stands, boats, carts, bolts, fishing, tridents, rockets -> items -> mobs -> experience -> LAN guests (`tick_lan`) -> sleep -> portals/end -> falling blocks, TNT -> block entities, sculk -> particles.
- Each stage is wrapped in `spans::time("name", ...)` for profiling.
- Scheduled ticks (`horizon_core::ticks`): per-chunk heap ordered by trigger time, priority (7 levels), sub-tick order; chunks compete by priority+sub-tick, never one global sort. First request per (type,pos) wins. Java array-heap tie behaviour and fastutil long-map iteration order are copied on purpose and diff-tested. Saved ticks store a signed 32-bit relative delay.

## 3. World generation

- Layout (`horizon-world/src/worldgen/`): `generator*.rs`, `router.rs` + `horizon-core/density/` (graph, cache, interpolation, spline, interval, beardifier, end_islands), `aquifer/` (751), `climate/` + `biome_source/` (multi-noise etc.), `material_surface/` + `material_condition/` (surface rules), `carver/` (cave/canyon, 328), `feature*.rs`, `placement/`, `tree/`, `state_provider/`, `decoration_order.rs`, `feature_graph.rs`, `structure/` (19.4k: jigsaw, mineshaft, stronghold, fortress, monument, mansion, end city, temples, ruined portal, templates, processors).
- Chunk status enum (`world/src/chunk_status.rs`, 26.3): empty, structure_starts, structure_references, biomes, terrain, features, initialize_light, light, spawn, full. (noise, surface and carvers run inside `terrain` in 26.3; vanilla 1.20.1 had separate noise/surface/carvers/liquid_carvers statuses.)
- Data source: noise parameters, density graphs, noise settings, biomes, configured/placed features, structures and templates are NOT hardcoded. `horizon-prepare` runs Java exporters (`Vanilla*Export*.java`) against the player's own jar into `.local/` (never committed; `tools/check_public_tree.py` blocks Mojang data). Rust decodes the JSON (serde `RawValue` to keep decimal precision and integer `-0`, Gson truncation rules) and executes the graph. Coded in Rust: algorithms (samplers, density node kinds, aquifer, surface rules, carvers, feature types, structure pieces).
- For us: same split. Data from jar (`data/minecraft/worldgen/**` JSON, datagen `--reports`), algorithms in Firn; json must come from the user's own jar or a non-committed cache.
- Exactness traps they document: bulk Perlin adds X/Z gradients before Y (scalar loop changes bits); Java wrapping coordinates; legacy Nether init consumes exactly 262 draws for skipped octaves; flattened stacks differ from nested ones; negative zero ordering in amplitude bounds.
- Density registry parity: 55 density definitions, 64 noise resources (26.3), 961,693 matching output lines per jar.

## 4. Random / noise port template

| File | Lines | Content |
|---|---|---|
| core/src/random.rs | 360 | `RandomSource` trait; `LegacyRandom` (Java LCG), `XoroshiroRandom`, `marsaglia_polar` |
| core/src/random/positional.rs | 79 | `PositionalRandom`, Legacy/Xoroshiro positional factories, `java_string_hash` |
| core/src/random/hash.rs | 157 | `seed_from_hash(name)` -> [u64;2] (Mojang seedFromHashOf; hashing of names for octave seeds) |
| core/src/random/worldgen.rs | 203 | `WorldgenRandom<R>`, `WorldgenPositional`, `slime_chunk_random` |
| core/src/jmath.rs | - | HotSpot-exact `Math` (libm 0.2.16 pinned) |
| core/src/noise/perlin.rs | 432 | ImprovedNoise + PerlinNoise 2D/3D, smeared Y, bulk volume |
| core/src/noise/simplex.rs | 121 | SimplexNoise (with/without offset draws) |
| core/src/noise/normal.rs | 387 | NormalNoise: two Perlin stacks, amplitudes, normalization modes, legacy Nether init |
| core/src/noise/stack.rs, volume.rs | 233, 94 | octave stacks, strided volumes (Z/X/Y flat order) |
| core/src/biome/seed_hash.rs | - | biome seed hash for zoom |

Our `lib/mc/mc_noise.fi` is generic and NOT vanilla exact; the files above are the exact targets.

## 5. Other systems

- **Entities/mobs**: `client/src/mobs/` (68.6k) one file per family (~100) on shared `living.rs`, `species.rs`, `data.rs` (attributes), `goals.rs` (goal selector), `brain.rs` + `*_brain.rs` (villager, warden, animal, monster), `nav.rs` + `path.rs` (pathfinding), `control.rs`, spawning (`spawn_rules`, `spawn_selection`, `natural_spawning`, `spawner`, `trial_spawner`), `finalize.rs`, `drops.rs`. Physics/fluids/collision in `horizon-world` (`entity_move`, `collision*`, `fluid*`). Harness `mob_ai` (probe `examples/mob_ai_probe.rs`; known failures listed by Horizon).
- **Redstone/blocks**: `world/src/level/{redstone.rs 1.4k, signals, pistons, update, tick.rs 1.8k, fluids, growth, rails, portal*, hooks, shapes, sculk}`. Neighbor-update order ported per block; harnesses `block_update`, `redstone`, `piston_movement`, `random_tick`.
- **Inventory/menus/crafting**: `horizon-items`, `world/src/{inventory_*,player_inventory,loot,item_*,food*}.rs`, `client/src/{crafting,menu/,recipe_book}.rs`, `game/lan_{crafting,stations,trading}.rs`.
- **Commands**: `horizon-command` = Brigadier clone (`packrat.rs` argument parser, `tree_*.rs` per-domain trees, `selector.rs`, `vanilla_order.rs` for wire order); execution `game/commands/*.rs`. Corpus harnesses `command_parse_corpus.py`, `command_exec_corpus.py`.
- **Light**: `horizon-client-render/src/light.rs` (+ `dynamic_light.rs`), harness `light`; not in the world crate.

## 6. Storage

- `world/src/nbt.rs`, `region.rs` (509), `compression.rs` (218): Anvil `.mca`, per-chunk compression gzip/zlib/none/lz4 (lz4-java block framing "LZ4Block", not LZ4 frame). `copy-region` keeps payloads and timestamps unless recompressed; external chunk files are refused. `chunk.rs` (920): sections, paletted storage, heightmaps, block entities, ticks.
- Entities in `entities/` regions, players in playerdata, `level.dat` gzip NBT; client `save.rs`, `game/saving.rs`, `persist.rs`.
- DataFixer: `world/src/datafix/` 19.5k lines, ports Mojang DFU schemas + fixes (old/mid/a/b/c, legacy flattening tables `flat_tables.rs`, `block_state_data.txt`) to DataVersion 5023. Harnesses `datafix`, `datafix_worlds.py`, `datapack_upgrade_*`.
- Autosave: every 6000 ticks and on exit (`save_game_with`).

## 7. Parity harness

- `tools/parity.py` runs `tools/test_*_parity.py` (~365 scripts) with ~313 Java oracles in `mods/horizon/src/test/java/dev/horizonlab/parity/Vanilla*{Oracle,Exporter}.java`; tracks file dependencies, reruns only affected ones; caches oracle output (`parity_shim.py` stands in for java/javac).
- Pattern: Python builds a deterministic corpus (fixed seed, edge values, doubles as hex of `struct.pack(">d")`), writes it as text lines; (a) a Java oracle compiled against the real vanilla jar and (b) a Rust probe (`horizon-core/src/bin/{parity,noise_parity,density_parity,tick_parity}.rs`: stdin line protocol, one answer line per query, exit 2 on error) answer; line diff, first mismatch printed.
- Firn plan `tools/mc/parity/`:
  1. `oracle/<Name>Oracle.java` compiled with `/root/tools/jdk25/bin/javac` against unobfuscated `server-26.x.jar` (inner jar + libs); for 1.20.1 use obfuscated jar + `server_mappings` or reflect via mapped names;
  2. `probe_<name>.fi`: reads the same stdin lines, prints the same format;
  3. `run.sh` (mktemp -d + trap, release and --no-opt stages, counter-check by perturbing one constant), corpus from a small fixed-seed Python script; jars only from `/root/.w5/jars`.
  Start with random/noise (cheap, exact, version-neutral), then density.

## 8. Port priority (Rust line counts; Firn expected ~1.0-1.5x)

| # | Item | Rust size | Notes |
|---|---|---|---|
| 1 | random + positional + hash + jmath | ~1.0k | base of everything; HotSpot-exact sin/cos needed |
| 2 | noise (perlin/simplex/normal/stack/volume) | ~1.6k | pure functions, easy differential |
| 3 | NBT/region/compression incl. lz4 block | ~1.5k | partly present (mc_nbt, mc_anvil, lib/compress/lz4): extend |
| 4 | density graph + router + aquifer | ~8k | terrain core; data from jar |
| 5 | climate/biome source + surface rules + carvers | ~6-8k | |
| 6 | chunk status pipeline + heightmaps + light | ~5-10k | |
| 7 | features/placement/trees | ~20k | long tail, test per feature type |
| 8 | scheduled ticks + block behaviour + fluids + redstone | ~25k | needed for a playable server |
| 9 | entity physics + items/components + inventory/crafting/loot | ~25k | |
| 10 | command tree + execution | ~10k + exec | |
| 11 | integrated server thread + in-process pipe (+ LAN TCP) | ~5k | we already have net |
| 12 | mobs/AI/brain/pathfinding | ~70k | by value: zombie, animals, villagers |
| 13 | structures (jigsaw, strongholds ...) | ~19k | |
| 14 | datafix | ~19k | skip when targeting one version |

Not worth porting: launcher, relay, friends, telemetry, mods, shaders, render crates (we have our own client).

## 9. Lessons

- Parity is unfinished; use Horizon oracles as ideas, not proof.
- 357k lines in one client crate is the cost of flat layout; keep our `lib/mc/<area>` split.
- Bit-level parity needs float discipline (f32 vs f64, no reordering, Java `Math` quirks); pin it in tests.
