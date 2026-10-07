#!/usr/bin/env python3
"""no_german.py -- the one shared "no German in code" guard for all repos (audit 07.10.2026).

One script for every repo instead of five separate checkers (Firn/Certus tools/english/*.py,
OpenPlan tools/english.py, LogicLab scripts/english.check.mts, Osum tools/english renamer).

  python3 no_german.py [--root DIR] [--config no-german.json] [--morphemes FILE]
                       [--count] [--files] [--json] [--summary]
                       [--baseline FILE] [--update-baseline]

Checks (tracked files only, `git ls-files`; symlinks skipped):
  paths        file and folder names (generated/vendored code like node_modules/, vendor/, dist/ is never checked)
  identifiers  code outside comments and strings, split snake_case/camelCase
  comments     line and block comments, Python/shell # comments
  strings      string literals (a German UI text belongs in a catalog, not in code)
Exit code 1 when anything is found (unless --count, which only prints the number).

Ratchet (--baseline FILE, default name no-german.baseline.json in the repo root):
  The file freezes the current count per kind, e.g. {"path": 12, "identifier": 400, "comment": 90, "string": 31}.
  The run fails when ANY kind is higher than frozen (German must only go down) and prints
  a hint to lower the file when a kind got lower. --update-baseline writes the current counts
  (only allowed to lower numbers unless --force is given). Without --baseline file the run is strict (0 allowed).
--summary prints the counts as one JSON line (for project_metric).

Allowlist (no-german.json in the repo root, all keys optional):
  {"skip": ["docs/", "tests/data/"],            # path prefixes that are not checked at all
   "catalogs": ["locale/", "src/i18n/locales/"], # extra prefixes where German text is allowed (UI catalogs);
                                                 # i18n/ locale/ lang/ catalogs/ de.json texts.fi always count as catalogs
   "words": ["lang", "listen"],                 # identifier parts that look German but are English
   "lines": ["MIT-MAGIC-COOKIE"]}               # regex; a matching line is ignored
A single line can also carry the marker `english: ok`.

Heuristic: German words come from the Firn morpheme table (tools/english/morphemes.tsv, the
single vocabulary) plus a short list of unambiguous function words. A part that is also an
English word (/usr/share/dict/words, if present) is dropped. Measured precision on samples
of the audit: strings ~95 %, comments ~95 %, identifiers ~85 % (false hits: short_circuit,
digit_count, listen_*). Tune with "words".
"""
import argparse, bisect, json, os, re, subprocess, sys, collections

FUNCTION_WORDS = set("""der die das den dem ein eine einen einem einer eines und oder aber nicht kein keine keinen
keiner nur noch schon auch sonst damit dass weil wenn dann als wie sind waren wird werden wurde wurden sein haben hatte
hatten koennen konnte muss muessen musste soll sollen darf duerfen fuer von vom mit ohne beim nach ueber unter zwischen
durch gegen seit aus zum zur sich jede jeder jedes alle alles dabei dafuer daraus davon dazu deshalb darum trotzdem
immer selten weniger sehr ganz genau erst zuerst danach spaeter zuvor wieder richtig falsch wichtig moeglich noetig
etwas nichts jetzt heute zeile zeilen datei dateien werte nummer seite seiten""".split())
# also English or too short: never counted
FUNCTION_WORDS -= {"die", "sein", "alle", "seite", "sich", "wie", "oder", "dann", "wieder", "als", "von", "der", "dem", "den"}
FUNCTION_WORDS |= {"der", "dem", "den", "sich", "oder", "wieder", "von"}   # kept: unambiguous in code comments
RE_UML = re.compile("[\u00e4\u00f6\u00fc\u00c4\u00d6\u00dc\u00df]")
SPLIT = re.compile(r"[A-Z]+(?![a-z])|[A-Z][a-z0-9]*|[a-z0-9]+")
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
C_LIKE = {".fi", ".rs", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".cpp", ".cc", ".c", ".h", ".hpp", ".java", ".kt", ".svelte", ".css"}
RX = {
    "c": re.compile(r"//[^\n]*|/\*.*?\*/|\"(?:\\.|[^\"\\\n])*\"|'(?:\\.|[^'\\\n]){0,40}'|`(?:\\.|[^`\\])*`", re.S),
    "py": re.compile(r"#[^\n]*|\"\"\"(?:.|\n)*?\"\"\"|'''(?:.|\n)*?'''|\"(?:\\.|[^\"\\\n])*\"|'(?:\\.|[^'\\\n])*'"),
    "sh": re.compile(r"(?<![\$\w])#[^\n]*|\"(?:\\.|[^\"\\])*\"|'[^']*'"),
}


