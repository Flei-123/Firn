#!/usr/bin/env python3
"""tools/lexnum/gen.py -- the literals for the differential test of the two
lexers (round 65).

It writes into the work directory:

  float_cases.fi    one floating point literal per line, as Firn source
  float_plain.txt   the same literals with the separator `_` removed, so
                    that C `strtod` and Python `float` can read them
  int_cases.fi      integer literals: decimal, hexadecimal, binary
  int_plain.txt     the same for the reference
  bad_cases.fi      literals that MUST be refused -- there only the two
                    lexers are held against each other, message for message

The point of the exercise is not to be pretty but to be MEAN: exact halfway
cases written out to the last digit, the tie of the smallest subnormal,
mantissas that no longer fit into a u64, exponents far outside the range,
and literals with eight hundred significant digits. Every one of them has
brought a naive number reader down.
"""
import math
import random
import struct
import sys
from decimal import Decimal, getcontext

getcontext().prec = 3000

DIGITS = "0123456789"
HEX_DIGITS = "0123456789abcdefABCDEF"


def double_of(pattern):
    return struct.unpack("<d", struct.pack("<Q", pattern))[0]


def plain(text):
    """A literal as C and Python want it: without the separator."""
    return text.replace("_", "")


def with_separators(rng, text):
    """Put `_` between two digits -- Firn allows that, and both lexers have
    to skip it in exactly the same places."""
    out = []
    for i, character in enumerate(text):
        out.append(character)
        if (character in DIGITS and i + 1 < len(text)
                and text[i + 1] in DIGITS and rng.random() < 0.25):
            out.append("_")
    return "".join(out)


def fixed(value):
    """A Decimal without an exponent -- that is what makes the literals long.

    A point is forced: from 2^54 on the middle between two doubles is a
    WHOLE number, and `81753873681479438581449069391443141214503572602880000`
    is an integer literal in Firn, not a floating point one (and one that no
    longer fits into 64 bit at that)."""
    text = format(value, "f")
    if "." not in text:
        text = text + ".0"
    return text


def case_shortest(rng, count):
    """Random doubles in their shortest form -- the everyday case."""
    out = []
    while len(out) < count:
        pattern = rng.getrandbits(63)
        if pattern >= 0x7FF0000000000000:
            continue
        out.append(repr(double_of(pattern)))
    return out


def case_random_digits(rng, count):
    """Digit strings of one to twenty-five digits with a random point and a
    random exponent. This is where the mantissa leaves the u64."""
    out = []
    for _ in range(count):
        n = rng.randint(1, 25)
        text = "".join(rng.choice(DIGITS) for _ in range(n))
        point = rng.randint(1, n)
        head, tail = text[:point], text[point:]
        body = head + "." + tail if tail else head
        if rng.random() < 0.7:
            body = body + rng.choice("eE") + rng.choice(["", "+", "-"]) \
                + str(rng.randint(0, 330))
        elif not tail:
            body = body + ".0"
        out.append(body)
    return out


def case_long_digits(rng, count):
    """Forty to eight hundred significant digits: more than a double can
    ever tell apart, so everything below the last place has to be folded
    into a sticky digit."""
    out = []
    for _ in range(count):
        n = rng.randint(40, 800)
        text = rng.choice("123456789") + "".join(
            rng.choice(DIGITS) for _ in range(n - 1))
        point = rng.randint(1, min(n, 20))
        body = text[:point] + "." + text[point:]
        if rng.random() < 0.6:
            body = body + "e" + rng.choice(["", "+", "-"]) \
                + str(rng.randint(0, 320))
        out.append(body)
    return out


def case_around_two53(rng, count):
    """2^53 is where a double stops counting whole numbers one by one."""
    out = []
    for k in range(-12, 13):
        value = (1 << 53) + k
        out.append("%d.0" % value)
        out.append("%de0" % value)
        out.append("%d.5" % value)
        out.append("0.%d" % value)
        out.append("%de-3" % value)
    for k in range(-4, 5):
        out.append("%d.0" % ((1 << 52) + k))
        out.append("%d.0" % ((1 << 62) + k))
        out.append("%d.0" % ((1 << 64) + k))
        out.append("%d.0" % ((1 << 70) + k))
    while len(out) < count:
        exponent = rng.randint(0, 80)
        out.append("%d.0" % ((1 << 53) + rng.randint(-3, 3) + (1 << exponent)))
    return out


