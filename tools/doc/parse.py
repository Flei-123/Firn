import importlib.util
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
_spec = importlib.util.spec_from_file_location("gen_libs_index", os.path.join(ROOT, "tools", "gen_libs_index.py"))
libs_index = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(libs_index)

ITEM_RE = re.compile(r"^(?:pub\s+)?(?:unsafe\s+)?(fn|struct|enum|const|type|interface|error|gc class)\s+([A-Za-z_]\w*)(.*)$")
SPDX_RE = re.compile(r"^(SPDX-|expect_|wasm_)")


def doc_above(lines, index):
    out = []
    i = index - 1
    while i >= 0 and lines[i].lstrip().startswith("//"):
        body = lines[i].lstrip()[2:]
        out.append(body[1:] if body.startswith(" ") else body)
        i -= 1
    out.reverse()
    return " ".join(l.strip() for l in out if l.strip() and not SPDX_RE.match(l.strip()))


def signature(kind, name, rest):
    head = rest.split("{")[0].rstrip()
    if kind == "const":
        head = head.split("=")[0].rstrip()
    return "%s %s%s" % (kind, name, head)


def parse_module(path, module):
    text = libs_index.read(path)
    lines = text.split("\n")
    exported = set(libs_index.exports_of(text))
    items = []
    for index, line in enumerate(lines):
        match = ITEM_RE.match(line)
        if not match:
            continue
        kind, name, rest = match.groups()
        if (exported and name not in exported) or (not exported and name.startswith("_")):
            continue
        items.append({"kind": kind, "name": name, "sig": signature(kind, name, rest), "doc": doc_above(lines, index)})
    return {"name": module, "purpose": libs_index.describe(text) or "", "items": items}


def module_name(source_dir, path):
    rel = os.path.relpath(path, source_dir)[: -len(".fi")]
    return rel.replace(os.sep, ".")


def read_manifest(directory):
    fields = {"package": os.path.basename(os.path.abspath(directory)), "version": "", "source": "."}
    with open(os.path.join(directory, "firn.package"), encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if len(parts) >= 2 and parts[0] in fields:
                fields[parts[0]] = parts[1]
    return fields


def collect(target, lib_root):
    if os.path.isfile(target):
        return os.path.basename(target), "", [parse_module(target, os.path.basename(target)[:-3])]
    if os.path.isdir(target):
        manifest = read_manifest(target) if os.path.isfile(os.path.join(target, "firn.package")) else {"package": os.path.basename(os.path.abspath(target)), "version": "", "source": "."}
        source = os.path.join(target, manifest["source"])
        paths = []
        for dirpath, dirnames, filenames in os.walk(source):
            dirnames[:] = sorted(d for d in dirnames if not d.startswith(".") and d not in ("tests", "dist"))
            paths += [os.path.join(dirpath, f) for f in sorted(filenames) if f.endswith(".fi")]
        return manifest["package"], manifest["version"], [parse_module(p, module_name(source, p)) for p in paths]
    as_path = os.path.join(lib_root, *target.split(".")) + ".fi"
    if os.path.isfile(as_path):
        return target, "", [parse_module(as_path, target)]
    return None
