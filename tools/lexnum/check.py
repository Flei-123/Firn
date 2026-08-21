#!/usr/bin/env python3
"""tools/lexnum/check.py -- holds four number readers against each other.

  1. firnc0        the lexer in Rust        (`firnc --emit=tokens`)
  2. firnc1        the lexer in Firn        (`bin/lexdump.fi`)
  3. C strtod      glibc                    (`tools/lexnum/ref.c`)
  4. Python float  CPython                  (right here)

Compared is the BIT PATTERN, not the printed value: two doubles that differ
by one ULP print the same in most formats, and that is exactly how the
divergence of round 63 stayed unnoticed for so long.

A deviation is a FAILURE, whoever it is against -- if the four do not agree,
one of them is wrong and it has to be found out which.
"""
import re
import struct
import sys

FLOAT_TOKEN = re.compile(r"Float\((\d+)\)")
INT_TOKEN = re.compile(r"Int\((\d+)\)")
SHOW = 10


def read_lines(path):
    with open(path) as handle:
        return [line.rstrip("\n") for line in handle if line.strip() != ""]


def read_tokens(path, pattern):
    out = []
    with open(path) as handle:
        for line in handle:
            found = pattern.search(line)
            if found:
                out.append(int(found.group(1)))
    return out


def python_float_bits(text):
    try:
        value = float(text)
    except (ValueError, OverflowError):
        return None
    return struct.unpack("<Q", struct.pack("<d", value))[0]


def python_int_value(text):
    body = text.replace("_", "")
    try:
        if body[:2].lower() == "0x":
            return int(body[2:], 16)
        if body[:2].lower() == "0b":
            return int(body[2:], 2)
        return int(body, 10)
    except ValueError:
        return None


def compare(name, cases, left, right, failures):
    """Column against column; `left` is always firnc0."""
    bad = 0
    for i, case in enumerate(cases):
        if right[i] is None:
            continue
        if left[i] != right[i]:
            bad += 1
            if len(failures) < SHOW:
                short = case if len(case) <= 60 else case[:57] + "..."
                failures.append("     %-14s %-62s %d != %d"
                                % (name, short, left[i], right[i]))
    print("   %-22s %6d differing" % (name, bad))
    return bad


def main():
    work = sys.argv[1] if len(sys.argv) > 1 else "."
    problems = 0
    failures = []

    # --- floating point ----------------------------------------------------
    cases = read_lines(work + "/float_cases.fi")
    plain = read_lines(work + "/float_plain.txt")
    firnc0 = read_tokens(work + "/float_a.txt", FLOAT_TOKEN)
    firnc1 = read_tokens(work + "/float_b.txt", FLOAT_TOKEN)
    strtod = [int(x) for x in read_lines(work + "/float_c.txt")]
    python = [python_float_bits(text) for text in plain]

    print("   float literals         %6d" % len(cases))
    for name, got in (("token count firnc0", firnc0),
                      ("token count firnc1", firnc1),
                      ("line count strtod", strtod)):
        if len(got) != len(cases):
            print("   %-22s %6d instead of %d -- the streams do not line up"
                  % (name, len(got), len(cases)))
            problems += 1
    if problems:
        return 1

    problems += compare("firnc0 vs firnc1", cases, firnc0, firnc1, failures)
    problems += compare("firnc0 vs C strtod", cases, firnc0, strtod, failures)
    problems += compare("firnc0 vs Python", cases, firnc0, python, failures)

    # --- integers ----------------------------------------------------------
    int_cases = read_lines(work + "/int_cases.fi")
    int_a = read_tokens(work + "/int_a.txt", INT_TOKEN)
    int_b = read_tokens(work + "/int_b.txt", INT_TOKEN)
    int_python = [python_int_value(text) for text in int_cases]
    print("   integer literals       %6d" % len(int_cases))
    if len(int_a) != len(int_cases) or len(int_b) != len(int_cases):
        print("   integer token count %d/%d instead of %d"
              % (len(int_a), len(int_b), len(int_cases)))
        return 1
    problems += compare("firnc0 vs firnc1", int_cases, int_a, int_b, failures)
    problems += compare("firnc0 vs Python", int_cases, int_a, int_python,
                        failures)

    if failures:
        print("   the first deviations:")
        for line in failures:
            print(line)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
