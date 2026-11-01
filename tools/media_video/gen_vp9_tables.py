import re
import sys
import os

src_root = sys.argv[1]
out_root = sys.argv[2]


def load(rel):
    text = open(os.path.join(src_root, rel)).read()
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    return text


def parse_braces(text, pos):
    assert text[pos] == "{"
    pos += 1
    items = []
    cur = ""
    while True:
        ch = text[pos]
        if ch == "{":
            sub, pos = parse_braces(text, pos)
            items.append(sub)
            continue
        if ch in ",}":
            tok = cur.strip()
            if tok:
                items.append(int(tok))
            cur = ""
            pos += 1
            if ch == "}":
                return items, pos
            continue
        cur += ch
        pos += 1


def extract(rel, name):
    text = load(rel)
    m = re.search(r"\b" + re.escape(name) + r"\b[^=;{]*=\s*\{", text)
    assert m, (rel, name)
    start = text.index("{", m.start() + len(name))
    start = m.end() - 1
    val, _ = parse_braces(text, start)
    return val


def flatten(v, shape):
    if len(shape) == 0:
        assert isinstance(v, int), v
        return [v]
    out = []
    assert isinstance(v, list), v
    assert len(v) <= shape[0], (len(v), shape)
    for i in range(shape[0]):
        if i < len(v):
            out.extend(flatten(v[i], shape[1:]))
        else:
            n = 1
            for s in shape[1:]:
                n *= s
            out.extend([0] * n)
    return out


def fmt_array(name, ty, vals, per=16):
    lines = ["static %s: [%s; %d] = [" % (name, ty, len(vals))]
    for i in range(0, len(vals), per):
        chunk = ", ".join(str(v) for v in vals[i:i + per])
        lines.append("    " + chunk + ("," if i + per < len(vals) else ""))
    lines.append("]")
    return "\n".join(lines)


