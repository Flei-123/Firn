#!/usr/bin/env python3
"""Generate docs/LIBS.md: an index of every module under lib/.

Deterministic (sorted, no timestamps).  Usage:
    python3 tools/gen_libs_index.py                 # write docs/LIBS.md
    python3 tools/gen_libs_index.py --out FILE      # write somewhere else
    python3 tools/gen_libs_index.py --check         # exit 1 if docs/LIBS.md is stale

Per module: name, one-line purpose (from the file's header comment), public
API (names in the `export { ... }` block), platform variants, tests that
import it, and topic docs that mention it.  Modules without a header
comment are marked NO DESCRIPTION (never invented).

The --check gate compares only what is derived from lib/ itself (a
fingerprint over names, purposes, exports and variants).  Test and doc
columns may lag a little; they refresh on the next regeneration.
"""
import hashlib
import os
import re
import sys
from collections import Counter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIB = os.path.join(ROOT, "lib")
DOCS = os.path.join(ROOT, "docs")
TESTS = os.path.join(ROOT, "tests")
OUT_DEFAULT = os.path.join(DOCS, "LIBS.md")
REGEN_CMD = "bash tools/gen_libs_index.sh"

API_LIMIT = 6
DESC_LIMIT = 130
TESTS_LIMIT = 3
DOCS_LIMIT = 2
FP_MARK = "lib-fingerprint:"

# Docs that are round logs / reports, not topic documentation.
DOC_SKIP = re.compile(
    r"^(ROUND|TEMPO|XMM|GAP|MODULE_REPORTS|BENCHMARKS|LIBS\.md|RELEASE-FAST|"
    r"CERTUS-COMPILER-MERGE|OSUM-ARCH|OSUM-K)"
)
NOISE = re.compile(
    r"^(SPDX-License|expect_|wasm_|Provenance:|Origin:|Copyright|-{3,}|={3,})"
)
DESC_LINE = re.compile(r"^(?:[\w@./+-]+\.fi|[\w@.]+)\s*(?:--|—|–)\s*(.*)$")


def read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def header_comments(text):
    """Comment lines (marker stripped) before the first line of code.
    Blank lines and `profile ...` lines (kernel/freestanding marker) are skipped."""
    out = []
    for raw in text.split("\n")[:120]:
        s = raw.strip()
        if s.startswith("//"):
            body = s[2:]
            if body.startswith(" "):
                body = body[1:]
            out.append(body.rstrip())
        elif s == "":
            out.append("")
        elif re.match(r"profile\s+\w+$", s):
            continue
        else:
            break
    return out


def first_sentence(s):
    m = re.search(r"[.!?](?:\s|$)", s)
    if m and m.end() >= 25:
        s = s[: m.end()].strip()
    s = re.sub(r"\s+", " ", s).strip()
    if len(s) > DESC_LIMIT:
        s = s[: DESC_LIMIT - 3].rstrip() + "..."
    return s


def describe(text):
    lines = [l for l in header_comments(text)]
    # 1) `<path or module> -- purpose` line, continued until a blank comment.
    for i, l in enumerate(lines):
        m = DESC_LINE.match(l.strip())
        if m and not NOISE.match(l.strip()):
            parts = [m.group(1).strip()] if m.group(1).strip() else []
            for nxt in lines[i + 1 :]:
                if nxt.strip() == "":
                    if parts:
                        break
                    continue
                parts.append(nxt.strip())
                if len(" ".join(parts)) > 400:
                    break
            d = first_sentence(" ".join(parts))
            if d:
                return d
    # 2) generated files
    for l in lines:
        if l.strip().startswith("GENERATED"):
            return first_sentence(l.strip())
    # 3) first meaningful comment line
    for l in lines:
        s = l.strip()
        if not s or NOISE.match(s) or re.fullmatch(r"[\w@./+-]+\.fi", s):
            continue
        return first_sentence(s)
    return None


EXPORT_RE = re.compile(r"^export\s*\{(.*?)\}", re.S | re.M)


def exports_of(text):
    names = []
    for m in EXPORT_RE.finditer(text):
        body = re.sub(r"//[^\n]*", "", m.group(1))
        for n in body.split(","):
            n = n.strip()
            if n and re.fullmatch(r"[A-Za-z_]\w*", n):
                names.append(n)
    return names


def api_summary(names):
    if not names:
        return "-"
    types = [n for n in names if n[0].isupper() and not n.isupper()]
    fns = [n for n in names if n[0].islower() or n[0] == "_"]
    consts = [n for n in names if n.isupper() or (n[0].isupper() and n not in types)]
    ordered = types + fns + consts
    shown = ordered[:API_LIMIT]
    s = ", ".join("`%s`" % n for n in shown)
    if len(ordered) > API_LIMIT:
        s += " +%d" % (len(ordered) - API_LIMIT)
    return s


