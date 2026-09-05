#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/js/harness_run.py -- the ENGINE against test262.

Every case of the chosen subset is really executed. The rules are those of
the suite (test262/INTERPRETING.md):

  * `raw`      -- run the file alone, nothing prepended,
  * otherwise  -- `assert.js` and `sta.js` first, then the files from
                  `includes`, then the case,
  * `onlyStrict` / `noStrict` decide whether the strict variant, the sloppy
    variant or BOTH are run; both count separately,
  * `negative: phase: parse` has to fail at parse time,
  * `negative: phase: resolution|runtime` has to throw the named type,
  * everything else has to run through without an exception.

NOTHING IS FILTERED. A case that uses a feature this engine does not have
(generators, async, RegExp, Proxy, ...) is a FAILURE like any other. The
report splits the failures by CAUSE, so that the quota can be read
honestly, but the quota itself counts them all.

Usage:
  python3 tools/js/harness_run.py <jsrun> [--root DIR] [--json out.json]
                                  [--show N] [--dir SUB] [--limit N]
"""
import json
import os
import re
import struct
import subprocess
import tempfile
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
FRONT = re.compile(r"/\*---(.*?)---\*/", re.S)


def meta(src):
    m = FRONT.search(src)
    out = {"flags": [], "includes": [], "features": []}
    if not m:
        return out
    body = m.group(1)
    neg = re.search(r"^negative:\s*$(.*?)(?=^\S|\Z)", body, re.S | re.M)
    if neg:
        phase = re.search(r"phase:\s*(\S+)", neg.group(1))
        typ = re.search(r"type:\s*(\S+)", neg.group(1))
        out["negative"] = {"phase": phase.group(1) if phase else "",
                           "type": typ.group(1) if typ else ""}
    for key in ("flags", "includes", "features"):
        m2 = re.search(r"^%s:\s*\[(.*?)\]" % key, body, re.M)
        if m2:
            out[key] = [x.strip() for x in m2.group(1).split(",") if x.strip()]
    m3 = re.search(r"^includes:\s*$((?:\s*-\s*\S+\s*$)+)", body, re.M)
    if m3:
        out["includes"] = re.findall(r"-\s*(\S+)", m3.group(1))
    return out


def cases(root):
    for base, _, files in os.walk(root):
        if "_FIXTURE" in base:
            continue
        for f in sorted(files):
            if not f.endswith(".js") or f.endswith("_FIXTURE.js"):
                continue
            yield os.path.join(base, f)


def classify(status, out, want, typ):
    """Why did a run fail? Only for the breakdown, not for the quota."""
    if status == "OK":
        return "async-incomplete"
    if status.startswith("PARSE"):
        code = status.split()[1]
        if code == "6":
            return "unsupported-syntax"
        return "parse"
    if status.startswith("THROW"):
        text = status[6:]
        for name in ("not supported", "not implemented", "no native table"):
            if name in text:
                return "unsupported-builtin"
        if "regular expressions" in text:
            return "unsupported-regexp"
        if "modules are not linked" in text:
            return "unsupported-module"
        return "throw"
    if status == "OOM":
        return "oom"
    if status == "CRASH":
        return "crash"
    if status == "CRASH":
        return "crash"
    if status == "TIMEOUT":
        return "timeout"
    return "wrong"


def main():
    exe = sys.argv[1]
    args = sys.argv[2:]
    root = os.environ.get("T262", os.path.join(ROOT, ".js-work", "t262"))
    jsonout = None
    show = 0
    sub = "test"
    limit = 0
    details = None
    i = 0
    while i < len(args):
        if args[i] == "--json":
            jsonout = args[i + 1]; i += 2
        elif args[i] == "--show":
            show = int(args[i + 1]); i += 2
        elif args[i] == "--dir":
            sub = args[i + 1]; i += 2
        elif args[i] == "--root":
            root = args[i + 1]; i += 2
        elif args[i] == "--limit":
            limit = int(args[i + 1]); i += 2
        elif args[i] == "--details":
            details = args[i + 1]; i += 2
        else:
            i += 1

    harness = os.path.join(root, "harness")
    hcache = {}

    def helper(name):
        if name not in hcache:
            hcache[name] = open(os.path.join(harness, name), encoding="utf-8").read()
        return hcache[name]

    jobs = []
    index = []
    for path in cases(os.path.join(root, sub)):
        src = open(path, encoding="utf-8").read()
        m = meta(src)
        flags = m["flags"]
        neg = m.get("negative") or {}
        if "module" in flags:
            variants = [(1, src, True)]
        elif "raw" in flags:
            variants = [(0, src, False)]
        else:
            pre = helper("assert.js") + "\n" + helper("sta.js") + "\n"
            # A case with the `async` flag calls `$DONE`; test262 expects
            # the runner to put `doneprintHandle.js` in front of it, and
            # most of the generated cases do not name it in `includes`.
            incs = list(m["includes"])
            if "async" in flags and "doneprintHandle.js" not in incs:
                incs.append("doneprintHandle.js")
            for inc in incs:
                pre += helper(inc) + "\n"
            variants = []
            if "onlyStrict" not in flags:
                variants.append((0, pre + src, False))
            if "noStrict" not in flags:
                variants.append((0, '"use strict";\n' + pre + src, True))
        for mode, text, strict in variants:
            data = text.encode("utf-8")
            jobs.append(struct.pack("<II", mode, len(data)) + data)
            index.append((path, neg.get("phase", ""), neg.get("type", ""),
                          m["features"], "async" in flags))
        if limit and len(index) >= limit:
            break

    # The engine is run in BATCHES, and the input goes through a FILE, not
    # through a pipe: with `input=` plus `timeout=` the writing thread of
    # `subprocess` can hang on a dead pipe and the whole harness stands
    # still. If a batch produces fewer blocks than it got jobs, the engine
    # died (or ran forever) on the case after the last complete block --
    # that one is recorded and the rest is retried in a fresh process. So a
    # single crash never swallows the remaining thousands of cases.
    blocks = []
    todo = list(range(len(jobs)))
    BATCH = 64
    BUDGET = 15
    while todo:
        chunk = todo[:BATCH]
        with tempfile.NamedTemporaryFile(delete=False) as tf:
            tf.write(b"".join(jobs[i] for i in chunk))
            inpath = tf.name
        raw = b""
        marker = "CRASH"
        try:
            with open(inpath, "rb") as fin:
                proc = subprocess.run([exe], stdin=fin,
                                      stdout=subprocess.PIPE, timeout=BUDGET)
                raw = proc.stdout
        except subprocess.TimeoutExpired as e:
            raw = e.stdout or b""
            marker = "TIMEOUT"
        finally:
            os.unlink(inpath)
        got = raw.decode("utf-8", "replace").split("\x00\n")
        if got and got[-1].strip() == "":
            got.pop()
        if len(got) >= len(chunk):
            blocks.extend(got[:len(chunk)])
            todo = todo[len(chunk):]
            continue
        blocks.extend(got)
        blocks.append(marker)
        todo = todo[len(got) + 1:]

    passed = 0
    failed = []
    reasons = {}
    for (path, phase, typ, feats, is_async), block in zip(index, blocks):
        lines = block.rstrip("\n").split("\n")
        status = lines[-1] if lines else ""
        want_parse_fail = phase in ("parse", "early")
        want_throw = phase in ("resolution", "runtime")
        ok = False
        if want_parse_fail:
            ok = status.startswith("PARSE")
        elif want_throw:
            ok = status.startswith("THROW") and (typ == "" or typ in status)
        elif is_async:
            # An ASYNC case (round 66) only counts as passed when it really
            # reached its end: test262 demands the exact line of
            # `doneprintHandle.js`. A run that finishes without the
            # promise ever settling is a FAILURE.
            ok = status == "OK" and "Test262:AsyncTestComplete" in block
        else:
            ok = status == "OK"
        if ok:
            passed += 1
        else:
            why = classify(status, block, phase, typ)
            reasons[why] = reasons.get(why, 0) + 1
            failed.append((os.path.relpath(path, root), status[:90], why))

    total = len(index)
    print("runs        : %d" % total)
    print("passed      : %d" % passed)
    print("failed      : %d" % (total - passed))
    print("quota       : %.2f%%" % (100.0 * passed / total if total else 0.0))
    print("failures by cause:")
    for k in sorted(reasons, key=lambda x: -reasons[x]):
        print("   %-22s %6d" % (k, reasons[k]))
    if show:
        print("--- the first %d failures ---" % show)
        for f in failed[:show]:
            print("  %-70s %s | %s" % (f[0], f[2], f[1]))
    if details:
        with open(details, "w") as fh:
            for f in failed:
                fh.write("%s\t%s\t%s\n" % (f[2], f[1].replace("\t", " "), f[0]))
    if jsonout:
        json.dump({"total": total, "passed": passed,
                   "failed": total - passed, "reasons": reasons,
                   "cases": [f[0] for f in failed[:4000]]},
                  open(jsonout, "w"))


main()
