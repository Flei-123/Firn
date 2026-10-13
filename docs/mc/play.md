# mcplay -- playable prototype (STATE: groundwork only, demo NOT written yet)

Done in this branch (mc-r5, partial):
* `lib/mc/mc_blockset.fi` (`world_set_block`, `world_get_block`, `container_set`): single block writes into a decoded world
  (needed for local break prediction and Block Update packets). Test `tests/2840_mc_blockset.fi` (3 build stages green).
* Semantic packets added to `tools/mcgen/semantic.json` and regenerated (`gen_protocol.py --all`, only `mc_sem.fi` + `v*/mc_proto_*.fi` changed):
  `PLAY_C2S_BLOCK_DIG` (status, location.x/y/z, face, sequence), `BLOCK_PLACE` (hand, location, direction, cursorX/Y/Z, insideBlock,
  sequence), `USE_ITEM`, `HELD_ITEM_SLOT` (slotId), `ARM_ANIMATION`. Send with `mc_client.client_send_values` + `mc_encode.vals_*`.

What already exists and the demo must use (found while reading, nothing to rebuild):
* Window: `window.cursor_mode(CURSOR_CAPTURED)`, `cursor_delta`, `raw_input`, `fullscreen`, `gpu_vsync`; `input.raw` state machine (keys by Linux
  scancode, mouse buttons need `window.full_input(w,true)` BEFORE `make`; `glwindow.open` does not set it -> add an Opts field).
  `raw.raw_pump` drops EV_CONFIGURE / EV_CLOSED: write an own pump (raw_feed per event + resize/close handling + cursor_delta).
* GL path: `gl_mc` + `gl_scene` (sky, entities, HUD rectangles via `mc_hud.hud_record`) + `scene_present_window` (no readback) + `glwindow.swap`.
  CPU path: `mc_raster` -> `window.to_bgrx` -> `window.image` + `window.present`.
* Audio: `engine.engine_audio_start(e, sink, chunk)` (audio thread; needs the `__thread_work` dispatcher) with `FIRN_AUDIO=file:|null|device`;
  bank from `tools/sounds/gen tools/sounds/mc.json <dir>` (bank.json + wavs) loaded by `mc_audio.mca_bank_load_file`; `mca_on_packet`, `mca_walk`.
* Movement + collision + pick: `mc_input.player_tick` / `input_pick` (docs/mc/input.md). 1.20.1 Block Update bit layout: position u64 = x(26) z(26) y(12);
  Multi Block Change = u64 section (x22 z22 y20) + varint n + varlong (state<<12 | lx<<8 | lz<<4 | ly).

NOT done (roadmap): the demo itself (demos/mcplay), packet senders/receivers glue, audio wiring, tools/mc/play/run.sh (Xvfb + server + xdotool).