def scan_lib():
    """module name -> {'files': [rel paths], 'variants': set, 'dir': top}"""
    mods = {}
    for dp, dns, fns in os.walk(LIB):
        dns.sort()
        for fn in sorted(fns):
            if not fn.endswith(".fi"):
                continue
            rel = os.path.relpath(os.path.join(dp, fn), LIB).replace(os.sep, "/")
            parts = rel[:-3].split("/")
            variant = None
            if parts[0].startswith("@"):
                variant = parts[0][1:]
                parts = parts[1:]
            last = parts[-1]
            if "." in last:
                last, variant2 = last.rsplit(".", 1)
                parts[-1] = last
                variant = variant2 if variant is None else variant + "/" + variant2
            name = ".".join(parts)
            e = mods.setdefault(name, {"files": [], "variants": set(), "base": False})
            e["files"].append(rel)
            if variant:
                e["variants"].add(variant)
            else:
                e["base"] = True
    return mods


def top_of(name):
    return name.split(".")[0] if "." in name else "(root)"


def scan_tests():
    by_mod = {}
    if not os.path.isdir(TESTS):
        return by_mod
    imp = re.compile(r"^\s*import\s+([A-Za-z_][\w.]*)", re.M)
    for fn in sorted(os.listdir(TESTS)):
        if not fn.endswith(".fi"):
            continue
        m = re.match(r"(\d+)_", fn)
        if not m:
            continue
        num = m.group(1)
        for mod in set(imp.findall(read(os.path.join(TESTS, fn)))):
            by_mod.setdefault(mod, set()).add(num)
    return by_mod


def scan_docs():
    """Return (topic docs, per-doc Counter of tokens)."""
    docs = {}
    tok_re = re.compile(r"lib/[\w@/.\-]+?\.fi|[A-Za-z_][\w]*(?:\.[A-Za-z_]\w*)+")
    dir_re = re.compile(r"lib/(@?[\w\-]+)/")
    for fn in sorted(os.listdir(DOCS)):
        if not fn.endswith(".md") or DOC_SKIP.match(fn):
            continue
        text = read(os.path.join(DOCS, fn))
        docs[fn] = (Counter(tok_re.findall(text)), Counter(dir_re.findall(text)))
    return docs


def esc(s):
    return s.replace("|", "\\|")


def build(with_extras=True):
    mods = scan_lib()
    tests = scan_tests() if with_extras else {}
    docs = scan_docs() if with_extras else {}

    rows = {}  # top -> list of row dicts
    for name in sorted(mods):
        e = mods[name]
        files = sorted(e["files"], key=lambda f: (f.count(".") > 1 or f.startswith("@"), f))
        desc = None
        exps = []
        for f in files:
            text = read(os.path.join(LIB, f))
            if desc is None:
                desc = describe(text)
            if not exps:
                exps = exports_of(text)
        rel0 = [f for f in files if not f.startswith("@") and f.count(".") == 1]
        relpath = "lib/" + (rel0[0] if rel0 else files[0])
        # docs mentioning this module
        dl = []
        if with_extras:
            scored = []
            for dfn, (toks, _d) in docs.items():
                c = toks.get(name, 0) + toks.get(relpath, 0)
                if c:
                    scored.append((-c, dfn))
            scored.sort()
            dl = [d for _c, d in scored[:DOCS_LIMIT]]
        rows.setdefault(top_of(name), []).append(
            {
                "name": name,
                "desc": desc,
                "exports": exps,
                "variants": sorted(e["variants"]),
                "tests": sorted(tests.get(name, ())),
                "docs": dl,
                "path": relpath,
            }
        )
    return rows, docs


def fingerprint(rows):
    h = hashlib.sha256()
    for top in sorted(rows):
        for r in rows[top]:
            h.update(
                (
                    "%s\x00%s\x00%s\x00%s\n"
                    % (r["name"], r["desc"] or "", ",".join(r["exports"]), ",".join(r["variants"]))
                ).encode()
            )
    return h.hexdigest()[:16]


def dir_docs(top, docs):
    scored = []
    norm = top.lower().replace("-", "").replace("_", "")
    for dfn, (_t, dirs) in docs.items():
        stem = dfn[:-3].lower().replace("-", "").replace("_", "")
        c = dirs.get(top, 0)
        if stem == norm:
            c += 1000
        if c >= 3:
            scored.append((-c, dfn))
    scored.sort()
    return [d for _c, d in scored[:3]]


