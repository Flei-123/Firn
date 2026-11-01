import os
import re
import subprocess
import sys
import tempfile

KINDS = ("Sequence Header", "Frame Header", "Tile Group")
SKIP = ("trailing_one_bit", "trailing_zero_bit", "zero_bit")
LINE = re.compile(r"^\d+\s+(\S+)\s+([01]*)\s*= (-?\d+)\s*$")


def strip(name):
    return re.sub(r"\[.*?\]", "", name).split(".")[-1]


def ffmpeg_elements(path, fmt):
    cmd = ["ffmpeg", "-nostdin", "-hide_banner", "-v", "trace", "-f", fmt, "-i", path, "-c", "copy", "-bsf:v", "trace_headers", "-f", "null", "-"]
    p = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    out = []
    kind = None
    in_extra = False
    for raw in (p.stderr + p.stdout).splitlines():
        m = re.search(r"\[trace_headers @ [0-9a-fx]+\] (.*)$", raw)
        if not m:
            continue
        body = m.group(1)
        if body == "Extradata":
            in_extra = True
            kind = None
            continue
        if body.startswith("Packet:"):
            in_extra = False
            kind = None
            continue
        mm = LINE.match(body)
        if mm:
            if kind in KINDS and not in_extra and mm.group(2) and strip(mm.group(1)) not in SKIP:
                name, nb, val = strip(mm.group(1)), len(mm.group(2)), int(mm.group(3))
                if name == "tx_mode" and nb == 1:
                    name, val = "tx_mode_select", val - 1
                out.append((name, nb, val, kind))
            continue
        kind = body if body in KINDS or body in ("OBU header", "Temporal Delimiter") else None
    return out


def mine_elements(exe, path):
    d = tempfile.mkdtemp()
    o = os.path.join(d, "o.txt")
    try:
        r = subprocess.run([exe, path, o], capture_output=True, timeout=300)
        out = []
        if os.path.exists(o):
            for line in open(o):
                line = line.rstrip("\n")
                if not line or line[0] in "@#":
                    continue
                n, b, v = line.rsplit(" ", 2)
                if int(b) > 0:
                    out.append((strip(n), int(b), int(v)))
        return r.returncode, out
    finally:
        for x in os.listdir(d):
            os.remove(os.path.join(d, x))
        os.rmdir(d)


def main():
    exe = sys.argv[1]
    bad = 0
    for path in sys.argv[2:]:
        fmt = "ivf" if path.endswith(".ivf") else "obu"
        ref = ffmpeg_elements(path, fmt)
        rc, mine = mine_elements(exe, path)
        a = [(x[0], x[1], x[2]) for x in ref]
        status = "OK"
        if rc != 0:
            status = "FAIL rc=%d" % rc
        if a != mine:
            status = "FAIL"
            n = min(len(a), len(mine))
            i = next((k for k in range(n) if a[k] != mine[k]), n)
            print("  first diff at %d: ffmpeg=%s mine=%s" % (i, ref[i] if i < len(ref) else None, mine[i] if i < len(mine) else None))
            print("  context:", ref[max(0, i - 3):i])
        if status != "OK":
            bad += 1
        print("%s %s elements=%d/%d" % (status, os.path.basename(path), len(a), len(mine)))
    sys.exit(1 if bad else 0)


main()
