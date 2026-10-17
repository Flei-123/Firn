import fnmatch, os, sys, tempfile


def make_tree(root):
    for i in range(24):
        d = os.path.join(root, f"d{i % 4}", f"s{i % 3}")
        os.makedirs(d, exist_ok=True)
        p = os.path.join(d, f"f{i:02d}." + ("txt", "log", "txt", "md")[i % 4])
        with open(p, "wb") as f:
            f.write(b"x" * (i * 100))
        os.utime(p, (1000000 + i * 1000, 1000000 + i * 1000))


def find(root, pattern, min_size, max_mtime):
    found = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for name in sorted(filenames):
            p = os.path.join(dirpath, name)
            st = os.stat(p)
            if fnmatch.fnmatch(name, pattern) and st.st_size >= min_size and st.st_mtime < max_mtime:
                found.append(p)
    return sorted(found)


def rename_all(root, files, prefix, dry_run):
    for p in files:
        d, name = os.path.split(p)
        new = os.path.join(d, prefix + os.path.splitext(name)[0] + ".md")
        print(("would rename " if dry_run else "rename ") + os.path.relpath(p, root) + " -> " + os.path.relpath(new, root))
        if not dry_run:
            os.rename(p, new)


with tempfile.TemporaryDirectory() as root:
    make_tree(root)
    files = find(root, "*.txt", 500, 1015000)
    rename_all(root, files, "old_", True)
    rename_all(root, files, "old_", False)
    left = find(root, "*.txt", 0, 1e12)
    print(len(files), "renamed,", len(left), "txt left,", len(find(root, "old_*.md", 0, 1e12)), "old_*.md")
