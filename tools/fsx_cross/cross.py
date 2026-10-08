#!/usr/bin/env python3
"""tools/fsx_cross/cross.py -- std.fsx against Python and cp (docs/SKRIPT-LIBS.md, r314).

usage: cross.py FSX_TOOL WORKDIR [SEED] [TREES]

  1. TREES random directory trees (default 500; nested directories, empty
     directories, files of 0..70000 octets with many modes, relative / absolute /
     dangling symbolic links, read-only directories, odd names). Per tree one
     variant is run, round robin:
       plain      copy_tree  vs  `cp -a`            AND  shutil.copytree(symlinks=True)
       ignore     copy_tree(ignore)  vs  shutil.copytree(ignore=ignore_patterns)
       exist_ok   copy_tree(dirs_exist_ok) into a populated target  vs  shutil dirs_exist_ok=True
       follow     copy_tree(follow)  vs  shutil.copytree(symlinks=False)
     The compared snapshot: every path, its kind, its permission bits (setuid and
     friends included), the content (sha256) of files, the target text of links.
     Both sides failing counts as agreement (their reasons differ by design);
     one side failing is a mismatch.
  2. copy_file vs `cp -p` on random single files (sizes around the 256 KiB buffer).
  3. normpath / abspath / relpath vs os.path on random paths (PATHS of them).
  4. fnmatch vs fnmatch.fnmatchcase on random patterns (PATTERNS of them).

Prints one line per section and `fsx_cross: ... mismatches=N`; exit code 1 on any.
"""
import fnmatch
import hashlib
import os
import random
import shutil
import stat
import subprocess
import sys

TOOL = sys.argv[1]
WORK = os.path.abspath(sys.argv[2])
SEED = int(sys.argv[3]) if len(sys.argv) > 3 else 20261008
TREES = int(sys.argv[4]) if len(sys.argv) > 4 else 500
PATHS = int(os.environ.get("FSX_CROSS_PATHS", "6000"))
PATTERNS = int(os.environ.get("FSX_CROSS_PATTERNS", "6000"))
FILES = int(os.environ.get("FSX_CROSS_FILES", "150"))

rng = random.Random(SEED)
mismatches = 0
US = "\x1f"


def bad(msg):
    global mismatches
    mismatches += 1
    if mismatches <= 25:
        print("  MISMATCH  " + msg)


# ---------------------------------------------------------------- trees

NAME_PARTS = ["a", "b", "c", "data", "x", "file", "dir", "t", "log", "doc", "img",
              "café", "日本", "sp ace", "dot.dot", ".hidden", "-dash", "_us", "UPPER", "z9"]
SUFFIXES = ["", "", "", ".txt", ".json", ".tmp", ".py", ".pyc", ".bak", ".md", ".log"]
FILE_MODES = [0o644, 0o644, 0o644, 0o755, 0o600, 0o400, 0o444, 0o700, 0o640, 0o4755, 0o664, 0o751]
DIR_MODES = [0o755, 0o755, 0o700, 0o750, 0o555, 0o2755, 0o500, 0o711]
IGNORES = ["*.tmp", "*.tmp|.hidden", "*.py?|*.log", "[a-c]*", "dir*|data", "?", "*"]


def rand_name(used):
    for _ in range(50):
        n = rng.choice(NAME_PARTS) + rng.choice(SUFFIXES)
        if rng.random() < 0.3:
            n += str(rng.randrange(100))
        if n not in used:
            used.add(n)
            return n
    return None


def rand_content():
    r = rng.random()
    if r < 0.15:
        return b""
    if r < 0.7:
        return os.urandom(rng.randrange(1, 300))
    if r < 0.95:
        return os.urandom(rng.randrange(300, 20000))
    return os.urandom(rng.randrange(20000, 70000))