def case_subnormal(rng, count):
    """Below 2^-1022 a double loses bits; the last one is 2^-1074."""
    out = ["5e-324", "4.9e-324", "4.94065645841246544e-324",
           "2.4703282292062327e-324", "2.4703282292062328e-324",
           "1e-323", "1.5e-323", "9.88131291682493088353e-324",
           "2.2250738585072011e-308", "2.2250738585072012e-308",
           "2.2250738585072013e-308", "2.2250738585072014e-308",
           "2.2250738585072009e-308", "1e-320", "1e-322"]
    for _ in range(count):
        pattern = rng.randint(1, (1 << 52) - 1)
        value = double_of(pattern)
        if rng.random() < 0.5:
            out.append(repr(value))
        else:
            out.append(fixed(Decimal(value)))
    return out


def case_halfway(rng, count):
    """THE hard case: exactly in the middle between two doubles. Written out
    to the last digit, plus one step above and one step below. Only a reader
    that rounds to the EVEN mantissa gets all three right."""
    out = []
    for _ in range(count):
        pattern = rng.randint(1, 0x7FEFFFFFFFFFFFFF)
        value = double_of(pattern)
        upper = math.nextafter(value, math.inf)
        if math.isinf(upper):
            continue
        middle = (Decimal(value) + Decimal(upper)) / 2
        step = Decimal(1).scaleb(middle.as_tuple().exponent - 1)
        for candidate in (middle, middle - step, middle + step):
            if candidate <= 0:
                continue
            out.append(fixed(candidate))
    return out


def case_boundary():
    """Named cases: the edges of the range, the famous hangers, zero in
    every shape, and exponents nobody wants."""
    out = [
        "0.0", "0e0", "0.000000", "00000.00000", "0e-999999", "0e999999",
        "0.0e-400", "000000000000000000000.000000000000000000001",
        "1.0", "1e0", "1.7976931348623157e308", "1.7976931348623158e308",
        "1.7976931348623159e308", "1.797693134862315808e308",
        "1.7976931348623157081452742373170435679807056752584e308",
        "1e308", "1e309", "1e-308", "1e-309", "1e-400", "1e400",
        "1e99999", "1e-99999", "1e2147483647", "1e-2147483647",
        "1e9223372036854775807", "1e-9223372036854775807",
        "9007199254740992.0", "9007199254740993.0", "9007199254740994.0",
        "9007199254740991.0", "9007199254740995.0",
        "18446744073709551616.0", "18446744073709551615.0",
        "18446744073709551617.0", "1844674407370955161.9",
        "123456789012345678901234567890.0",
        "340282366920938463463374607431768211456.0",
        "2.2250738585072011e-308", "2.2250738585072014e-308",
        "4.9406564584124654e-324", "1.1125369292536007e-308",
        "8.98846567431158e307", "8.988465674311579538646525e307",
        "9214364837600034844e-192", "8607405231447954055e-201",
        "1.448997445238699e-14", "2.47032822920623272e-324",
        "3.518437208883201171875e13", "62.5364939768271845828",
        "8.10109172351e-323",
        "1.50000000000000011102230246251565404236316680908203125",
        "1.4999999999999999444888487687421768903732299804687500",
        "1e23", "1.0000000000000002", "1.0000000000000001",
        "0.30000000000000004", "0.1", "0.2", "0.3",
        "1e22", "1e-22", "1e37", "1e38", "1e-37", "1e-38",
    ]
    # long shapes: the same magnitude, once through the digits and once
    # through the exponent
    out.append("1" + "0" * 400 + ".0")
    out.append("0." + "0" * 400 + "1")
    out.append("1." + "0" * 779 + "1")
    out.append("1." + "0" * 800 + "1")
    out.append("9" * 400 + ".0")
    out.append("0." + "0" * 322 + "5")
    out.append("0." + "0" * 323 + "49999999999999999999999999999999")
    out.append("0." + "0" * 323 + "50000000000000000000000000000001")
    out.append("1" + "0" * 308 + "." + "0" * 100 + "1")
    return out


