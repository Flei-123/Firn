#!/usr/bin/env python3
"""Random trees, patterns and the answers of Python's fnmatch / glob modules.

    gen.py SEED TREES WORKDIR OUTFILE

Builds TREES random directory trees under WORKDIR (real files, directories and
symbolic links; names with dots, accented letters, euro signs, emoji, spaces and
the glob characters themselves), and writes one case per line, tokens separated
by single spaces, all text as hex (`x` + hex digits, `x` alone = empty):

  G xROOT FLAGS xPATTERN <names...>      glob.glob(PATTERN, root_dir=ROOT, ...)
        FLAGS = sum of 1 recursive, 2 hidden, 4 follow; the answer is the sorted
        list, `none` when empty
  F xPATTERN xNAME 0|1                    fnmatch.fnmatchcase(NAME, PATTERN)
  E xTEXT xESCAPED                        glob.escape(TEXT)

Python is the reference. Python 3.11 answers "a/**" with "a/" for a directory
that does not exist; 3.12 fixed that and so does std.glob, so the reference
glob._glob2 is replaced by the 3.12 version. Cases where Python itself raises
are dropped. FLAGS has 4 (follow) only on trees where following cannot loop;
a tree with a link to a directory is queried with `**` only together with 4.
"""
import fnmatch
import glob
import os
import random
import sys
import warnings

warnings.simplefilter("ignore")


def fixed_glob2(dirname, pattern, dir_fd, dironly, include_hidden=False):
    assert glob._isrecursive(pattern)
    if not dirname or glob._isdir(dirname, dir_fd):
        yield pattern[:0]
        yield from glob._rlistdir(dirname, dir_fd, dironly, include_hidden=include_hidden)


glob._glob2 = fixed_glob2


def hx(s):
    return "x" + s.encode("utf-8", "surrogateescape").hex()


NAME_CHARS = ["a", "b", "c", "x", "d", ".", "1", "é", "€", "\U0001f600", " ", "-", "_", "A", "[", "]", "*", "?", "!"]
NAME_WEIGHTS = [10, 10, 8, 4, 3, 7, 3, 3, 2, 1, 1, 2, 1, 2, 1, 1, 1, 1, 1]


def rnd_name(rnd):
    n = rnd.choice([1, 1, 2, 2, 3, 4])
    s = "".join(rnd.choices(NAME_CHARS, NAME_WEIGHTS)[0] for _ in range(n))
    if s in (".", ".."):
        return "a"
    return s


def make_tree(rnd, root):
    """Creates a random tree; returns (list of relative paths, has_dir_link)."""
    os.makedirs(root, exist_ok=True)
    dirs = [""]
    paths = []
    count = rnd.randint(2, 22)
    tries = 0
    while len(paths) < count and tries < count * 6:
        tries += 1
        parent = rnd.choice(dirs)
        depth = parent.count("/") + (1 if parent else 0)
        name = rnd_name(rnd)
        rel = parent + "/" + name if parent else name
        full = os.path.join(root, rel)
        if os.path.lexists(full):
            continue
        if rnd.random() < 0.38 and depth < 4:
            os.mkdir(full)
            dirs.append(rel)
        else:
            with open(full, "w") as f:
                f.write("x")
        paths.append(rel)
    has_dir_link = False
    if rnd.random() < 0.3:
        for _ in range(rnd.randint(1, 3)):
            parent = rnd.choice(dirs)
            name = rnd_name(rnd)
            rel = parent + "/" + name if parent else name
            full = os.path.join(root, rel)
            if os.path.lexists(full):
                continue
            kind = rnd.random()
            if kind < 0.5 and len(dirs) > 1:
                target = rnd.choice(dirs[1:])
                os.symlink(os.path.relpath(os.path.join(root, target), os.path.dirname(full)), full)
                has_dir_link = True
            elif kind < 0.75 and paths:
                target = rnd.choice(paths)
                os.symlink(os.path.relpath(os.path.join(root, target), os.path.dirname(full)), full)
                if os.path.isdir(full):
                    has_dir_link = True
            else:
                os.symlink("no-such-target", full)
            paths.append(rel)
    if has_dir_link and has_cycle(root):
        return None
    return paths, has_dir_link


def has_cycle(root):
    def walk(path, stack):
        real = os.path.realpath(path)
        if real in stack:
            return True
        stack = stack | {real}
        try:
            names = os.listdir(path)
        except OSError:
            return False
        for n in names:
            p = os.path.join(path, n)
            if os.path.isdir(p) and walk(p, stack):
                return True
        return False
    return walk(root, frozenset())


