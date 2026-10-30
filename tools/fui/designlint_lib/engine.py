import os

from . import allow, calls, lexer, markers
from .rules import PROJECT_REGISTRY, REGISTRY


class Context:
    def __init__(self, path, src):
        self.path = path
        self.tokens = lexer.tokenize(src)
        self.calls, self.fields, self.assigns = calls.extract(self.tokens)


def collect_files(paths):
    files = []
    for p in paths:
        if os.path.isdir(p):
            for root, dirs, names in os.walk(p):
                dirs[:] = sorted(d for d in dirs if not d.startswith(".") and d not in ("vendor", "node_modules", "target"))
                files.extend(os.path.join(root, n) for n in sorted(names) if n.endswith(".fi"))
        elif os.path.isfile(p):
            files.append(p)
    return files


def display_path(path):
    rel = os.path.relpath(path)
    return path if rel.startswith("..") else rel


def lint_source(path, src, rules=None):
    ctx = Context(path, src)
    marks = markers.parse(src)
    found = []
    seen = set()
    for rule_id, _summary, fn in REGISTRY:
        if rules and rule_id not in rules:
            continue
        for f in fn(ctx):
            key = (f["line"], f["rule"], f["text"])
            if key in seen or markers.suppressed(marks, f):
                continue
            seen.add(key)
            found.append(f)
    found.sort(key=lambda f: (f["line"], f["rule"]))
    return found


def lint_project(files, rules=None):
    out = []
    for rule_id, _summary, fn in PROJECT_REGISTRY:
        if rules and rule_id not in rules:
            continue
        out.extend(fn(files))
    return out


def lint_paths(paths, rules=None):
    out = []
    loaded = []
    for path in collect_files(paths):
        with open(path, encoding="utf-8", errors="replace") as fh:
            src = fh.read()
        loaded.append((display_path(path), src))
        out.extend(lint_source(display_path(path), src, rules))
    out.extend(lint_project(loaded, rules))
    return out


def split_allowed(found, entries):
    kept = []
    skipped = []
    for f in found:
        (skipped if allow.allowed(entries, f) else kept).append(f)
    return kept, skipped
