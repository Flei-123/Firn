#!/usr/bin/env python3
import os
import re
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
STRUCT_RE = re.compile(r"^\s*(?:(gc)\s+)?(?:class|struct)\s+(\w+)\s*(?:\[[^\]]*\])?\s*\{")
FN_RE = re.compile(r"^(?:pub\s+)?fn\s+(\w+)")
ALLOC_RE = re.compile(r"heap_alloc\(")
CAST_RE = re.compile(r"as\s+\*mut\s+(?:[A-Za-z_]\w*\.)*([A-Z]\w*)")
SIZEOF_RE = re.compile(r"heap_alloc\(\s*size_of\[\s*(?:[A-Za-z_]\w*\.)*(\w+)\s*\]")
ROOTED_RE = re.compile(r"gc_root_add|gc_track|gc_root_register|heapobj\.zeroed|rooted")
DECL_RE = re.compile(r"\*mut\s+(?:[A-Za-z_]\w*\.)*([A-Z]\w*)\s*=\s*.*heap_alloc\(")


def strip_comments(text):
    out = []
    for line in text.split("\n"):
        i = line.find("//")
        out.append(line if i < 0 else line[:i])
    return out


def source_files(dirs):
    for d in dirs:
        for base, _, names in os.walk(d):
            for n in sorted(names):
                if n.endswith(".fi"):
                    yield os.path.join(base, n)


def parse_structs(files):
    structs = {}
    gc_classes = set()
    for path in files:
        lines = strip_comments(open(path, encoding="utf-8", errors="replace").read())
        i = 0
        while i < len(lines):
            m = STRUCT_RE.match(lines[i])
            if not m:
                i += 1
                continue
            name = m.group(2)
            if m.group(1):
                gc_classes.add(name)
            body = lines[i].split("{", 1)[1]
            depth = 1 + body.count("{") - body.count("}")
            fields = [body]
            while depth > 0 and i + 1 < len(lines):
                i += 1
                depth += lines[i].count("{") - lines[i].count("}")
                fields.append(lines[i])
            types = []
            for chunk in " ".join(fields).split(","):
                if ":" in chunk:
                    types.append(chunk.split(":", 1)[1].strip().rstrip("}").strip())
            structs.setdefault(name, []).append(types)
            i += 1
    return structs, gc_classes


def type_holds_gc(t, structs, gc_classes, seen):
    t = t.strip()
    if t == "str" or t.startswith("Gc[") or t.startswith("fn(") or t.startswith("fn "):
        return True
    if t.startswith("["):
        inner = t[1:].rsplit(";", 1)[0]
        return type_holds_gc(inner, structs, gc_classes, seen)
    if t.startswith("*") or t.startswith("Weak["):
        return False
    base = re.sub(r"\[.*$", "", t.split(".")[-1]).strip()
    if base in gc_classes:
        return True
    if base in structs and base not in seen:
        defs = structs[base]
        return all(any(type_holds_gc(f, structs, gc_classes, seen | {base}) for f in d) for d in defs)
    return False


def functions(lines):
    starts = [i for i, l in enumerate(lines) if FN_RE.match(l)]
    for k, s in enumerate(starts):
        e = starts[k + 1] if k + 1 < len(starts) else len(lines)
        yield FN_RE.match(lines[s]).group(1), s, e


GENERIC_FN_RE = re.compile(r"^(?:pub\s+)?fn\s+(\w+)\s*\[\s*(\w+)")


def alloc_wrappers(files):
    names = set()
    for path in files:
        lines = strip_comments(open(path, encoding="utf-8", errors="replace").read())
        for fname, s, e in functions(lines):
            m = GENERIC_FN_RE.match(lines[s])
            if not m:
                continue
            body = "\n".join(lines[s:e])
            if re.search(r"heap_alloc\(\s*size_of\[\s*%s\s*\]" % re.escape(m.group(2)), body) and not ROOTED_RE.search(body):
                names.add(fname)
    return names


def scan(files, structs, gc_classes):
    hits = []
    wrappers = alloc_wrappers(files)
    call_re = re.compile(r"\b(?:%s)\s*\[\s*(?:[A-Za-z_]\w*\.)*(\w+)\s*\]\s*\(" % "|".join(sorted(wrappers))) if wrappers else None
    for path in files:
        lines = strip_comments(open(path, encoding="utf-8", errors="replace").read())
        for fname, s, e in functions(lines):
            body = "\n".join(lines[s:e])
            if ROOTED_RE.search(body):
                continue
            found = set()
            for l in lines[s:e]:
                if not ALLOC_RE.search(l):
                    continue
                for rx in (CAST_RE, SIZEOF_RE, DECL_RE):
                    for m in rx.finditer(l):
                        found.add(m.group(1))
            if call_re:
                for l in lines[s:e]:
                    for m in call_re.finditer(l):
                        found.add(m.group(1))
            for t in sorted(found):
                if t in structs and type_holds_gc(t, structs, gc_classes, frozenset()):
                    hits.append("%s:%s:%s" % (os.path.relpath(path, ROOT), fname, t))
    return hits


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    update = "--update" in sys.argv
    dirs = args or [os.path.join(ROOT, "lib")]
    known_path = os.path.join(ROOT, "tools", "lint", "heap_gc_fields.known")
    files = list(source_files(dirs))
    structs, gc_classes = parse_structs(files)
    hits = sorted(set(scan(files, structs, gc_classes)))
    if update:
        open(known_path, "w").write("\n".join(hits) + ("\n" if hits else ""))
        print("recorded %d" % len(hits))
        return 0
    known = set(open(known_path).read().split()) if os.path.exists(known_path) else set()
    fresh = [h for h in hits if h not in known]
    for h in fresh:
        print("LINT heap_gc_fields: %s allocates a struct with str/Gc fields in raw heap memory without gc_root_add/gc_track/heapobj.zeroed" % h)
    if "--all" in sys.argv:
        print("total %d, known %d, new %d" % (len(hits), len(known), len(fresh)))
    return 1 if fresh else 0


if __name__ == "__main__":
    sys.exit(main())
