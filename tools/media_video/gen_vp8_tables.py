import ast
import re
import sys

root = sys.argv[1]
src = open(root + "/lib/webp/webp.fi").read()
src = src[src.index("fn tables_part0"):]
parts = re.findall(r"var t: \[u8; (\d+)\] = (b\"[^\"\n]*\")", src)
blob = b"".join(ast.literal_eval(lit) for _, lit in parts)
assert len(blob) == 3524, len(blob)

SECTIONS = [
    ("VP8_COEF_UPDATE", "u8", 0, 1056),
    ("VP8_COEF_DEFAULT", "u8", 1056, 1056),
    ("VP8_KF_BMODE", "u8", 2112, 900),
    ("VP8_DC_Q", "u16", 3012, 256),
    ("VP8_AC_Q", "u16", 3268, 256),
]

out = ["// SPDX-License-Identifier: MPL-2.0", "export { vp8_coef_update_addr, vp8_coef_default_addr, vp8_kf_bmode_addr, vp8_dc_q, vp8_ac_q }", ""]
for name, ty, off, size in SECTIONS:
    data = blob[off:off + size]
    if ty == "u16":
        vals = [data[i] | (data[i + 1] << 8) for i in range(0, size, 2)]
    else:
        vals = list(data)
    out.append("static %s: [%s; %d] = [" % (name, ty, len(vals)))
    for i in range(0, len(vals), 16):
        out.append("    " + ", ".join(str(v) for v in vals[i:i + 16]) + ",")
    out.append("]")
    out.append("")

for name, fn in (("VP8_COEF_UPDATE", "vp8_coef_update_addr"), ("VP8_COEF_DEFAULT", "vp8_coef_default_addr"), ("VP8_KF_BMODE", "vp8_kf_bmode_addr")):
    out.append("fn %s() -> u64 {\n    return (&%s[0]) as u64\n}\n" % (fn, name))
for name, fn in (("VP8_DC_Q", "vp8_dc_q"), ("VP8_AC_Q", "vp8_ac_q")):
    out.append("fn %s(q: i64) -> i64 {\n    return %s[q as usize] as i64\n}\n" % (fn, name))

open(root + "/lib/media/video/vp8/vp8tabs.fi", "w").write("\n".join(out))
