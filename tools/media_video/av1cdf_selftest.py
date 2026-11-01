import os
import re
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_av1_tables as g

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
FAILS = []


def check(cond, msg):
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


def wsum(vals):
    return sum(v * ((i % 251) + 1) for i, v in enumerate(vals))


def region_args(text, name):
    m = re.search(r"aom_cdf_prob\s+" + name + r"((?:\s*\[[^\]]*\])+)\s*=\s*\{", text)
    end = g.match_brace(text, m.end() - 1)
    out = []
    for mm in re.finditer(r"AOM_CDF(\d+)\(((?:[^()]|\([^()]*\))*)\)", text[m.end():end]):
        out.append((int(mm.group(1)), [g.safe_eval(a) for a in mm.group(2).split(",") if a.strip()]))
    return out


def source_cross_check(src, nc, coef_q):
    t = g.load(src, "entropymode.c")
    for name, _ in g.cdf_arrays(t):
        tb = [x for x in nc if x.name == g.short(name)][0]
        flat = region_args(t, name)
        got = [(n, a) for n, a in tb.leaf_list if n]
        check(flat == got, "entropymode " + name)
    tk = g.load(src, "token_cdfs.h")
    for idx, (name, _) in enumerate(g.cdf_arrays(tk)):
        flat = region_args(tk, name)
        got = []
        for q in range(4):
            got.extend(coef_q[q][idx])
        check(flat == got, "token_cdfs " + name)


def spec_constants(nc, mv):
    by = {x.name: x for x in nc + mv}
    check(by["INTRABC"].flat()[:3] == [30531, 32768, 0], "intrabc")
    check(by["MV_JOINTS"].flat()[:5] == [4096, 11264, 19328, 32768, 0], "mv joints")
    check(by["MV_SIGN"].flat()[:3] == [16384, 32768, 0], "mv sign")
    check(by["MV_CLASS0_HP"].flat()[0] == 160 * 128, "mv class0 hp")
    check(by["MV_BITS"].dims == [2, 2, 10], "mv bits dims")
    check(by["KF_Y_MODE"].dims == [5, 5] and by["KF_Y_MODE"].leaf == 14, "kf y mode shape")
    check(by["PARTITION"].dims == [20] and by["PARTITION"].leaf == 11, "partition shape")
    check(by["SKIP_TXFM"].dims == [3], "skip shape")
    check(by["INTRA_EXT_TX"].dims == [3, 4, 13] and by["INTRA_EXT_TX"].leaf == 8 and by["INTER_EXT_TX"].leaf == 17, "intra ext tx shape")
    check(by["TX_SIZE"].dims == [4, 3] and by["TX_SIZE"].leaf == 4, "tx size shape")
    check(by["PALETTE_Y_COLOR_INDEX"].leaf == 9, "palette color leaf")


