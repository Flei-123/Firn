import os
import re
import subprocess
import sys
import tempfile

BASE = "https://aomedia.googlesource.com/aom/+/refs/heads/main/"
SOURCES = [
    "av1/common/entropymode.c",
    "av1/common/entropymv.c",
    "av1/common/token_cdfs.h",
    "av1/common/scan.c",
    "av1/common/quant_common.c",
    "av1/common/filter.h",
    "av1/common/reconintra.c",
    "av1/common/reconintra.h",
    "av1/common/av1_txfm.c",
    "aom_dsp/intrapred_common.h",
]
HDR = "// SPDX-License-Identifier: MPL-2.0\n"
TX_NAMES = [
    "4X4", "8X8", "16X16", "32X32", "64X64", "4X8", "8X4", "8X16", "16X8", "16X32",
    "32X16", "32X64", "64X32", "4X16", "16X4", "8X32", "32X8", "16X64", "64X16",
]
SCAN_ALIAS = {
    "64X64": "32X32", "32X64": "32X32", "64X32": "32X32", "16X64": "16X32", "64X16": "32X16",
}


def fetch(dest):
    for rel in SOURCES:
        out = os.path.join(dest, os.path.basename(rel))
        raw = subprocess.run(["curl", "-sfL", BASE + rel + "?format=TEXT"], capture_output=True, check=True).stdout
        data = subprocess.run(["base64", "-d"], input=raw, capture_output=True, check=True).stdout
        open(out, "wb").write(data)


def load(src, name):
    t = open(os.path.join(src, name)).read()
    t = re.sub(r"/\*.*?\*/", "", t, flags=re.S)
    return re.sub(r"//[^\n]*", "", t)


