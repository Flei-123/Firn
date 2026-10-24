# mc_audio -- Minecraft audio on audio.engine

`lib/mc/mc_audio.fi` (import `mc.mc_audio`). Glue between the game state (mc_camera, mc_world, mc_registry, mc_entity,
1.20.1 sound packets) and `audio.engine` / `audio.bank` (docs/AUDIO.md). New file only; engine and bank are unchanged.

```
var m = mc_audio.mca_new(&e)                         // buses: master/music/sfx/ui + mc.<category>
mc_audio.mca_bank_load_file(&m, &b, "sounds.json")   // Aulos JSON bank, event names = Minecraft sound names
mc_audio.mca_update(&m, &cam, dt)                    // per frame: listener = camera, engine_update, finished voices
mc_audio.mca_update_entities(&m, &store)             // entity bound sources follow
mc_audio.mca_play_at / mca_play_entity / mca_play_ui / mca_block_sound / mca_walk
mc_audio.mca_decode_sound_effect / _entity_sound / _stop_sound + mca_apply_sound / mca_apply_stop
```

| Part | What |
|---|---|
| Listener | camera position, forward, up (camera_update first) -> engine_set_listener; velocity from the movement |
| Origin | engine coordinates are f32: positions go out relative to an origin that follows the camera in steps of 256 blocks (rebase at 1024); live sources are re-sent. Same gain/pan 3 000 000 blocks out (test G) |
| Distance | Minecraft = OpenAL linear clamped, reference 1, range 16: `gain = clamp(volume,0,1) * clamp((range-d)/(range-1),0,1)`. Set on the event at its first spatial play (`spatial`, min 1, max 16, linear); `mca_set_range` widens one event. Sources beyond the range are not started (counted `n_culled`) |
| Pan | engine math: `dot(rel, right)/dist`, equal power `cos/sin((pan+1) pi/4)`; yaw 0 looks +z, right is -x |
| Blocks | `mca_material_of_name` (13 sound types by block name) -> `block.<type>.<step|break|place|hit|fall>`; volume/pitch step .15/1.0, break+place 1.0/.8, hit .25/.5, fall .5/.75 |
| Walker | `mca_walk`: a step per 1/0.6 blocks of horizontal travel on the ground (none when sneaking), fall sound on landing after > 3 blocks (vanilla: only when fall damage applies); `mca_walker_server()` turns the fall prediction off because a connected server sends the fall sound itself (mcplay), offline/demo keeps it via `mca_walker_new()` |
| Categories | wire numbers master 0, music 1, record 2, weather 3, blocks 4, hostile 5, neutral 6, players 7, ambient 8, voice 9, plus `CAT_UI` (10). Each is a bus (`mc.blocks` ... under sfx / music / ui, master = master): `mca_set_category_volume`, `mca_set_muted` act on running voices at once |
| Packets | 1.20.1 Sound Effect, Entity Sound Effect, Stop Sound (holder: id+1 or inline name + optional range). Bodies only: the packet ids come from the connection |

## Tests
`bash tools/mc/audio/run.sh` (gate 121 in test.sh):
- `tools/mc/audio/dump.fi` + `check.py`: attenuation (linear/inverse), Minecraft gain, pan and pan law, 3 seeds x 400 cases each, built release-fast / --no-opt / dev-fast (dumps byte-identical), compared with numpy float32 in the engine's order of operations: **bit identical** (cos/sin 800/800 exact, max 0 ulp).
- `tests/2766_mc_audio.fi`: 67 checks through the real engine with synthetic sounds (DC samples, `audio.synth` sine): block table, pan/gain in the rendered output, culling, range, category volume on a running voice, mute, ui, chunk + registry 1.20.1, walker, entity following (mc_entity, 1.20.1 packets), packets (truncations, trailing octet, bad category, 3000 random bodies), rebase.