def render(rows, docs, fp):
    out = []
    w = out.append
    nmods = sum(len(v) for v in rows.values())
    nodesc = [(t, r["name"]) for t in sorted(rows) for r in rows[t] if not r["desc"]]
    w("# Firn library index (lib/)")
    w("")
    w("GENERATED by `tools/gen_libs_index.py` -- do not edit by hand.")
    w("")
    w("- **Before writing a new lib or function: read this file / `rg` in `lib/`. Do not rebuild what exists.**")
    w("- Regenerate: `%s` (after every rebase onto main, never hand-merge this file)." % REGEN_CMD)
    w("- Gate: `tools/hygiene/check.sh` fails when this index no longer matches `lib/`.")
    w("- Purpose = first header comment of the file; missing = **NO DESCRIPTION** (list at the end).")
    w("- API = names in the module's `export { ... }` block (types, then functions, then constants; limited).")
    w("- Platforms = `name.<os>.fi` twins and `lib/@<os>/...` overlays. Tests = `tests/NNNN_*.fi` that `import` the module.")
    w("- Topic docs are only linked, not repeated here (see also docs/APPLICATION_LIBS.md, docs/MC-LIBS.md, docs/ASYNC.md, docs/COMPRESSION.md).")
    w("")
    w("%d modules in %d lib directories. <!-- %s %s -->" % (nmods, len(rows), FP_MARK, fp))
    w("")
    w("## Directories")
    w("")
    w("| dir | modules | platform variants | with tests | NO DESC | topic docs |")
    w("|---|---|---|---|---|---|")
    for top in sorted(rows):
        rs = rows[top]
        nvar = sum(1 for r in rs if r["variants"])
        ntest = sum(1 for r in rs if r["tests"])
        nd = sum(1 for r in rs if not r["desc"])
        dd = ", ".join("[%s](%s)" % (d[:-3], d) for d in dir_docs(top, docs)) or "-"
        w("| [%s](#%s) | %d | %d | %d | %d | %s |" % (top, anchor(top), len(rs), nvar, ntest, nd, dd))
    w("")
    for top in sorted(rows):
        rs = rows[top]
        w("## %s" % top)
        w("")
        w("| module | purpose | API | platforms | tests | docs |")
        w("|---|---|---|---|---|---|")
        for r in rs:
            desc = esc(r["desc"]) if r["desc"] else "**NO DESCRIPTION**"
            plats = ", ".join(r["variants"]) or "-"
            tl = r["tests"]
            ts = ", ".join(tl[:TESTS_LIMIT]) + (" +%d" % (len(tl) - TESTS_LIMIT) if len(tl) > TESTS_LIMIT else "")
            ds = ", ".join("[%s](%s)" % (d[:-3], d) for d in r["docs"]) or "-"
            w("| `%s` | %s | %s | %s | %s | %s |" % (r["name"], desc, esc(api_summary(r["exports"])), plats, ts or "-", ds))
        w("")
    w("## NO DESCRIPTION")
    w("")
    w("%d modules have no header comment. Add one (`// lib/x/y.fi -- what it is for`) instead of guessing." % len(nodesc))
    w("")
    cur = None
    for t, n in nodesc:
        if t != cur:
            cur = t
            w("- **%s**: " % t + ", ".join("`%s`" % x for tt, x in nodesc if tt == t))
    w("")
    return "\n".join(out)


def anchor(s):
    return re.sub(r"[^a-z0-9_-]", "", s.lower().replace(" ", "-"))


def main(argv):
    out_path = OUT_DEFAULT
    check = False
    i = 0
    while i < len(argv):
        if argv[i] == "--out":
            out_path = argv[i + 1]
            i += 1
        elif argv[i] == "--check":
            check = True
        else:
            print("usage: gen_libs_index.py [--out FILE] [--check]", file=sys.stderr)
            return 2
        i += 1
    rows, docs = build()
    fp = fingerprint(rows)
    text = render(rows, docs, fp)
    if check:
        try:
            cur = read(OUT_DEFAULT)
        except OSError:
            print("LIBS: docs/LIBS.md is missing -- run: %s" % REGEN_CMD)
            return 1
        m = re.search(re.escape(FP_MARK) + r" ([0-9a-f]+)", cur)
        if not m or m.group(1) != fp:
            print("LIBS: docs/LIBS.md is stale (lib/ changed) -- run: %s" % REGEN_CMD)
            return 1
        if cur != text:
            print("LIBS: note: tests/docs columns of docs/LIBS.md lag behind -- refresh with: %s" % REGEN_CMD)
        print("LIBS: ok")
        return 0
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print("wrote %s (%d modules)" % (out_path, sum(len(v) for v in rows.values())))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
