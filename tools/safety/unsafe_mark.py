#!/usr/bin/env python3
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
FIRNC = os.environ.get("FIRNC", os.path.join(ROOT, "compiler/target/release/firnc"))
FN_HEAD = re.compile(r"^(\s*)(unsafe\s+)?fn\s+[A-Za-z_]")
SITE = re.compile(r"^(.*?):(\d+):\d+: warning: ")


def module_name(path):
    rel = os.path.relpath(path, os.path.join(ROOT, "lib"))
    return rel[: -len(".fi")].replace(os.sep, ".")


def find_sites(path, work, use_extra=False):
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
    extra = os.environ.get("SITES_FILE")
    if extra and use_extra:
        with open(extra) as f:
            for row in f:
                m = re.match(r"^(.*?):(\d+):\d+:", row.strip())
                if m and os.path.abspath(m.group(1)) == path:
                    lines.add(int(m.group(2)))
    return sorted(lines), out.returncode


def body_bounds(src_lines, head_idx, indent):
    open_idx = head_idx
    while open_idx < len(src_lines) and not src_lines[open_idx].rstrip().endswith("{"):
        open_idx += 1
    close_idx = open_idx + 1
    while close_idx < len(src_lines) and src_lines[close_idx] != indent + "}":
        close_idx += 1
    if close_idx >= len(src_lines):
        return None
    return open_idx, close_idx


def inline_body(line):
    k = line.rfind("fn ")
    if k < 0 or "{" not in line[k:]:
        return None
    i = line.index("{", k)
    depth = 0
    for j in range(i, len(line)):
        if line[j] == "{":
            depth += 1
        elif line[j] == "}":
            depth -= 1
            if depth == 0:
                return i, j
    return None


def wrap_inline(src_lines, idx):
    line = src_lines[idx]
    span = inline_body(line)
    if span is None or line[span[0] + 1:].lstrip().startswith("unsafe {"):
        return False
    i, j = span
    src_lines[idx] = line[:i + 1] + " unsafe {" + line[i + 1:j] + "} " + line[j:]
    return True


def enclosing_fn(src_lines, line_no):
    if inline_body(src_lines[line_no - 1]) is not None:
        return line_no - 1, "", False, None
    i = line_no - 1
    while i >= 0:
        m = FN_HEAD.match(src_lines[i])
        if m:
            bounds = body_bounds(src_lines, i, m.group(1))
            if bounds and bounds[0] <= line_no - 1 <= bounds[1]:
                return i, m.group(1), bool(m.group(2)), bounds
            return None
        i -= 1
    return None


def wrap_body(src_lines, bounds, indent):
    open_idx, close_idx = bounds
    if src_lines[open_idx + 1].strip() == "unsafe {":
        return False
    pad = indent + "    "
    body = [("    " + l if l.strip() else l) for l in src_lines[open_idx + 1:close_idx]]
    src_lines[open_idx + 1:close_idx] = [pad + "unsafe {"] + body + [pad + "}"]
    return True


def mark(path, work):
    with open(path) as f:
        src = f.read().split("\n")
    for attempt in range(12):
        sites, rc = find_sites(path, work, attempt == 0)
        if rc != 0 and not (attempt == 0 and os.environ.get("SITES_FILE")):
            print("%s: compile failed" % path, file=sys.stderr)
            return None
        if not sites:
            break
        heads = {}
        for ln in sites:
            hit = enclosing_fn(src, ln)
            if hit is not None and not hit[2]:
                heads[hit[0]] = hit
        changed = False
        for idx in sorted(heads, reverse=True):
            _, indent, _, bounds = heads[idx]
            if bounds is None:
                changed = wrap_inline(src, idx) or changed
            else:
                changed = wrap_body(src, bounds, indent) or changed
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