## Honest limits
- M1 sound types come from the block NAME (the registry has none); wrong for odd blocks (rules in `mca_material_of_name`).
- M2 numeric sound ids: names come from `tools/mcgen/gen_sounds.py` (lib/mc/vNNN/mc_snd_vNNN.fi, names only). Verified against real vanilla servers 1.20.1 and 1.21.4 (wire holder = registry id + 1 = minecraft-data id; `sound_id_shift` -1 in versions.json from protocol 761). 1.12.2 / 1.16.5 ids (wire id = registry id, from 0) and 1.20.2/1.21.1/1.21.8/1.21.11/26.1 are generated the same way but NOT verified live. 1.8.9 has no table (named sounds only). Protocol 48..209 and 755..760 (1.9-1.9.4, 1.17-1.19.2 partly) are not decoded (`SF_NONE`/classic layout unchecked).
- M3 the range is per event; volume > 1 does not widen it per voice.
- M4 no occlusion, reverb, block-sound velocity; Doppler only if the bank asks.
- M5 `spatial` is an engine flag of the EVENT: an event played at a point is spatial for all later plays.
- M6 `mca_on_packet` is the client hook; the demos (mcclient/mcview) do not call it yet (no audio device in them).
- M7 node-minecraft-protocol mis-parses INLINE sound holders (its raw buffer is cut short), so the recorded fixtures only hold registered sounds (mob sounds), stop sounds and world events; inline decoding is covered by the synthetic packets of tests/2766.
- M8 Not done: `lib/mc/mc_assets.fi` (optional local asset import, `sounds=synth|original|both`), bank cross-check against the id table: DONE (z1, tools/mc/audio/coverage.py, docs/audio/SYNTH.md: tier 1 has 0 gaps), audio.hub + bank test. Sound packets for the sound source itself (entity position) need the entity in the store.

## Sound ids, packets, world events (mc-p5)
```
import mc.v763.mc_snd_v763
mc_audio.mca_sound_table_load(&m, mc_snd_v763.SND_BLOB.p as u64, mc_snd_v763.SND_COUNT, mc_snd_v763.SND_FIRST)
mc_audio.mca_on_packet(&m, &ver, id, &body, &entity_store)   // true = was a sound packet (played / stopped), cursor not moved
```
- Semantic packets (semantic.json, generated): `SOUND_EFFECT`, `NAMED_SOUND_EFFECT`, `ENTITY_SOUND_EFFECT`, `STOP_SOUND`, `WORLD_EVENT`.
- Layouts by protocol: `SF_LEGACY` (47, named only, u8 pitch), `SF_CLASSIC` (210..760, id or name + category + 1/8 position + f32 pitch, seed from 759), `SF_HOLDER` (761+, holder + seed).
- World events: 2001 = block break (state in data, flattened versions) -> `block.<type>.break`; doors / trapdoors / gates / dispenser / fire extinguish (1000-1014, 1036, 1037) -> their event names.
- `mca_set_block_change_sounds` + `mca_on_block_change` (off by default): solid->air = break, air->solid = place, call before the world applies the change. A vanilla server already sends the event / sound packet, so leave it off there.
- `mca_ambient_tick` (`Ambient`): cave mood (dark for ~60 s -> `ambient.cave` 4..16 blocks away). Approximation of the game's mood sound, deterministic.
- Tests: `bash tools/mc/audio/run.sh` also checks recorded REAL server packets (`tools/mc/audio/fixtures/sounds_*.jsonl`, made by `tools/mc/audio/sounds_live.sh`, needs a JVM + the vanilla jar as black box) against node-minecraft-protocol (`check_wire.py`), and `tests/2792_mc_audio_wire.fi` (tables of 5 versions, dispatch).

## Block-type fallback + pack sounds (s5, 13.10.)
* `mca_find(m, name)`: exact event first, else `block.<type>.<action>` -> `block.<material>.<action>` (`mca_type_material`, 13 materials). Used by `mca_play_at/_entity/_ui`, stop-sound and sound packets. Unknown non-block names stay silent (`n_missing`).
* `mc_packsnd.packsnd_apply` loads resource-pack sounds (docs/mc/resourcepacks.md). Coverage tool + gap list: docs/audio/SYNTH.md.