# Generated, vendored or foreign code is never checked (substring match on the repo path).
FOREIGN = ("node_modules/", "tests/data/", "testdata/", "/generated/", "dist/", "build/", ".archrun/", "public/modforge/",
           "patches_applied/", ".min.", "plugins/", ".test-work/", ".js-work/", "target/", "docs/sdd/", "vendor/", "lib/@",
           ".gauntlet", "tests/golden", "package-lock", "public/assets", ".next/", ".testbuild/", "/third_party/")
# UI catalogs: German text is allowed there (it belongs there).
CATALOG = re.compile(r"(?i)(^|/)(i18n|locale|locales|lang|langs|catalogs?|translations?|l10n)(/|\.|$)|"
                     r"(^|/)(de|de-DE|de_DE)\.(json|toml|fi|ts|js|po|yaml|yml|ini)$|texts?\.fi$")


def load_vocab(path):
    morph = {}
    for line in open(path, encoding="utf8"):
        a = line.rstrip("\n").split("\t")
        if a and len(a[0]) >= 4:
            morph[a[0].lower()] = a[1] if len(a) > 1 else ""
    eng = set()
    if os.path.exists("/usr/share/dict/words"):
        eng = {w.strip().lower() for w in open("/usr/share/dict/words", errors="ignore")}
    return morph, eng


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    ap.add_argument("--config", default=None)
    ap.add_argument("--morphemes", default=None)
    ap.add_argument("--count", action="store_true")
    ap.add_argument("--files", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--summary", action="store_true")
    ap.add_argument("--baseline", default=None)
    ap.add_argument("--update-baseline", action="store_true")
    ap.add_argument("--force", action="store_true")
    a = ap.parse_args()
    root = os.path.abspath(a.root)
    cfgp = a.config or os.path.join(root, "no-german.json")
    cfg = json.load(open(cfgp)) if os.path.exists(cfgp) else {}
    skip = tuple(cfg.get("skip", []))
    catalogs = tuple(cfg.get("catalogs", []))
    allow_words = {w.lower() for w in cfg.get("words", [])}
    allow_lines = [re.compile(x) for x in cfg.get("lines", [])]
    here = os.path.dirname(os.path.abspath(__file__))
    mp = a.morphemes or next((p for p in (os.path.join(here, "morphemes.tsv"),
                                          os.path.join(root, "tools/english/morphemes.tsv"),
                                          "/root/firn/tools/english/morphemes.tsv") if os.path.exists(p)), None)
    if not mp:
        sys.exit("morphemes.tsv not found (--morphemes)")
    morph, eng = load_vocab(mp)
    words = sorted(eng)
    germ = {w for w in morph if w not in eng and w not in allow_words}
    words_re = re.compile(r"\b(" + "|".join(sorted(FUNCTION_WORDS, key=len, reverse=True)) + r")\b", re.I)
    out = subprocess.run(["git", "-C", root, "ls-files", "-z"], capture_output=True).stdout.decode("utf8", "replace")
    found = collections.defaultdict(list)   # file -> [(kind, line, text)]

    def part_is_german(tok):
        for s in SPLIT.findall(tok):
            s = re.sub(r"[0-9]+$", "", s.lower())
            if len(s) >= 4 and s in germ:
                return s
        return None

    for f in (x for x in out.split("\0") if x):
        fp = os.path.join(root, f)
        if f.startswith(skip) or any(m in f for m in FOREIGN) or os.path.islink(fp):
            continue
        for seg in re.split(r"[/.\-]", f):
            for sub in seg.split("_"):
                if part_is_german(sub):
                    found[f].append(("path", 0, f)); break
            else:
                continue
            break
        ext = os.path.splitext(f)[1].lower()
        fam = "c" if ext in C_LIKE else "py" if ext == ".py" else "sh" if ext == ".sh" else None
        if not fam:
            continue
        try:
            t = open(fp, encoding="utf8", errors="replace").read()
        except OSError:
            continue
        if len(t) > 800000 or "\0" in t[:4096]:
            continue
        in_catalog = bool(CATALOG.search(f)) or (f.startswith(catalogs) if catalogs else False)
        code, last = [], 0
        for m in RX[fam].finditer(t):
            code.append(t[last:m.start()]); last = m.end()
            s = m.group(0); ln = t.count("\n", 0, m.start()) + 1
            is_str = s[0] in "\"'`" and not s.startswith(('"""', "'''"))
            body = s[1:-1] if is_str else s
            if "english: ok" in t[max(0, m.start() - 100):m.end() + 60] or any(r.search(s) for r in allow_lines):
                continue
            if is_str:
                if in_catalog or len(body) < 3:
                    continue
                n_m = len({w for w in re.findall(r"[a-z\u00e4\u00f6\u00fc\u00df]+", body.lower()) if len(w) >= 5 and w in germ})
                if RE_UML.search(body) or words_re.search(body) or n_m >= 2:
                    found[f].append(("string", ln, body[:80]))
            else:
                for i, l in enumerate(s.split("\n")):
                    l2 = re.sub(r"`[^`]*`", " ", l)
                    if RE_UML.search(l2) or any(not w.group(0).isupper() for w in words_re.finditer(l2)):
                        found[f].append(("comment", ln + i, l.strip()[:80]))
        code.append(t[last:])
        seen = set()
        for tok in IDENT.findall("".join(code)):
            if tok in seen:
                continue
            seen.add(tok)
            if tok.lower() in allow_words:
                continue
            h = part_is_german(tok)
            if h:
                found[f].append(("identifier", 0, tok))
    total = sum(len(v) for v in found.values())
    kinds = collections.Counter(k for v in found.values() for k, _, _ in v)
    counts = {k: kinds.get(k, 0) for k in ("path", "identifier", "comment", "string")}
    bpath = a.baseline
    if bpath is None and (a.update_baseline or os.path.exists(os.path.join(root, "no-german.baseline.json"))):
        bpath = os.path.join(root, "no-german.baseline.json")
    if a.update_baseline:
        old = json.load(open(bpath)) if os.path.exists(bpath) else None
        if old and not a.force and any(counts[k] > old.get(k, 0) for k in counts):
            print("no-german: refusing to RAISE the baseline (use --force): " + json.dumps(counts), file=sys.stderr)
            sys.exit(1)
        json.dump(counts, open(bpath, "w"), indent=1); open(bpath, "a").write("\n")
        print("no-german: baseline written " + json.dumps(counts), file=sys.stderr); return
    if a.summary:
        print(json.dumps(counts)); return
    if a.count:
        print(total); return
    if a.json:
        print(json.dumps({f: v for f, v in found.items()}, ensure_ascii=False)); return
    if a.files:
        for f, v in sorted(found.items(), key=lambda x: -len(x[1]))[:50]:
            print(f"{len(v):6d}  {f}")
    elif not bpath:
        for f, v in sorted(found.items()):
            for k, ln, tx in v[:200]:
                print(f"{f}:{ln}: {k}: {tx}")
    if bpath:
        base = json.load(open(bpath))
        worse = {k: (base.get(k, 0), counts[k]) for k in counts if counts[k] > base.get(k, 0)}
        better = {k: (base.get(k, 0), counts[k]) for k in counts if counts[k] < base.get(k, 0)}
        print(f"no-german: {total} findings {counts} (baseline {base})", file=sys.stderr)
        if worse:
            # show where: findings by file, most first, so the new German is easy to find
            for f, v in sorted(found.items(), key=lambda x: -len(x[1]))[:15]:
                for k, ln, tx in v[:3]:
                    if k in worse:
                        print(f"  {f}:{ln}: {k}: {tx}", file=sys.stderr)
            print("no-german: FAIL, more German than the baseline allows: " + json.dumps(worse), file=sys.stderr)
            sys.exit(1)
        if better:
            print("no-german: OK, and lower than frozen -> lower the baseline: no_german.py --update-baseline  " + json.dumps(better), file=sys.stderr)
        else:
            print("no-german: OK", file=sys.stderr)
        return
    print(f"no-german: {total} findings ({dict(kinds)}) in {len(found)} files", file=sys.stderr)
    sys.exit(1 if total else 0)


if __name__ == "__main__":
    main()
