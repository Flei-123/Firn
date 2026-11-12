import re
import sys

VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$")
PARTIAL_RE = re.compile(r"^(\d+|[xX*])(?:\.(\d+|[xX*]))?(?:\.(\d+|[xX*]))?(?:-([0-9A-Za-z.-]+))?$")
OPERATORS = (">=", "<=", ">", "<", "=", "^", "~")
WILDCARDS = ("x", "X", "*")


class SemverError(ValueError):
    pass


def parse_version(text):
    match = VERSION_RE.match(text.strip())
    if not match:
        raise SemverError("bad version: " + text)
    major, minor, patch, pre = match.groups()
    return int(major), int(minor), int(patch), tuple(pre.split(".")) if pre else ()


def identifier_key(part):
    if part.isdigit():
        return (0, int(part), "")
    return (1, 0, part)


def sort_key(version):
    major, minor, patch, pre = version
    return (major, minor, patch, 1 if not pre else 0, tuple(identifier_key(p) for p in pre))


def parse_partial(text):
    match = PARTIAL_RE.match(text)
    if not match:
        raise SemverError("bad requirement: " + text)
    parts = [None if p is None or p in WILDCARDS else int(p) for p in match.groups()[:3]]
    pre = tuple(match.group(4).split(".")) if match.group(4) else ()
    for index in range(1, 3):
        if parts[index - 1] is None:
            parts[index:] = [None] * (3 - index)
            break
    return parts, pre


def lower(parts, pre):
    return (parts[0] or 0, parts[1] or 0, parts[2] or 0, pre)


def bump(major, minor, patch):
    return (major, minor, patch, ())


def caret_upper(parts):
    major, minor, patch = parts
    if major is None:
        return None
    if major > 0 or minor is None:
        return bump(major + 1, 0, 0)
    if minor > 0 or patch is None:
        return bump(0, minor + 1, 0)
    return bump(0, 0, patch + 1)


def tilde_upper(parts):
    major, minor, _ = parts
    if major is None:
        return None
    if minor is None:
        return bump(major + 1, 0, 0)
    return bump(major, minor + 1, 0)


def wildcard_upper(parts):
    major, minor, patch = parts
    if major is None:
        return None
    if minor is None:
        return bump(major + 1, 0, 0)
    if patch is None:
        return bump(major, minor + 1, 0)
    return None


def split_operator(token):
    for operator in OPERATORS:
        if token.startswith(operator):
            return operator, token[len(operator):].strip()
    return "", token


def expand(token):
    operator, rest = split_operator(token)
    parts, pre = parse_partial(rest)
    low = lower(parts, pre)
    exact = parts[2] is not None
    if operator == "" and any(w in rest for w in WILDCARDS):
        operator = "="
    if operator in ("", "^"):
        return [(">=", low)] + upper_bound("<", caret_upper(parts))
    if operator == "~":
        return [(">=", low)] + upper_bound("<", tilde_upper(parts))
    if operator == "=":
        if exact:
            return [("=", low)]
        return [(">=", low)] + upper_bound("<", wildcard_upper(parts))
    if operator in (">", "<=") and not exact:
        high = wildcard_upper(parts)
        if operator == ">":
            return [(">=", high)] if high else [(">=", (1 << 60, 0, 0, ()))]
        return upper_bound("<", high)
    return [(operator, low)]


def upper_bound(operator, bound):
    return [(operator, bound)] if bound else []


def compare(operator, version, bound):
    left, right = sort_key(version), sort_key(bound)
    return {
        "=": left == right,
        ">=": left >= right,
        ">": left > right,
        "<=": left <= right,
        "<": left < right,
    }[operator]


def parse_requirement(text):
    tokens = [t for t in re.split(r"[,\s]+", text.replace("> =", ">=").strip()) if t]
    if not tokens:
        raise SemverError("empty requirement")
    comparators = []
    merged = []
    for token in tokens:
        if merged and token[0].isdigit() and merged[-1] in OPERATORS:
            merged[-1] += token
        else:
            merged.append(token)
    for token in merged:
        comparators.extend(expand(token))
    return comparators


def satisfies(requirement, version):
    comparators = parse_requirement(requirement) if isinstance(requirement, str) else requirement
    if isinstance(version, str):
        version = parse_version(version)
    if version[3] and not any(b[:3] == version[:3] and b[3] for _, b in comparators):
        return False
    return all(compare(op, version, bound) for op, bound in comparators)


def resolve(requirement, versions):
    comparators = parse_requirement(requirement)
    parsed = [(parse_version(v), v) for v in versions]
    matching = [pair for pair in parsed if satisfies(comparators, pair[0])]
    if not matching:
        return None
    return max(matching, key=lambda pair: sort_key(pair[0]))[1]


def main(argv):
    if len(argv) == 3 and argv[1] == "resolve":
        found = resolve(argv[2], [line.strip() for line in sys.stdin if line.strip()])
        if found is None:
            return 1
        print(found)
        return 0
    if len(argv) == 4 and argv[1] == "satisfies":
        return 0 if satisfies(argv[2], argv[3]) else 1
    sys.stderr.write("usage: semver.py resolve REQ < versions | semver.py satisfies REQ VERSION\n")
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except SemverError as error:
        sys.stderr.write(str(error) + "\n")
        sys.exit(2)
