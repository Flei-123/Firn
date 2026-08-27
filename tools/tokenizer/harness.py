#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""Harness for the HTML5 tokenizer from lib/html/ against html5lib-tests.

A WORKBENCH, NOT A PRODUCT: this script contains NO tokenizer logic. It
turns the test cases into jobs, calls the binary written in Firn
exactly once and compares the answer with the expectation.

Rules of honesty (yardstick item a):
  * EVERY case from all .test files is counted -- `xmlViolationTests` too.
  * Cases under the key `xmlViolationTests` expect, according to the
    html5lib README, the XML adjustment ("Coercing an HTML DOM into an
    infoset"); they are therefore run with the job flag XML_MODUS set
    (bit 0, see PROTOKOLL.md). With `--no-xml-mode` the flag stays off --
    then those cases fail, but they are counted all the same.
  * `doubleEscaped: true` additionally unescapes \\uXXXX in input AND output.
  * `initialStates` and `lastStartTag` are honoured; a case only counts as
    passed when it is right in EVERY one of its start states.
  * Unsupported cases (the answer ["NICHT-UNTERSTUETZT"]) are
    FAILURES. There is no skipping and there are no filters.
  * With `--with-errors` the `errors` list of the case additionally has to
    match exactly (WHATWG code name, line and column, in the order of the
    expectation). Without the switch only the token stream counts. BOTH
    quotas are always reported -- the chosen one only decides about
    `passed` in the JSON balance and about the return value of the table.

Usage:  python3 tools/tokenizer/harness.py <binary> [--json file] [--show N]
                                           [--no-xml-mode] [--with-errors]
"""

import glob
import json
import os
import re
import struct
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
TESTDIR = os.path.join(ROOT, "testdata", "html5lib-tokenizer")

STATES = {
    "Data state": 0,
    "PLAINTEXT state": 1,
    "RCDATA state": 2,
    "RAWTEXT state": 3,
    "Script data state": 4,
    "CDATA section state": 5,
}

# Job flags (bit 0 = XML adjustment), see tools/tokenizer/LOG.md.
FLAG_XML = 1


def unescape(text):
    """\\uXXXX unescaping for `doubleEscaped` cases."""
    return re.sub(
        r"\\u([0-9A-Fa-f]{4})", lambda m: chr(int(m.group(1), 16)), text
    )


def unescape_token(tok):
    if isinstance(tok, str):
        return unescape(tok)
    if isinstance(tok, list):
        return [unescape_token(x) for x in tok]
    if isinstance(tok, dict):
        return {unescape(k): unescape_token(v) for k, v in tok.items()}
    return tok


def normalise(tokens):
    """Comparison form: merge character tokens, remove ParseError,
    unify StartTag to (name, attrs, self_closing)."""
    out = []
    for t in tokens:
        if t == "ParseError":
            continue
        if not isinstance(t, list):
            out.append(t)
            continue
        kind = t[0]
        if kind == "Character":
            if out and out[-1][0] == "Character":
                out[-1] = ["Character", out[-1][1] + t[1]]
            else:
                out.append(["Character", t[1]])
        elif kind == "StartTag":
            attrs = t[2] if len(t) > 2 else {}
            self_closing = bool(t[3]) if len(t) > 3 else False
            out.append(["StartTag", t[1], attrs, self_closing])
        elif kind == "EndTag":
            out.append(["EndTag", t[1]])
        else:
            out.append(list(t))
    return out


def load_cases(xml_mode=True):
    """Yields [(file, index, description, input, expectation, states, lasttag,
    flags)]. `xml_mode` switches the XML adjustment for `xmlViolationTests`."""
    cases = []

    for path in sorted(glob.glob(os.path.join(TESTDIR, "*.test"))):
        with open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        lst = data.get("tests")
        flags = 0
        if lst is None:
            # These cases expect the XML adjustment of the token stream.
            lst = data.get("xmlViolationTests", [])
            if xml_mode:
                flags = FLAG_XML
        for i, t in enumerate(lst):
            inp = t["input"]
            expected = t["output"]
            if t.get("doubleEscaped"):
                inp = unescape(inp)
                expected = unescape_token(expected)
            states = t.get("initialStates") or ["Data state"]
            cases.append(
                (
                    os.path.basename(path),
                    i,
                    t.get("description", ""),
                    inp,
                    normalise(expected),
                    states,
                    t.get("lastStartTag", ""),
                    flags,
                    t.get("errors", []),
                )
            )
    return cases


def jobs(cases):
    raw = bytearray()
    plan = []  # (fall_index, state_name)
    for k, (_, _, _, inp, _, states, lasttag, flags, _) in enumerate(cases):
        for st in states:
            code = STATES.get(st)
            if code is None:
                raise SystemExit("unknown start state: %r" % st)
            lt = lasttag.encode("utf-8", "surrogatepass")
            eb = inp.encode("utf-8", "surrogatepass")
            raw += struct.pack("<I", code)
            raw += struct.pack("<I", flags)
            raw += struct.pack("<I", len(lt)) + lt
            raw += struct.pack("<I", len(eb)) + eb
            plan.append((k, st))
    return bytes(raw), plan


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    binary = sys.argv[1]
    json_out = None
    show = 0
    xml_mode = True
    with_errors = False
    args = sys.argv[2:]
    while args:
        a = args.pop(0)
        if a == "--json":
            json_out = args.pop(0)
        elif a == "--show":
            show = int(args.pop(0))
        elif a == "--no-xml-mode":
            xml_mode = False
        elif a == "--with-errors":
            with_errors = True
        else:
            raise SystemExit("unknown option %r" % a)

    cases = load_cases(xml_mode)
    raw, plan = jobs(cases)
    p = subprocess.run([binary], input=raw, stdout=subprocess.PIPE)
    lines = p.stdout.decode("ascii", "replace").splitlines()
    if len(lines) != len(plan):
        print(
            "ERROR: %d answer lines for %d jobs -- the binary has "
            "stopped (exit %d)" % (len(lines), len(plan), p.returncode)
        )
        # Everything that is missing counts as a failure: pad with empty lines.
        lines += ["[]"] * (len(plan) - len(lines))

    # Two balances: without and with a comparison of the parse errors.
    ok_without = [True] * len(cases)
    ok_with = [True] * len(cases)
    reason = [None] * len(cases)
    reason_with = [None] * len(cases)
    for (k, st), line in zip(plan, lines):
        parts = line.split("\t")
        try:
            got = json.loads(parts[0])
        except ValueError:
            got = ["<unreadable>"]
        try:
            got_errors = json.loads(parts[1]) if len(parts) > 1 else []
        except ValueError:
            got_errors = [{"code": "<unreadable>"}]
        if got == ["NICHT-UNTERSTUETZT"]:
            if ok_without[k]:
                reason[k] = "state not implemented"
            if ok_with[k]:
                reason_with[k] = "state not implemented"
            ok_without[k] = False
            ok_with[k] = False
            continue
        if normalise(got) != cases[k][4]:
            if ok_without[k]:
                reason[k] = "output differs (%s)" % st
            if ok_with[k]:
                reason_with[k] = "output differs (%s)" % st
            ok_without[k] = False
            ok_with[k] = False
            continue
        if got_errors != cases[k][8]:
            if ok_with[k]:
                reason_with[k] = "error list differs (%s): %s instead of %s" % (
                    st, json.dumps(got_errors), json.dumps(cases[k][8])
                )
            ok_with[k] = False

    ok_per_case = ok_with if with_errors else ok_without
    if with_errors:
        reason = reason_with

    per_file = {}
    for k, f in enumerate(cases):
        d = per_file.setdefault(f[0], [0, 0, 0])
        d[1] += 1
        if ok_without[k]:
            d[0] += 1
        if ok_with[k]:
            d[2] += 1

    total = len(cases)
    passed = sum(1 for x in ok_per_case if x)
    passed_without = sum(1 for x in ok_without if x)
    passed_with = sum(1 for x in ok_with if x)
    head = " (chosen: with error codes)" if with_errors else " (chosen: without error codes)"
    print("file                        without error codes  with error codes" + head)
    print("-" * 78)
    for name in sorted(per_file):
        p_, g_, m_ = per_file[name]
        print("%-26s %5d / %5d %6.2f %%   %5d / %5d %6.2f %%"
              % (name, p_, g_, 100.0 * p_ / g_, m_, g_, 100.0 * m_ / g_))
    print("-" * 78)
    print("%-26s %5d / %5d %6.2f %%   %5d / %5d %6.2f %%"
          % ("TOTAL", passed_without, total, 100.0 * passed_without / total,
             passed_with, total, 100.0 * passed_with / total))

    if show:
        print("\nfirst %d failures:" % show)
        n = 0
        for k, f in enumerate(cases):
            if ok_per_case[k]:
                continue
            print("  %s #%d  %s  [%s]" % (f[0], f[1], f[2][:60], reason[k]))
            n += 1
            if n >= show:
                break

    if json_out:
        with open(json_out, "w", encoding="utf-8") as fh:
            json.dump(
                {
                    "suite": "html5lib-tokenizer",
                    "total": total,
                    "passed": passed,
                    "failed": total - passed,
                    "rate": passed / total,
                    "xml_mode": xml_mode,
                    "with_errors": with_errors,
                    "passed_without_errors": passed_without,
                    "passed_with_errors": passed_with,
                    "files": {
                        k: {
                            "passed": v[0],
                            "total": v[1],
                            "passed_with_errors": v[2],
                        }
                        for k, v in per_file.items()
                    },
                },
                fh,
                indent=1,
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
