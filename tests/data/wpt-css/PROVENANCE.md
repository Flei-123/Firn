# tests/data/wpt-css -- the official CSS layout tests of round B2

These files are the **Web Platform Tests**, `css/` area. They were
downloaded from

    https://github.com/web-platform-tests/wpt
    branch `master`, on 25 August 2026

with `raw.githubusercontent.com`, path for path. The directory layout is
the one of the WPT repository, so a `<link rel="stylesheet"
href="/css/css-flexbox/support/flexbox.css">` in a test resolves the same
way it does there.

## Which files, and by which rule

Every file under

    css/css-flexbox   css/CSS2   css/css-box   css/css-sizing
    css/css-position  css/css-align

that **includes `/resources/check-layout-th.js`** -- 471 files. Nothing
else was picked, and nothing was left out inside those directories.

`check-layout-th.js` is what makes them usable here. Most of the WPT css/
area consists of **reftests**: a test file and a reference file are
rendered and the pixels are compared, which needs a rasteriser round B2
does not have. The tests kept here carry their expectation IN the markup:

```html
<div data-expected-width="100" data-offset-y="20">
```

and that script checks it against `offsetWidth`, `offsetHeight`,
`offsetLeft`, `offsetTop`, `clientWidth`, `clientHeight`, `scrollWidth`,
`scrollHeight`, `getBoundingClientRect()`, the computed `display` and the
used margins and paddings -- the CSSOM View accessors. Those are
**position and size, not pixels**, which is exactly what a layout engine
produces.

Two files that are not tests came along because the tests link to them:

    fonts/ahem.css                     the @font-face of the test font
    css/css-flexbox/support/flexbox.css  the class shorthands the flexbox
                                       tests use

The three `/resources/*.js` files are NOT here: the harness does not run
them, it implements the same comparison
(`tools/layoutb2/harness.py`, and `check-layout-th.js` is quoted there
attribute for attribute).

## The font

Many of these tests set `font: 10px/1 Ahem`. Ahem is the test font of the
CSS working group: every glyph is a square of one em, the ascent is 0.8 em
and the descent 0.2 em. The measuring font of this engine
(`tools/layout/FirnMetric.ttf`, round 61) has exactly those metrics, and
`lib/layout/box.fi` computes with them for every font -- so an Ahem test
measures the same here as in a browser, and a test that uses another font
does not. That is a limit, and it is named in docs/ROUNDB2.md instead of
being filtered away.

## Licence

The WPT test suite is published under the 3-clause BSD licence
(`LICENSE` in the WPT repository).
