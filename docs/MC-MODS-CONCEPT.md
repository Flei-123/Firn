# MC-MODS-CONCEPT -- how mods work with mcplay and the Firn server (proposal 31.10.2026, Boss decides)

## Constraint
Fabric/Forge/NeoForge mods are Java bytecode against the Mojang classes. Firn runs neither. They never run
directly. The question is what replaces them. Effort unit: one "round" = one 60-minute worker session.

## Options

| | What | Covers | Does not cover | Effort |
|---|---|---|---|---|
| **b) Datapacks + Resourcepacks, fully** | Server: load datapack JSON (recipes, tags, loot tables, advancements, worldgen, dimension/biome JSON, `.mcfunction` functions) with the same loader that feeds the vanilla data (version = data). Client: resourcepacks (models, blockstates, animations, sounds, lang, fonts, custom model data) | Most "content" servers use: custom recipes/loot/structures/biomes, custom models, texture overhauls, map-making. Versionless at JSON level, only pack_format differs | New code logic, new block/item behaviour classes | M: ~10-15 rounds (loader + function interpreter + worldgen JSON hooks; the resourcepack side is mostly built: mc_model, animations, mc_packsnd) |
| **c) Compatible with modded servers** | mcplay stays a protocol-correct vanilla-compatible client; plugin channels (`minecraft:brand`, custom payload) pass through; server-only mods and Paper/Spigot plugins just work | Paper/Spigot/Purpur plugins, server-only Fabric mods, datapack servers, resourcepack-forced servers | Servers whose mods add blocks/items/entities or require a client mod (Forge/NeoForge FML handshake, registry sync): no | S-M: ~5-8 rounds (custom payload pass-through, server resourcepack prompt, registry-sync tolerance, per-version tests); continuous cost = every new protocol version |
| **a) Firn mod API (WASM)** | Own mod system (GDD G4): mods are WASM modules (compile Firn or any language to wasm) with a stable versioned host API, permission model, hot reload; content (blocks/items/entities) declared as data + logic in WASM; loaded by BOTH the Firn server and mcplay -> "Firn-modded servers" | Real new content and logic, client-side render/UI mods, performance mods | Existing Java mods (need a port) | XL: ~40+ rounds (API design, content registries in server+client, sandbox/permissions, wasm host: kernel/app/wasm.fi is an interpreter ~11x slower than native, no JIT -> needs a faster host or AOT for hot paths, mod manager in the launcher) |

## Recommendation
1. **b first** (on the critical path anyway: worldgen, recipes, loot, commands in the Firn server are datapack-driven; one loader serves vanilla data and user packs).
2. **c in parallel, cheap**: keep mcplay vanilla-protocol-correct and tolerant; document which modded servers work.
3. **a later**, after the server core (worldgen/entities/redstone) is stable: design the API on top of the data layer from b, so a mod is "data pack + wasm logic". Decide the wasm host (faster interpreter / AOT) before any API promise.

## Decision for the Boss
Start with b+c now and plan a for later (recommended), or start a now (large, delays server core), or only b.
