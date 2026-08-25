# tests/data/html5lib -- the official HTML tree construction tests

These `.dat` files are the html5lib `tree-construction` suite. They are no
longer kept in the `html5lib/html5lib-tests` repository: with commit
`224991ec10db04f056a89eed8b0bd8695fd2950e` ("Tree construction tests have
moved to WPT", 26 June 2026) the directory was deleted there and the README
now points at

    https://github.com/web-platform-tests/wpt/tree/master/html/syntax/parsing

The files here were downloaded from the maintained upstream location

    web-platform-tests/wpt : html/syntax/parsing/resources/*.dat

on 25 August 2026. That set is the html5lib set plus
`processing-instructions.dat`; the four `scripted_*.dat` files are the ones
that need a script-executing parser.

Format: `#data`, `#errors`, optionally `#document-fragment` (the context
element for fragment parsing), optionally `#script-on` / `#script-off`,
then `#document` with the expected tree. The reader lives in
`tools/html/harness_tree.py`; `tools/domb1/run.sh` drives the whole set.

Licence: the WPT test suite is published under the 3-clause BSD licence
(`LICENSE` in the WPT repository).