def match_brace(t, pos):
    depth = 0
    i = pos
    while True:
        c = t[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1


def safe_eval(expr):
    e = " ".join(expr.replace("CDF_PROB_TOP", "32768").split())
    assert re.fullmatch(r"[0-9 +\-*/()]+", e), expr
    return int(eval(e))


CDF_TOKEN = re.compile(r"AOM_CDF(\d+)\(((?:[^()]|\([^()]*\))*)\)|[{}]")


def parse_tree(body):
    stack = [[]]
    for m in CDF_TOKEN.finditer(body):
        s = m.group(0)
        if s == "{":
            stack.append([])
        elif s == "}":
            node = stack.pop()
            if not node:
                node = [(0, [])]
            stack[-1].append(node)
        else:
            n = int(m.group(1))
            args = [safe_eval(a) for a in m.group(2).split(",") if a.strip()]
            assert len(args) == n - 1, (n, args)
            assert all(0 <= a <= 32768 for a in args), args
            stack[-1].append((n, args))
    assert len(stack) == 1
    return stack[0][0]


def leaves(node):
    if isinstance(node, tuple):
        return [node]
    out = []
    for c in node:
        out.extend(leaves(c))
    return out


def shape_of(node):
    dims = []
    while isinstance(node, list) and not (len(node) == 1 and isinstance(node[0], tuple)):
        dims.append(len(node))
        node = node[0]
    return dims


def cdf_arrays(text):
    out = []
    for m in re.finditer(r"aom_cdf_prob\s+(\w+)((?:\s*\[[^\]]*\])+)\s*=\s*\{", text):
        end = match_brace(text, m.end() - 1)
        out.append((m.group(1), parse_tree(text[m.end() - 1:end + 1])))
    return out


class Table:
    def __init__(self, name, dims, leaf_list):
        self.name = name
        self.dims = dims
        self.leaf_list = leaf_list
        self.leaf = max(n for n, _ in leaf_list) + 1
        count = 1
        for d in dims:
            count *= d
        assert count == len(leaf_list), (name, dims, len(leaf_list))
        self.size = count * self.leaf
        self.off = 0

    def flat(self):
        out = []
        for n, args in self.leaf_list:
            row = args + [32768, 0] if n else []
            out.extend(row + [0] * (self.leaf - len(row)))
        return out

    def counters(self):
        return [i * self.leaf + n for i, (n, _) in enumerate(self.leaf_list) if n]

    def strides(self):
        s = []
        acc = self.leaf
        for d in reversed(self.dims):
            s.append(acc)
            acc *= d
        return list(reversed(s))


def short(name):
    n = re.sub(r"^(av1_)?default_", "", name)
    n = re.sub(r"_cdfs?$", "", n)
    return n.upper()


def build_tables(src):
    nc = []
    for name, tree in cdf_arrays(load(src, "entropymode.c")):
        nc.append(Table(short(name), shape_of(tree), leaves(tree)))
    mvtree = None
    t = load(src, "entropymv.c")
    m = re.search(r"default_nmv_context\s*=\s*\{", t)
    body = t[m.end() - 1:match_brace(t, m.end() - 1) + 1]
    mvtree = parse_tree(body)
    joints = mvtree[0]
    comps = mvtree[1]
    assert len(comps) == 2 and len(comps[0]) == 8
    fields = ["CLASSES", "CLASS0_FP", "FP", "SIGN", "CLASS0_HP", "HP", "CLASS0", "BITS"]
    mv = [Table("MV_JOINTS", [2], leaves(joints) * 2)]
    for fi, fname in enumerate(fields):
        per = [leaves(comps[c][fi]) for c in range(2)]
        inner = len(per[0])
        seq = []
        for _ in range(2):
            for c in range(2):
                seq.extend(per[c])
        dims = [2, 2] + ([inner] if inner > 1 else [])
        mv.append(Table("MV_" + fname, dims, seq))
    coef_q = [[] for _ in range(4)]
    coef = []
    for name, tree in cdf_arrays(load(src, "token_cdfs.h")):
        dims = shape_of(tree)
        assert dims[0] == 4
        ls = leaves(tree)
        per = len(ls) // 4
        for q in range(4):
            coef_q[q].append(ls[q * per:(q + 1) * per])
    first = cdf_arrays(load(src, "token_cdfs.h"))
    for idx, (name, tree) in enumerate(first):
        dims = shape_of(tree)[1:]
        coef.append(Table(short(name), dims, coef_q[0][idx]))
    for q in range(1, 4):
        for idx, tb in enumerate(coef):
            assert [n for n, _ in coef_q[q][idx]] == [n for n, _ in tb.leaf_list]
    return nc, mv, coef, coef_q


def layout(nc, mv, coef):
    pos = 0
    for tb in nc + mv:
        tb.off = pos
        pos += tb.size
    dn_total = pos
    coef_base = pos
    for tb in coef:
        tb.off = pos
        pos += tb.size
    return dn_total, coef_base, pos


def fmt_array(vals, per=16):
    lines = []
    for i in range(0, len(vals), per):
        lines.append("    " + ", ".join(str(v) for v in vals[i:i + per]))
    return ",\n".join(lines)


def static_block(const_name, ty, vals, fn_name):
    s = "static %s: [%s; %d] = [\n%s\n]\n\n" % (const_name, ty, len(vals), fmt_array(vals))
    s += "fn %s() -> u64 {\n    return (&%s[0]) as u64\n}\n" % (fn_name, const_name)
    return s


def write(outdir, name, text):
    open(os.path.join(outdir, name), "w").write(text)


def emit_idx(outdir, nc, mv, coef, dn_total, coef_base, total, counters):
    names = []
    body = ""
    for tb in nc + mv + coef:
        base = "CDF_" + tb.name if not tb.name.startswith("MV_") else "CDF_" + tb.name
        names.append(base)
        body += "const %s: usize = %d\n" % (base, tb.off)
        body += "const %s_LEAF: usize = %d\n" % (base, tb.leaf)
        names.append(base + "_LEAF")
        for i, st in enumerate(tb.strides()):
            body += "const %s_S%d: usize = %d\n" % (base, i, st)
            names.append("%s_S%d" % (base, i))
    glob = [
        ("CDF_DEFAULT_TOTAL", dn_total), ("CDF_COEF_BASE", coef_base), ("CDF_COEF_TOTAL", total - coef_base),
        ("CDF_ALL", total), ("CDF_COUNTERS", len(counters)),
    ]
    for n, v in glob:
        body += "const %s: usize = %d\n" % (n, v)
        names.append(n)
    write(outdir, "av1cdfidx.fi", HDR + "export { " + ", ".join(sorted(names)) + " }\n\n" + body)


def emit_defaults(outdir, nc, mv, coef_q, coef):
    vals = []
    for tb in nc + mv:
        vals.extend(tb.flat())
    write(outdir, "av1cdfdn.fi", HDR + "export { cdf_default_addr }\n\n" + static_block("CDF_DEFAULT", "u16", vals, "cdf_default_addr"))
    for q in range(4):
        vals = []
        for idx, tb in enumerate(coef):
            t2 = Table(tb.name, tb.dims, coef_q[q][idx])
            vals.extend(t2.flat())
        fn = "cdf_coef_q%d_addr" % q
        write(outdir, "av1cdfq%d.fi" % q, HDR + "export { %s }\n\n" % fn + static_block("CDF_COEF_Q%d" % q, "u16", vals, fn))


def emit_counters(outdir, counters):
    write(outdir, "av1cdfcn.fi", HDR + "export { cdf_counter_pos_addr }\n\n" + static_block("CDF_COUNTER_POS", "u32", counters, "cdf_counter_pos_addr"))


def extract(text, name):
    m = re.search(r"\b" + re.escape(name) + r"((?:\s*\[[^\]]*\])+)\)?\s*=\s*\{", text)
    assert m, name
    end = match_brace(text, m.end() - 1)
    body = text[m.end():end]
    body = re.sub(r"[{}]", " ", body)
    out = []
    for tok in body.split(","):
        tok = tok.strip()
        if tok:
            out.append(safe_eval(tok))
    return out


def emit_scan(outdir, src):
    text = load(src, "scan.c")
    names = []
    for m in re.finditer(r"int16_t,\s*((?:default|mrow|mcol)_scan_(\d+x\d+))\[(\d+)\]", text):
        names.append((m.group(1), m.group(2).upper(), int(m.group(3))))
    body = ""
    have = {}
    for nm, sz, n in names:
        vals = extract(text, nm)
        assert len(vals) == n, (nm, len(vals))
        cn = nm.upper()
        body += static_block(cn, "u16", vals, nm + "_addr") + "\n"
        have[(nm.split("_")[0], sz)] = nm
    fn = "fn scan_addr(kind: i64, tx: i64) -> u64 {\n"
    kinds = ["default", "mrow", "mcol"]
    for ti, tn in enumerate(TX_NAMES):
        sz = SCAN_ALIAS.get(tn, tn)
        fn += "    if tx == %d {\n" % ti
        for ki, k in enumerate(kinds):
            key = (k, sz) if (k, sz) in have else ("default", sz)
            if k == "default":
                continue
            fn += "        if kind == %d {\n            return %s_addr()\n        }\n" % (ki, have[key])
        fn += "        return %s_addr()\n    }\n" % have[("default", sz)]
    fn += "    return 0\n}\n\n"
    fn += "fn scan_kind(tx_type: i64) -> i64 {\n"
    fn += "    if tx_type == 10 || tx_type == 12 || tx_type == 14 {\n        return 1\n    }\n"
    fn += "    if tx_type == 11 || tx_type == 13 || tx_type == 15 {\n        return 2\n    }\n"
    fn += "    return 0\n}\n\n"
    fn += "fn scan_for(tx: i64, tx_type: i64) -> u64 {\n    return scan_addr(scan_kind(tx_type), tx)\n}\n"
    exports = sorted([n + "_addr" for n, _, _ in names] + ["scan_addr", "scan_kind", "scan_for"])
    write(outdir, "av1tabscan.fi", HDR + "export { " + ", ".join(exports) + " }\n\n" + body + fn)
    return len(names)


def emit_quant(outdir, src):
    text = load(src, "quant_common.c")
    pairs = [("dc_qlookup_QTX", "dc_q8"), ("dc_qlookup_10_QTX", "dc_q10"), ("dc_qlookup_12_QTX", "dc_q12"),
             ("ac_qlookup_QTX", "ac_q8"), ("ac_qlookup_10_QTX", "ac_q10"), ("ac_qlookup_12_QTX", "ac_q12")]
    body = ""
    for cname, fname in pairs:
        vals = extract(text, cname)
        assert len(vals) == 256
        body += static_block(fname.upper(), "u16", vals, fname + "_addr") + "\n"
    body += "fn dc_q_addr(bd_idx: i64) -> u64 {\n    if bd_idx == 1 {\n        return dc_q10_addr()\n    }\n    if bd_idx == 2 {\n        return dc_q12_addr()\n    }\n    return dc_q8_addr()\n}\n\n"
    body += "fn ac_q_addr(bd_idx: i64) -> u64 {\n    if bd_idx == 1 {\n        return ac_q10_addr()\n    }\n    if bd_idx == 2 {\n        return ac_q12_addr()\n    }\n    return ac_q8_addr()\n}\n"
    write(outdir, "av1tabq.fi", HDR + "export { ac_q_addr, dc_q_addr }\n\n" + body.replace("fn dc_q8_addr", "fn dc_q8_addr"))
    vals = extract(text, "iwt_matrix_ref")
    assert len(vals) == 15 * 2 * 3344
    for part in range(3):
        body = ""
        exports = []
        for lvl in range(part * 5, part * 5 + 5):
            chunk = vals[lvl * 6688:(lvl + 1) * 6688]
            fn = "qm_level%d_addr" % lvl
            exports.append(fn)
            body += static_block("QM_LEVEL%d" % lvl, "u8", chunk, fn) + "\n"
        write(outdir, "av1tabqm%d.fi" % part, HDR + "export { " + ", ".join(exports) + " }\n\n" + body)


def emit_tx(outdir, src):
    text = load(src, "av1_txfm.c")
    cos = extract(text, "av1_cospi_arr_data")
    sin = extract(text, "av1_sinpi_arr_data")
    assert len(cos) == 256 and len(sin) == 20
    body = static_block("COSPI_ARR", "i32", cos, "cospi_arr_addr") + "\n" + static_block("SINPI_ARR", "i32", sin, "sinpi_arr_addr")
    write(outdir, "av1tabtx.fi", HDR + "export { cospi_arr_addr, sinpi_arr_addr }\n\n" + body)


def emit_filter(outdir, src):
    text = load(src, "filter.h")
    order = ["av1_sub_pel_filters_8", "av1_sub_pel_filters_8smooth", "av1_sub_pel_filters_8sharp", "av1_bilinear_filters", "av1_sub_pel_filters_4", "av1_sub_pel_filters_4smooth"]
    vals = []
    for nm in order:
        v = extract(text, nm)
        assert len(v) == 128, (nm, len(v))
        vals.extend(v)
    ibc = extract(text, "av1_intrabc_bilinear_filter")
    assert len(ibc) == 32
    body = static_block("SUBPEL_FILTERS", "i16", vals, "subpel_filters_addr") + "\n" + static_block("INTRABC_BILINEAR", "i16", ibc, "intrabc_bilinear_addr")
    write(outdir, "av1tabfilt.fi", HDR + "export { intrabc_bilinear_addr, subpel_filters_addr }\n\n" + body)


def emit_intra(outdir, src):
    ri = load(src, "reconintra.h")
    rc = load(src, "reconintra.c")
    ic = load(src, "intrapred_common.h")
    dr = extract(ri, "dr_intra_derivative")
    assert len(dr) == 90
    ft = extract(rc, "av1_filter_intra_taps")
    assert len(ft) == 5 * 8 * 8
    sm = extract(ic, "smooth_weights")
    body = static_block("DR_INTRA_DERIVATIVE", "u16", dr, "dr_intra_derivative_addr") + "\n"
    body += static_block("FILTER_INTRA_TAPS", "i16", ft, "filter_intra_taps_addr") + "\n"
    body += static_block("SMOOTH_WEIGHTS", "u8", sm, "smooth_weights_addr")
    write(outdir, "av1tabintra.fi", HDR + "export { dr_intra_derivative_addr, filter_intra_taps_addr, smooth_weights_addr }\n\n" + body)
    return len(sm)


def main():
    outdir = sys.argv[1]
    os.makedirs(outdir, exist_ok=True)
    src = None
    for i, a in enumerate(sys.argv):
        if a == "--src":
            src = sys.argv[i + 1]
    tmp = None
    if src is None:
        tmp = tempfile.TemporaryDirectory()
        src = tmp.name
        fetch(src)
    nc, mv, coef, coef_q = build_tables(src)
    dn_total, coef_base, total = layout(nc, mv, coef)
    counters = []
    for tb in nc + mv + coef:
        counters.extend(tb.off + c for c in tb.counters())
    emit_idx(outdir, nc, mv, coef, dn_total, coef_base, total, counters)
    emit_defaults(outdir, nc, mv, coef_q, coef)
    emit_counters(outdir, counters)
    ns = emit_scan(outdir, src)
    emit_quant(outdir, src)
    emit_tx(outdir, src)
    emit_filter(outdir, src)
    nsm = emit_intra(outdir, src)
    print("tables nc=%d mv=%d coef=%d total_u16=%d dn=%d coef=%d counters=%d scans=%d smooth=%d" % (
        len(nc), len(mv), len(coef), total, dn_total, total - coef_base, len(counters), ns, nsm))


if __name__ == "__main__":
    main()
