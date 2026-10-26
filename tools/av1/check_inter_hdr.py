#!/usr/bin/env python3
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FIRNC = os.environ.get("FIRNC", "/root/firn/compiler/target/release/firnc")

VARIANTS = {
    "lag0": ("testsrc2=s=128x96:r=10", 6, "-lag-in-frames 0", ""),
    "lag8": ("testsrc2=s=160x96:r=10", 12, "-lag-in-frames 8", ""),
    "errres": ("testsrc2=s=128x96:r=10", 6, "-lag-in-frames 0", "error-resilient=1"),
    "noorder": ("testsrc2=s=128x96:r=10", 6, "-lag-in-frames 0", "enable-order-hint=0"),
    "grain": ("testsrc2=s=128x96:r=10", 6, "-lag-in-frames 0", "film-grain-test=3"),
    "aq": ("mandelbrot=s=160x96:r=10", 6, "-lag-in-frames 0", "aq-mode=1"),
    "gmv": ("mandelbrot=s=160x96:r=10", 8, "-lag-in-frames 4", "enable-global-motion=1:enable-warped-motion=1"),
    "mono": ("testsrc2=s=96x96:r=10,format=gray", 5, "-lag-in-frames 0", ""),
}


def trace(path):
    out = subprocess.run(["ffmpeg", "-v", "debug", "-i", path, "-c", "copy", "-bsf:v", "trace_headers", "-f", "null", "-"], stderr=subprocess.PIPE, stdout=subprocess.PIPE).stderr.decode(errors="replace")
    frames = []
    cur = None
    for ln in out.split("\n"):
        m = re.search(r"trace_headers @ \S+\] (\d+)\s+(\S+)\s+[01]* = (-?\d+)", ln)
        if not m:
            continue
        pos, name, val = int(m.group(1)), m.group(2), int(m.group(3))
        if name == "show_existing_frame":
            cur = {"start": pos, "se": val, "refs": [None] * 7, "end": None}
            frames.append(cur)
        elif cur is not None:
            if name == "frame_type":
                cur["type"] = val
            elif name == "primary_ref_frame":
                cur["prim"] = val
            elif name == "refresh_frame_flags":
                cur["refresh"] = val
            elif name == "order_hint":
                cur["hint"] = val
            elif name.startswith("ref_frame_idx["):
                cur["refs"][int(name[14])] = val
            elif name == "reference_select":
                cur["rsel"] = val
            elif name == "skip_mode_present":
                cur["skip"] = val
            elif name == "allow_warped_motion":
                cur["warp"] = val
            elif name == "use_ref_frame_mvs":
                cur["urm"] = val
            elif name == "allow_high_precision_mv":
                cur["hp"] = val
            elif name in ("zero_bit", "trailing_zero_bit") and cur["end"] is None:
                cur["end"] = pos - cur["start"]
    return frames


def firn(dump, path):
    out = subprocess.run([dump], stdin=open(path, "rb"), stdout=subprocess.PIPE).stdout.decode()
    res = []
    for ln in out.strip().split("\n"):
        d = dict(kv.split("=") for kv in ln.split())
        res.append(d)
    return res


def main():
    tmp = tempfile.mkdtemp()
    dump = os.path.join(tmp, "hd")
    r = subprocess.run([FIRNC, os.path.join(ROOT, "tools/av1/hdrdump.fi"), "-o", dump], env=dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib")), stderr=subprocess.PIPE)
    if r.returncode:
        print(r.stderr.decode()[:500])
        return 1
    bad = 0
    for name, (src, n, lag, params) in VARIANTS.items():
        path = os.path.join(tmp, name + ".obu")
        cmd = ["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", src, "-frames:v", str(n), "-c:v", "libaom-av1"] + lag.split() + ["-cpu-used", "8", "-crf", "32"]
        if params:
            cmd += ["-aom-params", params]
        cmd += ["-f", "obu", path]
        if subprocess.run(cmd).returncode:
            print(name, "ENCODE FAILED")
            bad += 1
            continue
        t = trace(path)
        f = firn(dump, path)
        if len(t) != len(f):
            print(name, "FRAME COUNT", len(t), len(f))
            bad += 1
            continue
        nbad = 0
        for i, (a, b) in enumerate(zip(t, f)):
            if b["ok"] != "1":
                nbad += 1
                print(name, i, "header refused")
                continue
            if a["se"]:
                continue
            checks = [("type", a["type"], int(b["type"])), ("prim", a["prim"] if "prim" in a else 7, int(b["prim"])), ("refresh", a.get("refresh", 255), int(b["refresh"])), ("hdrbits", a["end"], int(b["hdrbits"]))]
            if a["type"] == 1:
                checks.append(("refs", a["refs"], [int(ch) for ch in b["refs"]]))
                for k, key in (("rsel", "rsel"), ("skip", "skip"), ("warp", "warp"), ("urm", "urm"), ("hp", "hp")):
                    if k in a:
                        checks.append((k, a[k], int(b[key])))
            for k, x, y in checks:
                if x is None:
                    continue
                if x != y:
                    nbad += 1
                    print(name, i, k, "ref", x, "firn", y)
        print(name, "frames", len(t), "BAD" if nbad else "ok")
        bad += nbad
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
