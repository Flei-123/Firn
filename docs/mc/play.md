# mcplay -- playable prototype (STATE: CPU window demo works; GL path + sound NOT wired)

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


## mc-s2 (13.10.): demos/mcplay + lib/mc/mc_play.fi

* `demos/mcplay/main.fi` (`mcplay <host-ipv4> <port> <name>`, options by environment, header of the file): offline login, CPU rasterizer
  (`mc_render_world` + `mc_raster`), X11 window (`window.image` + `present`), own event pump (EV_CONFIGURE / EV_CLOSED / raw keys /
  `cursor_delta`), 20 Hz ticks (`mc_input.player_tick` + position/look packet), pick + selection box, HUD with measured FPS.
  Resize rebuilds the raster; F11 fullscreen; frame cap `MCPLAY_FPS` (sleep via wait_event; CPU path has no vsync); Esc releases the mouse.
* Input: W A S D / Space / Shift / Ctrl, 1-9 + wheel -> HELD_ITEM_SLOT, LMB -> ARM_ANIMATION + BLOCK_DIG (creative: start only + local
  break; survival: start, finish after `MCPLAY_DIG_MS`, cancel on release/target change), RMB -> BLOCK_PLACE (face from the pick normal,
  cursor = face middle; local prediction only with `MCPLAY_PLACE_ID`, otherwise the server's Block Update shows the block), F fly, N noclip.
* Server -> client: Block Update / Multi Block Change (`mc_play.decode_block_change`, `multi_begin/next`) -> `mc_blockset.world_set_block`
  + `wr_mark_block`; chunk load/unload mark/drop the mesh cache.
* `lib/mc/mc_play.fi` (window free, test `tests/2890_mc_play.fi`): key map, hotbar keys, face numbers, decoders, schema driven senders.
* `glwindow.Opts.full_input` added (calls `window.full_input` before `make`), both Linux and Windows twin.
* `tools/mc/play/run.sh`: Xvfb + official 1.20.1 server (creative, offline) + mcplay + xdotool, checks frames/chunks/ticks, server saw the join,
  no "moved wrongly", screenshot PNG + hash. Needs java + network (server jar); run via heavy.

Not done (honest): GL path (`MCPLAY_GL=1` only prints a notice; gl_scene/glwindow wiring is the next step), sound (r89: engine_audio_start,
mca_update/mca_walk/block sounds/mca_on_packet), inventory (what the held slot contains is unknown, so place uses the server's answer), time-of-day
packet, entities, survival dig timing (no hardness table), Windows build/Wine check, gamepad.
Gamepad PLAN: `lib/input/pad*.fi` -> left stick = WASD vector (needs analog move in `InputEvents`, today only K_* bits), right stick = `player_look`
dx/dy scaled per tick, A = jump, RT/LT = place/break, shoulder buttons = hotbar -/+, Start = Esc.
