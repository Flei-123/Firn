#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/lexnum/check.py -- holds four number readers against each other.

  1. firnc0        the lexer in Rust        (`firnc --emit=tokens`)
  2. firnc1        the lexer in Firn        (`bin/lexdump.fi`)
  3. C strtod      glibc                    (`tools/lexnum/ref.c`)
  4. Python float  CPython                  (right here)

ROUND 71 does the same for `f32`, with two references of its own:

  3'. C strtof     glibc                    (`tools/lexnum/ref32.c`)
  4'. Decimal/Fraction                      (right here, exact, from scratch)

Both of them round DECIMAL -> BINARY32 directly. `numpy.float32("...")` is
deliberately NOT used as the yardstick: it parses the text into a double and
casts afterwards -- exactly the way that this round found to be wrong. The
column here computes the exact rational with `Decimal`/`Fraction` and rounds
ONCE, half to even, without a float appearing anywhere. Firn does not: it reads the
correctly rounded binary64 and narrows once. That the two ways give the same
result is a theorem (Figueroa 1995: 53 >= 2*24+2 bits) -- and this is where
it gets measured, on the exact middles between two neighbouring binary32
values, where a double rounding would be the first thing to go wrong.

Compared is the BIT PATTERN, not the printed value: two doubles that differ
by one ULP print the same in most formats, and that is exactly how the
divergence of round 63 stayed unnoticed for so long.

A deviation is a FAILURE, whoever it is against -- if the four do not agree,
one of them is wrong and it has to be found out which.
"""
import re
import struct
import sys
from decimal import Decimal, InvalidOperation, localcontext
from fractions import Fraction

FLOAT_TOKEN = re.compile(r"\bFloat\((\d+)\)")
F32_TOKEN = re.compile(r"\bFloatF32\((\d+)\)")
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


def python_f32_bits(text):
    """Exact decimal -> IEEE-754 binary32, round half to even.

    Written out by hand and without a single floating point operation:
    `Decimal` gives the exact rational, `Fraction` keeps it exact, the
    rounding is a comparison of two integers. That makes this column a
    yardstick that shares no line of code and no algorithm with either of
    the two lexers -- and none with `strtof` either."""
    # The default precision of `decimal` is 28 digits and its exponent range
    # is finite -- both would be a silent lie here. A context of its own with
    # room for the literals of this test (up to 800 digits, exponents beyond
    # 1e300) makes `Decimal` exact.
    with localcontext() as ctx:
        ctx.prec = 2000
        ctx.Emax = 999999
        ctx.Emin = -999999
        try:
            d = Decimal(text.strip())
            return f32_bits_of(d)
        except (InvalidOperation, ArithmeticError):
            return None


def f32_bits_of(d):
    if d.is_nan() or d.is_infinite():
        return None
    negative = d.is_signed()
    x = Fraction(abs(d))
    if x == 0:
        return 0x80000000 if negative else 0
    # the binary exponent: 2^e <= x < 2^(e+1)
    e = x.numerator.bit_length() - x.denominator.bit_length()
    while Fraction(2) ** e > x:
        e -= 1
    while Fraction(2) ** (e + 1) <= x:
        e += 1
    exponent = max(e - 23, -149)          # -149 = the subnormal grid
    scaled = x / Fraction(2) ** exponent
    whole, rest = divmod(scaled.numerator, scaled.denominator)
    if 2 * rest > scaled.denominator or (2 * rest == scaled.denominator
                                         and whole % 2 == 1):
        whole += 1
    while whole >= (1 << 24):
        whole >>= 1
        exponent += 1
    if exponent == -149 and whole < (1 << 23):
        bits = whole                       # subnormal
    else:
        field = exponent + 23 + 127
        if field >= 255:
            bits = 0x7F800000              # overflow -> infinity
        else:
            bits = (field << 23) | (whole - (1 << 23))
    if negative:
        bits |= 0x80000000
    return bits


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

    # --- floating point, single width (round 71) ---------------------------
    f32_cases = read_lines(work + "/f32_cases.fi")
    f32_plain = read_lines(work + "/f32_plain.txt")
    f32_a = read_tokens(work + "/f32_a.txt", F32_TOKEN)
    f32_b = read_tokens(work + "/f32_b.txt", F32_TOKEN)
    strtof = [int(x) for x in read_lines(work + "/f32_c.txt")]
    f32_python = [python_f32_bits(text) for text in f32_plain]

    print("   f32 literals           %6d" % len(f32_cases))
    for name, got in (("token count firnc0", f32_a),
                      ("token count firnc1", f32_b),
                      ("line count strtof", strtof)):
        if len(got) != len(f32_cases):
            print("   %-22s %6d instead of %d -- the streams do not line up"
                  % (name, len(got), len(f32_cases)))
            problems += 1
    if problems:
        return 1

    problems += compare("f32 firnc0 vs firnc1", f32_cases, f32_a, f32_b,
                        failures)
    problems += compare("f32 firnc0 vs C strtof", f32_cases, f32_a, strtof,
                        failures)
    problems += compare("f32 firnc0 vs exact Python", f32_cases, f32_a,
                        f32_python, failures)

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
