# tools/html/gaps/ -- what the tree construction can(not) do yet

THESE CASES FAIL -- ON PURPOSE.

They record what the tree construction of round 54 CANNOT do. The expected
trees are the RIGHT ones (from the WHATWG standard, checked against
html5lib 1.1). The runner drives them separately from the main quota
(tools/html/harness_tree.py --gaps) and reports them on their own.

Why at all: a test suite that only contains what already works says nothing
about what is missing. This file is the counter-calculation.

1..5  foreign content (SVG/MathML): the namespace on the root element
exists, the rule set for the CONTENT is missing (name correction, attribute
adjustment, integration points, breakout tags).
6..9  `<template>`: the 23rd insertion mode and its own content tree.
10    fragment parsing with a context element (`innerHTML`).

The expected trees in `known_gaps.dat` are the RIGHT ones (from the
WHATWG standard). The runner drives them separately from the main quota:

    python3 tools/html/harness_tree.py <binary> --gaps

They are reported on their own and do NOT go into the quota of
`tools/html/cases/`. A test suite that only contains what already works says
nothing about what is missing -- this file is the counter-calculation.

The four `<template>` cases carry `#oracle-deviation`: html5lib 1.1 does not
put the template content into a content tree of its own and can therefore not
confirm the expectation. It is written by hand from the standard.

## Round B1 — nine of the ten gaps are closed

Foreign content (1..5) and `<template>` (6..9) work; `docs/ROUNDB1.md` has
the numbers. Two entries were corrected while doing it:

* Case 9, `<template><td>x</td></template>`, carried `#oracle-deviation`
  and the hand written expectation `content "x"` -- the `td` was supposed
  to be dropped. It is not: the official suite
  (`tests/data/html5lib/template.dat`) keeps the `td` inside the template
  contents in every comparable case, because "in template" switches to
  "in row" and "clear the stack back to a table row context" stops at the
  template. The expectation was corrected to the one of the suite.
* Case 10, fragment parsing with a context element, stays a failure HERE
  and only here: `lib/browser/parse_main.fi` (the driver of round 54) has
  no field for the context element in its job format. Fragment parsing
  itself is implemented; `lib/browser/b1_main.fi` takes the context, and
  the 66 + 81 + 9 fragment cases of the official suite pass
  (`tools/domb1/run.sh`).
