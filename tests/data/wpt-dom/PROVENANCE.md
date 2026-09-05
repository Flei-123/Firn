# tests/data/wpt-dom -- the official DOM tests of round B4

These files are the **Web Platform Tests**. They were downloaded from

    https://github.com/web-platform-tests/wpt
    branch `master`, on 26 August 2026

with `raw.githubusercontent.com`, path for path, by
`tools/liveb4/harvest.py`. The directory layout is the one of the WPT
repository, so a `<script src="/resources/testharness.js">` in a test
resolves the same way it does there.

## Which files, and by which rule

Every `*.html` DIRECTLY IN

    dom/nodes, dom/events, dom/traversal, dom/lists, dom/ranges, dom/abort

that includes `/resources/testharness.js`: **313 files**, together with
**32** support files (the scripts and stylesheets they reference, and
`resources/testharness.js` itself). Nothing was picked by hand and nothing
was left out because it looked hard.

`testharness.js` is what makes them usable here: it is the OFFICIAL
harness, unmodified, and it runs inside this engine. Its result is read
through `add_completion_callback`, the same interface a real browser
runner uses. A test this engine cannot pass is a FAILING test in the
quota, not a missing file.

## What is deliberately NOT counted

A test whose harness never reaches `add_completion_callback` -- because it
needs a second browsing context, a worker, `fetch`, or an API this round
does not have at all -- is counted as **could not run** and is reported
separately from the failures. Counting it as a failure would be honest
too; counting it as a pass would not, and neither would leaving it out of
the corpus.