def make_tree(root, depth, allow_dir_links, allow_dangling, pending):
    """Builds `root`; the (possibly read-only) directory modes are applied last (pending)."""
    os.makedirs(root, exist_ok=True)
    used = set()
    files, dirs = [], []
    for _ in range(rng.randrange(1, 9) if depth == 0 else rng.randrange(0, 6)):
        kind = rng.random()
        name = rand_name(used)
        if name is None:
            break
        path = os.path.join(root, name)
        if kind < 0.5:
            with open(path, "wb") as f:
                f.write(rand_content())
            os.chmod(path, rng.choice(FILE_MODES))
            files.append(name)
        elif kind < 0.75 and depth < 3:
            dirs.append(name)
            make_tree(path, depth + 1, allow_dir_links, allow_dangling, pending)
            pending.append((path, rng.choice(DIR_MODES)))
        elif kind < 0.82:
            os.makedirs(path)  # an empty directory
            pending.append((path, rng.choice(DIR_MODES)))
        else:
            r = rng.random()
            if not allow_dangling and not files:
                os.makedirs(path)  # no file to point at, no dangling links wanted
                pending.append((path, rng.choice(DIR_MODES)))
            elif not allow_dangling:
                os.symlink(rng.choice(files), path)
            elif r < 0.3 and files:
                os.symlink(rng.choice(files), path)  # relative, to a sibling file
            elif r < 0.5:
                os.symlink("/nonexistent/abs/" + name, path)  # absolute, dangling
            elif r < 0.65:
                os.symlink("no_such_" + name, path)  # relative, dangling
            elif r < 0.8 and files:
                os.symlink("../" + os.path.basename(root) + "/" + rng.choice(files), path)
            elif allow_dir_links and dirs:
                os.symlink(rng.choice(dirs), path)  # to a sibling directory
            else:
                os.symlink("x_" + name, path)
    return root


def snapshot(root):
    """{relpath: (kind, mode, extra)} for the whole tree (symbolic links not followed)."""
    out = {}

    def one(path, rel):
        st = os.lstat(path)
        mode = stat.S_IMODE(st.st_mode)
        if stat.S_ISLNK(st.st_mode):
            out[rel] = ("link", 0, os.readlink(path))
        elif stat.S_ISDIR(st.st_mode):
            out[rel] = ("dir", mode, None)
            for n in sorted(os.listdir(path)):
                one(os.path.join(path, n), os.path.join(rel, n) if rel else n)
        else:
            with open(path, "rb") as f:
                h = hashlib.sha256(f.read()).hexdigest()
            out[rel] = ("file", mode, h)

    one(root, "")
    return out


def diff(a, b):
    for k in sorted(set(a) | set(b)):
        if a.get(k) != b.get(k):
            return "%r: %r vs %r" % (k, a.get(k), b.get(k))
    return None


def rm(path):
    """Remove a tree whose directories may be read-only."""
    if not os.path.lexists(path):
        return
    for dp, dns, fns in os.walk(path):
        try:
            os.chmod(dp, 0o700)
        except OSError:
            pass
    shutil.rmtree(path, ignore_errors=True)


def run_tool(*args):
    r = subprocess.run([TOOL] + list(args), capture_output=True, timeout=120)
    return r.returncode, r.stdout.decode(errors="replace").strip(), r.stderr.decode(errors="replace")


