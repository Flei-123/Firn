#!/usr/bin/env python3
"""Count code lines: non-empty, non-comment-only lines.

Usage: loc.py [--json] [--ext .fi,.rs] <files or dirs...>
Dirs are walked recursively (skips node_modules, target, .git, dist, build).
Languages: .fi .rs .py .js .ts .svelte .html .css .slint .qml
"""
import os
import re
import sys
import json

EXT = {".fi", ".rs", ".py", ".js", ".ts", ".svelte", ".html", ".css", ".slint", ".qml"}
SKIP_DIRS = {"node_modules", "target", ".git", "dist", "build", "__pycache__", ".svelte-kit"}

# line comment prefixes per extension
LINE = {
    ".fi": ["//"], ".rs": ["//"], ".js": ["//"], ".ts": ["//"], ".slint": ["//"],
    ".qml": ["//"], ".py": ["#"], ".svelte": ["//"], ".html": [], ".css": [],
}
# block comments: (open, close)
BLOCK = {
    ".fi": [("/*", "*/")], ".rs": [("/*", "*/")], ".js": [("/*", "*/")], ".ts": [("/*", "*/")],
    ".slint": [("/*", "*/")], ".qml": [("/*", "*/")], ".css": [("/*", "*/")],
    ".html": [("<!--", "-->")], ".svelte": [("<!--", "-->"), ("/*", "*/")],
}


def count_text(text, ext):
    line_pfx = LINE.get(ext, [])
    blocks = BLOCK.get(ext, [])
    n = 0
    in_block = None
    for raw in text.splitlines():
        s = raw.strip()
        if not s:
            continue
        # consume block comments
        rest = s
        code = False
        while rest:
            if in_block:
                i = rest.find(in_block[1])
                if i < 0:
                    rest = ""
                    break
                rest = rest[i + len(in_block[1]):].strip()
                in_block = None
                continue
            # find earliest block open
            best = None
            for o, c in blocks:
                i = rest.find(o)
                if i >= 0 and (best is None or i < best[0]):
                    best = (i, o, c)
            lp = None
            for p in line_pfx:
                if ext == ".py" and p == "#":
                    pass
                i = rest.find(p)
                if i >= 0 and (lp is None or i < lp):
                    lp = i
            if lp is not None and (best is None or lp < best[0]):
                # keep text before line comment (but not inside URL like http://)
                pre = rest[:lp].strip()
                if pre and not pre.endswith(":"):
                    code = True
                elif pre:
                    code = True
                rest = ""
                break
            if best is None:
                if rest:
                    code = True
                rest = ""
                break
            pre = rest[:best[0]].strip()
            if pre:
                code = True
            in_block = (best[1], best[2])
            rest = rest[best[0] + len(best[1]):]
        if code:
            n += 1
    return n


def count_file(path):
    ext = os.path.splitext(path)[1].lower()
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return count_text(f.read(), ext)
    except OSError:
        return 0


def collect(args, exts):
    out = []
    for a in args:
        if os.path.isdir(a):
            for root, dirs, files in os.walk(a):
                dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
                for fn in sorted(files):
                    if os.path.splitext(fn)[1].lower() in exts:
                        out.append(os.path.join(root, fn))
        elif os.path.isfile(a):
            if os.path.splitext(a)[1].lower() in exts:
                out.append(a)
            else:
                print("skip (ext):", a, file=sys.stderr)
        else:
            print("missing:", a, file=sys.stderr)
    return out


def main():
    argv = sys.argv[1:]
    as_json = False
    exts = set(EXT)
    files = []
    i = 0
    while i < len(argv):
        if argv[i] == "--json":
            as_json = True
        elif argv[i] == "--ext":
            i += 1
            exts = {e if e.startswith(".") else "." + e for e in argv[i].split(",")}
        else:
            files.append(argv[i])
        i += 1
    if not files:
        print(__doc__)
        sys.exit(2)
    paths = collect(files, exts)
    rows = [(p, count_file(p)) for p in paths]
    total = sum(n for _, n in rows)
    if as_json:
        print(json.dumps({"total": total, "files": dict(rows)}))
        return
    for p, n in rows:
        print("%7d  %s" % (n, p))
    print("%7d  TOTAL (%d files)" % (total, len(rows)))


if __name__ == "__main__":
    main()
