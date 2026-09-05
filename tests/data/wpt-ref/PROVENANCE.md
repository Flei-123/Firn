# tests/data/wpt-ref -- the official REFERENCE TESTS of round B3

These files are the **Web Platform Tests**, the `css/css-backgrounds/` and
`css/css-color/` areas. They were downloaded from

    https://github.com/web-platform-tests/wpt
    branch `master`, on 26 August 2026

with `raw.githubusercontent.com`, path for path, by
`tools/paintb3/harvest.py`. The directory layout is the one of the WPT
repository, so a `<link rel="match" href="reference/foo-ref.html">` in a
test resolves the same way it does there.

## Which files, and by which rule

Every file **directly in**

    css/css-backgrounds/      css/css-color/

that carries

```html
<link rel="match" href="...">
```

-- 541 files -- together with the reference file it names and every
stylesheet either of the two links to. Nothing was picked out by hand and
nothing was left out because it looked hard. A reference test whose
expectation this engine cannot reach is a **failing test in the quota**,
not a missing file.

`pairs.txt` lists the pairs, one per line: the test and its reference.

## Why reference tests, and why they are the right measure now

Round B2 measured against the part of the WPT `css/` area that is
**self-describing**: the expected `offsetWidth` and its kin stand in the
markup as `data-expected-*` attributes, so a layout engine can be measured
without a picture. That was the honest thing to do for a round that
produced no picture.

Most of `css/` is not like that. It is **reference tests**: two documents
that have to be rendered and whose PIXELS are then compared. The test file
reaches its result the complicated way -- through the property under test
-- and the reference file the simple way, with a plain box in the right
place. Nothing but a rasteriser can run them, and running them is what
round B3 is for.

## The trap, and what this corpus does about it

A reference test is the easiest measurement in the world to pass by
accident: **an engine that draws nothing passes every single one of them.**
Both sides come out white, white equals white, the quota says 100 per cent.

`tools/paintb3/reftest.py` therefore counts a pair as passed only if the
two pictures match AND the test rendering is **not empty** -- at least 64
pixels of it differ from the blank page. Pairs that match while being
empty are counted separately as `vacuous` and are **not** in the quota.
The number in `docs/ROUNDB3.md` is the first one.

## The font

The tests do not carry a font with them; they name one. A page that names
**Ahem** gets `tests/data/fonts/Ahem.ttf` (the test font of the CSS working
group, every glyph a square of one em), everything else gets
`tests/data/fonts/FirnSans.ttf`. The SAME choice is made for a test and for
its reference, because a comparison in which the two sides use different
fonts is not a comparison.

## Licence

The WPT test suite is published under the 3-clause BSD licence (`LICENSE`
in the WPT repository).
