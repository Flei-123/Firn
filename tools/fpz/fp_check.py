#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Round B6, chapter Z: does the defence against fingerprinting do what it
claims, and can it be SEEN to do it?

WHAT IS BEING MEASURED, AND WHY EACH NUMBER IS THERE

A defence like this has two failure modes and they pull in opposite
directions:

  * it does nothing.  Two sites read the same canvas and get the same
    octets, so the reading still identifies the machine.  The measurement
    for that is the number of DISTINCT results over many origins -- and it
    is worth nothing on its own, because a program that returned random
    noise every time would score perfectly.  So there is a counter-check
    (`op 4`, the same path with the farbling taken out) which MUST give
    exactly ONE distinct result.  Without that line the first number only
    proves that the harness works.

  * it does too much.  A reading that differs from itself is worse than no
    defence: a script reads twice, sees the difference, averages it away
    and knows it is being lied to.  So: the same session and the same
    origin must give BYTE-IDENTICAL answers, twenty times over; and the
    picture must still be the picture -- the largest deviation of any
    colour channel is reported, not a mean, and the alpha channel is
    checked separately because a flipped alpha bit is visible.

Everything here talks to `build/fpz` over a pipe.  This file computes the
hashes, the deviations and the counts itself; nothing is taken on trust
from the program under test.

