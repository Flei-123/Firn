# Markdown: `lib/markdown` and `fui.markdownview`

A CommonMark parser with the GitHub extensions, a tree it produces, an HTML
writer, and a scrolling view for fUi windows. Made for the descriptions of
mods and projects (Modrinth, GitHub READMEs): tables, task lists, bare URLs,
images, and HTML that must **not** be trusted.

| file | what |
|---|---|
| `lib/markdown/md.fi` | `parse(src, opts) -> Doc`, `to_html`, `to_html_safe` -- the one import |
| `lib/markdown/doc.fi` | the tree (an arena of nodes, one text pool), `Opts`, node kinds, tree surgery |
| `lib/markdown/block.fi` | the block parser (CommonMark's line-by-line algorithm) |
| `lib/markdown/inline.fi` | the inline parser (delimiter stack, brackets, autolinks) |
| `lib/markdown/scan.fi` | character classes, UTF-8, entities, inline-HTML grammar, link labels |
| `lib/markdown/html.fi` | tree -> HTML (commonmark.js output), safe mode, an indented tree dump |
| `lib/markdown/entities.txt` | the 2,125 HTML5 entity names that end in `;` (generated) |
| `lib/fui/markdownview.fi` | the view: layout at a width, painting, scrolling, links, images |

## Use

```firn
import markdown.md
import markdown.doc

var d: doc.Doc = md.parse("# Hello *world*", doc.opts_default())
var c: u32 = doc.node_first(&d, 0 as u32)      // node 0 is the document
// doc.node_kind / node_next / node_text / node_url / node_a ...
doc.doc_free(&d)                                 // always, once
```

Options (`doc.Opts`): `tables`, `strike` (`~~x~~`, also `~x~`), `autolink`
(bare `http://`, `https://`, `www.`), `tasks` (`- [ ]`, `- [x]`) -- all on in
`opts_default()` and off in `opts_commonmark()` -- and `html`:

| `html` | what happens to raw HTML |
|---|---|
| `HTML_IGNORE` (default) | tags vanish, the text between them stays; `<br>` becomes a hard break, `<img src alt title>` an image, `<a href>...</a>` a link; comments, declarations, `<script>` and `<style>` bodies are dropped |
| `HTML_ESCAPE` | tags are shown as literal text |
| `HTML_RAW` | `HTML_BLOCK` / `HTML_INLINE` nodes are kept; `html.render` writes them verbatim -- trusted input only |

## The tree

Blocks: `PARA`, `HEADING` (`a` = level), `THEMATIC`, `CODE_BLOCK` (text, `url` =
info string), `QUOTE`, `LIST` (`b` ordered, `a` start, `c` tight), `ITEM` (`a` =
task state), `HTML_BLOCK`, `TABLE`, `TROW` (`b` = header), `TCELL` (`a` =
alignment). Inlines: `TEXT`, `SOFTBREAK`, `HARDBREAK`, `CODE`, `EMPH`, `STRONG`,
`STRIKE`, `LINK` (url, title), `IMAGE` (url, title, children = alt),
`HTML_INLINE`. Nothing in a `Doc` is HTML: text is text, and the renderer
escapes. `html.dump` prints the tree, one node per line.

Nesting is bounded: block containers 100 deep, inline containers 64 deep (the
markers beyond stay literal), so no reader needs unbounded recursion.

## What proves it

* `tests/2090_markdown.fi`: **all 652 examples of the CommonMark 0.31.2 spec**
  (`tests/data/commonmark-spec-0.31.2.json`), markdown in, HTML out, byte for
  byte; 37 GFM cases whose expected HTML comes from markdown-it-py 4.2.0
  (`tests/data/markdown-gfm-cases.json`); the default-mode tree dumps; the
  safe renderer; pathological and absurdly nested input (run in all four build
  levels by `test.sh`).
* `tools/fui/mdview_main.fi` (section 18p4 of `tools/fui/run.sh`): the view,
  pixel by pixel, light and dark, wide and narrow.

## Honest limits

* One difference from markdown-it, on purpose: a single `~` also strikes
  (GFM allows one or two); the GFM table cell splits on every unescaped `|`,
  also inside a code span (as GitHub does).
* GFM bare-URL autolinks follow the GFM rules for trailing punctuation and
  unbalanced `)`; the e-mail form without `<>` is not recognised.
* Link-label case folding covers ASCII, Latin-1, Latin Extended-A, Greek and
  Cyrillic simple folds, and ss for sharp s; other scripts compare as written.
* `HTML_IGNORE` is a policy for display, not a sanitiser for output: use
  `html.render(..., safe = true)` (empty targets for `javascript:`,
  `vbscript:`, `file:`, `data:`) if you write HTML.
* The view: no text selection or copy, code lines wrap instead of scrolling
  sideways, a table cell shows inline content only (no nested blocks), HTML
  blocks are drawn as code only with `HTML_RAW`.

## The view

```firn
var v: markdownview.MdView = markdownview.mdv_new()
markdownview.mdv_set_markdown(&v, text, doc.opts_default())
markdownview.mdv_set_link_cb(&v, ud, on_link)         // fn(ud, url_ptr, url_len)
markdownview.mdv_set_image_cb(&v, ud, on_image)       // fn(ud, url, n, *mut ImgInfo) -> IMG_*
// every frame:  mdv_set_rect(&v, x, y, w, h);  mdv_paint(&v, ctx)
// input:        mdv_wheel, mdv_key, mdv_pointer_move / _down / _up / _leave
```

Images are never loaded by the view. The hook answers `IMG_PENDING` (frame,
symbol and alt text are drawn), `IMG_READY` (RGBA bytes in `ImgInfo`, scaled
down to the width, never up) or `IMG_FAILED`; when a picture arrives later the
program calls `mdv_invalidate` and the layout is redone with the scroll
position kept. Colours are theme tokens; links use `kitcolor.accent_text`, so
the accent is darkened on a light page until it reads (4.5:1).
