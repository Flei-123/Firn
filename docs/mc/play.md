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

* `demos/mcplay/main.fi` (`mcplay <host> <port> <name>`, options by environment, header of the file): offline login, CPU rasterizer
  (`mc_render_world` + `mc_raster`), X11 window (`window.image` + `present`), own event pump (EV_CONFIGURE / EV_CLOSED / raw keys /
  `cursor_delta`), 20 Hz ticks (`mc_input.player_tick` + position/look packet), pick + selection box, HUD with measured FPS.
  Resize rebuilds the raster; F11 fullscreen; frame cap `MCPLAY_FPS` (sleep via wait_event; CPU path has no vsync); Esc releases the mouse.
* Input: W A S D / Space / Shift / Ctrl, 1-9 + wheel -> HELD_ITEM_SLOT, LMB -> ARM_ANIMATION + BLOCK_DIG (creative: start only + local
  break; survival: start, finish after `MCPLAY_DIG_MS`, cancel on release/target change), RMB -> BLOCK_PLACE (face from the pick normal,
  cursor = face middle; local prediction only with `MCPLAY_PLACE_ID`, otherwise the server's Block Update shows the block), F fly, N noclip.
* Server -> client: Block Update / Multi Block Change (`mc_play.decode_block_change`, `multi_begin/next`) -> `mc_blockset.world_set_block`
  + `wr_mark_block`; chunk load/unload mark/drop the mesh cache.
* `lib/mc/mc_play.fi` (window free, test `tests/4390_mc_play.fi`): key map, hotbar keys, face numbers, decoders, schema driven senders.
* `glwindow.Opts.full_input` added (calls `window.full_input` before `make`), both Linux and Windows twin.
* `tools/mc/play/run.sh`: Xvfb + official 1.20.1 server (creative, offline) + mcplay + xdotool, checks frames/chunks/ticks, server saw the join,
  no "moved wrongly", screenshot PNG + hash. Needs java + network (server jar); run via heavy.

Not done (honest): GL path (`MCPLAY_GL=1` only prints a notice; gl_scene/glwindow wiring is the next step), sound (r89: engine_audio_start,
mca_update/mca_walk/block sounds/mca_on_packet), inventory (what the held slot contains is unknown, so place uses the server's answer), time-of-day
packet, entities, survival dig timing (no hardness table), Windows build/Wine check, gamepad.
Gamepad PLAN: `lib/input/pad*.fi` -> left stick = WASD vector (needs analog move in `InputEvents`, today only K_* bits), right stick = `player_look`
dx/dy scaled per tick, A = jump, RT/LT = place/break, shoulder buttons = hotbar -/+, Start = Esc.

## Update 14.10. (mc-t2)
* Time of day from the update_time packet (MCPLAY_TICKS only until the first packet), vertex light (mc_wlight) + sky daylight uniform every frame, resource pack tick/hot reload (F5) on the current back end.
* Sound: `FIRN_AUDIO=file:/x.wav|null|device`, bank from `tools/sounds/gen tools/sounds/mc.json <dir>` (MCPLAY_SOUNDS, default ~/.cache/mcplay/sounds; run.sh builds it when missing). Walk steps (mca_walk), break/place block sounds (place plays when the server's Block Update arrives), sound packets (mca_on_packet), volumes MCPLAY_VOL_<CAT>=0..100, M = mute. At exit: `audio bytes= crc= sounds= played=`. NOT heard on the server; the WAV is the proof. Device sink = audio thread (`__thread_work` in main.fi).
* GL path: `MCPLAY_GL=1` (+ MCPLAY_VSYNC=0|1): glwindow (EGL/GLX) + gl_mc back end + gl_scene (sky, world via wr_render_b, selection box = 12 thin boxes, HUD rectangles) + scene_present_window (no read back) + swap; resize = glbe_resize. Screenshot = one read back at exit. run.sh: GL stage under Xvfb + llvmpipe (software, NOT a real GPU): 147 frames at ~23 fps vs CPU 3-5 fps in the same test.
* glctx.fi: local `at` renamed (clashed with a generic `at` pulled in by audio.engine -> any program mixing audio.engine and gpu.glctx failed to compile).
* Not done: survival hardness table / inventory, Windows build check, GL translucent pass (wr_set_translucent) in mcplay, vsync value is the requested one (MCPLAY_VSYNC=0 prints vsync=1 on llvmpipe: swap_interval_get).

## Update 14.10. (mc-u3): GL transparency, inventory, break time
* GL path (`MCPLAY_GL=1`): `wr_set_translucent` (water / ice / glass second mesh, back to front, blended, `MCPLAY_TRANS=0` switches it off). Checked on llvmpipe with a glass wall + water: glass see-through, water translucent.
* `lib/mc/mc_dig.fi` + generated `lib/mc/v763/mc_dig_v763.fi` (`tools/mcgen/gen_dig.py <data> 1.20.1 763 <out>`): vanilla break time from hardness, tool speed, harvest tools, ground / water factor. Test `tests/2940_mc_dig.fi` (stone by hand 7.5 s, iron pickaxe 0.4 s, obsidian 9.4 / 8.35 s, bedrock). Survival: dig finishes after `ticks * 50 ms` (other protocols: `MCPLAY_DIG_MS`), instant blocks finish at once, bedrock is not started, a progress bar shows under the crosshair.
* `lib/mc/mc_inv.fi`: window 0 from Window Items / Set Slot (1.13.2 - 1.20.4 slot layout, NBT skipped), hotbar labels (3 letters + count), E = inventory panel, `MCPLAY_GIVE="stone:64,diamond_pickaxe:1"` fills the hotbar in creative (Set Creative Slot). Test `tests/2941_mc_inv.fi`. New semantic packets WINDOW_ITEMS, SET_SLOT, SET_CREATIVE_SLOT (semantic.json, all versions regenerated).

## mc-v3: sound proof, GL-vs-CPU picture, survival tables
* Footsteps + place sound e2e (y4): steps follow the walked DISTANCE (1 per 1.67 blocks, also over hops/step-ups; teleports ignored), never frames: at 3-6 fps the tick loop drops ticks (`ticks_dropped=` in the `mcplay: walk` line) but the distance per tick stays. run.sh walks on a flat stone platform (server `fill`/`tp`) and checks `walked_mm`/`steps`; a place needs a target OUTSIDE the player's own hitbox (the server refuses it otherwise and answers with block updates showing air). Debug: `MCPLAY_DEBUG=1` prints `dbg place`/`dbg block_change` lines, `MCPLAY_ONLY_DAY=1 bash tools/mc/play/run.sh` stops after the day stage. Engine stats `peak_l/peak_r` = last mixed block only; `peak_hold_*` (printed as peak_l100/peak_r100) = loudest block since start. The bank cache dir in run.sh is keyed by the recipe hash.
* Sound is checked objectively (no ears): `MCPLAY_AUDIO_LOG=1` prints `audiolog frame=.. ev=.. cat=.. vol100=.. pitch100=..` per started voice (frame = sink position), `MCPLAY_AUDIO_PROBE=1` plays 12 known sounds (block step/break/place/hit/fall, glass, wood, grass, gravel, cave, zombie, UI), `MCPLAY_AMBIENT_DARK=1 MCPLAY_AMBIENT_X=n` drives the cave-mood clock. `tools/mc/play/audio_check.py <dump.wav> <out>` finds every event in the FIRN_AUDIO=file dump (onset latency, peak, spectral centroid, clipping, DC, silence share, engine stats: stolen/dropped/underruns must be 0). `--bursts N` checks a recording of the REAL device sink (private PulseAudio daemon + null sink, parec on the monitor; run.sh does this when pulseaudio+parec exist).
* Bank note: every sample of tools/sounds/gen is normalised to the same peak, so equal window peaks are not clipping.
* GL vs CPU: run.sh builds a pool + glass + tinted glass + wool backdrop next to the player (server `fill`), shoots both paths (`tools/mc/play/psnr.py`, threshold `MCPLAY_PSNR_MIN`, default 30 dB, F3 text masked). Both paths now draw the translucent pass (MCPLAY_TRANS=0 off). Measured on llvmpipe: 33.9 dB whole / 31 dB top / 46 dB bottom; the rest is 1-pixel edge rows of the rasteriser.
* Survival: `mc_dig_sel.dig_for_protocol` (754 / 758 / 762 / 763 / 764 tables, `tests/2951_mc_dig_versions.fi`). 1.21+ and 1.8-1.12 have no inventory decode (mc_inv): fixed MCPLAY_DIG_MS. gen_registry: 1.16.x data lists bedrock/barrier/command blocks with hardness 0, the generator now makes them unbreakable.

## mc-w4: 1.20.5+ inventory (data components) + break tables for 1.21.x
* Slot layout from 1.20.5 (protocol >= 766): `varint count` (0 = empty), `varint item`, `varint added`, `varint removed`, typed components (`type varint` + payload, some hold a nested Slot). Hand-writing a skipper for ~100 component types would rot per version; `mc_inv.inv_on_window_items_v / inv_on_set_slot_v` instead run the version's generated packet schema (`mc_schema.walk_values`, exact, bounded) and read `items[N].itemCount/itemId` (`item.itemCount/itemId` + `slot`) from the dump lines. <= 1.20.4 keeps the hand reader. A malformed packet leaves the inventory untouched and counts `bad`.
* Set Creative Slot for 1.20.5+: `i16 slot, varint count, varint item, varint 0, varint 0` (empty: `varint 0`).
* Break tables 767 / 769 / 770 / 772 / 774 / 775: from 1.21 the tool speed is the item's default `minecraft:tool` component + block tags, NOT minecraft-data's `materials` (those name tags and miss e.g. iron ore; copper tools carry a placeholder speed 1.0). `tools/mcgen/gen_dig.py <data> <version> <protocol> <out>` fetches the public misode/mcmeta data (`<v>-summary` item_components, `<v>-data` block tags; cache `$MCMETA_CACHE`, default /tmp/mcmeta-cache) and writes the same FDG1 blob (synthetic material = speed vector per block; harvest list = first matching `correct_for_drops` rule). Blocks that need a correct tool at all still come from minecraft-data `harvestTools`. Test `tests/2958_mc_dig_121.fi`.
* Test `tests/2959_mc_inv_121.fi` (5 versions, components incl. nested slot, NBT, removed component, foreign window, truncated packet). Real server: `tools/mc/play/run_inv.sh <mc_version>` (creative give + survival dig with the held pickaxe, needs `MCPLAY_CREATIVE=0` for survival).
* e1: every stack also carries a component signature (`inv_sig`, FNV over the indexed component lines of the schema dump, 0 = no components) and its `max_stack_size` component (`inv_max_stack`, type 1 in all 1.21.x registries). Click prediction merges only stacks with equal item + Efficiency + signature (vanilla isSameItemSameComponents) and caps at the component limit (else the registry default); a swapped stack takes signature + limit along. Test `tests/2997_mc_inv_stack_components.fi` (767/769/772/774/775). (Shift click / double click / drag: see f1.)
* Not modelled: Efficiency / Haste / Fatigue (enchantment components are skipped, not read), water/aqua affinity.
* f1: window 0 click prediction covers shift click (mode 1), double click (6, PICKUP_ALL) and drag (5, QUICKCRAFT) now; only the crafting result slot (shift click on slot 0) and other windows (chests: no Open Screen handling yet) are left to the server. The changed-slot list of the packet is the diff of all 46 slots before / after the click (like the vanilla client). Shift click = InventoryMenu.quickMoveStack: main <-> hotbar, craft grid / armour -> inventory, armour piece / elytra / shield / head / carved pumpkin -> its free slot (judged by the registry NAME, `equip_slot`; wrong only for modded or exotic items, the server then corrects), merge before fill, stack limit from the registry / `max_stack_size`. Double click collects equal stacks (not-full first, then full; left walks up, right down; result slot skipped; needs a hand stack and an EMPTY clicked slot). Drag: state in `Inv` (qc_status/type/mask), distribution at the end packet (left = split evenly, right = one each, clone only after `inv_set_creative`); API `inv_drag_begin/add/end`. mcplay: the inventory panel sends a double click (second left press on the same slot within 250 ms, hand not empty); `MCPLAY_CLICK_TEST2=1` is the live script (tools/mc/play/run_inv.sh stage "shift + drag + double click"). Legacy (<= 1.16.5) packets report the clicked stack like vanilla: shift click = the moved copy only while the slot still holds the item (else empty), double click / drag = empty. Tests 3031 (shift), 3032 (double click + legacy tail), 3033 (drag), 3034 (components), 2973 (shift click now predicted).

## mc-f4b: container windows (chest / hopper / dispenser)
* `lib/mc/mc_inv.fi`: `inv_on_open_window_v` (1.14+: varint id + menu type 0-5 = generic_9x1..9x6, 6 = generic_3x3; 1.13.2: u8 id + type string + slot count) and `inv_on_close_window_v` track the open window (`inv_window`, `inv_window_slots`; -1 slots = a type we do not model: items ignored, no clicks sent). Container slot k lives at internal slot 46 + k (`INV_CAP` 100); the window's 36 player slots map to internal 9..44 (`inv_win_to_int` / `inv_int_to_win`).
* Window Items / Set Slot of that window (hand reader <= 1.20.4 and the schema path 1.20.5+) are translated; Click Container carries the window id and window slot numbers, changed slots in window order. Shift click = ChestMenu.quickMoveStack (container -> player reverse over main + hotbar, player -> container forward); pick up / drag (masks cover 128 slots) / double click use the window slots only (no crafting / armour / off hand slots). Close: `inv_close_window(cl, inv)` (c2s id u8, varint from protocol 768).
* Test `tests/3140_mc_inv_container.fi` (1.14.4, 1.16.5, 1.20.1, 1.21.4, 26.1: open, items, set slot, shift both ways, pick up, drag incl. slots >= 64, close, unsupported type; click packets parse with the version schema). Real server: stage "chest" in `tools/mc/play/run_inv.sh` (`MCPLAY_CHEST_TEST=1`: /setblock chest next to the player, right click, clicks, close; chest NBT + hotbar on the server must equal the prediction).
* Not done: chest GUI in mcplay (packets only), other menu types (furnace, crafting table, anvil, villager, ...), menu type ids beyond 1.21.4 not verified, 1.13.2 open path only by reading (no 1.13.2 registry in versions).

## mc-s7: container GUI details
* Title: `inv_on_open_window_v` keeps the plain title text (NBT string / `text` / `translate` from 1.20.3, JSON before; `container.chest` -> "Chest", `container.hopper` -> "Item Hopper", `container.dispenser`, `container.dropper`, `container.shulkerBox`, `container.barrel`, `container.enderchest`, `container.chestDouble`); `draw_container` shows it (fallback "Container").
* Layouts: menu types hopper (15, 16 from 1.20.3) = 5 slots in one row, generic_3x3 = 3x3, shulker_box (19 / 20) = 9x3; `inv_window_cols`. Narrow containers are centred above the 9 wide inventory.
* Sneak: mcplay tells the server when the sneak key changes (`mc_play.send_sneak`: Entity Action 0 / 1 up to 1.21.1, Player Input shift flag from 1.21.2). The SERVER decides from that state whether a right click on a chest opens it or places the held block. Gate `tools/mc/play/run_cont.sh [version]` (stage `contplay`): 1.20.1 proves sneak + click places stone above the chest, plain click opens it; 1.21.4 (Player Input) is sent but the server still opens the chest (open, roadmap).
* Env: `MCPLAY_SNEAK_TEST=1` (script of run_cont.sh), `MCPLAY_FORCE_SNEAK=1` (sneak key held).