Usage:  python3 tools/fpz/fp_check.py <binary>
"""

import collections
import hashlib
import struct
import subprocess
import sys

ORIGINS = 500
REPEATS = 20


def run(binary, op, seed, origin, payload=b""):
    o = origin.encode()
    job = struct.pack("<IQI", op, seed, len(o)) + o + payload
    p = subprocess.run([binary], input=job, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, timeout=120)
    if p.returncode != 0:
        raise RuntimeError("fpz exited %d: %s" % (p.returncode,
                                                  p.stderr[-300:]))
    out = {}
    for line in p.stdout.decode().splitlines():
        if not line:
            continue
        k, _, v = line.partition(" ")
        out[k] = v
    return out


def canvas(binary, seed, origin, pixels, raw=False):
    payload = struct.pack("<I", len(pixels) // 4) + pixels
    r = run(binary, 4 if raw else 1, seed, origin, payload)
    return bytes.fromhex(r["PIXELS"]), int(r["TOUCHED"])


def a_picture(w, h):
    """Something with structure in it -- a flat colour would hide a bug
    that only shows where two colours meet."""
    px = bytearray()
    for y in range(h):
        for x in range(w):
            px += bytes(((x * 7 + y * 3) % 256,
                         (x * x + y) % 256,
                         (x ^ y) % 256,
                         255))
    return bytes(px)


def main():
    binary = sys.argv[1]
    fails = []
    checks = 0

    def ok(cond, what, detail=""):
        nonlocal checks
        checks += 1
        if not cond:
            fails.append("%s   %s" % (what, detail))
        return cond

    big = a_picture(64, 48)          # 3072 pixels
    small = a_picture(16, 16)        # what a fingerprinting script uses

    # ---- 1. the same session and the same origin: always the same answer
    first, touched = canvas(binary, 0xA5A5A5A5A5A5A5A5, "https://a.example",
                            big)
    same = all(canvas(binary, 0xA5A5A5A5A5A5A5A5, "https://a.example",
                      big)[0] == first for _ in range(REPEATS - 1))
    ok(same, "stable: %d reads of one canvas in one session on one origin"
       % REPEATS)
    print("   stable over %d reads on one origin       %s"
          % (REPEATS, "identical" if same else "DIFFERENT"))

    # ---- 2. different origins, one session
    seen = set()
    per_origin = []
    for i in range(ORIGINS):
        out, t = canvas(binary, 0x1234567812345678, "https://s%d.example" % i,
                        big)
        seen.add(hashlib.sha256(out).hexdigest())
        per_origin.append((out, t))
    ok(len(seen) == ORIGINS, "separation: %d origins give %d distinct canvases"
       % (ORIGINS, len(seen)))
    print("   %d origins, one session                 %d distinct"
          % (ORIGINS, len(seen)))

    # ---- 3. THE COUNTER-CHECK. Without the farbling every origin has to
    #         give the SAME octets, or the number above measures the
    #         harness and not the defence.
    raw = set()
    for i in range(ORIGINS):
        out, _ = canvas(binary, 0x1234567812345678, "https://s%d.example" % i,
                        big, raw=True)
        raw.add(hashlib.sha256(out).hexdigest())
    ok(len(raw) == 1, "counter-check: the unfarbled path over %d origins"
       % ORIGINS, "%d distinct, expected 1" % len(raw))
    print("   the same path WITHOUT farbling          %d distinct (must be 1)"
          % len(raw))

    # ---- 4. different sessions, one origin
    sess = set()
    for i in range(ORIGINS):
        out, _ = canvas(binary, 0x9000000000000000 + i, "https://a.example",
                        big)
        sess.add(hashlib.sha256(out).hexdigest())
    ok(len(sess) == ORIGINS, "separation: %d sessions give %d distinct"
       % (ORIGINS, len(sess)))
    print("   %d sessions, one origin                 %d distinct"
          % (ORIGINS, len(sess)))

    # ---- 5. is the picture still the picture?
    worst = 0
    alpha_touched = 0
    changed_total = 0
    for out, t in per_origin:
        for i in range(0, len(big), 4):
            for c in range(3):
                d = abs(out[i + c] - big[i + c])
                if d > worst:
                    worst = d
                if d:
                    changed_total += 1
            if out[i + 3] != big[i + 3]:
                alpha_touched += 1
    npx = len(big) // 4
    share = changed_total / (npx * ORIGINS)
    ok(worst <= 1, "invisible: the largest deviation of a colour channel",
       "%d, expected at most 1" % worst)
    ok(alpha_touched == 0, "invisible: the alpha channel is never touched",
       "%d pixels" % alpha_touched)
    print("   largest deviation of a channel          %d" % worst)
    print("   alpha channels touched                  %d" % alpha_touched)
    print("   share of pixels touched                 %.2f %% (1 in %.0f)"
          % (share * 100, 1 / share if share else 0))
    ok(0.01 < share < 0.10, "the rate is in the intended range",
       "%.4f" % share)

    # ---- 6. the small canvas: a 16 x 16 read is the usual one in the
    #         wild, and it must still come out different per origin.
    small_seen = set()
    untouched = 0
    for i in range(ORIGINS):
        out, t = canvas(binary, 0x1234567812345678, "https://t%d.example" % i,
                        small)
        small_seen.add(hashlib.sha256(out).hexdigest())
        if t == 0:
            untouched += 1
    print("   16 x 16 canvas, %d origins              %d distinct, %d untouched"
          % (ORIGINS, len(small_seen), untouched))
    ok(len(small_seen) >= ORIGINS - untouched,
       "the small canvas separates as far as it is touched at all")

    # ---- 7. `navigator`
    nav = [run(binary, 2, 0x1234567812345678, "https://n%d.example" % i)
           for i in range(ORIGINS)]
    hwc = collections.Counter(int(n["HWC"]) for n in nav)
    mem = collections.Counter(int(n["MEM"]) for n in nav)
    uas = set(n["UA"] for n in nav)
    ok(len(uas) == 1, "the user agent is FROZEN, not noised",
       "%d distinct" % len(uas))
    ok(all(2 <= v <= 12 for v in hwc), "hardwareConcurrency stays plausible",
       str(sorted(hwc)))
    stable_nav = all(
        run(binary, 2, 0x1234567812345678, "https://n7.example")["HWC"]
        == nav[7]["HWC"] for _ in range(5))
    ok(stable_nav, "hardwareConcurrency is stable per origin and session")
    ok(len(hwc) > 1 and len(mem) > 1,
       "navigator really varies over origins",
       "hwc=%d mem=%d" % (len(hwc), len(mem)))
    print("   navigator.hardwareConcurrency           %s"
          % dict(sorted(hwc.items())))
    print("   navigator.deviceMemory                  %s"
          % dict(sorted(mem.items())))
    print("   navigator.userAgent                     %d distinct: %s"
          % (len(uas), list(uas)[0][:46] + "..."))

    # ---- 8. the clock
    real = 1000000123
    reads = [int(run(binary, 3, 0x1234567812345678,
                     "https://c%d.example" % i,
                     struct.pack("<Q", real))["CLOCK"]) for i in range(64)]
    ok(all(v % 100 == 0 for v in reads),
       "every clock reading is a multiple of the step")
    ok(all(v <= real for v in reads),
       "a clock reading never runs ahead of the real one")
    ok(len(set(reads)) > 1, "the step boundary lies elsewhere per origin",
       "%d distinct" % len(set(reads)))
    ok(max(reads) - min(reads) <= 100,
       "the offset never moves a reading by more than one step",
       "%d us" % (max(reads) - min(reads)))
    print("   clock: %d readings of one instant        %d distinct, all "
          "multiples of 100 us" % (len(reads), len(set(reads))))

    print()
    if fails:
        print("FPZ FAILED: %d of %d checks" % (len(fails), checks))
        for f in fails:
            print("  FAIL  %s" % f)
        return 1
    print("FPZ OK: %d checks, %d origins, %d sessions -- canvas farbling and "
          "the navigator fields, per origin and per session"
          % (checks, ORIGINS, ORIGINS))
    return 0


if __name__ == "__main__":
    sys.exit(main())
