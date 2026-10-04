#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/markdown/gen_entities.py -- MAKES lib/markdown/entities.txt.
#
# The HTML5 named character references that end in `;` (2,125 of them), one per
# line: "name codepoint [codepoint]", sorted by the octets of the name (the parser
# binary-searches it). Source: Python's html.entities.html5, the WHATWG list.
#
#     python3 tools/markdown/gen_entities.py > lib/markdown/entities.txt
import html.entities as e
import sys

rows = []
for k, v in e.html5.items():
    if k.endswith(";"):
        rows.append((k[:-1], " ".join(str(ord(c)) for c in v)))
for name, cps in sorted(set(rows), key=lambda t: t[0].encode()):
    sys.stdout.write("%s %s\n" % (name, cps))
