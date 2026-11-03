#!/usr/bin/env python3
import argparse
import fnmatch
import os
import subprocess
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from rn_forms import Forms, NAME_RE
from rn_report import render

LEGACY_KEYS = ("env_prefix", "package_file", "lock_file", "cli")
SKIP_DIRS = ("staff_scratch/", "compiler/target/")


def git(root, *args, check=True):
    r = subprocess.run(["git", "-C", str(root), *args], capture_output=True)
    if check and r.returncode != 0:
        sys.exit("git %s failed: %s" % (" ".join(args), r.stderr.decode(errors="replace").strip()))
    return r.stdout


def tracked(root):
    raw = git(root, "ls-files", "-z", "--stage").split(b"\0")
    out = []
    for rec in raw:
        if not rec:
            continue
        meta, path = rec.split(b"\t", 1)
        mode = meta.split()[0].decode()
        out.append((path.decode("utf-8", "surrogateescape"), mode))
    return out


def excluded(path, patterns):
    if path.startswith(SKIP_DIRS):
        return True
    return any(fnmatch.fnmatch(path, p) or path.startswith(p.rstrip("/") + "/") for p in patterns)


def read_text(full):
    data = full.read_bytes()
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def map_path(forms, path):
    return "/".join(forms.replace(part) for part in path.split("/"))


def scan(root, forms, excludes):
    stats = {"files": 0, "lines": 0, "occurrences": 0, "binary": 0, "symlinks": 0, "arrays": 0, "bytes": [],
             "by_category": Counter(), "by_dir": Counter(), "env_names": set()}
    edges, odd = Counter(), Counter()
    changes, renames, symlinks = [], [], []
    for path, mode in tracked(root):
        if excluded(path, excludes):
            continue
        new_path = map_path(forms, path)
        if new_path != path:
            renames.append((path, new_path))
        full = root / path
        if mode == "120000":
            target = os.readlink(full)
            if forms.count(target):
                stats["symlinks"] += 1
                symlinks.append((path, forms.replace(target)))
            continue
        text = read_text(full)
        if text is None:
            stats["binary"] += 1
            continue
        n = forms.count(text)
        for tok in set(forms.odd_case(text)):
            odd[tok] += 1
        if not n:
            continue
        stats["files"] += 1
        if forms.byte_sequences(text):
            stats["bytes"].append(path)
        stats["arrays"] += forms.count_arrays(text)
        stats["occurrences"] += n
        lines = sum(1 for line in text.splitlines() if forms.count(line))
        stats["lines"] += lines
        stats["by_dir"][path.split("/")[0] if "/" in path else "(root)"] += lines
        for tok in forms.tokens(text):
            cat = forms.classify(tok)
            stats["by_category"][cat] += 1
            if cat == "env_variable":
                stats["env_names"].add(tok)
            reason = forms.edge_reason(tok)
            if reason:
                edges[(tok, reason)] += 1
        changes.append(path)
    return stats, edges, odd, changes, renames, symlinks


def find_conflicts(root, renames, forms):
    existing = {p for p, _ in tracked(root)}
    moved = {o for o, _ in renames}
    seen, conflicts = {}, []
    for old, new in renames:
        if new in existing and new not in moved:
            conflicts.append("target exists: %s -> %s" % (old, new))
        if new in seen:
            conflicts.append("two sources map to %s: %s, %s" % (new, seen[new], old))
        seen[new] = old
    return conflicts


def read_brand(root):
    p = root / "brand.toml"
    values = {}
    if not p.is_file():
        return values
    section = ""
    for line in p.read_text().splitlines():
        line = line.strip()
        if line.startswith("["):
            section = line.strip("[]")
        elif "=" in line and not line.startswith("#") and section == "":
            k, v = line.split("=", 1)
            values[k.strip()] = v.strip().strip('"')
    return values


