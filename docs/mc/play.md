# mcplay -- playable prototype (STATE: CPU window demo works; GL path (MCPLAY_GL=1), sound, vertex light + server time of day wired)

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

## Update 14.10. (mc-t2)
* Time of day from the update_time packet (MCPLAY_TICKS only until the first packet), vertex light (mc_wlight) + sky daylight uniform every frame, resource pack tick/hot reload (T) on the current back end.
* Sound: `FIRN_AUDIO=file:/x.wav|null|device`, bank from `tools/sounds/gen tools/sounds/mc.json <dir>` (MCPLAY_SOUNDS, default ~/.cache/mcplay/sounds; run.sh builds it when missing). Walk steps (mca_walk), break/place block sounds (place plays when the server's Block Update arrives), sound packets (mca_on_packet), volumes MCPLAY_VOL_<CAT>=0..100, M = mute. At exit: `audio bytes= crc= sounds= played=`. NOT heard on the server; the WAV is the proof. Device sink = audio thread (`__thread_work` in main.fi).
* GL path: `MCPLAY_GL=1` (+ MCPLAY_VSYNC=0|1): glwindow (EGL/GLX) + gl_mc back end + gl_scene (sky, world via wr_render_b, selection box = 12 thin boxes, HUD rectangles) + scene_present_window (no read back) + swap; resize = glbe_resize. Screenshot = one read back at exit. run.sh: GL stage under Xvfb + llvmpipe (software, NOT a real GPU): 147 frames at ~23 fps vs CPU 3-5 fps in the same test.
* glctx.fi: local `at` renamed (clashed with a generic `at` pulled in by audio.engine -> any program mixing audio.engine and gpu.glctx failed to compile).
* Not done: survival hardness table / inventory, Windows build check, GL translucent pass (wr_set_translucent) in mcplay, vsync value is the requested one (MCPLAY_VSYNC=0 prints vsync=1 on llvmpipe: swap_interval_get).

## Update 14.10. (mc-u3): GL transparency, inventory, break time
* GL path (`MCPLAY_GL=1`): `wr_set_translucent` (water / ice / glass second mesh, back to front, blended, `MCPLAY_TRANS=0` switches it off). Checked on llvmpipe with a glass wall + water: glass see-through, water translucent.
* `lib/mc/mc_dig.fi` + generated `lib/mc/v763/mc_dig_v763.fi` (`tools/mcgen/gen_dig.py <data> 1.20.1 763 <out>`): vanilla break time from hardness, tool speed, harvest tools, ground / water factor. Test `tests/2940_mc_dig.fi` (stone by hand 7.5 s, iron pickaxe 0.4 s, obsidian 9.4 / 8.35 s, bedrock). Survival: dig finishes after `ticks * 50 ms` (other protocols: `MCPLAY_DIG_MS`), instant blocks finish at once, bedrock is not started, a progress bar shows under the crosshair.
* `lib/mc/mc_inv.fi`: window 0 from Window Items / Set Slot (1.13.2 - 1.20.4 slot layout, NBT skipped), hotbar labels (3 letters + count), E = inventory panel, `MCPLAY_GIVE="stone:64,diamond_pickaxe:1"` fills the hotbar in creative (Set Creative Slot). Test `tests/2941_mc_inv.fi`. New semantic packets WINDOW_ITEMS, SET_SLOT, SET_CREATIVE_SLOT (semantic.json, all versions regenerated).

## mc-v3: sound proof, GL-vs-CPU picture, survival tables
* Sound is checked objectively (no ears): `MCPLAY_AUDIO_LOG=1` prints `audiolog frame=.. ev=.. cat=.. vol100=.. pitch100=..` per started voice (frame = sink position), `MCPLAY_AUDIO_PROBE=1` plays 12 known sounds (block step/break/place/hit/fall, glass, wood, grass, gravel, cave, zombie, UI), `MCPLAY_AMBIENT_DARK=1 MCPLAY_AMBIENT_X=n` drives the cave-mood clock. `tools/mc/play/audio_check.py <dump.wav> <out>` finds every event in the FIRN_AUDIO=file dump (onset latency, peak, spectral centroid, clipping, DC, silence share, engine stats: stolen/dropped/underruns must be 0). `--bursts N` checks a recording of the REAL device sink (private PulseAudio daemon + null sink, parec on the monitor; run.sh does this when pulseaudio+parec exist).
* Bank note: every sample of tools/sounds/gen is normalised to the same peak, so equal window peaks are not clipping.
* GL vs CPU: run.sh builds a pool + glass + tinted glass + wool backdrop next to the player (server `fill`), shoots both paths (`tools/mc/play/psnr.py`, threshold `MCPLAY_PSNR_MIN`, default 30 dB, F3 text masked). Both paths now draw the translucent pass (MCPLAY_TRANS=0 off). Measured on llvmpipe: 33.9 dB whole / 31 dB top / 46 dB bottom; the rest is 1-pixel edge rows of the rasteriser.
* Survival: `mc_dig_sel.dig_for_protocol` (754 / 758 / 762 / 763 / 764 tables, `tests/2951_mc_dig_versions.fi`). 1.21+ and 1.8-1.12 have no inventory decode (mc_inv): fixed MCPLAY_DIG_MS. gen_registry: 1.16.x data lists bedrock/barrier/command blocks with hardness 0, the generator now makes them unbreakable.
