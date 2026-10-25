# mcplay vanilla main menu

- Shown by `startscreen_pick` when a vanilla pack is found (`MC_VANILLA != 0`, `MCPLAY_MENU != 0`); otherwise the old start form.
- Assets are read at run time from the launcher jar (`MC_VANILLA_JAR`) and the asset index (`MC_VANILLA_ASSETS`, `MC_VANILLA_INDEX`; the panorama lives in the index). Nothing from Mojang is in the repo.
- Screens: title, multiplayer list (add / edit / delete / direct / join / refresh), singleplayer and realms notices, options skeleton.
- Server list rows show MOTD, online/max and latency from a status ping run on a 2-worker job pool (`lib/mc/mc_menu_ping.fi`, `mc_net.net_status_on`). Refresh and list edits re-ping once no ping is pending.
- Window pixels go through `mc_present.rgba_to_bgrx` (the raster is RGBA, X11/DIB want BGRX).

## Gate

`heavy bash tools/mc/menu/run_menu.sh` (needs the jar and assets, java, Xvfb, xdotool, Pillow)

- A: `menu_dump` renders the title at 854x480, 640x360, 1280x720; `layout_check.py` compares button borders and the language icon with `widgets.png` from the jar (counter probe: a 1 px shifted layout must fail).
- B: Xvfb window: title layout, Multiplayer, Add Server, typed address, ping text drawn, select, Join Server against the official 1.20.1 server; cfg keeps the entry.
- C: Quit Game exits 0 without connecting.
