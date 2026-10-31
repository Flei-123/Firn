#!/usr/bin/env python3
import re, sys, os

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SRC = os.path.join(ROOT, "lib/ton/mp3_vec.fi")
DST = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "lib/@web/ton/mp3_vec.fi")

text = open(SRC).read()
text = re.sub(r"__v128_(\w+)\(", r"v4soft.\1(", text)
text = re.sub(r"\bv128\b", "v4soft.V128", text)
text = text.replace("import ton.mp3\n", "import ton.mp3\nimport ton.v4soft\n", 1)
os.makedirs(os.path.dirname(DST), exist_ok=True)
open(DST, "w").write(text)
