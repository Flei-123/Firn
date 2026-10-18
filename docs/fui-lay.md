# fui lay — CSS flexbox for the ten-line app API

`lib/fui/lay.fi` makes web-mobile layouts buildable with `app.*`. `app.row/column/card`
are unchanged (centred); `lay.*` is the CSS way: **justify start, align stretch, gap 0, padding 0**.
All sizes are design points (= CSS px).

```firn
import fui.app
import fui.lay

lay.page(a)                      // root column: stretch, start, no padding
let top = lay.vbox(a)            // flex-direction: column
lay.fill_h(a, top)               // fills the window height
lay.pad_all(a, top, 16)
lay.gap(a, top, 0, 12)           // column-gap 0, row-gap 12
  let t = lay.label(a, "Title", 20)    // wraps at the container width, text-align start
  let c = app.card(a)
    let e = app.entry(a, "Name")
    lay.fill_w(a, e)             // width: 100%
  app.end(a)
  let r = lay.hbox(a)
    let f = app.entry(a, "Search")
    lay.grow(a, f, 1000)         // takes the rest of the row
    app.button(a, "Go", cb)
  app.end(a)
  let tags = lay.hwrap(a)        // flex-wrap: wrap, real line logic (height follows the lines)
  app.end(a)
app.end(a)
```

## API (signatures are fixed; `a: *mut app.App`, `id` = any node)
- Constants: `AL_START/CENTER/END/STRETCH/BASELINE`, `JU_START/CENTER/END/BETWEEN/AROUND/EVENLY`.
- Boxes (close with `app.end`): `vbox(a)`, `hbox(a)`, `hwrap(a)`; `page(a)` configures the root.
- Box setters: `align(a,id,AL)`, `justify(a,id,JU)`, `gap(a,id,x,y)` (x = column-gap, y = row-gap), `pad(a,id,l,t,r,b)`, `pad_all`.
- Child setters: `align_self`, `margin(l,t,r,b)`, `grow(permille)`, `shrink(permille)`, `basis(pt)`, `size(w,h)` (0 = auto),
  `min_w/max_w/min_h/max_h`, `fill_w`, `fill_h`, `aspect(w,h)`, `hidden(bool)` (= visibility), `inset_safe(a,id)`.
- Text: `label(a,s,size)` (wrapping, start-aligned), `text_wrap(a,id)`, `text_start(a,id)`, `spacer_grow(a)`.
- Not yet: `scroll_v` (scroll container) — open, see roadmap.

## How it works
- `scene.fi` got additive Node fields (all zero = old behaviour): `strict` (CSS mode of a box), margins, min/max,
  fill flags, aspect, natural size. `strict` boxes: a child with an explicit cross size is **not** stretched
  (unless `fill_*`), the automatic minimum size of a fixed child is `min(content, fixed)`, an empty block has size 0.
- Margins are done by inflating the flex item and shrinking the rectangle afterwards (`item_extras`, `lay_adjust`).
- `flex.fi verteile` now follows css-flexbox-1 §9.7 exactly (grow/shrink by hypothetical sizes, freeze by the sign
  of the total violation). New `FlexItem.mgn` so the shrink share is scaled by the size without margins, as CSS.
- A wrapping `strict` row re-measures its height at the width of its last layout (reflow, like paragraphs).

## Differences to CSS
- No auto margins, no `order`, no `display:none`, no column-wrap, no fractional flex-factor rule (<1).
- Overflow with end/centre justify is clamped at the start (CSS goes negative).
- Cross-axis min/max clamp the rectangle; the item stays at the cross start.

## Tests
`tools/fui/lay_ref.py` lays 190 flex cases (20 hand-written, rest seeded random) out in real Chromium and writes whole-pixel
results to `tools/fui/lay_cases.txt`; `tools/fui/lay_main.fi` (run.sh 12y) rebuilds each with `lay.*` and requires 0 px
deviation, plus a mobile page (stretch, full-width fields, paragraph wrap, rest-of-row entry, wrapping row).
