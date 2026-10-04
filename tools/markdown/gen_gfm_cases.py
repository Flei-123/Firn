#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/markdown/gen_gfm_cases.py -- MAKES tests/data/markdown-gfm-cases.json.
#
# The expected HTML of the GFM cases (tables, strikethrough) comes from an
# implementation nobody here wrote: markdown-it-py (pip install markdown-it-py,
# version 4.2.0 when the file was made) with `table` and `strikethrough`
# enabled. Two adjustments, both on purpose: markdown-it writes `<s>` where GFM
# writes `<del>` (replaced here), and the case with a single `~` is left out
# (GFM allows one or two tildes, markdown-it only two; lib/markdown follows GFM).
#
#     python3 tools/markdown/gen_gfm_cases.py > tests/data/markdown-gfm-cases.json
import json
import sys
from markdown_it import MarkdownIt
md = MarkdownIt('commonmark', {'html': True}).enable(['table','strikethrough'])
cases = [
"| a | b |\n|---|---|\n| 1 | 2 |\n",
"a | b\n--|--\n1 | 2\n",
"| left | center | right |\n|:-----|:------:|------:|\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n",
"| a | b |\n|---|---|\n| 1 |\n",
"| a | b |\n|---|---|\n| 1 | 2 | 3 |\n",
"| a | b |\n|---|---|\n",
"| a | b |\n|---|---|\n| 1 | 2 |\n\nafter\n",
"| a | b |\n|---|---|\n| 1 | 2 |\nnext line\n",
"| a | b |\n|---|---|\n| `x\\|y` | c \\| d |\n",
"| *a* | **b** |\n|---|---|\n| [l](http://x) | ~~s~~ |\n",
"text before\n| a | b |\n|---|---|\n| 1 | 2 |\n",
"para line 1\npara line 2\n\n| a |\n|---|\n| 1 |\n",
"| a | b |\n|---|\n| 1 | 2 |\n",
"| a |\n|---|\n| 1 |\n| 2 |\n",
"> | a | b |\n> |---|---|\n> | 1 | 2 |\n",
"- | a | b |\n  |---|---|\n  | 1 | 2 |\n",
"| a | b |\n|---|---|\n| 1 | 2 |\n> quote\n",
"| a | b |\n|---|---|\n| 1 | 2 |\n# heading\n",
"| a | b |\n| - | - |\n| 1 | 2 |\n",
"|a|b|\n|-|-|\n|1|2|\n",
"| a | b |\n|---|---|\n\n| c | d |\n|---|---|\n",
"~~strike~~\n",
"a ~~b~~ c\n",
"~~a **b** c~~\n",
"~~ not~~\n",
"**~~a~~**\n",
"~~a\nb~~\n",
"~~~\ncode\n~~~\n",
"~~a~~b ~~c~~\n",
"\\~~a~~\n",
"| a |\n|:-:|\n| x |\n",
"Hello\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nWorld\n",
"| h1 | h2 |\n|----|----|\n| `code` | **bold** |\n| a<b | c&d |\n",
"| a | b |\n|---|---|\n| 1 | 2 |\n\n\n| x |\n|---|\n| y |\n",
"|  |  |\n|--|--|\n|  |  |\n",
"| a | b |\n|---|---|\n|   | 2 |\n",
"Not a table | b\nstill | para\n",
"a|b\n-|-\n",
]
out = []
for c in cases:
    if c == "\\~~a~~\n":
        continue  # single tilde: documented difference
    html = md.render(c).replace("<s>", "<del>").replace("</s>", "</del>")
    out.append({"markdown": c, "html": html})
json.dump(out, sys.stdout, indent=1)
sys.stdout.write("\n")
