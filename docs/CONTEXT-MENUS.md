# Context menus, once, for everything (fUi roadmap r162-r171)

Status 07.10.2026. Branch `fui-contextmenu`, merged to `main` when the acceptance run is green.
Justin's brief: right click / long press / menu key on the task manager, task bar, desktop,
explorer, editor, lists, title bars -- **built once in fUi and reused**, not once per program.

## 0. The decision in one page

| Question | Decision | Why (and what was rejected) |
|---|---|---|
| Where does the *meaning* of a menu entry live? | In an **action registry** (`lib/fui/action.fi`): id, label key, icon, shortcut, group, level, `enabled`/`checked`/`visible` functions, handler. | One declaration serves the context menu, a menu bar, a toolbar, the shortcut, the a11y default action **and** the OrientOS action bus. Rejected: handlers per menu (what OrientOS does today: ten right-click places, ten label tables). |
| How is a menu put together? | **Providers** along the chain *target node -> root* (`lib/fui/ctxmenu.fi`), merged by group / priority, duplicates removed, nearest provider wins. | The DOM/WPF/Flutter way of "who can say something about this element". Rejected: one big function per program that knows every element (does not compose, cannot be extended by a plugin). |
| What does the request look like? | **One event**, `EV_CONTEXT(target, x, y, source, mods)`, through the normal capture/target/bubble path; a handler may `H_PREVENT` it. Sources: mouse, touch, pen, menu key, Shift+F10, API, a11y, bus. | The DOM `contextmenu` event. Rejected: every host calling a different menu function. |
| What is the popup? | A **small scene per level** (`lib/fui/menuview.fi`), one node per entry, own painter, own a11y tree, painted **above** the page and clipped to the bounds it is given. | Reuses measuring, styling, theme tokens, scaling, hit test and a11y of the page; does not eat the page's 128 nodes. Rejected: a widget kind that paints rows by hand (`wave2.draw_menu_item` only draws one row; no keyboard, no hit test, no submenu). |
| Own window for the popup? | Interface and bounds are there (`mv_set_bounds`: the window now, the screen for a popup of its own); the OS-window backends (X11 override-redirect, Win32 `WS_POPUP`, OrientOS layer `L_POPUP`) are **roadmap** (r147/r168). See section 9. | An honest split: the overlay is complete and tested; a second OS window needs the window layer to deliver events for two windows. |

Everything below is checked by `tools/fui/ctxmenu_main.fi` (model, 200+ checks), `tools/fui/menuview_main.fi`
(picture, hit test, host path, speed) and `tools/fui/app_main.fi` (fui.app end to end), all in
`tools/fui/run.sh` section 18q / 18p.

## 1. The input (point 1 of the brief)

```
 right press ---------+
 long press (touch/pen)+--> fuiwirt.host_context(source, target, x, y, ay_alt)
 menu key             |        |  1. event.ev_context -> capture/target/bubble (H_PREVENT stops it)
 Shift+F10            |        |  2. the request hook (menuview.mv_request) -> ctxmenu.ctx_request
 app.open_context     +        v
```

* **Right button**: the menu opens on the *press*, as GTK and Qt do on X11. Windows waits for the release,
  but the Win32 window layer today delivers no release events at all, so a release rule would never open
  a menu there. While a menu is open a right press elsewhere closes it and opens the next in one go.
* **Long press** (finger or pen): `event.EV_LONG_PRESS` (500 ms, arena of event.fi); the mouse does not
  long-press. `app.fi` registers the listener when the first menu feature is used.
* **Menu key** and **Shift+F10**: `window.K_MENU` / `K_F10` (new, mapped for X11 keysyms and Win32
  `VK_APPS` / `VK_F10`), the menu opens at the **lower left corner of the focused node**, flips above its top edge.