def section_trees():
    cases = 0
    entries_total = 0
    both_failed = dict((v, 0) for v in ["plain", "ignore", "exist_ok", "follow"])
    variants = ["plain", "ignore", "exist_ok", "follow"]
    counts = dict((v, 0) for v in variants)
    for i in range(TREES):
        variant = variants[i % 4]
        counts[variant] += 1
        base = os.path.join(WORK, "tree%d" % i)
        rm(base)
        os.makedirs(base)
        src = os.path.join(base, "src")
        pending = []
        make_tree(src, 0, variant != "follow", variant != "follow" or i % 8 == 3, pending)
        for path, mode in sorted(pending, key=lambda t: -len(t[0])):
            os.chmod(path, mode)
        os.chmod(src, rng.choice(DIR_MODES))
        ignore = rng.choice(IGNORES) if variant == "ignore" else ""
        flags = {"plain": "", "ignore": "", "exist_ok": "e", "follow": "f"}[variant]
        d_fx = os.path.join(base, "fx", "deep", "dst")  # the parents are made by copy_tree
        d_py = os.path.join(base, "py")
        if variant == "exist_ok":
            # one populated target, built the same way on both sides: extra files stay,
            # same-name regular files are overwritten, same-name directories are merged
            plan = []
            for n in sorted(os.listdir(src)):
                p = os.path.join(src, n)
                if rng.random() < 0.4 and not os.path.islink(p):
                    plan.append((n, os.path.isdir(p)))
            for d in (d_fx, d_py):
                os.makedirs(d)
                with open(os.path.join(d, "extra.txt"), "w") as f:
                    f.write("keep")
                for n, is_dir in plan:
                    if is_dir:
                        os.makedirs(os.path.join(d, n), exist_ok=True)
                        with open(os.path.join(d, n, "old"), "w") as f:
                            f.write("old")
                    else:
                        with open(os.path.join(d, n), "w") as f:
                            f.write("OLD CONTENT")
        rc, out, err = run_tool("copytree", src, d_fx, ignore, flags)
        fx_ok = rc == 0
        py_ok = True
        try:
            ig = shutil.ignore_patterns(*ignore.split("|")) if ignore else None
            shutil.copytree(src, d_py, symlinks=(variant != "follow"), ignore=ig, dirs_exist_ok=(variant == "exist_ok"))
        except (shutil.Error, OSError):
            py_ok = False
        cases += 1
        if fx_ok != py_ok:
            bad("tree %d [%s]: firn %s (%s), python %s" % (i, variant, "ok" if fx_ok else "failed", out, "ok" if py_ok else "failed"))
        elif fx_ok:
            a = snapshot(d_fx)
            entries_total += len(a)
            d = diff(a, snapshot(d_py))
            if d:
                bad("tree %d [%s] vs shutil: %s" % (i, variant, d))
            if variant == "plain":
                d_cp = os.path.join(base, "cp")
                r = subprocess.run(["cp", "-a", src, d_cp], capture_output=True)
                cases += 1
                if r.returncode != 0:
                    bad("tree %d: cp -a failed: %s" % (i, r.stderr.decode()))
                else:
                    d = diff(a, snapshot(d_cp))
                    if d:
                        bad("tree %d vs cp -a: %s" % (i, d))
                    # the reported number is the number of entries below the top
                    if int(out.split()[1]) != len(a) - 1:
                        bad("tree %d: copy_tree reported '%s', the tree has %d entries" % (i, out, len(a) - 1))
        else:
            both_failed[variant] += 1
            if os.environ.get("FSX_CROSS_DEBUG"):
                print("   refused: tree %d [%s] firn says '%s'" % (i, variant, out))
        rm(base)
    print("trees: %d trees (%s), %d comparisons, %d snapshot entries compared, both sides refused: %s"
          % (TREES, ", ".join("%s %d" % (v, counts[v]) for v in variants), cases, entries_total,
             ", ".join("%s %d" % (v, both_failed[v]) for v in variants)))


