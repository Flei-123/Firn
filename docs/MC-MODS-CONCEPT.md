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

## Boss question: can compiled Fabric/Forge mods be taken over? (honest answer: not directly)
A compiled mod is JVM bytecode that (1) calls `net.minecraft.*` classes (names remapped at load time: Fabric intermediary/Yarn, Forge SRG/Mojmap) and (2) usually patches Mojang classes with Mixins (Fabric) or event/ASM hooks (Forge). Both need the real Minecraft classes, which mcplay does not have: the engine is Firn.

| Way | How | Works for | Breaks on | Effort / limit |
|---|---|---|---|---|
| **a) embedded JVM only for mods** | load JDK in-process (JNI), run mod jars | pure-library mods, mods using only a tiny API slice | needs `net.minecraft.*`: either ship the vanilla classes (then Java Minecraft runs the game - defeats the engine) or write a Java shim of the Mojang API on top of Firn (thousands of classes); Mixins cannot target classes that do not exist; +100-300 MB RAM and JVM start time hurt the "ultra light" goal (measured mcplay 637 MB vs vanilla 1884 MB at r16) | XXL, 100+ rounds, never complete. Not recommended |
| **b) bytecode -> Firn/WASM translation** | AOT-translate .class files (like TeaVM/J2CL idea) | self-contained logic (math, data processing) | same Mojang API gap as (a) plus: a Java class library (collections, reflection, class loading, Kotlin/Scala runtimes), Mixins/ASM re-implemented per mod, multi-thread model | XL-XXL, 60-100 rounds for a whitelisted API subset; every mod still needs shim work. Not recommended as a general path |
| **c) bridge: real Fabric/Forge server + mcplay as client** | the mod runs in the real Java server (or Firn server + a side JVM later); mcplay speaks only the protocol | all SERVER-ONLY mods: Paper/Spigot plugins, server-only Fabric mods (Carpet, Lithium, FerriteCore, LuckPerms, WorldEdit, Spark, anti-cheat, Polymer-based content mods which fake custom blocks/items with vanilla assets + server resourcepack), datapack mods | mods that add blocks/items/entities/GUIs/renderers/keybinds (Create, Mekanism, AE2, Thermal, BOP, JEI...): the client needs their code and registry sync; Forge/NeoForge FML handshake rejects non-Forge clients; client-only mods (Sodium, Iris, minimaps) do not apply - mcplay is its own renderer | S: 0-5 rounds (protocol stays vanilla-compatible, launcher profile "Fabric server + Flei Engine client", tests against a Fabric server). A "client-code needed" mod stays unsupported until someone ports it to the Firn mod API (option a above) |

Conclusion: only **c** gives real compiled-mod support at low cost, and only for server-side mods. Everything that needs client code is either a port to the Firn WASM mod API (per mod, content as data + logic in WASM) or not supported. A Firn-specific content negotiation (server tells the client about custom blocks/items + a resourcepack, as Polymer does with vanilla clients) is the cheapest way to make "content" servers playable later: ~15-25 rounds on top of b.

## Recommendation
1. **b first** (on the critical path anyway: worldgen, recipes, loot, commands in the Firn server are datapack-driven; one loader serves vanilla data and user packs).
2. **c in parallel, cheap**: keep mcplay vanilla-protocol-correct and tolerant; document which modded servers work.
3. **a later**, after the server core (worldgen/entities/redstone) is stable: design the API on top of the data layer from b, so a mod is "data pack + wasm logic". Decide the wasm host (faster interpreter / AOT) before any API promise.

## Decision for the Boss
Start with b+c now and plan a for later (recommended), or start a now (large, delays server core), or only b.
