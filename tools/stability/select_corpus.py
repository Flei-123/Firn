#!/usr/bin/env python3
import re
import sys
from pathlib import Path

ALLOWED = set("rt core vec str map math bytes option io json deque heap base64 hash text num bisect testkit".split())
IMPORT = re.compile(r"^import\s+([A-Za-z_][A-Za-z0-9_.]*)")


def imports_ok(text):
    for line in text.split("\n"):
        m = IMPORT.match(line)
        if not m:
            continue
        parts = m.group(1).split(".")
        if parts[0] != "std" or len(parts) != 2 or parts[1] not in ALLOWED:
            return False
    return True


def header(text):
    return text.split("\n", 1)[0]


def positive_ok(text):
    h = header(text)
    if not (h.startswith("// expect_exit:") or h.startswith("// expect_out:")):
        return False
    if "// only_mode:" in text or "// not_mode:" in text:
        return False
    if "fn main" not in text or "#[test]" in text:
        return False
    if "__include" in text or "testdata/" in text or "data/" in text:
        return False
    return imports_ok(text)


def negative_ok(text):
    return "// expect_error:" in text.split("\n", 3)[0] and imports_ok(text)


def main():
    root = Path(sys.argv[1])
    mode = sys.argv[2]
    pattern, check = ("*.fi", positive_ok) if mode == "positive" else ("neg/*.fi", negative_ok)
    for f in sorted((root / "tests").glob(pattern)):
        if check(f.read_text()):
            print(f.name)


main()