def module(path, exports, body):
    text = "// SPDX-License-Identifier: MPL-2.0\n"
    text += "export { %s }\n\n" % ", ".join(sorted(exports))
    text += "\n\n".join(body) + "\n"
    full = os.path.join(out_root, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    open(full, "w").write(text)


def table(fname, cname, ty, vals):
    return fmt_array(cname, ty, vals) + "\n\nfn %s() -> u64 {\n    return (&%s[0]) as u64\n}" % (fname, cname)


E = "vp9/common/vp9_entropy.c"
M = "vp9/common/vp9_entropymode.c"
V = "vp9/common/vp9_entropymv.c"
S = "vp9/common/vp9_scan.c"
Q = "vp9/common/vp9_quant_common.c"
F = "vp9/common/vp9_filter.c"

coef = []
for n in ("4x4", "8x8", "16x16", "32x32"):
    coef += flatten(extract(E, "default_coef_probs_" + n), [2, 2, 6, 6, 3])

regions = []


def add(name, vals):
    regions.append((name, vals))


add("COEF", coef)
add("SKIP", flatten(extract(M, "default_skip_probs"), [3]))
tx_raw = open(os.path.join(src_root, M)).read()
m = re.search(r"default_tx_probs = \{(.*?)\};", re.sub(r"//[^\n]*", "", tx_raw), re.S)
nums = [int(x) for x in re.findall(r"-?\d+", m.group(1))]
assert len(nums) == 6 + 4 + 2, nums
add("TX32", nums[0:6])
add("TX16", nums[6:10])
add("TX8", nums[10:12])
add("YMODE", flatten(extract(M, "default_if_y_probs"), [4, 9]))
add("UVMODE", flatten(extract(M, "default_if_uv_probs"), [10, 9]))
add("PART", flatten(extract(M, "default_partition_probs"), [16, 3]))
add("INTRA_INTER", flatten(extract(M, "default_intra_inter_p"), [4]))
add("COMP_INTER", flatten(extract(M, "default_comp_inter_p"), [5]))
add("COMP_REF", flatten(extract(M, "default_comp_ref_p"), [5]))
add("SINGLE_REF", flatten(extract(M, "default_single_ref_p"), [5, 2]))
add("INTER_MODE", flatten(extract(M, "default_inter_mode_probs"), [7, 3]))
add("INTERP", flatten(extract(M, "default_switchable_interp_prob"), [4, 2]))

mvt = load(V)
mm = re.search(r"default_nmv_context = \{(.*?)\n\};", mvt, re.S)
body = mm.group(1)
joints = [int(x) for x in re.findall(r"\d+", re.search(r"\{\s*(\d+,\s*\d+,\s*\d+)\s*\}", body).group(1))]
assert joints == [32, 64, 96], joints
comp_vals = []
for part in re.findall(r"\{\s*128,.*?\n\s*\}", body, re.S):
    nums = [int(x) for x in re.findall(r"\d+", part)]
    assert len(nums) == 1 + 10 + 1 + 10 + 6 + 3 + 1 + 1, len(nums)
    comp_vals.append(nums)
assert len(comp_vals) == 2
add("MV_JOINT", joints)
add("MV_COMP0", comp_vals[0])
add("MV_COMP1", comp_vals[1])

offsets = {}
flat = []
for name, vals in regions:
    offsets[name] = len(flat)
    flat += vals
for v in flat:
    assert 0 <= v <= 255

consts = []
for name, vals in regions:
    consts.append("const P_%s: usize = %d" % (name, offsets[name]))
consts.append("const P_TOTAL: usize = %d" % len(flat))
exports = ["P_" + n for n, _ in regions] + ["P_TOTAL", "default_probs_addr"]
module("media/video/vp9/vp9defp.fi", exports, ["\n".join(consts), table("default_probs_addr", "VP9_DEFAULT_PROBS", "u8", flat)])

kf = flatten(extract(M, "vp9_kf_y_mode_prob"), [10, 10, 9])
kfuv = flatten(extract(M, "vp9_kf_uv_mode_prob"), [10, 9])
kfp = flatten(extract(M, "vp9_kf_partition_probs"), [16, 3])
module("media/video/vp9/vp9kfp.fi", ["kf_y_addr", "kf_uv_addr", "kf_part_addr"],
       [table("kf_y_addr", "VP9_KF_Y", "u8", kf), table("kf_uv_addr", "VP9_KF_UV", "u8", kfuv), table("kf_part_addr", "VP9_KF_PART", "u8", kfp)])

par = flatten(extract(E, "vp9_pareto8_full"), [255, 8])
band8 = flatten(extract(E, "vp9_coefband_trans_8x8plus"), [1024])
band4 = flatten(extract(E, "vp9_coefband_trans_4x4"), [16])
cat6 = flatten(extract(E, "vp9_cat6_prob"), [14])
invmap = extract("vp9/decoder/vp9_dsubexp.c", "inv_map_table")
invmap = flatten(invmap, [len(invmap)])
module("media/video/vp9/vp9entp.fi", ["pareto_addr", "band8_addr", "band4_addr", "cat6_addr", "invmap_addr"],
       [table("pareto_addr", "VP9_PARETO", "u8", par), table("band8_addr", "VP9_BAND8", "u8", band8),
        table("band4_addr", "VP9_BAND4", "u8", band4), table("cat6_addr", "VP9_CAT6", "u8", cat6),
        table("invmap_addr", "VP9_INVMAP", "u8", invmap)])

scan_body = []
scan_exports = []
for n, cnt in (("4x4", 16), ("8x8", 64), ("16x16", 256), ("32x32", 1024)):
    kinds = ("default", "col", "row") if n != "32x32" else ("default",)
    for k in kinds:
        v = flatten(extract(S, "%s_scan_%s" % (k, n)), [cnt])
        fn = "scan_%s_%s_addr" % (k, n)
        scan_body.append(table(fn, "SCAN_%s_%s" % (k.upper(), n.upper()), "u16", v))
        scan_exports.append(fn)
        nb = flatten(extract(S, "%s_scan_%s_neighbors" % (k, n)), [(cnt + 1) * 2])
        fn = "nb_%s_%s_addr" % (k, n)
        scan_body.append(table(fn, "NB_%s_%s" % (k.upper(), n.upper()), "u16", nb))
        scan_exports.append(fn)
module("media/video/vp9/vp9scan.fi", scan_exports, scan_body)

dcq = flatten(extract(Q, "dc_qlookup"), [256])
acq = flatten(extract(Q, "ac_qlookup"), [256])
module("media/video/vp9/vp9qtab.fi", ["dc_q_addr", "ac_q_addr"],
       [table("dc_q_addr", "VP9_DC_Q", "u16", dcq), table("ac_q_addr", "VP9_AC_Q", "u16", acq)])

ft = load(F)
filt = []
for n in ("sub_pel_filters_8", "sub_pel_filters_8lp", "sub_pel_filters_8s", "bilinear_filters"):
    v = flatten(extract(F, n), [16, 8])
    filt += [x + 128 for x in v]
module("media/video/vp9/vp9ftab.fi", ["filt_addr"], [table("filt_addr", "VP9_FILT", "u16", filt)])
print("ok", len(flat))
