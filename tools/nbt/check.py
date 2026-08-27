#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/nbt/check.py -- the SECOND NBT parser (round 76).

The Firn side (`tools/nbt/dump.fi`) turns an NBT file into a canonical text.
This file does the same in Python, with a parser that shares not one line
with the repository, and `tools/nbt/run.sh` compares the two texts with
`diff`. If the Firn reader misplaces one octet, a name, a length or a
nesting level, the texts differ and the round fails.

It can also WRITE -- so that the traffic runs in both directions: a file
built here, read by Firn.

    check.py dump  <file> [anon]     canonical text on stdout
    check.py write <file>            an NBT file with all thirteen tag types
    check.py bytes <a> <b> <n>       first n octets of a and b identical?
"""
import struct
import sys

END, BYTE, SHORT, INT, LONG, FLOAT, DOUBLE = 0, 1, 2, 3, 4, 5, 6
BYTE_ARRAY, STRING, LIST, COMPOUND, INT_ARRAY, LONG_ARRAY = 7, 8, 9, 10, 11, 12


def fnv(b):
    h = 14695981039346656037
    for x in b:
        h = ((h ^ x) * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return h


class Reader:
    def __init__(self, data):
        self.d = data
        self.p = 0

    def take(self, n):
        if self.p + n > len(self.d):
            raise EOFError("underflow at %d, wanted %d, %d left"
                           % (self.p, n, len(self.d) - self.p))
        r = self.d[self.p:self.p + n]
        self.p += n
        return r

    def u1(self):
        return self.take(1)[0]

    def i1(self):
        return struct.unpack(">b", self.take(1))[0]

    def u2(self):
        return struct.unpack(">H", self.take(2))[0]

    def i2(self):
        return struct.unpack(">h", self.take(2))[0]

    def i4(self):
        return struct.unpack(">i", self.take(4))[0]

    def i8(self):
        return struct.unpack(">q", self.take(8))[0]

    def name(self):
        return self.take(self.u2())


OUT = []


def line(depth, t, name, val):
    OUT.append("%d|%d|%s|%s" % (depth, t, name.hex(), val))


def value(r, t, depth):
    if t == BYTE:
        line_val = str(r.i1())
    elif t == SHORT:
        line_val = str(r.i2())
    elif t == INT:
        line_val = str(r.i4())
    elif t == LONG:
        line_val = str(r.i8())
    elif t == FLOAT:
        line_val = "f%d" % struct.unpack(">I", r.take(4))[0]
    elif t == DOUBLE:
        line_val = "d%d" % struct.unpack(">Q", r.take(8))[0]
    elif t == STRING:
        s = r.name()
        line_val = "%d:%s" % (len(s), s.hex())
    elif t in (BYTE_ARRAY, INT_ARRAY, LONG_ARRAY):
        width = {BYTE_ARRAY: 1, INT_ARRAY: 4, LONG_ARRAY: 8}[t]
        n = r.i4()
        if n < 0:
            raise ValueError("negative array length %d" % n)
        raw = r.take(n * width)
        line_val = "n=%d fnv=%016x" % (n, fnv(raw))
    elif t == LIST:
        elem = r.u1()
        n = r.i4()
        if n < 0:
            raise ValueError("negative list length %d" % n)
        line_val = "elem=%d n=%d" % (elem, n)
        OUT[-1] = OUT[-1] + line_val
        for _ in range(n):
            OUT.append("%d|%d|%s|" % (depth + 1, elem, ""))
            value(r, elem, depth + 1)
        return
    elif t == COMPOUND:
        OUT[-1] = OUT[-1] + "-"
        compound(r, depth + 1)
        return
    else:
        raise ValueError("bad tag %d" % t)
    OUT[-1] = OUT[-1] + line_val


def compound(r, depth):
    while True:
        t = r.u1()
        if t == END:
            OUT.append("%d|0||-" % depth)
            return
        nm = r.name()
        OUT.append("%d|%d|%s|" % (depth, t, nm.hex()))
        value(r, t, depth)


def do_dump(path, anon):
    r = Reader(open(path, "rb").read())
    t = r.u1()
    if t != COMPOUND:
        raise ValueError("root is tag %d, not a compound" % t)
    nm = b"" if anon else r.name()
    OUT.append("0|10|%s|-" % nm.hex())
    compound(r, 1)
    if r.p != len(r.d):
        raise ValueError("%d octets left over after the root compound"
                         % (len(r.d) - r.p))
    sys.stdout.write("\n".join(OUT) + "\n")


# ---------------------------------------------------------------- writing

def w_name(o, s):
    o += struct.pack(">H", len(s)) + s
    return o


def do_write(path):
    """An NBT file with ALL thirteen tag types, values chosen so that every
    edge is in it: the extremes of every width, a negative number in every
    signed type, an empty string, an empty compound, an empty list, a list
    of lists, and nesting."""
    o = b""
    o += bytes([COMPOUND])
    o = w_name(o, b"from-python")
    o += bytes([BYTE]) + struct.pack(">H", 3) + b"b-1" + struct.pack(">b", -1)
    o += bytes([BYTE]) + struct.pack(">H", 5) + b"b-min" + struct.pack(">b", -128)
    o += bytes([SHORT]) + struct.pack(">H", 5) + b"s-min" + struct.pack(">h", -32768)
    o += bytes([INT]) + struct.pack(">H", 5) + b"i-min" + struct.pack(">i", -2147483648)
    o += bytes([LONG]) + struct.pack(">H", 5) + b"l-min" + struct.pack(">q", -(2**63))
    o += bytes([FLOAT]) + struct.pack(">H", 5) + b"f-pi\x21" + struct.pack(">f", 3.14159265)
    o += bytes([DOUBLE]) + struct.pack(">H", 5) + b"d-pi\x21" + struct.pack(">d", 3.141592653589793)
    o += bytes([STRING]) + struct.pack(">H", 6) + b"s-utf8"
    o = w_name(o, "grüße, wörld ✓".encode("utf-8"))
    o += bytes([STRING]) + struct.pack(">H", 7) + b"s-empty" + struct.pack(">H", 0)
    o += bytes([BYTE_ARRAY]) + struct.pack(">H", 2) + b"ba" + struct.pack(">i", 260)
    o += bytes(range(256)) + bytes([0, 1, 2, 3])
    o += bytes([INT_ARRAY]) + struct.pack(">H", 2) + b"ia" + struct.pack(">i", 4)
    o += struct.pack(">iiii", -2147483648, -1, 0, 2147483647)
    o += bytes([LONG_ARRAY]) + struct.pack(">H", 2) + b"la" + struct.pack(">i", 3)
    o += struct.pack(">qqq", -(2**63), 0, 2**63 - 1)
    # an empty list, a list of lists, a list of compounds
    o += bytes([LIST]) + struct.pack(">H", 6) + b"l-void" + bytes([END]) + struct.pack(">i", 0)
    o += bytes([LIST]) + struct.pack(">H", 6) + b"l-list" + bytes([LIST]) + struct.pack(">i", 2)
    o += bytes([INT]) + struct.pack(">i", 2) + struct.pack(">ii", 7, 8)
    o += bytes([END]) + struct.pack(">i", 0)
    o += bytes([LIST]) + struct.pack(">H", 6) + b"l-comp" + bytes([COMPOUND]) + struct.pack(">i", 2)
    o += bytes([INT]) + struct.pack(">H", 1) + b"a" + struct.pack(">i", 1) + bytes([END])
    o += bytes([END])  # the second element: an EMPTY compound
    # a compound that contains nothing
    o += bytes([COMPOUND]) + struct.pack(">H", 7) + b"c-empty" + bytes([END])
    # three levels of nesting
    o += bytes([COMPOUND]) + struct.pack(">H", 2) + b"c1"
    o += bytes([COMPOUND]) + struct.pack(">H", 2) + b"c2"
    o += bytes([COMPOUND]) + struct.pack(">H", 2) + b"c3"
    o += bytes([INT]) + struct.pack(">H", 4) + b"deep" + struct.pack(">i", 42)
    o += bytes([END, END, END])
    o += bytes([END])
    open(path, "wb").write(o)
    sys.stderr.write("check.py: wrote %d octets to %s\n" % (len(o), path))


def main():
    if len(sys.argv) < 3:
        sys.stderr.write(__doc__)
        return 2
    cmd = sys.argv[1]
    if cmd == "dump":
        do_dump(sys.argv[2], len(sys.argv) > 3 and sys.argv[3] == "anon")
        return 0
    if cmd == "write":
        do_write(sys.argv[2])
        return 0
    if cmd == "bytes":
        a = open(sys.argv[2], "rb").read()
        b = open(sys.argv[3], "rb").read()
        n = int(sys.argv[4])
        if len(a) < n or len(b) < n:
            print("SHORT a=%d b=%d want=%d" % (len(a), len(b), n))
            return 1
        for i in range(n):
            if a[i] != b[i]:
                print("DIFFER at octet %d: %02x vs %02x" % (i, a[i], b[i]))
                return 1
        print("IDENTICAL over %d octets" % n)
        return 0
    sys.stderr.write("unknown command %r\n" % cmd)
    return 2


if __name__ == "__main__":
    sys.exit(main())
