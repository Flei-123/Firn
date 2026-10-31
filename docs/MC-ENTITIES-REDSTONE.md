# Entities + Redstone (server side): neutral core + version adapter

## Layers
- Neutral core (no protocol number, no packet field name): `lib/mc/server/ent/*`, `lib/mc/server/rs/*`.
- Adapter (data + names of one version): `lib/mc/server/v763/sv_ent_v763.fi`, `sv_rs_v763.fi`. A new version adds its own adapter module and registers it in the server version table; the core is untouched.
- Registry data (type ids, block states, properties) comes from `mc_registry` (generated from minecraft-data / jar datagen); the core only knows names.

## Entities
| Part | Module | Neutral / per version |
|---|---|---|
| Table (eid, uuid, kind, pos, vel, rot, item, track mask) | ent_table | neutral |
| Kind registry (item, player, ...) -> network type id | ent_kinds | name -> id via registry (data) |
| Motion (gravity, drag, block collision) | ent_move | neutral constants per kind |
| Item rules (pickup delay, pickup radius, despawn age) | ent_item | neutral |
| Tracking (range, spawn/destroy/move diffs per player) | ent_track | neutral |
| Wire ops (spawn, metadata, teleport, move, velocity, destroy, collect, player info) | v763/sv_ent_v763 | per version: field names, metadata layout, slot format |
| Tick phase `entities` | ent_phase | neutral |
| AI goals / attributes | later: goal registry keyed by kind | attribute ids per version (data) |

Stages: 1 table + tracking + players as entities + item drop/pickup (done here). 2 living entities: attributes, health, damage, knockback. 3 AI goal registry + pathfinding. 4 spawning rules (biome + light), persistence (entities in Anvil). Effort: S1 small, S2 medium, S3 large, S4 medium.

## Redstone
- State lives in block-state properties (`power`, `powered`, `lit`, `facing`, `delay`, `north..west`); engine reads/writes via `rs_state` (registry property indices, no state ids typed by hand).
- World access through `RsWorld` (get/set + notify callback): same engine runs on the server world and on a plain test grid.
- Scheduler `rs_sched`: ordered by (due tick, priority, sequence), like vanilla scheduled ticks.
- Components are a registry (`rs_comp`): one entry per block name with `neighbor_changed`, `tick`, `signal(dir)`, `direct_signal(dir)`; wire, lever, lamp, torch, repeater now; button/plate/comparator/piston/observer/door/dropper later as new entries.
- Per version: block names + property names bound in `v763/sv_rs_v763` (`rs_bind`); behaviour is shared (1.20.1 and 26.x differ only in wire update order, handled by a flag in the binding).

Stages: 1 lever, wire, lamp, torch, repeater (done here). 2 buttons, plates, comparator, observer, target, daylight. 3 piston + sticky (block events, moving blocks). 4 dispenser/dropper/hopper (need containers + entities). 5 wire update-order options (26.x experimental order).

## Verification
- Unit tests on a plain grid world (tests 6500+).
- Gate stage vs vanilla 1.20.1: two bots, same actions, compare block states and packets (`tools/mc/server/run.sh`, new stages K entities / L redstone).