* **The selection rule** is Windows': right click on an item that is *not* selected selects it first (and
  drops the rest); on a selected item the selection stays so "Delete" acts on all of them; on empty ground
  the selection is dropped (`ctxmenu.selection_policy`, applied through the program's `SelHooks`).
  fui.app's list rows have the hooks built in.

## 2. Actions (point 2)

```fi
let c = app.action_add(a, "copy", "&Copy", do_copy)       // id, label (catalog key or text), handler(ud=App, cx)
action.act_set_icon(reg, c, icon); action.act_set_shortcut(reg, c, 67, action.MOD_CTRL)
action.act_set_enabled(reg, c, has_selection)               // fn(ud, cx) -> bool, asked EVERY time
action.act_set_check(reg, c, is_on)                         // CHK_OFF / CHK_ON  (menuitemcheckbox)
action.act_danger(reg, d)                                   // last in the menu, error colour, "critical" on the bus
```

* `ActCtx` (target node, key, how many selected, source, modifiers, position, row key) is what every state
  function and handler gets: the same action behaves the same from a menu, a shortcut and the bus.
* **Labels** go through (1) a translator hook (`appkit.texts.tr` fits), (2) a built-in catalog, (3) the key
  itself; `&` marks the mnemonic letter, `&&` a literal `&`. The standard entries' English and German texts
  are `lib/fui/locale/ctx.en.opmsg` / `ctx.de.opmsg` (`key = text`), embedded with `__include_str`.
* **The bus**: `action.act_manifest(reg, "Title", buf, cap)` writes the `ACTIONS` file in the format of
  `docs/ACTION-BUS.md` section 3 (`manifest 1`, `app`, `title`, `action <app>.<verb> <read|write|critical> "..."`).
  `action.act_run_bus(reg, "notes.copy", cx)` is what the OrientOS broker calls after its rights check --
  so **every menu entry is also callable by Jarvis / MCP**. A dangerous action is `critical` (a human says yes).
  Ids are verbs (`edit-cut`, `file-delete`, `proc-end`) with a hyphen, so the bus name stays `<app>.<verb>`.
* **Shortcuts** are a *logical key* (upper-case ASCII or `SK_*`) plus `MOD_*` bits, no platform key codes.
  `act_dispatch_key` runs the action only when it is visible **and** enabled (a shortcut never does what a
  greyed-out menu entry would not). fui.app tries them after the focused text field passed on a key.

## 3. Putting a menu together (point 3)

```fi
fn row_menu(ud: u64, pc: *mut ctxmenu.ProviderCtx) {        // no closures in Firn: a named fn + ud
    let e = ctxmenu.pc_add(pc, A_OPEN)                       // an action of the registry
    let s = ctxmenu.pc_submenu(pc, "&Sort by", ICON_NONE, action.G_VIEW)
    ctxmenu.pc_add_to(pc, s, A_SORT_NAME)
    ctxmenu.pc_tag(pc, ".txt")                               // plugin entries match on tags
}
app.on_context(a, list_node, row_menu)                       // keyed to a node (survives rebuilds)
ctxmenu.ctx_provider_kind(cm, widget.KIND_TEXTBOX, f, ud)    // every text field
ctxmenu.ctx_provider_global(cm, f, ud)                       // every request in the window
```

The merge (all of it in `ctx_build`, checked in `ctxmenu_main` section 2):

1. **Chain**: from the target node up to the root, then the global providers, then plugin entries.
2. **Hidden actions** (`visible` false) are left out; **disabled** ones stay, greyed out (and focusable, as
   WAI-ARIA wants).
3. **Duplicates**: the same action from two providers appears **once**; the provider *nearer* the target
   keeps its place. Two submenus with the same label are **one** submenu. A submenu without entries is dropped.
4. **Order**: `(group, priority, order added)`; groups `PRIMARY, EDIT, SELECTION, VIEW, EXTENSION, DANGER`
   (delete / close last). A **separator** stands between two groups, never first, never last, never twice.
5. **Multi selection**: providers read `pc_selected` (0 / 1 / many) and `pc_item`; an action's `visible`/
   `enabled` get the same numbers in `ActCtx`.
6. **Plugins / manifests**: `ctx_extension_add(m, action, owner, tag, prio)`; shown when a provider declared the
   tag (or `"*"`). **Rights**: `ctx_set_grant(ud, f(owner, level))` -- an entry whose owner has no grant at
   that level is not shown and is counted (`ST_DENIED`); a *critical* plugin entry always sits in the danger
   group. An entry a provider already offers is never overridden by a plugin.

## 4. The widget (point 4)

* **Scene per level** (menu, submenu, sub-submenu): root frame (`TOK_SURFACE_RAISED`, border, radius) and one
  node per entry (key = slot + 1). Rows are the theme's row height (>= 32); a **touch** request makes them
  >= 44. Width = widest label + shortcut + arrow, clamped to the bounds; height is cut at the bounds
  (scrolling is roadmap).
* **Painting** (`paint_row`): highlight, gutter (icon, tick, radio dot), label with the **mnemonic underlined**,
  shortcut right-aligned muted, submenu triangle; disabled = disabled colour, dangerous = error colour,
  focused+disabled = quiet highlight. All colours are theme tokens: dark, light and every theme file work.
  **Icons sit on the row's midline** (rule r441): measured by pixel (ink rows centred on the row, +-1 px).
* **Placement** (`place_popup`, `place_submenu`, pure and tested): at the pointer; no room right -> opens
  **left** of it; no room below -> **above** (for a key request: above the *target's top edge*); neither fits
  -> the side with more room, clamped; a submenu opens right of its header overlapping 4 pt, flips left, shifts up.
* **Keyboard**: Up/Down (wrap, skip separators, disabled entries focusable), Home/End (and PgUp/PgDn), Right
  opens a submenu, Left/Esc close the deepest level, Enter/Space activate, Tab/Menu/F10 close; a letter that is a
  **mnemonic** activates (two with the same letter: cycle); other letters **type ahead** (1 s window, one letter
  cycles). The menu is **modal**: every key and character is consumed.
* **Mouse**: hover focuses; **hover delay** 250 ms opens a submenu, 300 ms closes one the pointer left (moving into
  the submenu in time cancels it); a press activates; only the main button activates (another one just points);
  the wheel is swallowed.
* **Close rules**: Esc, an **outside press** (closes *and passes the press through*), focus loss, window
  move/resize/other window (`ctx_window_changed`), a new request, activating an entry (the menu closes **first**,
  so the focus is back on the target when the handler runs), the **screen lock** (`ctx_set_locked` closes an open
  menu and refuses new ones -- OrientOS lockseal: nothing over the lock).
* **Accessibility**: roles `menu`, `menuitem`, **`menuitemcheckbox`**, **`menuitemradio`** (two roles added to
  `a11y.fi`; the role-name table is now 16 wide), `separator`; name, `checked`, `collapsed`, `disabled`, position
  `k of n` counted across the entry kinds. The **announcement** ("Context menu, 7 items", then "Copy, Ctrl+C,
  2 of 7", "Sort by, submenu", "By name, checked") is built in the model and exposed as
  `ctx_announce_seq/text`; the bridge to UIA / AT-SPI (r19/r103) polls it.
* **Speed** (release-fast, 1240x720): request + build + layout + paint of page and menu **2.5 ms**, a frame
  with an open menu **0.7 ms** (target <= 16 ms, both asserted in `menuview speed`).

## 5. The API for a program (point 5)

fui.app: `app.action_add`, `app.actions`, `app.on_context`, `app.on_context_global`, `app.std_edit` (every text
field/area of the window gets Undo/Redo | Cut/Copy/Paste/Delete | Select all, with their shortcuts),
`app.set_language("de")`, `app.open_context(node)`. See `examples/fui/context.fi` (a file list + a note field).

Standard entries, `lib/fui/ctxstd.fi` -- six families, each = labels (en/de), icons, shortcuts, groups + a provider:

| Family | Entries | Hang it on |
|---|---|---|
| `FAM_EDIT` | Undo, Redo, Cut, Copy, Paste, Delete, Select all | `ctx_provider_kind(cm, KIND_TEXTBOX, std_prov_edit)` |
| `FAM_LIST` | Select all, Invert selection | the list/table node (`ctx_provider_key`) |
| `FAM_FILE` | Open, Cut, Copy, Paste, Rename, Properties, Delete; tags `dir` / `.ext` for "Open with ..." plugins | a file element |
| `FAM_WINDOW` | Restore, Move, Size, Minimize, Maximize, Close | a title bar |
| `FAM_TASK` | Pin / Unpin (one of them hidden by `can`), Close window | a task-bar entry |
| `FAM_PROC` | Go to details, Open file location, End task, End process tree (critical) | a process-list row |

A program fills **one `StdOps`** per family: `act(ud, op, cx)` does the work, `can(ud, op, cx)` answers
`CAN_OFF | CAN_HIDE | CAN_ON` bits, `tag(ud, cx)` names the thing for plugins; then `std_register`.

## 6. Migration (point 6) -- what is done, what waits

Done in Firn (this branch): everything above, wired into fui.app (`app.fi`), Win32/X11 key mapping, the example.

OrientOS today (read from the code on 07.10.2026): programs use `wlib` (their own immediate-mode library),
not fUi scenes -- `taskbar.fi` `rechtsklick()` -> `wlib.menu_open(...)` (menu = a real wm window on layer
`L_POPUP`), `explorer.fi` has its own `offen` state machine (1 = context, 2 = file, 3 = view menu),
`widgetdemo.fi`, `desktop.fi`, `wm.fi` title bar. They can only move to this system **after r41 (wlib ->
fUi) and the vendor jump r61**; the order that makes sense:

1. Task manager process list -> `FAM_PROC` + `StdOps` (End task = critical).
2. Task bar entries -> `FAM_TASK`; the desktop -> a global provider (New, View, Refresh) + `FAM_FILE` on icons.
3. Explorer -> `FAM_FILE` + `FAM_LIST` + `ctx_extension_add` for "Open with"; editor -> `FAM_EDIT`.
4. The popup of the **OrientOS window backend** (`lib/window/osum.fi`) = a wm window on `L_POPUP` -- the
   popup-surface hook of section 9, which also removes "menus cut off by the window".
5. `act_manifest` -> the ACTIONS file of each app, `act_run_bus` -> the app's bus handler.

The OrientOS chat (explorer / task manager / list rows / command bar in fUi) was told the interface; to avoid
conflicts it should not edit `action.fi`, `ctxmenu.fi`, `menuview.fi`, `ctxstd.fi`, the `EV_CONTEXT` parts of
`event.fi` / `fuiwirt.fi`, or the two new roles in `a11y.fi`.

## 7. Tests (point 7)

| What | Where | Count |
|---|---|---|
| actions, shortcuts, mnemonics, manifest, bus run, catalogs | `ctxmenu_main` 1 | 38 |
| provider chain, merge, dedupe, submenu merge/prune, selection rule, plugins + rights | `ctxmenu_main` 2 | 50+ |
| keyboard, mnemonic, typeahead, hover delay, close rules, lock, disabled | `ctxmenu_main` 3 | 60+ |
| flip placement, clamp, submenu flip | `ctxmenu_main` 4 | 11 |
| announcements | `ctxmenu_main` 5 | 6 |
| standard entries (text field by widget kind, list, file, title bar states, task pin/unpin, process, en/de) | `ctxmenu_main` 6 | 40+ |
| pixels: frame, separator, highlight, disabled/danger colours, shortcut, **icon midline**, tick, dot, arrow; dark+light | `menuview_main` 1 | 2x38 |
| hit test, flip at the edges, touch rows, tiny window | `menuview_main` 3 | 16 |
| a11y tree of the menu and a submenu | `menuview_main` 4 | 12 |
| host path: right press, click runs, outside press passes, keys, mnemonic, Space, menu key, Shift+F10, prevented request, lock, delay | `menuview_main` 5 | 40 |
| speed (release-fast) | `menuview speed` | 3 |
| fui.app: right click on a row selects + opens, click runs, text-field menu, select-all by menu, long press, shortcut, manifest, audit | `app_main` | 28 |

## 8. Alternatives that were rejected

* **A menu widget kind in the scene tree** (children = entries): eats 20-25 of the page's 128 nodes and
  cannot extend outside the page's clip. A separate scene per level costs nothing on the page.
* **One immediate-mode `popup_menu(items[])` function** (like `kit.modal_*`): no merge, no providers, nothing for
  a plugin to extend.
* **Opening on release** (Windows): impossible on the Win32 layer as it is (no release events).
* **Labels in the registry in all languages**: the catalog is a separate file per language; the registry only
  holds the key.
* **Lambdas**: Firn has no closures; the price is `fn(ud, ...)` with a program pointer, as everywhere in fUi.

## 9. Not done yet (on the roadmap, not hidden)

* **The popup as a window of its own** (r168): the overlay is clipped to the window. A menu taller or wider
  than a small window is flipped/clamped; one that must reach outside needs X11 override-redirect / Win32
  `WS_POPUP` / OrientOS `L_POPUP`, and the loop must step two windows. The view takes its bounds from the caller.
* **Scrolling an over-tall menu** and **mirroring for right-to-left** (r169).
* **F-keys as shortcuts** on the host (F2 rename, Alt+F4): the host maps only F10 today; Delete / Enter /
  Backspace and Ctrl/Alt+letter work.
* **Web** (`contextmenu` event + `preventDefault` in `firn.js`) and **Android** long press via the router are
  wired in the model but not in the platform glue (r171).
* **UIA / AT-SPI bridge** (r19/r103): the data is there (a11y tree + announcements), the bridge is not.
* **A kernel-profile variant** (u64 text instead of `str`) for the OrientOS kernel UI.
