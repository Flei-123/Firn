# fUi navigation toolkit (nav_route, nav_layout, nav_rail, nav_palette)

Four small modules in `lib/fui/`, one job each, no host, no drawing, no colours. A host (fUi window, OrientOS,
a browser page) draws; the models say what to draw and where. Tests: `tests/1933` .. `tests/1936`. Host preview
with pictures: `tools/fui/navmock_main.fi`.

## What was already there (nothing is built twice)

| need | existing | used here |
|---|---|---|
| overflow of a row of buttons | `cmdbar.fit_count` | `nav_rail.nrail_fit` / `nrail_hit` call it |
| action list, shortcuts, enabled/visible | `action.fi` (registry) | `nav_palette` lists it |
| fuzzy match + highlight | `text/fuzzy.fi` | `nav_palette` ranks with it |
| rail / tab drawing | `kit.nav_draw` (collapsed), `kit.tabbar_draw` | the preview draws with them |
| colours | `theme`, `style` tokens | the preview only asks tokens |

`palette.fi` is a colour table, not a command palette; `navkit.fi` is file manager parts; `splitter.fi` is a
resizable split, not a route model. So only the four models below were new.

## nav_route

Registry of routes `(id, scheme, pattern)`, pattern segments split by `/`, `:name` is a parameter (max 4).

* `nroute_add(rs, "chat", "fleitec", "chat/:id")`, `nroute_add(rs, "mail", "app", "mail/:acct/:uid")`
* `nroute_parse(rs, "app://mail/work/42?folder=sent#x")` -> `Match` (route, params as slices of the url, query);
  `nroute_get`, `nroute_query`, `nroute_decode` (percent decoding).
* `nroute_make(rs, "chat", "!room:srv", "", "", out, cap)` -> `fleitec://chat/%21room%3Asrv` (builder encodes);
  `nroute_match_new` + `nroute_set` + `nroute_build` for named values and a query.
* Back stack `Nav` (24 entries, URLs as text): `nnav_push` (validates against the registry, same top = no-op,
  oldest dropped when full), `nnav_pop` (never pops the root), `nnav_replace`, `nnav_top`, `nnav_at`.
* State: `nnav_save` writes `nav1\n<url>\n...`, `nnav_restore` reads it, drops lines with unknown routes. The text is
  what a host stores (config file, `localStorage`, Android saved state).

## nav_layout

Pure rectangles (`i32`, points; a host multiplies by its scale). `nlay_compute(w, h, &params) -> Layout`.

| width | mode | result |
|---|---|---|
| >= 1100 | WIDE | rail, list, content, optional detail column (content keeps >= `min_content`) |
| 700..1099 | MID | rail, list (max 36 % of the rest), content; detail opens as an overlay at the right |
| < 700 | STACK | no rail; one page full size (`page` = list / content / detail); tab bar at the bottom on the list page only |

Defaults (`nlay_params`): rail 64, list 320, detail 280, tab bar 56, min content 360. Never negative sizes,
columns never overlap (sweep test over widths 200..1800).

## nav_rail

`Rail` of up to 12 entries `(id, label, icon, badge, state)`, state ON / GREY / HIDDEN.

* `nrail_activate` refuses grey or hidden entries; an active entry turned grey moves on to the next usable one.
* `nrail_next(from, dir)` for the keyboard (skips grey and hidden, wraps).
* Badges: `nrail_set_badge`, `nrail_total_badge` (only ON entries), `nrail_badge_text` ("7", "99", "99+").
* Geometry: `nrail_fit` (how many entries fit, via `cmdbar.fit_count`), `nrail_item_y`, `nrail_hit`.
* Stack mode: `nrail_tabs(max, out)` gives the entries of the bottom tab bar; one slot stays for "More" when more
  entries exist than slots.

## nav_palette

Command palette model (Ctrl+K / Cmd+K): entries from an `action.Actions` registry (`npal_add_actions`, label,
shortcut, enabled, visible) and from routes (`npal_add_route(title, hint, url)`). Fuzzy filter with
`text.fuzzy.rank`, 64 results at most.

* Keys: `npal_key(key, mods)` -> `NK_HANDLED` / `NK_PICK` (Enter on an enabled entry) / `NK_CLOSED` / `NK_IGNORED`;
  text via `npal_type` (bytes >= 32) and `npal_backspace` (UTF-8 aware).
* Result: `npal_pick` -> `Pick {ok, kind, action, url}`. The host runs `action.act_run(reg, pick.action, cx)` or
  `nnav_push(nav, rs, pick.url)`. A disabled action is listed but cannot be picked.
* Drawing helpers: `npal_title`, `npal_hint`, `npal_shortcut_text`, `npal_positions` (byte offsets for highlighting).
* `npal_refresh_actions` re-asks enabled / visible when the state changed.

## Wiring the four together (FirnChat / suite)

1. App start: register the routes of every module (`chat`, `mail`, `cal`, ...), `nnav_restore` the saved state.
2. Rail click or Ctrl+K pick -> `nnav_push(url)`; Back key / swipe -> `nnav_pop`; a deep link from the OS ->
   `nroute_parse` + `nnav_push`.
3. Every frame: `nlay_compute(window w, h, params)` with `params.page` derived from the stack depth of the current
   section (0 list, 1 content, 2 detail); draw the rectangles.
4. On exit: `nnav_save`.

## OrientOS

Not built into Osum (docs only). OrientOS resolves `import fui.*` from its own `lib/fui/` first, then from the pinned
Firn (`vendor/firn/COMMIT`); `lib/FROM-FIRN.md` describes the mirror rule that `navkit.fi` already follows. Two ways:

* move the pin past the commit that holds these four files -> nothing to copy;
* or mirror the four files byte-identical into `osum/lib/fui/` and add them to the sync check next to `navkit`.

Candidates in OrientOS: launcher / settings routes (`osum://settings/<page>`), the rail for the system apps, Ctrl+K in
the shell on top of the `ACTIONS` manifest of each app (`action.act_manifest`).

## Honest limits

* No per-section back stacks yet (one stack for the whole app); a rail with one stack per entry is a small extension.
* Palette entries keep `str` views: titles of routes and actions must outlive the palette (literals or registry).
* Icons: lucide has no calendar symbol yet, the preview uses `CLOCK` for Calendar.
* The preview draws with the kit parts, not with the final FirnChat views; pictures are in `docs/fui/nav/`.
