#!/usr/bin/env python3
"""Includer for the Firn library of the module `str`.

Stage 0 has no module system (yet): `firnc` compiles exactly ONE file.
So that `lib/str/*.fi` and `lib/num/*.fi` still exist only once and do not
have to be copied into every test file, this tool resolves lines of the form

    //#include lib/str/str16.fi

recursively (every file at most once) and writes the result as a
standalone .fi program. The generated files in tests/ are part of the
tree so that `test.sh` can compile them without an extra tool.

Usage:  tools/strlib/expand.py <source.fi> <target.fi>
        tools/strlib/expand.py --all      (regenerate all test sources)
        tools/strlib/expand.py --check    (are the generated files current?)
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# source (in tools/strlib/src) -> generated file (in tests/ resp. tools/)
TARGETS = [
    ("tools/strlib/src/300_str16_surrogate.fi", "tests/300_str16_surrogate.fi"),
    ("tools/strlib/src/301_bytes_utf8.fi", "tests/301_bytes_utf8.fi"),
    ("tools/strlib/src/302_atom_intern.fi", "tests/302_atom_intern.fi"),
    ("tools/strlib/src/303_wtf8_roundtrip.fi", "tests/303_wtf8_roundtrip.fi"),
    ("tools/strlib/src/304_strtod_hardcases.fi", "tests/304_strtod_hardcases.fi"),
    ("tools/strlib/src/305_dtoa_hardcases.fi", "tests/305_dtoa_hardcases.fi"),
    ("tools/strlib/src/306_dtoa_roundtrip_small.fi", "tests/306_dtoa_roundtrip_small.fi"),
    ("tools/strlib/src/307_bignum.fi", "tests/307_bignum.fi"),
    ("tools/strlib/src/308_str16_api.fi", "tests/308_str16_api.fi"),
    ("tools/strlib/src/dtoa_stream.fi", "tools/dtoa_vectors/dtoa_stream.fi"),
    # std facade (round 39): lib/str and lib/num are include
    # libraries; the facade is put together textually as ONE module.
    ("tools/strlib/src/std_str.fi", "lib/std/str.fi"),
    ("tools/strlib/src/std_num.fi", "lib/std/num.fi"),
    # ROUND 73: the freestanding half of the library. `lib/std/core.fi`
    # holds everything that needs neither an allocator nor a system call
    # and is therefore admissible under `profile kernel`; `lib/std/math.fi`
    # became a generated file in the same move, because its body now lives
    # in lib/math/core_math.fi and is included by BOTH.
    ("tools/strlib/src/std_core.fi", "lib/std/core.fi"),
    ("tools/strlib/src/std_math.fi", "lib/std/math.fi"),
    ("tools/strlib/src/neg/str_bytes_is_no_text.fi", "tests/neg/str_bytes_is_no_text.fi"),
    ("tools/strlib/src/neg/str16_is_no_bytes.fi", "tests/neg/str16_is_no_bytes.fi"),
]


def expand(path, seen, out, stack):
    real = os.path.normpath(os.path.join(ROOT, path))
    if real in seen:
        return
    seen.add(real)
    if not os.path.exists(real):
        raise SystemExit("expand.py: '%s' does not exist (included from %s)"
                         % (path, stack[-1] if stack else "<oben>"))
    with open(real, encoding="utf-8") as f:
        lines = f.read().split("\n")
    stack.append(path)
    for line in lines:
        s = line.strip()
        if s.startswith("//#include"):
            inc = s[len("//#include"):].strip()
            expand(inc, seen, out, stack)
        elif s.startswith("//#str "):
            # //#str name text   ->   fn text_name(b: *mut Bytes) { ... }
            rest = s[len("//#str "):]
            name, _, text = rest.partition(" ")
            out.extend(gen_text_fn(name, text))
        else:
            out.append(line)
    stack.pop()


def gen_text_fn(name, text):
    """Produces a Firn function that puts `text` into a Bytes as ASCII.

    Stage 0 has no string literals yet (connecting
    compiler/src/strings.rs to the lexer belongs to the module kern); until
    then this tool builds the octet sequence.
    """
    lines = ["// Text: %s" % text, "fn text_%s(b: *mut Bytes) {" % name,
             "    bytes_clear(b)"]
    for ch in text.encode("utf-8"):
        lines.append("    bytes_push(b, %d as u8)" % ch)
    lines.append("}")
    return lines


def fix_error_line(lines):
    """`// expect_error: ?:COLUMN text` gets the line number of the line
    marked with `// FEHLERZEILE` -- the library grows, the expectation
    stays right."""
    mark = None
    for i, l in enumerate(lines):
        if l.rstrip().endswith("// FEHLERZEILE"):
            mark = i + 1
            break
    if mark is None:
        return lines
    for i, l in enumerate(lines):
        if l.startswith("// expect_error: ?:"):
            lines[i] = l.replace("// expect_error: ?:", "// expect_error: %d:" % mark, 1)
            break
    return lines


def build(src, dst):
    out = []
    expand(src, set(), out, [])
    text = "\n".join(out)
    # merge double empty lines so that the generated file stays readable
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")
    # ROUND 73: an included file that ends with a blank line leaves that
    # blank line in the middle resp. at the end of the product -- and
    # `firnfmt -c` then reports the GENERATED file as not canonical, which
    # cannot be repaired by hand (it would be overwritten on the next run).
    # So the seam is cleaned here, where it comes into being.
    text = text.rstrip("\n") + "\n"
    header = "// GENERATED by tools/strlib/expand.py from %s -- do not edit by hand.\n" % src
    # The expectation line (// expect_*) has to stay line 1 (test.sh reads it).
    lines = text.split("\n")
    if lines and lines[0].startswith("// expect"):
        lines = [lines[0], header.rstrip("\n")] + lines[1:]
    else:
        lines = [header.rstrip("\n")] + lines
    # Only now are the line numbers finally settled.
    lines = fix_error_line(lines)
    return "\n".join(lines)


def main(argv):
    if len(argv) == 2 and argv[1] in ("--all", "--check"):
        bad = 0
        for src, dst in TARGETS:
            text = build(src, dst)
            full = os.path.join(ROOT, dst)
            old = None
            if os.path.exists(full):
                with open(full, encoding="utf-8") as f:
                    old = f.read()
            if argv[1] == "--check":
                if old != text:
                    print("out of date: %s" % dst)
                    bad += 1
            else:
                if old != text:
                    os.makedirs(os.path.dirname(full), exist_ok=True)
                    with open(full, "w", encoding="utf-8") as f:
                        f.write(text)
                    print("generated: %s" % dst)
        if argv[1] == "--check":
            print("expand.py: %d files out of date" % bad)
            return 1 if bad else 0
        return 0
    if len(argv) != 3:
        print(__doc__)
        return 2
    text = build(argv[1], argv[2])
    with open(argv[2], "w", encoding="utf-8") as f:
        f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