def expected_lines(src, nc, mv, coef, coef_q):
    exp = {}
    dn = []
    for tb in nc + mv:
        dn.extend(tb.flat())
    for q, qi in enumerate([0, 1, 2, 3]):
        blk = list(dn)
        for idx, tb in enumerate(coef):
            blk.extend(g.Table(tb.name, tb.dims, coef_q[qi][idx]).flat())
        exp[("all_q", q)] = wsum(blk)
    text = g.load(src, "scan.c")
    pairs = re.findall(r"\{\s*(\w+),\s*(\w+)\s*\}", text[text.index("av1_scan_orders"):])
    check(len(pairs) >= 304, "scan order entries %d" % len(pairs))
    for tx in range(19):
        for ty in range(16):
            nm = pairs[tx * 16 + ty][0]
            exp[("scanfor", tx * 100 + ty)] = wsum(g.extract(text, nm)[:16])
    qt = g.load(src, "quant_common.c")
    for d, (dc, ac) in enumerate([("dc_qlookup_QTX", "ac_qlookup_QTX"), ("dc_qlookup_10_QTX", "ac_qlookup_10_QTX"), ("dc_qlookup_12_QTX", "ac_qlookup_12_QTX")]):
        exp[("dcq", d)] = wsum(g.extract(qt, dc))
        exp[("acq", d)] = wsum(g.extract(qt, ac))
    qm = g.extract(qt, "iwt_matrix_ref")
    for lvl in (0, 7, 14):
        exp[("qm%d" % lvl, 0)] = wsum(qm[lvl * 6688:(lvl + 1) * 6688])
    tx = g.load(src, "av1_txfm.c")
    exp[("cospi", 0)] = wsum(g.extract(tx, "av1_cospi_arr_data"))
    exp[("sinpi", 0)] = wsum(g.extract(tx, "av1_sinpi_arr_data"))
    ft = g.load(src, "filter.h")
    sub = []
    for nm in ["av1_sub_pel_filters_8", "av1_sub_pel_filters_8smooth", "av1_sub_pel_filters_8sharp", "av1_bilinear_filters", "av1_sub_pel_filters_4", "av1_sub_pel_filters_4smooth"]:
        sub.extend(g.extract(ft, nm))
    exp[("subpel", 0)] = wsum(sub)
    exp[("ibc_bilinear", 0)] = wsum(g.extract(ft, "av1_intrabc_bilinear_filter"))
    exp[("dr_deriv", 0)] = wsum(g.extract(g.load(src, "reconintra.h"), "dr_intra_derivative"))
    exp[("filter_intra", 0)] = wsum(g.extract(g.load(src, "reconintra.c"), "av1_filter_intra_taps"))
    exp[("smooth", 0)] = wsum(g.extract(g.load(src, "intrapred_common.h"), "smooth_weights"))
    exp[("intrabc", 0)] = 30531
    exp[("intrabc1", 0)] = 32768
    exp[("intrabc2", 0)] = 0
    exp[("mv_joints0", 0)] = 19328
    exp[("mv_joints_ctx1", 0)] = 19328
    exp[("mv_sign", 0)] = 16384
    exp[("counters_cleared", 0)] = 0
    exp[("state_frame_intrabc", 0)] = 123
    exp[("state_frame_counter", 0)] = 0
    exp[("state_reset", 0)] = 30531
    exp[("state_load_ok", 0)] = 1
    exp[("state_loaded", 0)] = 123
    exp[("state_missing_slot", 0)] = 1
    return exp


def main():
    firnc = sys.argv[1]
    src = None
    for i, a in enumerate(sys.argv):
        if a == "--src":
            src = sys.argv[i + 1]
    tmp = tempfile.TemporaryDirectory()
    if src is None:
        src = os.path.join(tmp.name, "src")
        os.makedirs(src)
        g.fetch(src)
    nc, mv, coef, coef_q = g.build_tables(src)
    g.layout(nc, mv, coef)
    source_cross_check(src, nc, coef_q)
    spec_constants(nc, mv)
    regen = os.path.join(tmp.name, "regen")
    subprocess.run([sys.executable, "-I", os.path.join(ROOT, "tools/media_video/gen_av1_tables.py"), regen, "--src", src], check=True, capture_output=True)
    for f in sorted(os.listdir(regen)):
        a = open(os.path.join(regen, f)).read()
        b = open(os.path.join(ROOT, "lib/media/video/av1", f)).read()
        check(a == b, "generated file out of date: " + f)
    exe = os.path.join(tmp.name, "cdftest")
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    r = subprocess.run([firnc, "-o", exe, os.path.join(ROOT, "tools/media_video/av1cdftest.fi")], env=env, capture_output=True, text=True)
    check(r.returncode == 0, "compile av1cdftest: " + r.stdout[-400:] + r.stderr[-400:])
    if r.returncode == 0:
        outp = os.path.join(tmp.name, "out.txt")
        r = subprocess.run([exe, outp], capture_output=True, text=True)
        check(r.returncode == 0, "run av1cdftest rc=%d" % r.returncode)
        got = {}
        for ln in open(outp).read().split("\n"):
            if ln:
                k, i, v = ln.split()
                got[(k, int(i))] = int(v)
        exp = expected_lines(src, nc, mv, coef, coef_q)
        for key, v in exp.items():
            check(got.get(key) == v, "value %s expected %s got %s" % (key, v, got.get(key)))
        print("checked", len(exp), "values")
    print("av1cdf selftest:", "FAIL %d" % len(FAILS) if FAILS else "ok")
    sys.exit(1 if FAILS else 0)


main()
