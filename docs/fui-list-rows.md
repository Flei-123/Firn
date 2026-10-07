# fUi list rows with symbols (lib/fui/listrow.fi, roadmap r129)

For OrientOS (Explorer, Settings) and any other fUi program: **a list row =
symbol on the left, text, optional detail text and optional badge on the
right**, as a small widget API. This page is the contract; OrientOS can write
down here what it needs beyond it (see "Open").

```
[icon] Documents                      12 items  [3]
[icon] notes.txt                      2 KB
[icon] Locked   (disabled)
```

## Two layers

| layer | file | for |
|---|---|---|
| scene tree | `lib/fui/listrow.fi` | any program that builds a `scene.Scene` (OrientOS `fuiscene`, Certus, CodeHub) |
| `fui.app` | `lib/fui/app.fi` (`app.list`, `app.list_item`, `app.chosen`) | tkinter-sized programs, example `examples/fui/files.fi` |

The rows are ordinary nodes: they measure, align, scale (dpi) and are styled
by a `sheet.Sheet` like everything else. Nothing is painted by hand except
the symbol (a Lucide icon in the text colour).

## Scene-tree API (`import fui.listrow`)

```firn
var sh: sheet.Sheet = sheet.sheet_new()
listrow.listrow_rules(&sh)                       // once; later rules of the app win

let list: usize = listrow.listrow_list(&sc, parent)   // a column; parent = scene.SCENE_NONE for root
var r: listrow.Row = listrow.row_new(1, "Documents")  // key >= 1, text
r.icon     = listrow.icon_named("folder")        // a Lucide name; ICON_NONE = no symbol
r.detail   = "12 items"                          // "" = none
r.badge    = "3"                                 // "" = none
r.selected = false                               // start chosen
r.disabled = false
let node: usize = listrow.listrow_add(&sc, list, &r)   // SCENE_NONE if the tree is full or key == 0

listrow.listrow_select(&sc, list, key)           // choose one row (0 = none); true if it changed
listrow.listrow_selected(&sc, list)              // key of the chosen row, 0 = none
listrow.listrow_hover(&sc, list, key)            // mark the row under the pointer (0 = none)
listrow.listrow_hit(&sc, px, py)                 // key of the row at a point; 0 = none / disabled
listrow.listrow_row_of(&sc, list, key)           // its node
listrow.listrow_key_of(&sc, list, node)          // key of the row that holds `node`
listrow.listrow_disable(&sc, list, key, true)
```

* **Key.** A number >= 1 the caller picks (file number, table index + 1). It
  is the node id of the row, so `event.fi` handlers (`on`), the a11y tree and
  `scene_find_id` find the row by it. Unique within a list.
* **Texts are not copied** (a node holds the pointer, like `node_set_text`):
  a literal, a `static`, or a buffer that lives as long as the tree.
  `app.list_item` copies them for you.
* **Selection** is the class `row-sel`, **hover** is the node state
  `STATE_HOVER`, **disabled** is the class `row-off` plus `STATE_DISABLED`.
  The program (not the library) decides when: call `listrow_hover` from its
  pointer-move and `listrow_select` from its click, then repaint when they
  answer true.

## The look (theme tokens only, no colour of its own)

| part | class | look |
|---|---|---|
| list | `row-list` | bare column, rows 2 points apart, no surface of its own |
| row | `row` | transparent, radius 6, text `TOK_TEXT`, padding 8/5/10/5, gap 10 |
| hovered | `row` + `STATE_HOVER` | `TOK_BUTTON_HOVER` |
| chosen | `row-sel` | `TOK_SELECTION` / `TOK_SELECTION_TEXT` (wins over hover) |
| disabled | `row-off` | `TOK_TEXT_DISABLED`, never hit |
| symbol | `row-icon` | 16 x 16 points, text colour of the row |
| text | `row-text` | grows, shrinks first (min 24 points), cut with an ellipsis |
| detail | `row-detail` | `TOK_TEXT_MUTED`, 13 points; selection text in a chosen row |
| badge | `row-badge` | pill, `TOK_ACCENT` / `TOK_TEXT_ON_ACCENT`, 12 points; colours swapped in a chosen row |

Fill rule (Justin, 02.10.): a row with a surface never has the ground's colour
-- checked in `tools/fui/listrow_main.fi` (hover and selection differ from the
ground in both themes). The class names are in `listrow.listrow_names()`; an
app restyles with its own rules written after `listrow_rules`.

## fui.app

```firn
app.list(a)                                            // rows below one another until app.end(a)
app.list_item(a, "folder", "Documents", "12 items", "3", open_docs)
app.list_item(a, "pencil", "notes.txt", "2 KB", "", open_notes)
app.end(a)
// in a handler: app.chosen(a, list) = the chosen row's node
```

A click chooses the row (one at a time) and runs its handler; the row under
the mouse is highlighted. Rows are named for the accessibility audit
(`ROLE_LISTITEM`, name = text). About 24 rows fit the 128 nodes of a window
(SCENE_MAX, r102).

## Checked

`tools/fui/listrow_main.fi` (section 18p3 of `tools/fui/run.sh`): position and
order of the parts, no overlap, hit test (a point on the text answers the row),
hover and selection colours in light and dark, one choice at a time, disabled
rows, a narrow list where the text gives way (badge stays inside, drift 0).
`tools/fui/app_main.fi` section "files": hover, click, handlers, selection,
audit; the browser draws the same picture (`tools/wasm/appcheck.py`, files.wasm).

## Open (write here what OrientOS needs)

* Keyboard: Up / Down / Enter on a list (a focus model for rows).
* More than one chosen row (Ctrl / Shift click), rename in place.
* Long lists: needs the pane box (`scene` viewport) with row virtualisation;
  today a list is limited by the node count.
* Symbols other than Lucide (file-type icons, bitmaps): set `ICON_NONE` and
  hang an image node (`scene.node_set_image`) in the row, or give the row a
  painter of its own (`node_set_draw`).
* A mixed list (some rows without symbol) does not align the texts; give every
  row a symbol or none.
* `lib/fui/icons.fi` pulls in the glyph cache; the kernel profile (OrientOS
  ring 0) draws its symbols itself -- then use `ICON_NONE` and `node_set_draw`.
* Context menu on a row (07.10.2026, done): a right click / long press / menu key on a list row opens the
  row's menu, the right-clicked row is chosen first (Windows' rule); fui.app has the hooks built in. See
  `docs/CONTEXT-MENUS.md` (`app.on_context`, `FAM_FILE` / `FAM_LIST` standard entries). A multiple selection
  (Ctrl / Shift click, r134) plugs into the same menu through `ctxmenu.SelHooks` (`count`, `is_selected`,
  `select_only`, `clear`): the entries then see `pc_selected` = "many".
