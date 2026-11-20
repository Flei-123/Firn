#!/usr/bin/env python3
import re
import sys
from pathlib import Path

DECL = re.compile(r"^(?:unsafe\s+)?(?:gc\s+)?(fn|struct|enum|error|interface|const|type|static|class)\s+([A-Za-z_][A-Za-z0-9_]*)")
EXPORT_START = re.compile(r"^export\s*\{")


def exported_names(lines):
    names = []
    i = 0
    while i < len(lines):
        if EXPORT_START.match(lines[i]):
            text = ""
            while i < len(lines):
                stripped = re.sub(r"//.*", "", lines[i])
                text += " " + stripped
                if "}" in stripped:
                    break
                i += 1
            body = text[text.index("{") + 1 : text.index("}")]
            names += [n.strip() for n in body.split(",") if n.strip()]
        i += 1
    return names


def collapse(text):
    return re.sub(r"\s+", " ", text).strip()


def block_signature(lines, start):
    depth = 0
    out = []
    i = start
    seen_open = False
    while i < len(lines):
        line = re.sub(r"//.*", "", lines[i])
        out.append(line)
        depth += line.count("{") - line.count("}")
        if "{" in line:
            seen_open = True
        if seen_open and depth <= 0:
            break
        i += 1
    return collapse(" ".join(out)).replace(", }", " }").replace("{ ", "{ ")


def fn_signature(lines, start):
    out = []
    i = start
    while i < len(lines):
        line = re.sub(r"//.*", "", lines[i])
        if "{" in line:
            out.append(line.split("{", 1)[0])
            break
        out.append(line)
        i += 1
    return collapse(" ".join(out))


def const_signature(lines, start):
    line = re.sub(r"//.*", "", lines[start])
    head = line.split("=", 1)[0]
    return collapse(head)


def extract(path, module):
    lines = path.read_text().split("\n")
    names = set(exported_names(lines))
    if not names:
        names = {m.group(2) for m in map(DECL.match, lines) if m}
    found = {}
    for i, raw in enumerate(lines):
        m = DECL.match(raw)
        if not m or m.group(2) not in names:
            continue
        kind, name = m.group(1), m.group(2)
        if kind == "fn":
            sig = fn_signature(lines, i)
        elif kind in ("struct", "enum", "error", "interface", "class"):
            sig = block_signature(lines, i)
        else:
            sig = const_signature(lines, i)
        found.setdefault(name, []).append(sig)
    result = []
    for name in sorted(names):
        for sig in found.get(name, ["(declared elsewhere) " + name]):
            result.append("%s | %s" % (module, sig))
    return result


def main():
    lib = Path(sys.argv[1])
    modules = Path(sys.argv[2]).read_text().split()
    out = []
    for m in modules:
        path = lib / "std" / (m + ".fi")
        if not path.exists():
            print("missing module: std.%s" % m, file=sys.stderr)
            sys.exit(1)
        out += extract(path, "std." + m)
    sys.stdout.write("\n".join(sorted(set(out))) + "\n")


main()
