#!/usr/bin/env python3
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.realpath(__file__))
ROOT = os.path.realpath(os.path.join(HERE, "..", ".."))
OUTPUT = os.path.join(HERE, "static", "completions.json")

DECL = re.compile(r"^(?:unsafe\s+)?(?:gc\s+)?(fn|struct|enum|error|interface|const|type|static)\s+([A-Za-z_][A-Za-z0-9_]*)")
TOP_LEVEL_FN = re.compile(r"^(?:unsafe\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")
EXPORT_START = re.compile(r"^export\s*\{")
INCLUDE = re.compile(r"^//#include\s+(\S+)")
IMPL = re.compile(r"^impl\s+([A-Za-z_][A-Za-z0-9_]*)")
METHOD = re.compile(r"^\s+fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*(\[[^\]]*\])?\s*\((\*mut\s+self|\*self|self)\b")
KEYWORD_BLOCK = re.compile(r"const KEYWORDS: &\[&str\] = &\[(.*?)\];", re.S)
PRELUDE_WRAPPER = re.compile(r'\("(\w+)",\s*"(fn [^"]*)"\)')
SKIPPED_MODULES = {"rt", "core"}

METHOD_SOURCES = {
    "Vec": ("lib/rt/vec.fi", {"Vec"}),
    "Map": ("lib/rt/map.fi", {"Map"}),
    "Set": ("lib/rt/map.fi", {"Set"}),
    "str": ("lib/std/str.fi", {"Span"}),
    "str+": ("lib/std/strmethods.fi", {"StrMethods"}),
    "int": ("lib/std/nummethods.fi", {"i32", "i64", "usize"}),
    "int+": ("lib/std/tostr.fi", {"i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize"}),
    "float": ("lib/std/nummethods.fi", {"f64"}),
    "float+": ("lib/std/tostr.fi", {"f64", "f32"}),
    "bool": ("lib/std/tostr.fi", {"bool"}),
}


def collapse(text):
    return re.sub(r"\s+", " ", text).strip()


def read_lines(path, seen=None):
    seen = seen if seen is not None else set()
    real = os.path.realpath(path)
    if real in seen or not os.path.isfile(real):
        return []
    seen.add(real)
    lines = []
    with open(real, encoding="utf-8") as handle:
        for line in handle.read().split("\n"):
            lines.append(line)
            match = INCLUDE.match(line)
            if match:
                lines += read_lines(os.path.join(ROOT, match.group(1)), seen)
    return lines


def exported_names(lines):
    names = []
    index = 0
    while index < len(lines):
        if EXPORT_START.match(lines[index]):
            text = ""
            while index < len(lines):
                stripped = re.sub(r"//.*", "", lines[index])
                text += " " + stripped
                if "}" in stripped:
                    break
                index += 1
            body = text[text.index("{") + 1:text.index("}")]
            names += [name.strip() for name in body.split(",") if name.strip()]
        index += 1
    return names


def signature_until_body(lines, start):
    parts = []
    for line in lines[start:start + 12]:
        cleaned = re.sub(r"//.*", "", line)
        if "{" in cleaned:
            parts.append(cleaned.split("{", 1)[0])
            break
        parts.append(cleaned)
        if cleaned.rstrip().endswith(")") and "->" not in cleaned:
            break
    return collapse(" ".join(parts))


def declaration_entry(kind, name, lines, start):
    if kind == "fn":
        return {"name": name, "kind": "function", "sig": signature_until_body(lines, start)}
    if kind == "const" or kind == "static":
        return {"name": name, "kind": "constant", "sig": collapse(re.sub(r"//.*", "", lines[start]).split("=", 1)[0])}
    return {"name": name, "kind": "type", "sig": kind + " " + name}


def module_entries(path):
    lines = read_lines(path)
    names = set(exported_names(lines))
    if not names:
        names = {match.group(1) for match in map(TOP_LEVEL_FN.match, lines) if match and not match.group(1).startswith("_")}
    if not names:
        return []
    found = {}
    for index, raw in enumerate(lines):
        match = DECL.match(raw)
        if match and match.group(2) in names and match.group(2) not in found:
            found[match.group(2)] = declaration_entry(match.group(1), match.group(2), lines, index)
    for name in names:
        found.setdefault(name, {"name": name, "kind": "function", "sig": name})
    return sorted(found.values(), key=lambda entry: entry["name"])


def std_modules():
    directory = os.path.join(ROOT, "lib", "std")
    result = {}
    for name in sorted(os.listdir(directory)):
        stem, extension = os.path.splitext(name)
        if extension != ".fi" or "." in stem or stem in SKIPPED_MODULES:
            continue
        entries = module_entries(os.path.join(directory, name))
        if entries:
            result[stem] = entries
    return result


def method_entries(path, impl_names):
    lines = read_lines(os.path.join(ROOT, path))
    entries = {}
    active = False
    for index, line in enumerate(lines):
        impl = IMPL.match(line)
        if impl:
            active = impl.group(1) in impl_names
            continue
        if not active:
            continue
        match = METHOD.match(line)
        if match and match.group(1) not in entries:
            signature = signature_until_body(lines, index)
            signature = re.sub(r"\((\*mut\s+self|\*self|self)\s*,?\s*", "(", signature).replace("(,", "(")
            entries[match.group(1)] = {"name": match.group(1), "kind": "method", "sig": signature}
    return entries


def methods():
    grouped = {}
    for key, (path, impl_names) in METHOD_SOURCES.items():
        group = key.rstrip("+")
        grouped.setdefault(group, {}).update(
            {name: entry for name, entry in method_entries(path, impl_names).items() if name not in grouped.get(group, {})}
        )
    return {group: sorted(entries.values(), key=lambda entry: entry["name"]) for group, entries in grouped.items()}


def keywords():
    with open(os.path.join(ROOT, "compiler", "src", "lsp.rs"), encoding="utf-8") as handle:
        block = KEYWORD_BLOCK.search(handle.read()).group(1)
    return re.findall(r'"([a-z]+)"', block)


def prelude_functions():
    with open(os.path.join(ROOT, "compiler", "src", "prelude.rs"), encoding="utf-8") as handle:
        wrappers = [{"name": name, "kind": "function", "sig": re.sub(r"\s*\{.*$", "", sig)} for name, sig in PRELUDE_WRAPPER.findall(handle.read())]
    wrappers.append({"name": "format", "kind": "function", "sig": "fn format(template: str, ..args) -> str"})
    return wrappers


def build():
    return {
        "keywords": keywords(),
        "prelude": prelude_functions(),
        "modules": std_modules(),
        "methods": methods(),
    }


def main():
    text = json.dumps(build(), separators=(",", ":"), sort_keys=True) + "\n"
    if len(sys.argv) > 1 and sys.argv[1] == "--check":
        with open(OUTPUT, encoding="utf-8") as handle:
            return 0 if handle.read() == text else 1
    with open(OUTPUT, "w", encoding="utf-8") as handle:
        handle.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
