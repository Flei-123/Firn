#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import re
import struct
import sys

src = sys.argv[1] if len(sys.argv) > 1 else "lib/docview/pdf/pdffonts_data.fi"
out = sys.argv[2] if len(sys.argv) > 2 else "demos/webdemo/pdffonts.pack"
text = open(src).read()
offs = [int(v) for v in re.search(r"PDFFONT_OFF: \[usize; \d+\] = \[([^\]]*)\]", text).group(1).split(",")]
body = re.search(r"PDFFONT_DATA: \[u8; (\d+)\] = \[([^\]]*)\]", text, re.S)
data = bytes(int(v) for v in body.group(2).replace("\n", " ").split(",") if v.strip())
assert len(data) == int(body.group(1)) == offs[-1]
count = len(offs) - 1
head = struct.pack("<I", count) + b"".join(struct.pack("<I", o) for o in offs)
open(out, "wb").write(head + data)
print(out, len(head) + len(data))
