#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/f32data/gen.py -- the test data of round 71, produced instead of
checked in.

Two file formats that carry 32-bit floats, and both of them for a reason:

  testdata/f32/tone.wav     WAV with 32-bit float PCM (format tag 3). The
                            format in which every audio program of the last
                            twenty years hands sound around.
  testdata/f32/tri.glb      binary glTF. Vertex positions are float32 VEC3
                            in the BIN chunk -- that is how a 3D model
                            travels between two programs.

Next to them the file `expected.txt` holds, for every value that the Firn
probe reads, the DECIMAL TEXT that Python produces from the very same four
octets. That is the yardstick: Firn is not compared against its own idea of
what should be in the file, but against what a second implementation reads
out of the same octets.

Usage:  tools/f32data/gen.py <target directory>
"""

import math
import os
import struct
import sys

# The samples of the WAV. Deliberately not round numbers: values whose
# shortest decimal really needs the last digit, plus the edges of the format.
SAMPLE_COUNT = 64
SAMPLE_RATE = 48000

# The vertices of the glTF model, as float32 VEC3. A triangle plus the two
# smallest positive values and the largest one, so that the reader has to
# survive subnormals and the top of the range.
VERTICES = [
    (0.0, 0.0, 0.0),
    (1.0, 0.0, 0.0),
    (0.0, 1.0, 0.0),
    (0.1, 0.2, 0.3),
    (-1.5, 2.25, -0.125),
    (3.4028234663852886e38, 1.1754943508222875e-38, 1.401298464324817e-45),
    (1.0 / 3.0, 2.0 / 3.0, 1.0e-20),
    (16777216.0, 16777217.0, 123456792.0),
]


def samples():
    out = []
    for i in range(SAMPLE_COUNT):
        # a sine with an amplitude that is not exactly representable, so
        # that nearly every sample has a long decimal
        out.append(0.37 * math.sin(2.0 * math.pi * 440.0 * i / SAMPLE_RATE))
    # the edges after it
    out[0] = 0.0
    out[1] = -0.0
    out[2] = 1.0
    out[3] = -1.0
    out[4] = 3.4028234663852886e38
    out[5] = 1.401298464324817e-45
    out[6] = 1.1754943508222875e-38
    out[7] = 0.1
    return out


def single(x):
    """The value as it survives in a binary32."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


def shortest(x):
    """The shortest decimal that reads back as this binary32 -- in the same
    shape that `dtoa`/`ftoa32` writes it (ECMAScript rules)."""
    value = single(x)
    if value != value:
        return "NaN"
    if value == float("inf"):
        return "Infinity"
    if value == float("-inf"):
        return "-Infinity"
    if value == 0.0:
        return "0"
    for digits in range(1, 10):
        text = "%.*e" % (digits - 1, value)
        try:
            if single(float(text)) == value:
                break
        except OverflowError:
            continue
    mantissa, exponent = text.split("e")
    exponent = int(exponent)
    negative = mantissa.startswith("-")
    if negative:
        mantissa = mantissa[1:]
    digit_string = mantissa.replace(".", "").rstrip("0") or "0"
    k = len(digit_string)
    n = exponent + 1
    if k <= n <= 21:
        body = digit_string + "0" * (n - k)
    elif 0 < n <= 21:
        body = digit_string[:n] + "." + digit_string[n:]
    elif -6 < n <= 0:
        body = "0." + "0" * (-n) + digit_string
    else:
        body = digit_string[0]
        if k > 1:
            body = body + "." + digit_string[1:]
        body = body + "e" + ("+" if n - 1 >= 0 else "") + str(n - 1)
    return ("-" if negative else "") + body


def write_wav(path, values):
    data = b"".join(struct.pack("<f", v) for v in values)
    fmt = struct.pack("<HHIIHH", 3, 1, SAMPLE_RATE, SAMPLE_RATE * 4, 4, 32)
    body = (b"WAVE"
            + b"fmt " + struct.pack("<I", len(fmt)) + fmt
            + b"data" + struct.pack("<I", len(data)) + data)
    with open(path, "wb") as handle:
        handle.write(b"RIFF" + struct.pack("<I", len(body)) + body)


def write_glb(path, vertices):
    binary = b"".join(struct.pack("<fff", *v) for v in vertices)
    while len(binary) % 4 != 0:
        binary += b"\x00"
    json_text = (
        '{"asset":{"version":"2.0"},'
        '"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],'
        '"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],'
        '"accessors":[{"bufferView":0,"componentType":5126,"count":%d,'
        '"type":"VEC3"}],'
        '"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":%d}],'
        '"buffers":[{"byteLength":%d}]}' % (len(vertices), len(binary),
                                            len(binary))
    ).encode()
    while len(json_text) % 4 != 0:
        json_text += b" "
    total = 12 + 8 + len(json_text) + 8 + len(binary)
    with open(path, "wb") as handle:
        handle.write(b"glTF" + struct.pack("<II", 2, total))
        handle.write(struct.pack("<I", len(json_text)) + b"JSON" + json_text)
        handle.write(struct.pack("<I", len(binary)) + b"BIN\x00" + binary)


def main():
    target = sys.argv[1] if len(sys.argv) > 1 else "testdata/f32"
    os.makedirs(target, exist_ok=True)
    values = samples()
    write_wav(os.path.join(target, "tone.wav"), values)
    write_glb(os.path.join(target, "tri.glb"), VERTICES)

    lines = []
    for v in values:
        lines.append(shortest(v))
    with open(os.path.join(target, "wav_expected.txt"), "w") as handle:
        handle.write("\n".join(lines) + "\n")

    flat = [c for vertex in VERTICES for c in vertex]
    lines = [shortest(v) for v in flat]
    with open(os.path.join(target, "glb_expected.txt"), "w") as handle:
        handle.write("\n".join(lines) + "\n")

    print("   wav samples      %6d" % len(values))
    print("   glb coordinates  %6d" % len(flat))


if __name__ == "__main__":
    main()
