#!/usr/bin/env python3
import os
import re
import sys

FENCE = re.compile(r"^```(\S*)\s*(.*)$")


def blocks(path):
    out = []
    current = None
    with open(path, encoding="utf-8") as f:
        for number, line in enumerate(f, 1):
            line = line.rstrip("\n")
            match = FENCE.match(line)
            if current is None:
                if match and match.group(1):
                    current = {"lang": match.group(1), "mode": match.group(2).strip(), "line": number, "text": []}
            elif line.startswith("```"):
                out.append(current)
                current = None
            else:
                current["text"].append(line)
    return out


def extract(book_dir, work_dir):
    cases = []
    for dirpath, dirnames, filenames in os.walk(book_dir):
        dirnames.sort()
        for name in sorted(filenames):
            if not name.endswith(".md"):
                continue
            path = os.path.join(dirpath, name)
            found = blocks(path)
            for index, block in enumerate(found):
                if block["lang"] != "firn" or block["mode"] not in ("run", "build", "test"):
                    continue
                expected = None
                if block["mode"] == "run" and index + 1 < len(found) and found[index + 1]["lang"] == "output":
                    expected = "\n".join(found[index + 1]["text"]) + "\n"
                stem = "%s_%d" % (os.path.relpath(path, book_dir).replace(os.sep, "_")[:-3], block["line"])
                source = os.path.join(work_dir, stem + ".fi")
                with open(source, "w", encoding="utf-8") as f:
                    f.write("\n".join(block["text"]) + "\n")
                if expected is not None:
                    with open(os.path.join(work_dir, stem + ".expected"), "w", encoding="utf-8") as f:
                        f.write(expected)
                cases.append("%s\t%s\t%s:%d" % (block["mode"], source, os.path.relpath(path, book_dir), block["line"]))
    return cases


if __name__ == "__main__":
    for case in extract(sys.argv[1], sys.argv[2]):
        print(case)
