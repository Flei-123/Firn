# tools/html/luecken/ -- what the tree construction can(not) do yet

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

The expected trees in `bekannte_luecken.dat` are the RIGHT ones (from the
WHATWG standard). The runner drives them separately from the main quota:

    python3 tools/html/harness_tree.py <binary> --gaps

They are reported on their own and do NOT go into the quota of
`tools/html/cases/`. A test suite that only contains what already works says
nothing about what is missing -- this file is the counter-calculation.

The four `<template>` cases carry `#orakel-abweichung`: html5lib 1.1 does not
put the template content into a content tree of its own and can therefore not
confirm the expectation. It is written by hand from the standard.
