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
| Walker | `mca_walk`: a step per 1/0.6 blocks of horizontal travel on the ground (none when sneaking), fall sound on landing after > 1 block |
| Categories | wire numbers master 0, music 1, record 2, weather 3, blocks 4, hostile 5, neutral 6, players 7, ambient 8, voice 9, plus `CAT_UI` (10). Each is a bus (`mc.blocks` ... under sfx / music / ui, master = master): `mca_set_category_volume`, `mca_set_muted` act on running voices at once |
| Packets | 1.20.1 Sound Effect, Entity Sound Effect, Stop Sound (holder: id+1 or inline name + optional range). Bodies only: the packet ids come from the connection |

## Tests
`bash tools/mc/audio/run.sh` (gate 121 in test.sh):
- `tools/mc/audio/dump.fi` + `check.py`: attenuation (linear/inverse), Minecraft gain, pan and pan law, 3 seeds x 400 cases each, built release-fast / --no-opt / dev-fast (dumps byte-identical), compared with numpy float32 in the engine's order of operations: **bit identical** (cos/sin 800/800 exact, max 0 ulp).
- `tests/2766_mc_audio.fi`: 67 checks through the real engine with synthetic sounds (DC samples, `audio.synth` sine): block table, pan/gain in the rendered output, culling, range, category volume on a running voice, mute, ui, chunk + registry 1.20.1, walker, entity following (mc_entity, 1.20.1 packets), packets (truncations, trailing octet, bad category, 3000 random bodies), rebase.

## Honest limits
- M1 sound types come from the block NAME (the registry has none); wrong for odd blocks (rules in `mca_material_of_name`).
- M2 numeric sound ids need the id -> name list of the version (`mca_sound_id_set`); not generated yet (minecraft-data has `sounds.json` only from 1.21 on in this checkout). Inline named sounds work.
- M3 the range is per event; volume > 1 does not widen it per voice.
- M4 no occlusion, reverb, block-sound velocity; Doppler only if the bank asks.
- M5 `spatial` is an engine flag of the EVENT: an event played at a point is spatial for all later plays.
- M6 the packet decoders are not wired into the client dispatch (`mc_sem` has no sound entries yet): the host feeds the body.
