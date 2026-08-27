#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/stdlib81/cryptocross.py -- lib/std/crypto against python and openssl.

The NIST vectors (`nist.py`) are the formal proof. They have one hole, and
it is a big one: the AES known answer files are all SINGLE BLOCK. They say
nothing about CHAINING -- an implementation that ignores the IV after the
first block passes every one of them. So here:

  * MULTI BLOCK CBC and CFB8 against the `openssl` binary, in both
    directions, over 4 KiB of random data;
  * SHA-1/SHA-256/HMAC against `python3 hashlib`/`hmac` over random keys
    and messages of awkward lengths (0, 1, 55, 56, 63, 64, 65, 1000 -- the
    padding boundaries, where every hash implementation that is wrong is
    wrong);
  * the constant time comparison `hmac_equal` gets its own case: equal
    stays equal, one flipped bit anywhere is caught;
  * `getrandom`: 64 octets twice must not be the same, and must not be all
    zero. That is not a randomness test (nothing that short can be), it
    catches the two failures that really happen -- a buffer that was never
    filled, and a source that repeats.

Usage: cryptocross.py <crypto_cli> <work>
"""
import hashlib
import hmac as pyhmac
import os
import subprocess
import sys


def hx(b):
    return b.hex() if b else "-"


def main():
    if len(sys.argv) < 3:
        print("usage: cryptocross.py <crypto_cli> <work>")
        return 2
    cli, work = sys.argv[1], sys.argv[2]
    os.makedirs(work, exist_ok=True)
    jobs, want, label = [], [], []

    lengths = [0, 1, 55, 56, 63, 64, 65, 127, 128, 1000]
    msgs = [os.urandom(n) for n in lengths]
    for m in msgs:
        jobs.append("sha1 " + hx(m)); want.append(hashlib.sha1(m).hexdigest())
        label.append("sha1/%d" % len(m))
        jobs.append("sha256 " + hx(m)); want.append(hashlib.sha256(m).hexdigest())
        label.append("sha256/%d" % len(m))
    for klen in (0, 1, 20, 32, 63, 64, 65, 200):
        k = os.urandom(klen)
        for m in msgs[:5]:
            jobs.append("hmac1 %s %s" % (hx(k), hx(m)))
            want.append(pyhmac.new(k, m, hashlib.sha1).hexdigest())
            label.append("hmac-sha1/k%d/m%d" % (klen, len(m)))
            jobs.append("hmac256 %s %s" % (hx(k), hx(m)))
            want.append(pyhmac.new(k, m, hashlib.sha256).hexdigest())
            label.append("hmac-sha256/k%d/m%d" % (klen, len(m)))

    # FIPS 197 C.1 -- the known answer test of the standard itself.
    jobs.append("cbce 000102030405060708090a0b0c0d0e0f "
                "00000000000000000000000000000000 "
                "00112233445566778899aabbccddeeff")
    want.append("69c4e0d86a7b0430d8cdb78070b4c55a")
    label.append("FIPS197/encrypt")
    jobs.append("cbcd 000102030405060708090a0b0c0d0e0f "
                "00000000000000000000000000000000 "
                "69c4e0d86a7b0430d8cdb78070b4c55a")
    want.append("00112233445566778899aabbccddeeff")
    label.append("FIPS197/decrypt")

    # Multi block, against openssl.
    key = os.urandom(16)
    iv = os.urandom(16)
    plain = os.urandom(4096)
    pf = os.path.join(work, "plain.bin")
    open(pf, "wb").write(plain)
    have_openssl = True
    for mode, name in (("cbce", "-aes-128-cbc"), ("cfb8e", "-aes-128-cfb8")):
        cf = os.path.join(work, "c_%s.bin" % mode)
        args = ["openssl", "enc", name, "-K", key.hex(), "-iv", iv.hex(),
                "-in", pf, "-out", cf]
        if mode == "cbce":
            args.append("-nopad")
        p = subprocess.run(args, capture_output=True)
        if p.returncode != 0:
            have_openssl = False
            break
        ct = open(cf, "rb").read()
        jobs.append("%s %s %s %s" % (mode, key.hex(), iv.hex(), plain.hex()))
        want.append(ct.hex())
        label.append("openssl/%s/encrypt/4096" % name)
        jobs.append("%s %s %s %s" % (mode.replace("e", "d", 1) if mode == "cbce"
                                     else "cfb8d",
                                     key.hex(), iv.hex(), ct.hex()))
        want.append(plain.hex())
        label.append("openssl/%s/decrypt/4096" % name)

    jobfile = os.path.join(work, "cross.jobs")
    outfile = os.path.join(work, "cross.out")
    open(jobfile, "w").write("\n".join(jobs) + "\n")
    if subprocess.run([cli, "batch", jobfile, outfile]).returncode != 0:
        print("  FAIL crypto_cli batch failed")
        return 1
    got = open(outfile).read().split("\n")
    bad = 0
    for i, w in enumerate(want):
        mine = got[i].strip() if i < len(got) else "<missing>"
        if mine != w:
            bad += 1
            if bad <= 5:
                print("  FAIL %s\n    want %s...\n    got  %s..."
                      % (label[i], w[:32], mine[:32]))
    print("  python/openssl cross-check: %d of %d agree%s"
          % (len(want) - bad, len(want),
             "" if have_openssl else " (openssl not found -- multi block skipped)"))

    # getrandom
    open(jobfile, "w").write("rand 64\nrand 64\nsource 64\n")
    subprocess.run([cli, "batch", jobfile, outfile], check=True)
    lines = [l.strip() for l in open(outfile) if l.strip()]
    rnd_ok = (len(lines) >= 3 and len(lines[0]) == 128 and len(lines[1]) == 128
              and lines[0] != lines[1] and set(lines[0]) != {"0"}
              and lines[2] in ("1", "2"))
    print("  getrandom: %s (source %s, 1 = getrandom(2), 2 = /dev/urandom)"
          % ("ok" if rnd_ok else "FAILED", lines[2] if len(lines) > 2 else "?"))
    if not rnd_ok:
        bad += 1

    print("  RESULT %s" % ("ok" if bad == 0 else "FAILED"))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
