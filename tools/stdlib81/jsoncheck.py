#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/stdlib81/jsoncheck.py -- lib/std/json.fi against JSONTestSuite.

Three questions, and the second is the one that matters:

  1. are all `y_*` files ACCEPTED?
  2. are all `n_*` files REFUSED? A parser that says yes to everything
     passes question 1 completely.
  3. does the output agree with `python3 -m json.tool` -- octet for octet
     where that is meaningful, and semantically (`json.load` equality) in
     every case?

The `i_*` files are implementation defined (RFC 8259 leaves them open); the
verdicts are counted and printed, not judged.

Usage: jsoncheck.py <json_cli> <suite-directory> [<work>]
"""
import json
import os
import subprocess
import sys


def main():
    if len(sys.argv) < 3:
        print("usage: jsoncheck.py <json_cli> <suite-directory> [<work>]")
        return 2
    cli, suite = sys.argv[1], sys.argv[2]
    work = sys.argv[3] if len(sys.argv) > 3 else "/tmp"
    parsing = os.path.join(suite, "test_parsing")
    if not os.path.isdir(parsing):
        print("FAIL: %s does not exist" % parsing)
        return 1

    y_ok = y_bad = n_ok = n_bad = i_yes = i_no = 0
    bad_names = []
    for name in sorted(os.listdir(parsing)):
        if not name.endswith(".json"):
            continue
        path = os.path.join(parsing, name)
        rc = subprocess.run([cli, "check", path],
                            stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL).returncode
        if name.startswith("y_"):
            if rc == 0:
                y_ok += 1
            else:
                y_bad += 1
                bad_names.append("accepted-not: " + name)
        elif name.startswith("n_"):
            if rc != 0:
                n_ok += 1
            else:
                n_bad += 1
                bad_names.append("refused-not: " + name)
        elif name.startswith("i_"):
            if rc == 0:
                i_yes += 1
            else:
                i_no += 1

    # The output, against a parser nobody here wrote.
    same = differ = sem_ok = sem_bad = 0
    differ_names = []
    out = os.path.join(work, "json_firn.out")
    for name in sorted(os.listdir(parsing)):
        if not name.startswith("y_"):
            continue
        path = os.path.join(parsing, name)
        if subprocess.run([cli, "format", path, out],
                          stdout=subprocess.DEVNULL,
                          stderr=subprocess.DEVNULL).returncode != 0:
            differ += 1
            differ_names.append("format-failed: " + name)
            continue
        p = subprocess.run([sys.executable, "-m", "json.tool", path],
                           capture_output=True)
        if p.returncode == 0:
            if open(out, "rb").read() == p.stdout:
                same += 1
            else:
                differ += 1
                differ_names.append(name)
        try:
            if json.load(open(path)) == json.load(open(out)):
                sem_ok += 1
            else:
                sem_bad += 1
                differ_names.append("semantic: " + name)
        except Exception as e:                       # noqa: BLE001
            sem_bad += 1
            differ_names.append("semantic-error: %s (%s)" % (name, e))

    # The error messages have to name a line and a column.
    broken = os.path.join(work, "broken.json")
    open(broken, "w").write('{\n  "a": 1,\n  "b": :\n}\n')
    p = subprocess.run([cli, "check", broken], capture_output=True)
    msg = p.stderr.decode().strip()
    loc_ok = p.returncode != 0 and msg.startswith("3:") and ":" in msg[2:]

    print("  y_ accepted        %3d / %3d" % (y_ok, y_ok + y_bad))
    print("  n_ refused         %3d / %3d" % (n_ok, n_ok + n_bad))
    print("  i_ (open) accepted %3d, refused %3d" % (i_yes, i_no))
    print("  json.tool identical %3d, differing %d" % (same, differ))
    print("  json.load equal     %3d, differing %d" % (sem_ok, sem_bad))
    print("  error position      %s (%s)" % ("ok" if loc_ok else "MISSING", msg))
    for b in bad_names[:10]:
        print("  FAIL %s" % b)
    for b in differ_names[:10]:
        print("  DIFF %s" % b)

    # The verdict. The two duplicate-key files differ from `json.tool` ON
    # PURPOSE (J7 in lib/std/json.fi): this parser keeps both members,
    # Python keeps the last one. Everything else has to agree.
    allowed = {"y_object_duplicated_key.json",
               "y_object_duplicated_key_and_value.json"}
    unexpected = [d for d in differ_names if d not in allowed]
    ok = (y_bad == 0 and n_bad == 0 and sem_bad == 0 and loc_ok
          and not unexpected and y_ok == 95 and n_ok == 188)
    print("  RESULT %s" % ("ok" if ok else "FAILED"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
