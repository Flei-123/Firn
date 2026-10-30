import fnmatch
import shlex


def load(path):
    entries = []
    try:
        fh = open(path, encoding="utf-8")
    except OSError:
        return entries
    with fh:
        for raw in fh:
            line = raw.strip()
            if not line or line.startswith("#"):
                continue
            parts = shlex.split(line)
            if len(parts) < 2:
                continue
            entries.append((parts[0], parts[1], " ".join(parts[2:])))
    return entries


def allowed(entries, finding):
    for glob, rule, needle in entries:
        if not fnmatch.fnmatch(finding["file"], glob):
            continue
        if rule not in ("*", finding["rule"]):
            continue
        if needle and needle not in finding["text"]:
            continue
        return True
    return False
