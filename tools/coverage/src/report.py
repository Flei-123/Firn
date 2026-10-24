import os


def selected(path, include, exclude):
    if include and not any(path == p or path.startswith(p.rstrip("/") + "/") for p in include):
        return False
    return not any(path == p or path.startswith(p.rstrip("/") + "/") for p in exclude)


def percent(hit, total):
    return 100.0 if total == 0 else 100.0 * hit / total


def summarize(profile, include=(), exclude=()):
    files = {}
    for path, table in profile["files"].items():
        if selected(path, include, exclude):
            files[path] = {"lines": table, "functions": []}
    for entry in profile["functions"].values():
        if entry["file"] in files:
            files[entry["file"]]["functions"].append(entry)
    return files


def counts(item):
    lines = len(item["lines"])
    lines_hit = sum(1 for h in item["lines"].values() if h > 0)
    functions = len(item["functions"])
    functions_hit = sum(1 for f in item["functions"] if f["hits"] > 0)
    return lines_hit, lines, functions_hit, functions


def ranges_text(numbers):
    out = []
    start = prev = None
    for number in sorted(numbers):
        if start is None:
            start = prev = number
        elif number <= prev + 1:
            prev = number
        else:
            out.append((start, prev))
            start = prev = number
    if start is not None:
        out.append((start, prev))
    return ",".join(str(a) if a == b else f"{a}-{b}" for a, b in out)


def text(profile, include=(), exclude=(), missing=False):
    files = summarize(profile, include, exclude)
    width = max([len(p) for p in files] + [4])
    rows = [f"{'file':<{width}}  {'lines':>13}  {'%':>6}  {'functions':>11}  {'%':>6}"]
    total = [0, 0, 0, 0]
    for path in sorted(files):
        lh, lt, fh, ft = counts(files[path])
        total = [total[0] + lh, total[1] + lt, total[2] + fh, total[3] + ft]
        rows.append(f"{path:<{width}}  {lh:>6}/{lt:<6}  {percent(lh, lt):>6.1f}  {fh:>5}/{ft:<5}  {percent(fh, ft):>6.1f}")
        if missing:
            gaps = [n for n, h in files[path]["lines"].items() if h == 0]
            if gaps:
                rows.append(f"{'':<{width}}  uncovered lines: {ranges_text(gaps)}")
            dead = sorted(f["name"] for f in files[path]["functions"] if f["hits"] == 0)
            if dead:
                rows.append(f"{'':<{width}}  uncovered functions: {', '.join(dead)}")
    rows.append(f"{'total':<{width}}  {total[0]:>6}/{total[1]:<6}  {percent(total[0], total[1]):>6.1f}  {total[2]:>5}/{total[3]:<5}  {percent(total[2], total[3]):>6.1f}")
    return "\n".join(rows) + "\n"


def totals(profile, include=(), exclude=()):
    total = [0, 0, 0, 0]
    for item in summarize(profile, include, exclude).values():
        total = [a + b for a, b in zip(total, counts(item))]
    return {"lines_hit": total[0], "lines": total[1], "functions_hit": total[2], "functions": total[3],
            "line_percent": percent(total[0], total[1]), "function_percent": percent(total[2], total[3])}


def lcov(profile, include=(), exclude=()):
    out = ["TN:"]
    files = summarize(profile, include, exclude)
    for path in sorted(files):
        item = files[path]
        out.append(f"SF:{path}")
        for entry in sorted(item["functions"], key=lambda f: (f["line"], f["name"])):
            out.append(f"FN:{entry['line']},{entry['name']}")
        for entry in sorted(item["functions"], key=lambda f: (f["line"], f["name"])):
            out.append(f"FNDA:{entry['hits']},{entry['name']}")
        lh, lt, fh, ft = counts(item)
        out.append(f"FNF:{ft}")
        out.append(f"FNH:{fh}")
        for number in sorted(item["lines"]):
            out.append(f"DA:{number},{item['lines'][number]}")
        out.append(f"LF:{lt}")
        out.append(f"LH:{lh}")
        out.append("end_of_record")
    return "\n".join(out) + "\n"


def annotate(profile, path, root):
    table = profile["files"].get(path)
    if table is None:
        raise KeyError(f"{path}: not in the profile")
    source = os.path.join(root, path)
    with open(source, errors="replace") as handle:
        content = handle.read().splitlines()
    out = []
    for number, row in enumerate(content, 1):
        if number not in table:
            mark = " " * 9
        elif table[number] == 0:
            mark = "#####".rjust(9)
        else:
            mark = str(table[number]).rjust(9)
        out.append(f"{mark}|{number:>5}|{row}")
    return "\n".join(out) + "\n"