def mutate_component(rnd, comp):
    r = rnd.random()
    if r < 0.28:
        return comp
    if r < 0.40:
        return "*"
    if r < 0.46:
        return "**"
    if r < 0.58 and comp:
        i = rnd.randrange(len(comp))
        return comp[:i] + "?" + comp[i + 1:]
    if r < 0.68 and comp:
        i = rnd.randrange(len(comp))
        return comp[:i] + "*"
    if r < 0.76 and comp:
        i = rnd.randrange(len(comp))
        return "*" + comp[i:]
    if r < 0.88 and comp:
        i = rnd.randrange(len(comp))
        cls = rnd.choice(["[%s]", "[!%s]", "[a-%s]", "[%s-z]", "[]%s]", "[!]%s]", "[%s-]", "[-%s]", "[z-%s]"]) % rnd.choice(["a", "b", "c", "x", ".", "d", "é", "1", comp[i]])
        return comp[:i] + cls + comp[i + 1:]
    return "".join(rnd.choice(["a", "b", "*", "?", "[", "]", "!", "-", ".", "c", "é"]) for _ in range(rnd.randint(1, 4)))


def rnd_pattern(rnd, paths):
    if rnd.random() < 0.12 or not paths:
        n = rnd.randint(1, 3)
        parts = ["".join(rnd.choice(["a", "b", "*", "?", "[", "]", "!", "-", ".", "c", "x", "é", "**"]) for _ in range(rnd.randint(1, 4))) for _ in range(n)]
        return "/".join(parts)
    base = rnd.choice(paths)
    comps = base.split("/")
    out = [mutate_component(rnd, c) for c in comps]
    if rnd.random() < 0.15:
        out.insert(rnd.randrange(len(out) + 1), "**")
    if rnd.random() < 0.2:
        out = out[: rnd.randint(1, len(out))]
    p = "/".join(out)
    if rnd.random() < 0.12:
        p += "/"
    if rnd.random() < 0.04:
        p = "./" + p
    if rnd.random() < 0.03:
        p = p.replace("/", "//", 1)
    return p


def py_glob(root, pat, flags):
    res = glob.glob(pat, root_dir=root, recursive=bool(flags & 1), include_hidden=bool(flags & 2))
    return sorted(res)


def main():
    seed, trees, work, outfile = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
    rnd = random.Random(seed)
    lines = []
    ng = nf = ne = 0
    # build every tree first: a pattern like `../**` looks at the neighbours, and the
    # expected answer must not depend on how many trees existed at that moment
    made_trees = []
    for t in range(trees):
        root = os.path.join(work, "t%d_%d" % (seed, t))
        made = None
        while made is None:
            if os.path.exists(root):
                import shutil
                shutil.rmtree(root)
            made = make_tree(rnd, root)
        made_trees.append((root, made[0], made[1]))
    for root, paths, has_dir_link in made_trees:
        for _ in range(rnd.randint(5, 9)):
            flags = 0
            if rnd.random() < 0.6:
                flags |= 1
            if rnd.random() < 0.3:
                flags |= 2
            if has_dir_link and (flags & 1):
                flags |= 4
            pat = rnd_pattern(rnd, paths)
            try:
                res = py_glob(root, pat, flags)
            except Exception:
                continue
            toks = " ".join(hx(r) for r in res) if res else "none"
            lines.append("G %s %d %s %s" % (hx(root), flags, hx(pat), toks))
            ng += 1
    # fnmatch cases: random patterns against random names
    pat_chars = ["a", "b", "c", "*", "?", "[", "]", "!", "-", "^", "\\", "&", "|", "~", ".", "é", "€", "/", "x", "\U0001f600"]
    name_chars = ["a", "b", "c", ".", "-", "!", "]", "[", "^", "\\", "&", "|", "~", "é", "€", "/", "x", "\n", "\U0001f600"]
    nfn = trees * 12
    while nf < nfn:
        pat = "".join(rnd.choice(pat_chars) for _ in range(rnd.randint(0, 9)))
        name = "".join(rnd.choice(name_chars) for _ in range(rnd.randint(0, 7)))
        if rnd.random() < 0.3:
            # a character class between small letters, ranges (also backwards), dashes and brackets
            letters = ["a", "b", "c", "d", "-", "!", "^", "]", "\u00e9", "z"]
            cls = "[" + rnd.choice(["", "", "!", "]", "^"]) + "".join(rnd.choice(letters) for _ in range(rnd.randint(1, 5))) + "]"
            pat = rnd.choice(["", "a", "*"]) + cls + rnd.choice(["", "b", "*"])
            name = rnd.choice(["", "a"]) + rnd.choice(letters) + rnd.choice(["", "b"])
        elif rnd.random() < 0.3:
            # a name that was made from the pattern, to get real matches
            name = "".join(rnd.choice(name_chars) if c in "*?" else (c if c not in "[]" else rnd.choice(name_chars)) for c in pat)
        try:
            ok = fnmatch.fnmatchcase(name, pat)
        except Exception:
            continue
        lines.append("F %s %s %d" % (hx(pat), hx(name), 1 if ok else 0))
        nf += 1
    for _ in range(trees // 4 + 1):
        s = "".join(rnd.choice(["a", "*", "?", "[", "]", "/", ".", "é"]) for _ in range(rnd.randint(0, 8)))
        lines.append("E %s %s" % (hx(s), hx(glob.escape(s))))
        ne += 1
    rnd.shuffle(lines)
    open(outfile, "w").write("\n".join(lines) + "\n")
    print("glob cases: %d glob (on %d trees), %d fnmatch, %d escape" % (ng, trees, nf, ne))


main()
