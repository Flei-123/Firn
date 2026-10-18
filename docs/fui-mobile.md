# fui mobile — phone screen parts in one call each

`lib/fui/mobile.fi` builds on `lay.fi` (CSS flex) and `webtheme.fi` (tokens of `public/style.css`).
Look: radius 16 cards, 12 fields, round pills, 44 pt touch targets. Colours are theme tokens only.
Test + picture: `tools/fui/mobile_main.fi` (73 checks, `appmobile_components-{dark,light}.png`, run.sh `build mobile`).

```firn
import fui.app
import fui.mobile

mobile.page(a)                                   // root = mobile page (column, stretch, no padding)
let bar = mobile.app_bar(a, "Chats", "menu", "plus", on_bar)   // cb(a, 0) left, cb(a, 1) right
let ct = mobile.content(a)                       // 12 pt side margin, grows between bar and nav
  let sec = mobile.section(a, "Today")
    let card = mobile.card(a)
      mobile.list_tile(a, "folder", "Files", "3 items", "", on_tile)   // cb(a, node)
    app.end(a)
  app.end(a)
  mobile.chip_row(a)  mobile.chip(a, "All", true, on_chip)  app.end(a)  // cb(a, index)
  mobile.text_field(a, "E-mail", "name@example.com", false)
  mobile.bubble(a, "Hello", true)                // true = mine (right, accent gradient)
app.end(a)
mobile.bottom_nav(a, 0, on_nav)  mobile.nav_item(a, "folder", "Files")  app.end(a)  // cb(a, index)
```

## API
- Containers (close with `app.end`): `content`, `card`, `section`, `chip_row`, `segmented`, `bottom_nav`, `bubble_open`.
- Leaves: `list_tile`, `app_bar`, `chip`, `chip_closable`, `segment`, `nav_item`, `text_field`, `text_area`, `bubble`, `pill`, `badge`, `empty_state`, `skeleton`, `spinner`, `divider`.
- State: `selected/select` (segmented, nav; select = by program, no callback), `chip_selected/set_chip_selected`, `field_error`, `nav_badge`.
- `sync(a)`: re-applies safe-area padding (bars grow with the top/bottom inset), the 82 % bubble width, theme classes. Call once per frame / after resize or theme change.
- Taps: `KIND_BUTTON` containers (focus, Enter/space, pressed state). A hit on any descendant counts for the button (`scene_panel_hit`); a nested button wins.
- Rules: at most 24 own style rules + webtheme's; the window cap is 64.

## Notes
- Segments are whole pixels: widths may differ by 1-2 px (CSS would use fractions).
- A test that builds a tree must `fuiwirt.host_bind` AFTER building (the panel knows the node count only then); the app loop does it.
- Open: `lay.scroll_v` (content does not scroll yet), Golden of whole screens (`appmobile_screens`).
