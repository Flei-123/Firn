#!/usr/bin/env python3
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FIRNC = os.environ.get("FIRNC", os.path.join(ROOT, "compiler/target/release/firnc"))
FN_HEAD = re.compile(r"^fn\s+[A-Za-z_]")
SITE = re.compile(r"^(.*?):(\d+):\d+: warning: ")


def module_name(path):
    rel = os.path.relpath(path, os.path.join(ROOT, "lib"))
    return rel[: -len(".fi")].replace(os.sep, ".")


def find_sites(path, work):
    wrapper = os.path.join(work, "m.fi")
    with open(wrapper, "w") as f:
        f.write("import %s\nfn main() -> i32 {\n    return 0\n}\n" % module_name(path))
    env = dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib"))
    out = subprocess.run(
        [FIRNC, "--unsafe=warn", "--unsafe-scope=" + os.path.relpath(path, ROOT), wrapper, "-o", os.path.join(work, "m.out")],
        capture_output=True, text=True, env=env,
    )
    lines = set()
    for row in out.stderr.splitlines():
        m = SITE.match(row)
        if m and os.path.abspath(m.group(1)) == path:
            lines.add(int(m.group(2)))
    return sorted(lines), out.returncode


def enclosing_fn(src_lines, line_no):
    i = line_no - 1
    while i >= 0:
        if FN_HEAD.match(src_lines[i]):
            return i
        if src_lines[i].startswith("impl ") or src_lines[i].startswith("interface "):
            return None
        i -= 1
    return None


def mark(path, work):
    with open(path) as f:
        src = f.read().split("\n")
    unsupported = []
    for _ in range(4):
        sites, rc = find_sites(path, work)
        if rc != 0:
            print("%s: compile failed" % path, file=sys.stderr)
            return None
        if not sites:
            break
        changed = False
        for ln in sites:
            idx = enclosing_fn(src, ln)
            if idx is None:
                unsupported.append(ln)
                continue
            src[idx] = "unsafe " + src[idx]
            changed = True
        with open(path, "w") as f:
            f.write("\n".join(src))
        if not changed:
            break
    sites, _ = find_sites(path, work)
    return sites


def main():
    if len(sys.argv) < 2:
        print("usage: unsafe_mark.py lib/<dir>/<file>.fi ...", file=sys.stderr)
        return 2
    status = 0
    with tempfile.TemporaryDirectory() as work:
        for arg in sys.argv[1:]:
            path = os.path.abspath(arg)
            left = mark(path, work)
            if left is None:
                status = 1
            elif left:
                print("%s: %d sites left (lines %s)" % (arg, len(left), ",".join(map(str, left[:10]))))
                status = 1
            else:
                print("%s: clean" % arg)
    return status


sys.exit(main())
