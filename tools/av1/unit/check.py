#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/av1/check.py <avif_main binary> <dir> [--fuzz N] -- the AVIF container/sequence-header parser against Pillow (libavif).

Per file: size, alpha plane, chroma format (from the file name s*_420/_444/_422/_400) and the 8-bit depth must match;
then --fuzz N: N truncations / bit flips per file must end with exit 0 or 1 (never a signal).  Exit 0 when all pass."""
import glob, os, random, re, subprocess, sys
from PIL import Image

def run(binary, data):
    r = subprocess.run([binary], input=data, capture_output=True, timeout=30)
    kv = dict(l.split(b"=", 1) for l in r.stdout.split(b"\n") if b"=" in l)
    return r.returncode, {k.decode(): int(v) for k, v in kv.items()}

def trace(path):
    """ffmpeg trace_headers of the first frame header: name -> int of the LAST occurrence (indexed names keep [i])."""
    r = subprocess.run(["ffmpeg", "-hide_banner", "-v", "trace", "-i", path, "-c", "copy", "-bsf:v", "trace_headers", "-f", "null", "-"],
                       capture_output=True, text=True, timeout=60)
    out = {}
    on = False
    for l in r.stderr.splitlines():
        l = l.split("] ", 1)[-1]
        if l.startswith("Frame Header") or l.startswith("Frame\n") or l == "Frame":
            on = True
            continue
        if l.startswith("Tile Group") or l.startswith("Frame 0"):
            on = False
        m = re.match(r"\d+\s+(\S+)\s+[01]*\s*= (-?\d+)", l)
        if on and m and m.group(1) not in out:
            out[m.group(1)] = int(m.group(2))
    return out

def cmp_trace(kv, tr):
    errs = []
    want = {"base_q": "base_q_idx", "lf0": "loop_filter_level[0]", "lf1": "loop_filter_level[1]", "lf_sharp": "loop_filter_sharpness",
            "cdef_bits": "cdef_bits", "seg_en": "segmentation_enabled", "qm": "using_qmatrix", "dq_pres": "delta_q_present",
            "dq_res": "delta_q_res", "dis_cdf": "disable_cdf_update", "sct": "allow_screen_content_tools", "intrabc": "allow_intrabc",
            "tcols_log2": "tile_cols_log2", "trows_log2": "tile_rows_log2", "red_tx": "reduced_tx_set", "cdef_yp0": "cdef_y_pri_strength[0]",
            "lr0": None}
    for k, tn in want.items():
        if tn is None or tn not in tr:
            if tn is not None and k in ("lf0", "lf1", "cdef_bits", "cdef_yp0") and kv[k] != 0: errs.append("%s=%d (no trace)" % (k, kv[k]))
            continue
        if kv[k] != tr[tn]: errs.append("%s %d vs trace %d" % (k, kv[k], tr[tn]))
    if "cdef_damping_minus_3" in tr and kv["cdef_damp"] != tr["cdef_damping_minus_3"] + 3: errs.append("cdef_damp")
    if "tx_mode" in tr and kv["tx_mode"] != tr["tx_mode"]: errs.append("tx_mode %d vs %d" % (kv["tx_mode"], tr["tx_mode"]))
    if "lr_unit_shift" in tr and kv["lr_shift"] != tr["lr_unit_shift"]: errs.append("lr_shift %d vs %d" % (kv["lr_shift"], tr["lr_unit_shift"]))
    if "lr_type[0]" in tr and kv["lr0"] != {0: 0, 1: 3, 2: 1, 3: 2}[tr["lr_type[0]"]]: errs.append("lr0 %d vs raw %d" % (kv["lr0"], tr["lr_type[0]"]))
    for k, tn in (("dq_udc", "delta_q_u_dc.delta_q"), ("dq_uac", "delta_q_u_ac.delta_q")):
        if tn in tr and kv[k] - 128 != tr[tn]: errs.append("%s %d vs %d" % (k, kv[k] - 128, tr[tn]))
    return errs

def main():
    binary, d = sys.argv[1], sys.argv[2]
    nfuzz = int(sys.argv[sys.argv.index("--fuzz") + 1]) if "--fuzz" in sys.argv else 0
    bad = 0
    files = sorted(glob.glob(os.path.join(d, "*.avif")))
    for f in files:
        data = open(f, "rb").read()
        im = Image.open(f)
        rc, kv = run(binary, data)
        name = os.path.basename(f)
        errs = []
        if rc != 0:
            errs.append("rc=%d" % rc)
        else:
            if (kv["ispe_w"], kv["ispe_h"]) != im.size: errs.append("ispe %s vs %s" % ((kv["ispe_w"], kv["ispe_h"]), im.size))
            if (kv["seq_maxw"], kv["seq_maxh"]) != im.size: errs.append("seq size %s" % ((kv["seq_maxw"], kv["seq_maxh"]),))
            if kv["depth"] != 8: errs.append("depth %d" % kv["depth"])
            if (kv["alpha_len"] > 0) != ("A" in im.mode): errs.append("alpha %d vs %s" % (kv["alpha_len"], im.mode))
            m = re.search(r"_(420|422|444|400)", name)
            if m:
                want = {"420": (0, 1, 1), "422": (0, 1, 0), "444": (0, 0, 0), "400": (1, 1, 1)}[m.group(1)]
                if (kv["mono"], kv["subx"], kv["suby"]) != want: errs.append("chroma %s vs %s" % ((kv["mono"], kv["subx"], kv["suby"]), want))
            if not kv["fh_ok"]: errs.append("frame header not parsed")
            errs += cmp_trace(kv, trace(f))
            if kv["obu_seq"] != 1 or kv["obu_frame"] < 1 or kv["obu_tile"] < 1: errs.append("obus %s" % kv)
        print(("FAIL " if errs else "ok   ") + name + " " + "; ".join(errs))
        bad += bool(errs)
        random.seed(hash(name) & 0xffff)
        for i in range(nfuzz):
            b = bytearray(data)
            k = i % 3
            if k == 0: b = b[:random.randrange(len(b))]
            elif k == 1:
                for _ in range(random.randrange(1, 6)): b[random.randrange(min(len(b), 400))] ^= 1 << random.randrange(8)
            else:
                for _ in range(3): b[random.randrange(len(b))] = random.randrange(256)
            try:
                rc, _ = run(binary, bytes(b))
            except subprocess.TimeoutExpired:
                rc = -999
            if rc not in (0, 1):
                print("FUZZ CRASH rc=%d in %s case %d" % (rc, name, i)); bad += 1; break
    print("%d files, %d problems" % (len(files), bad))
    sys.exit(1 if bad else 0)
main()
