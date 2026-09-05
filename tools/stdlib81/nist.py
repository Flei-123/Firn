#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/stdlib81/nist.py -- the official test vectors against lib/std/crypto.

This script does not implement any cryptography. It reads the NIST CAVP
response files (`.rsp`) out of `testdata/crypto/`, turns every vector into
one line of a job file for `crypto_cli`, runs it once, and compares the
answers. Whatever `crypto_cli` writes is compared against what NIST wrote --
this file only shuffles hex around.

Two things the format does that surprise people, and both are handled here:

  * in the SHA files `Len = 0` comes with `Msg = 00`. The message is EMPTY;
    the `00` is a placeholder. A parser that hashes one zero octet fails
    exactly one vector per file and looks fine otherwise.
  * in `HMAC.rsp` the expected MAC is TRUNCATED to `Tlen` octets, and `Tlen`
    varies inside one file. The full MAC is computed and cut here.

Usage: nist.py <crypto_cli> <vector-directory> [<work-directory>]
"""
import os
import subprocess
import sys


def blocks(path):
    """Every vector as a dict; `[L=20]` style section headers come along."""
    cur, section = {}, {}
    for raw in open(path, encoding="latin-1"):
        line = raw.strip()
        if not line or line.startswith("#"):
            if cur:
                yield dict(section, **cur)
                cur = {}
            continue
        if line.startswith("[") and line.endswith("]"):
            if cur:
                yield dict(section, **cur)
                cur = {}
            body = line[1:-1]
            if "=" in body:
                k, v = body.split("=", 1)
                section[k.strip()] = v.strip()
            else:
                section["MODE"] = body.strip()
            continue
        if "=" in line:
            k, v = line.split("=", 1)
            cur[k.strip()] = v.strip()
    if cur:
        yield dict(section, **cur)


def hexfield(v):
    return v if v else "-"


def collect(vecdir):
    """(job line, expected hex, label) for every vector that is found."""
    jobs = []
    for name, op in (("SHA1ShortMsg.rsp", "sha1"), ("SHA1LongMsg.rsp", "sha1"),
                     ("SHA256ShortMsg.rsp", "sha256"),
                     ("SHA256LongMsg.rsp", "sha256")):
        path = os.path.join(vecdir, "sha", name)
        if not os.path.exists(path):
            continue
        for b in blocks(path):
            if "Msg" not in b or "MD" not in b:
                continue
            msg = b["Msg"]
            if int(b.get("Len", "0")) == 0:
                msg = ""          # the placeholder, see the docstring
            jobs.append(("%s %s" % (op, hexfield(msg)), b["MD"].lower(),
                         "%s:%s" % (name, b.get("Len", "?"))))

    path = os.path.join(vecdir, "hmac", "HMAC.rsp")
    if os.path.exists(path):
        for b in blocks(path):
            if "Key" not in b or "Mac" not in b:
                continue
            length = b.get("L", "20")
            op = {"20": "hmac1", "32": "hmac256"}.get(length)
            if op is None:
                continue           # SHA-224/384/512 are not built in this round
            tlen = int(b["Tlen"])
            jobs.append(("%s %s %s" % (op, hexfield(b["Key"]),
                                       hexfield(b.get("Msg", ""))),
                         b["Mac"].lower()[:tlen * 2],
                         "HMAC.rsp:L=%s:%s" % (length, b.get("Count", "?"))))

    aesdir = os.path.join(vecdir, "aes")
    if os.path.isdir(aesdir):
        for name in sorted(os.listdir(aesdir)):
            if not name.endswith("128.rsp"):
                continue
            if name.startswith("CBC"):
                enc, dec = "cbce", "cbcd"
            elif name.startswith("CFB8"):
                enc, dec = "cfb8e", "cfb8d"
            else:
                continue           # ECB has no IV -- not a mode of this round
            mode = None
            for b in blocks(os.path.join(aesdir, name)):
                mode = b.get("MODE", mode)
                if "KEY" not in b:
                    continue
                iv = b.get("IV", "0" * 32)
                if "PLAINTEXT" in b and "CIPHERTEXT" in b:
                    if mode == "ENCRYPT":
                        jobs.append(("%s %s %s %s" % (enc, b["KEY"], iv,
                                                      b["PLAINTEXT"]),
                                     b["CIPHERTEXT"].lower(),
                                     "%s:enc:%s" % (name, b.get("COUNT", "?"))))
                    else:
                        jobs.append(("%s %s %s %s" % (dec, b["KEY"], iv,
                                                      b["CIPHERTEXT"]),
                                     b["PLAINTEXT"].lower(),
                                     "%s:dec:%s" % (name, b.get("COUNT", "?"))))
    return jobs


def main():
    if len(sys.argv) < 3:
        print("usage: nist.py <crypto_cli> <vector-directory> [<work>]")
        return 2
    cli, vecdir = sys.argv[1], sys.argv[2]
    work = sys.argv[3] if len(sys.argv) > 3 else "/tmp"
    jobs = collect(vecdir)
    if not jobs:
        print("FAIL: no vectors found under %s" % vecdir)
        return 1
    jobfile = os.path.join(work, "nist.jobs")
    outfile = os.path.join(work, "nist.out")
    with open(jobfile, "w") as f:
        f.write("\n".join(j[0] for j in jobs) + "\n")
    rc = subprocess.run([cli, "batch", jobfile, outfile]).returncode
    if rc != 0:
        print("FAIL: crypto_cli batch exited with %d" % rc)
        return 1
    got = open(outfile).read().split("\n")
    ok = bad = 0
    groups = {}
    for i, (_, want, label) in enumerate(jobs):
        mine = got[i].strip() if i < len(got) else "<missing>"
        # HMAC vectors are compared over the truncated length.
        if len(want) < len(mine):
            mine = mine[:len(want)]
        key = label.split(":")[0]
        g = groups.setdefault(key, [0, 0])
        if mine == want:
            ok += 1
            g[0] += 1
        else:
            bad += 1
            g[1] += 1
            if bad <= 5:
                print("  MISMATCH %s\n    want %s\n    got  %s"
                      % (label, want, mine))
    for name in sorted(groups):
        good, wrong = groups[name]
        print("  %-22s %5d ok, %d wrong" % (name, good, wrong))
    print("NIST TOTAL: %d ok, %d wrong (%d vectors)" % (ok, bad, len(jobs)))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