def case_separator(rng, base, count):
    """The same literals with `_` in between."""
    picked = rng.sample(base, min(count, len(base)))
    return [with_separators(rng, text) for text in picked]


def integer_cases(rng, count):
    out = ["0", "1", "007", "18446744073709551615", "9223372036854775808",
           "9223372036854775807", "4294967295", "4294967296",
           "0x0", "0xFFFFFFFFFFFFFFFF", "0xffffffffffffffff",
           "0Xdeadbeef", "0xDEADBEEF", "0x7fffffffffffffff",
           "0b0", "0b1", "0b" + "1" * 64, "0b1010101010",
           "0x0000000000000001", "0b" + "0" * 63 + "1",
           "1_000_000", "0xFF_FF", "0b1010_1010",
           "18_446_744_073_709_551_615"]
    while len(out) < count:
        kind = rng.randint(0, 2)
        if kind == 0:
            text = str(rng.getrandbits(rng.randint(1, 64)))
        elif kind == 1:
            n = rng.randint(1, 16)
            text = "0x" + "".join(rng.choice(HEX_DIGITS) for _ in range(n))
        else:
            n = rng.randint(1, 64)
            text = "0b" + "".join(rng.choice("01") for _ in range(n))
        if rng.random() < 0.2:
            text = with_separators(rng, text)
        out.append(text)
    return out


def bad_cases():
    """Literals that have to be REFUSED -- with the same message from both
    lexers, down to the line and the column."""
    return [
        "1e", "1e+", "1e-", "1.5e", "18446744073709551616",
        "99999999999999999999999999", "0x", "0b", "0xG", "0b2", "0x1g",
        "0b1012", "123abc", "1_e", "0x_", "1.5e_",
    ]


def main():
    count = int(sys.argv[1]) if len(sys.argv) > 1 else 5000
    seed = int(sys.argv[2]) if len(sys.argv) > 2 else 65065
    work = sys.argv[3] if len(sys.argv) > 3 else "."
    rng = random.Random(seed)

    share = max(count // 6, 1)
    groups = [
        ("shortest", case_shortest(rng, share)),
        ("random digits", case_random_digits(rng, share)),
        ("long digits", case_long_digits(rng, max(share // 4, 1))),
        ("around 2^53", case_around_two53(rng, max(share // 4, 1))),
        ("subnormal", case_subnormal(rng, max(share // 2, 1))),
        ("halfway", case_halfway(rng, max(share // 2, 1))),
        ("boundary", case_boundary()),
    ]
    everything = []
    for _, items in groups:
        everything.extend(items)
    separators = case_separator(rng, everything, max(share // 4, 1))
    groups.append(("with separators", separators))
    everything.extend(separators)

    with open(work + "/float_cases.fi", "w") as handle:
        handle.write("\n".join(everything) + "\n")
    with open(work + "/float_plain.txt", "w") as handle:
        handle.write("\n".join(plain(t) for t in everything) + "\n")

    integers = integer_cases(rng, max(count // 4, 1))
    with open(work + "/int_cases.fi", "w") as handle:
        handle.write("\n".join(integers) + "\n")
    with open(work + "/int_plain.txt", "w") as handle:
        handle.write("\n".join(plain(t) for t in integers) + "\n")

    bad = bad_cases()
    with open(work + "/bad_cases.fi", "w") as handle:
        handle.write("\n".join(bad) + "\n")

    for name, items in groups:
        print("   %-16s %6d" % (name, len(items)))
    print("   %-16s %6d" % ("FLOAT TOTAL", len(everything)))
    print("   %-16s %6d" % ("integers", len(integers)))
    print("   %-16s %6d" % ("refused", len(bad)))


if __name__ == "__main__":
    main()