def write_legacy(root, old_values):
    p = root / "brand.toml"
    text = p.read_text()
    head = text.split("[legacy]")[0].rstrip() + "\n\n[legacy]\n"
    for k in LEGACY_KEYS:
        head += '%s = "%s"\n' % (k, old_values.get(k, ""))
    p.write_text(head)


def sync_generated(root):
    script = root / "tools" / "brand.sh"
    target = root / "lib" / "toolchain" / "tcbrand.fi"
    if script.is_file() and target.is_file():
        out = subprocess.run(["bash", str(script), "--emit-fi"], capture_output=True, check=True, cwd=root)
        target.write_bytes(out.stdout)


def apply(root, forms, changes, renames, symlinks):
    for path in changes:
        full = root / path
        data = full.read_bytes().decode("utf-8")
        full.write_bytes(forms.replace(data).encode("utf-8"))
    for path, target in symlinks:
        full = root / path
        full.unlink()
        os.symlink(target, full)
    for old, new in sorted(renames, key=lambda r: -r[0].count("/")):
        (root / new).parent.mkdir(parents=True, exist_ok=True)
        git(root, "mv", "--", old, new)


def residual(root, forms, excludes, keep):
    left = Counter()
    for path, mode in tracked(root):
        if excluded(path, excludes) or mode == "120000":
            continue
        text = read_text(root / path)
        if text is None:
            continue
        n = sum(1 for line in text.splitlines() if forms.count(line))
        if n and not (keep and path == "brand.toml"):
            left[path] = n
    return left


def main():
    ap = argparse.ArgumentParser(description="Rename a project name across a git repository.")
    ap.add_argument("--from", dest="old", required=True)
    ap.add_argument("--to", dest="new", required=True)
    ap.add_argument("--root", default=".")
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--keep-aliases", action="store_true")
    ap.add_argument("--exclude", action="append", default=[])
    ap.add_argument("--report")
    args = ap.parse_args()
    if not NAME_RE.match(args.old) or not NAME_RE.match(args.new):
        sys.exit("names must match [a-z][a-z0-9_]*")
    if args.old == args.new:
        sys.exit("--from and --to are equal")
    root = Path(args.root).resolve()
    forms = Forms(args.old, args.new)
    stats, edges, odd, changes, renames, symlinks = scan(root, forms, args.exclude)
    conflicts = find_conflicts(root, renames, forms)
    warnings = []
    probe = Forms(args.new, args.old)
    pre = 0
    for path, mode in tracked(root):
        if mode != "120000" and not excluded(path, args.exclude):
            t = read_text(root / path)
            if t is not None and probe.count(t):
                pre += 1
    if pre:
        warnings.append("target name '%s' already occurs in %d files; a rename back is not exact" % (args.new, pre))
    if args.keep_aliases and not (root / "brand.toml").is_file():
        warnings.append("--keep-aliases needs brand.toml; no alias section will be written")
    for path in stats["bytes"]:
        warnings.append("name spelled as byte values (not renamed, fix by hand): %s" % path)
    report = render(stats, renames, edges, odd, conflicts, warnings)
    if args.apply and not args.dry_run:
        if conflicts:
            sys.stdout.write(report)
            sys.exit("conflicts found, nothing changed")
        if git(root, "status", "--porcelain", "--untracked-files=no").strip():
            sys.exit("working tree has uncommitted changes; commit or stash first")
        old_brand = read_brand(root)
        apply(root, forms, changes, renames, symlinks)
        if args.keep_aliases and (root / "brand.toml").is_file():
            write_legacy(root, old_brand)
            sync_generated(root)
        left = residual(root, forms, args.exclude, args.keep_aliases)
        report += "\nAPPLIED. files still containing '%s': %d\n" % (args.old, len(left))
        for path, n in left.most_common(20):
            report += "  %s (%d lines)\n" % (path, n)
    else:
        report = "DRY RUN (use --apply to change files)\n\n" + report
    sys.stdout.write(report)
    if args.report:
        Path(args.report).write_text(report)


if __name__ == "__main__":
    main()
