#!/usr/bin/env python3
import re
import sys

IDENT = re.compile(r"[A-Za-z0-9_]")
BOOL_OP = re.compile(r"(==|!=)\s*(true|false)\b")
UNSAFE_BEFORE = set("!*&-+~<>=|^%/")


def in_string(line, pos):
    count = 0
    i = 0
    while i < pos:
        if line[i] == "\\":
            i += 2
            continue
        if line[i] == '"':
            count += 1
        i += 1
    return count % 2 == 1


def skip_group_back(text, end):
    close = text[end]
    open_ch = {")": "(", "]": "["}[close]
    depth = 0
    i = end
    while i >= 0:
        ch = text[i]
        if ch == close:
            depth += 1
        elif ch == open_ch:
            depth -= 1
            if depth == 0:
                return i
        i -= 1
    return -1


def operand_start(text, end):
    i = end
    start = -1
    while i >= 0:
        ch = text[i]
        if IDENT.match(ch):
            while i >= 0 and IDENT.match(text[i]):
                i -= 1
            start = i + 1
        elif ch in ")]":
            j = skip_group_back(text, i)
            if j < 0:
                return -1
            i = j - 1
            start = j
            if ch == ")" and (i < 0 or not (IDENT.match(text[i]) or text[i] in ")]")):
                break
        else:
            break
        if i >= 0 and text[i] == ".":
            i -= 1
            continue
        if i >= 0 and (text[i] in ")]" or IDENT.match(text[i])):
            continue
        break
    return start


def fix_bool_comparison(line):
    changed = False
    while True:
        found = None
        for m in BOOL_OP.finditer(line):
            if in_string(line, m.start()):
                continue
            end = m.start() - 1
            while end >= 0 and line[end] == " ":
                end -= 1
            if end < 0:
                continue
            start = operand_start(line, end)
            if start < 0:
                continue
            before = line[:start].rstrip()
            if before and before[-1] in UNSAFE_BEFORE and not before.endswith(("&&", "||")):
                continue
            if re.search(r"\bas$", before):
                continue
            after = line[m.end():].lstrip()
            if after and after[0] in ".[(":
                continue
            found = (start, end + 1, m)
            break
        if found is None:
            return line, changed
        start, end, m = found
        operand = line[start:end]
        keep = (m.group(1) == "==") == (m.group(2) == "true")
        text = operand if keep else "!" + operand
        line = line[:start] + text + line[m.end():]
        changed = True


def remove_import(lines, idx):
    if not re.match(r"\s*import\s+[A-Za-z0-9_.]+(\s+as\s+\w+)?\s*$", lines[idx]):
        return False
    del lines[idx]
    return True


def item_end(lines, idx):
    is_fn = re.match(r"\s*(pub\s+)?fn\b", lines[idx]) is not None
    depth = 0
    seen_brace = False
    i = idx
    while i < len(lines):
        text = lines[i]
        j = 0
        while j < len(text):
            ch = text[j]
            if text[j:j + 2] == "//":
                break
            if ch == '"':
                j += 1
                while j < len(text) and text[j] != '"':
                    j += 2 if text[j] == "\\" else 1
            elif ch == "'":
                m = re.match(r"'(\\.[^']*|[^'\\])'", text[j:])
                if m:
                    j += len(m.group(0)) - 1
            elif ch in "({[":
                depth += 1
                if ch == "{":
                    seen_brace = True
            elif ch in ")}]":
                depth -= 1
            j += 1
        if depth == 0 and (seen_brace or not is_fn) and (seen_brace or not text.rstrip().endswith(("=", ",", "(", "[", "{"))):
            return i
        if depth < 0:
            return -1
        i += 1
    return -1


def remove_item(lines, idx):
    if not re.match(r"\s*(pub\s+)?(fn|static|const)\b", lines[idx]):
        return False
    end = item_end(lines, idx)
    if end < 0:
        return False
    start = idx
    while start > 0 and lines[start - 1].lstrip().startswith("//") and not lines[start - 1].lstrip().startswith("// SPDX") and start - 1 > 0:
        start -= 1
    if start > 0 and lines[start - 1].lstrip().startswith("#["):
        return False
    del lines[start:end + 1]
    if start < len(lines) and lines[start].strip() == "" and (start == 0 or lines[start - 1].strip() == ""):
        del lines[start]
    return True


def main():
    rule = sys.argv[1]
    path = sys.argv[2]
    numbers = sorted({int(a) for a in sys.argv[3:]}, reverse=True)
    with open(path, encoding="utf-8") as handle:
        lines = handle.read().split("\n")
    done = 0
    for number in numbers:
        idx = number - 1
        if idx < 0 or idx >= len(lines):
            continue
        if rule == "var-never-mutated":
            new = re.sub(r"\bvar\b", "let", lines[idx], count=1)
            ok = new != lines[idx]
            lines[idx] = new
        elif rule == "bool-comparison":
            lines[idx], ok = fix_bool_comparison(lines[idx])
        elif rule == "unused-import":
            ok = remove_import(lines, idx)
        elif rule == "unused-private-item":
            ok = remove_item(lines, idx)
        else:
            sys.exit(2)
        done += 1 if ok else 0
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("\n".join(lines))
    print(done)


main()
