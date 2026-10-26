import os, random, struct, subprocess, sys, tempfile

BIN = "/root/jarvis/projects/u_DiS4in7esMF1/firncodec/tools/libjxl/bin"
out_dir = sys.argv[1]
test_path = sys.argv[2]
random.seed(5)
P = 1000000007


def write_int(path, w, h, d, maxv):
    px = []
    for y in range(h):
        for x in range(w):
            for c in range(d):
                px.append(((x * maxv // max(1, w - 1)) + (y * 37) * (c + 1) + random.randrange(40) + c * 900) % (maxv + 1))
    with open(path, "wb") as f:
        if d == 4:
            f.write(f"P7\nWIDTH {w}\nHEIGHT {h}\nDEPTH 4\nMAXVAL {maxv}\nTUPLTYPE RGB_ALPHA\nENDHDR\n".encode())
        else:
            f.write(f"P{6 if d == 3 else 5}\n{w} {h}\n{maxv}\n".encode())
        for v in px:
            f.write(struct.pack(">H", v))


def write_float(path, w, h, d):
    with open(path, "wb") as f:
        f.write(f"{'PF' if d == 3 else 'Pf'}\n{w} {h}\n-1.0\n".encode())
        for y in range(h):
            for x in range(w):
                for c in range(d):
                    f.write(struct.pack("<f", (x / w) * (c + 1) / 3 + random.random() * 0.01 + y * 0.001))


def read_pam(path):
    data = open(path, "rb").read()
    hdr, rest = data.split(b"ENDHDR\n", 1)
    f = dict(l.split() for l in hdr.decode().split("\n")[1:] if l)
    w, h, d, mv = int(f["WIDTH"]), int(f["HEIGHT"]), int(f["DEPTH"]), int(f["MAXVAL"])
    if mv > 255:
        vals = struct.unpack(">%dH" % (w * h * d), rest)
    else:
        vals = list(rest)
    return w, h, d, vals


def read_pfm(path):
    data = open(path, "rb").read()
    parts = data.split(b"\n", 3)
    d = 3 if parts[0] == b"PF" else 1
    w, h = map(int, parts[1].split())
    scale = float(parts[2])
    fmt = (">" if scale > 0 else "<") + "%dI" % (w * h * d)
    vals = struct.unpack(fmt, parts[3][: w * h * d * 4])
    rows = [vals[y * w * d:(y + 1) * w * d] for y in range(h)]
    flat = []
    for r in reversed(rows):
        flat.extend(r)
    return w, h, d, flat


def sample_hash(vals):
    s = 0
    for v in vals:
        s = (s * 31 + v) % P
    return s


cases = [
    ("rgb16", "ppm", (70, 50, 3, 65535)),
    ("grey16", "pgm", (64, 40, 1, 65535)),
    ("rgba16", "pam", (50, 40, 4, 65535)),
    ("rgb12", "ppm", (60, 40, 3, 4095)),
    ("rgb10", "ppm", (60, 40, 3, 1023)),
    ("f3", "pfm", (48, 32, 3)),
    ("f1", "pfm", (48, 32, 1)),
]
os.makedirs(out_dir, exist_ok=True)
rows = []
with tempfile.TemporaryDirectory() as t:
    for case in cases:
        name, ext, args = case[0], case[1], case[2]
        extra = case[3] if len(case) > 3 else []
        src = f"{t}/{name}.{ext}"
        if ext == "pfm":
            write_float(src, *args)
        else:
            write_int(src, *args)
        jxl = f"{out_dir}/d_{name}.jxl"
        subprocess.run([f"{BIN}/cjxl", src, jxl, "-d", "0", "-e", "3"] + extra, check=True, capture_output=True)
        if ext == "pfm":
            ref = f"{t}/{name}.pfm.out"
            subprocess.run([f"{BIN}/djxl", jxl, ref + ".pfm"], check=True, capture_output=True)
            w, h, d, vals = read_pfm(ref + ".pfm")
            rows.append((name, w, h, d, 4, 16 if extra else 32, sample_hash(vals)))
        else:
            ref = f"{t}/{name}.out.pam"
            subprocess.run([f"{BIN}/djxl", jxl, ref], check=True, capture_output=True)
            w, h, d, vals = read_pam(ref)
            mv = args[3]
            bits = mv.bit_length()
            rows.append((name, w, h, d, 2, bits, sample_hash(vals)))

lines = ["import jxl.jxl", "import jxl.jxl_float", "import std.fs", "import std.rt", "", "static mut failures: i32 = 0", "static mut first_bad: i32 = 0 - 1",
         "static mut index: i32 = 0", "", """fn expect_float(raw: i64, bits: u32, exp_bits: u32, want: u64) {
    if jxl_float.int_to_float_bits(raw, bits, exp_bits) != want {
        failures = failures + 1
    }
}

fn sample(pic: *mut jxl.JxlPicture, i: usize) -> u64 {
    let at: usize = i * ((*pic).bytes_per_sample as usize)
    var v: u64 = 0
    var k: usize = 0
    while k < ((*pic).bytes_per_sample as usize) {
        v = v | ((rt.ld8((*pic).data, at + k) as u64) << ((k * 8) as u64))
        k = k + 1
    }
    return v
}

fn expect(name: str, w: u32, h: u32, ch: u32, bps: u32, bits: u32, want: u64) {
    var data: rt.Buf = rt.buf_new()
    let n: usize = fs.read_file(name, &data) catch 0
    var pic: jxl.JxlPicture = jxl.jxl_picture_new()
    let ok: bool = jxl.jxl_decode(rt.buf_ptr(&data), n, &pic) catch false
    var s: u64 = 0
    var good: bool = ok && pic.width == w && pic.height == h && pic.channels == ch && pic.bytes_per_sample == bps && pic.bits == bits
    if good {
        var i: usize = 0
        while i < ((w * h * ch) as usize) {
            s = (s * 31 + sample(&pic, i)) % 1000000007
            i = i + 1
        }
        good = s == want
    }
    if !good {
        failures = failures + 1
        if first_bad < 0 {
            first_bad = index
        }
    }
    index = index + 1
    jxl.jxl_picture_free(&pic)
    rt.buf_free(&data)
}

fn main() -> i32 {"""]
for name, w, h, d, bps, bits, hv in rows:
    lines.append(f'    expect("tests/data/jxl/depth/d_{name}.jxl", {w}, {h}, {d}, {bps}, {bits}, {hv})')
half_raw = [0, 0x8000, 0x3c00, 0xbc00, 1, 0x3ff, 0x400, 0x7bff, 0xfbff, 0x3555, 0x5640, 0x0155, 0x8001, 0x2e66]
for raw in half_raw:
    bits = struct.unpack("<I", struct.pack("<f", struct.unpack("<e", struct.pack("<H", raw))[0]))[0]
    lines.append(f"    expect_float({raw}, 16, 5, {bits})")
for raw in [0x3f80, 0xbf80, 0x0001, 0x4049, 0x7f7f, 0x8000]:
    lines.append(f"    expect_float({raw}, 16, 8, {raw << 16})")
lines += ["    return failures", "}", ""]
open(test_path, "w").write("\n".join(lines))