def section_files():
    d = os.path.join(WORK, "files")
    rm(d)
    os.makedirs(d)
    sizes = [0, 1, 4095, 4096, 4097, 262143, 262144, 262145, 524288, 1048577]
    for i in range(FILES):
        src = os.path.join(d, "s%d" % i)
        dst = os.path.join(d, "d%d" % i)
        size = rng.choice(sizes) if rng.random() < 0.5 else rng.randrange(0, 700000)
        with open(src, "wb") as f:
            f.write(os.urandom(size))
        os.chmod(src, rng.choice(FILE_MODES))
        if rng.random() < 0.3:
            with open(dst, "wb") as f:
                f.write(b"previous")
            os.chmod(dst, rng.choice([0o444, 0o600, 0o644]))
        rc, out, err = run_tool("copyfile", src, dst)
        if rc != 0 or out != "ok %d" % size:
            bad("file %d size %d: rc=%s '%s'" % (i, size, rc, out))
            continue
        ref = dst + ".cp"
        subprocess.run(["cp", "-p", src, ref], check=True)
        a, b = snapshot(dst)[""], snapshot(ref)[""]
        if a != b:
            bad("file %d vs cp -p: %r vs %r" % (i, a, b))
        if [n for n in os.listdir(d) if ".tmp-" in n]:
            bad("file %d: a temporary name was left behind" % i)
    rm(d)
    print("files: %d single files (sizes 0..1 MiB around the 256 KiB buffer, existing targets, many modes) against cp -p" % FILES)


# ---------------------------------------------------------------- paths

COMPS = ["", ".", "..", "a", "b", "c", "ab", "a.b", "..a", "a..", "café", "x y", "...", "-", "~", "dir", "a b", "tmp"]


def rand_path():
    parts = [rng.choice(COMPS) for _ in range(rng.randrange(0, 8))]
    p = "/".join(parts)
    r = rng.random()
    if r < 0.4:
        p = "/" + p
    elif r < 0.5:
        p = "//" + p
    elif r < 0.55:
        p = "///" + p
    elif r < 0.6:
        p = "////" + p
    if rng.random() < 0.2:
        p += "/"
    if rng.random() < 0.05:
        p += "//"
    return p


def section_paths():
    cwd = os.path.join(WORK, "cwd")
    os.makedirs(cwd, exist_ok=True)
    os.chdir(cwd)
    cases = [(rand_path(), rand_path()) for _ in range(PATHS)]
    inp = os.path.join(WORK, "paths.in")
    with open(inp, "w", encoding="utf-8", newline="\n") as f:
        for p, b in cases:
            f.write(p + US + b + "\n")
    rc, out, err = run_tool("paths", inp)
    lines = out.split("\n")
    if rc != 0 or len(lines) != len(cases):
        bad("paths: rc=%s, %d answers for %d questions %s" % (rc, len(lines), len(cases), err[:200]))
        return
    for (p, b), line in zip(cases, lines):
        got = line.split(US)
        want_n = os.path.normpath(p)
        want_a = os.path.abspath(p)
        want_r = os.path.relpath(p, b) if p else ""
        if len(got) != 3 or got[0] != want_n:
            bad("normpath(%r) = %r, python %r" % (p, got[0], want_n))
        elif got[1] != want_a:
            bad("abspath(%r) = %r, python %r" % (p, got[1], want_a))
        elif got[2] != want_r:
            bad("relpath(%r, %r) = %r, python %r" % (p, b, got[2], want_r))
    print("paths: %d random (path, base) pairs: normpath, abspath, relpath = %d answers against os.path"
          % (len(cases), 3 * len(cases)))


# ---------------------------------------------------------------- fnmatch

PAT_ALPHA = list("ab.-_") + ["*", "*", "?", "[", "]", "!", "^", "[a-c]", "[!a]", "[]", "[!]", "a-b"]
NAME_ALPHA = list("ab.-_]![^")


def reversed_range(pat):
    """Does a class of `pat` hold a reversed range like [z-a]? Python's fnmatch merges the
    neighbouring chunks of such a class in a way no other glob does ([]-[!a] becomes a
    negation); std.fsx treats a reversed range as empty. Those patterns are not compared."""
    i, n = 0, len(pat)
    while i < n:
        c = pat[i]
        i += 1
        if c != "[":
            continue
        j = i
        if j < n and pat[j] == "!":
            j += 1
        if j < n and pat[j] == "]":
            j += 1
        while j < n and pat[j] != "]":
            j += 1
        if j >= n:
            continue
        stuff = pat[i:j]
        chunks = []
        k = i + 2 if pat[i] == "!" else i + 1
        start = i
        while True:
            k = pat.find("-", k, j)
            if k < 0:
                break
            chunks.append(pat[start:k])
            start = k + 1
            k = k + 3
        chunk = pat[start:j]
        if chunk:
            chunks.append(chunk)
        elif chunks:
            chunks[-1] += "-"
        for q in range(len(chunks) - 1, 0, -1):
            if chunks[q - 1] and chunks[q] and chunks[q - 1][-1] > chunks[q][0]:
                return True
        i = j + 1
    return False


