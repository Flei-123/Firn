import re

_MARK = re.compile(r"//\s*designlint:\s*(ignore-file|ignore)\b([^\n]*)")


def _rules(tail):
    found = set(re.findall(r"[A-Z][A-Z_]+", tail))
    return found or {"*"}


def parse(src):
    lines = {}
    whole = set()
    own = set()
    for no, text in enumerate(src.split("\n"), 1):
        m = _MARK.search(text)
        if not m:
            continue
        rules = _rules(m.group(2))
        if m.group(1) == "ignore-file":
            whole |= rules
        else:
            lines.setdefault(no, set()).update(rules)
            if not text[: m.start()].strip():
                own.add(no)
    return lines, whole, own


def suppressed(marks, finding):
    lines, whole, own = marks
    rule = finding["rule"]
    if "*" in whole or rule in whole:
        return True
    for no in (finding["line"], finding["line"] - 1):
        if no != finding["line"] and no not in own:
            continue
        got = lines.get(no)
        if got and ("*" in got or rule in got):
            return True
    return False
