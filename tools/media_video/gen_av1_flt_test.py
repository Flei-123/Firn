import subprocess
import sys

CLIPS = [
    ("cdef", "testsrc2", "128x96", "yuv420p", "enable-palette=0:enable-intrabc=0:enable-restoration=0", ["-crf", "30"], []),
    ("sgr", "testsrc2", "128x96", "yuv420p", "enable-palette=0:enable-intrabc=0:enable-cdef=0", ["-cpu-used", "2", "-crf", "30"], []),
    ("wiener", "testsrc2", "320x240", "yuv420p", "enable-palette=0:enable-intrabc=0", ["-cpu-used", "1", "-crf", "50"], ["-vf", "gblur=sigma=1,noise=alls=8"]),
    ("c444", "testsrc2", "96x64", "yuv444p", "enable-palette=0:enable-intrabc=0", ["-cpu-used", "2", "-crf", "30"], []),
]


def hash_bytes(data):
    h = 0
    for b in data:
        h = (h * 31 + b) & 2147483647
    return h


def encode(workdir, name, src, size, pix, params, enc, vf):
    ivf = f"{workdir}/{name}.ivf"
    ref = f"{workdir}/{name}.ref"
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-f", "lavfi", "-i", f"{src}=size={size}:rate=25", "-frames:v", "1", "-pix_fmt", pix, "-c:v", "libaom-av1", "-g", "1", "-cpu-used", "8", "-aom-params", params] + enc + vf + [ivf], check=True)
    subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-c:v", "libdav1d", "-i", ivf, "-f", "rawvideo", "-pix_fmt", pix, ref], check=True)
    return open(ivf, "rb").read(), open(ref, "rb").read()


def fmt_array(name, ty, values):
    lines = []
    for i in range(0, len(values), 24):
        lines.append("    " + ", ".join(str(v) for v in values[i:i + 24]))
    return f"static {name}: [{ty}; {len(values)}] = [\n" + ",\n".join(lines) + "\n]\n"


def main():
    workdir = sys.argv[1]
    blob = []
    offs = [0]
    hashes = []
    lens = []
    for name, src, size, pix, params, enc, vf in CLIPS:
        ivf, ref = encode(workdir, name, src, size, pix, params, enc, vf)
        blob += list(ivf)
        offs.append(len(blob))
        hashes.append(hash_bytes(ref))
        lens.append(len(ref))
    out = "// expect_exit: 0\nimport std.rt\nimport media.video.vp9.vp9mem\nimport media.video.av1.av1dec\n\n"
    out += fmt_array("CLIPS", "u8", blob) + "\n"
    out += fmt_array("OFFSETS", "i64", offs) + "\n"
    out += fmt_array("HASHES", "i64", hashes) + "\n"
    out += fmt_array("LENGTHS", "i64", lens) + "\n"
    out += open(sys.argv[2]).read()
    open(sys.argv[3], "w").write(out)


main()