def section_fnmatch():
    cases = []
    skipped = 0
    while len(cases) < PATTERNS:
        pat = "".join(rng.choice(PAT_ALPHA) for _ in range(rng.randrange(0, 8)))
        name = "".join(rng.choice(NAME_ALPHA) for _ in range(rng.randrange(0, 8)))
        if reversed_range(pat):
            skipped += 1
            continue
        cases.append((pat, name))
    inp = os.path.join(WORK, "fn.in")
    with open(inp, "w", newline="\n") as f:
        for p, n in cases:
            f.write(p + US + n + "\n")
    rc, out, err = run_tool("fnmatch", inp)
    lines = out.split("\n")
    if rc != 0 or len(lines) != len(cases):
        bad("fnmatch: rc=%s, %d answers for %d questions" % (rc, len(lines), len(cases)))
        return
    matches = 0
    for (pat, name), line in zip(cases, lines):
        want = fnmatch.fnmatchcase(name, pat)
        matches += 1 if want else 0
        if (line == "1") != want:
            bad("fnmatch(%r, %r) = %s, python %s" % (pat, name, line, want))
    print("fnmatch: %d random (pattern, name) pairs against fnmatch.fnmatchcase (%d of them match; %d with a reversed range left out)"
          % (len(cases), matches, skipped))


def selftest():
    """The comparison must STRIKE: alter a copy in six ways, each one has to be seen."""
    base = os.path.join(WORK, "selftest")
    rm(base)
    os.makedirs(os.path.join(base, "src", "sub"))
    with open(os.path.join(base, "src", "f"), "wb") as f:
        f.write(b"content")
    os.symlink("f", os.path.join(base, "src", "l"))
    os.makedirs(os.path.join(base, "src", "empty"))
    os.chmod(os.path.join(base, "src", "f"), 0o640)
    seen = 0
    alterations = [
        lambda d: os.chmod(os.path.join(d, "f"), 0o600),
        lambda d: open(os.path.join(d, "f"), "wb").write(b"Content"),
        lambda d: (os.remove(os.path.join(d, "l")), os.symlink("sub", os.path.join(d, "l"))),
        lambda d: open(os.path.join(d, "extra"), "w").write("x"),
        lambda d: os.rmdir(os.path.join(d, "empty")),
        lambda d: os.chmod(os.path.join(d, "sub"), 0o700),
    ]
    for i, alter in enumerate(alterations):
        c = os.path.join(base, "c%d" % i)
        shutil.copytree(os.path.join(base, "src"), c, symlinks=True)
        if diff(snapshot(c), snapshot(os.path.join(base, "src"))) is not None:
            bad("selftest: identical copy %d reported as different" % i)
        alter(c)
        if diff(snapshot(c), snapshot(os.path.join(base, "src"))) is None:
            bad("selftest: alteration %d was NOT detected" % i)
        else:
            seen += 1
    rm(base)
    print("selftest: %d of %d deliberate alterations of a copy are detected by the comparison" % (seen, len(alterations)))


def main():
    os.makedirs(WORK, exist_ok=True)
    os.chdir(WORK)
    selftest()
    section_trees()
    section_files()
    section_paths()
    section_fnmatch()
    print("fsx_cross: seed=%d trees=%d files=%d paths=%d patterns=%d mismatches=%d"
          % (SEED, TREES, FILES, PATHS, PATTERNS, mismatches))
    sys.exit(1 if mismatches else 0)


main()
