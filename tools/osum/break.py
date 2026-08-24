#!/usr/bin/env python3
"""tools/osum/break.py -- ELF files with exactly one thing wrong.

A loader is only worth as much as its refusals. This tool takes a program
that WORKS and changes ONE field, so that what the kernel says can be
traced back to that one field and to nothing else. The reason numbers are
`elf.R_*` in `demos/kernel/elf.fi`.

    magic    the four octets 0x7F E L F        -> 4  R_MAGIC
    class    ELFCLASS64 -> ELFCLASS32          -> 5  R_CLASS
    endian   little -> big                     -> 6  R_ENDIAN
    version  e_ident version 1 -> 0            -> 7  R_VERSION
    type     ET_EXEC -> ET_DYN                 -> 8  R_TYPE
    machine  x86-64 -> i386                    -> 9  R_MACHINE
    phent    e_phentsize 56 -> 32              -> 10 R_SIZES
    phnum    e_phnum -> 99                     -> 11 R_PHNUM
    phoff    e_phoff -> 0xFFFFFFFFFFFFFF00     -> 12 R_PHOFF
    short    the file cut to 32 octets         -> 3  R_SHORT
    noload   every PT_LOAD -> PT_NOTE          -> 21 R_NOLOAD
    segfile  segment 0 moved to the last page  -> 13 R_SEGFILE
    bigoff   p_offset -> 0xFFFFFFFFFFFFF000    -> 13 R_SEGFILE
    memsz    p_filesz > p_memsz                -> 14 R_MEMSZ
    range    p_vaddr -> 0x50000000             -> 15 R_RANGE
    bigmem   p_memsz -> 0xFFFFFFFFFFFF0000     -> 15 R_RANGE
    align    p_vaddr + 8                       -> 16 R_ALIGN
    overlap  segment 1 onto segment 0          -> 17 R_OVERLAP
    noexec   the entry segment loses PF_X      -> 19 R_ENTRY

The last four of the numeric ones (`phoff`, `bigoff`, `bigmem`) are the
important ones: they are HUGE, and a loader that computes `off + filesz`
on them wraps around. Under `profile kernel` that is a CHECKED addition,
it calls `osum_panic`, and the kernel is dead instead of the file being
refused. Every bound in `elf.fi` is written as a subtraction because of
these three cases.

Usage: break.py <in.elf> <out.elf> <what>
"""

import struct
import sys

E_TYPE, E_MACHINE, E_ENTRY, E_PHOFF = 16, 18, 24, 32
E_PHENTSIZE, E_PHNUM = 54, 56
P_TYPE, P_FLAGS, P_OFFSET, P_VADDR, P_FILESZ, P_MEMSZ = 0, 4, 8, 16, 32, 40
PHENT = 56
PT_LOAD, PT_NOTE = 1, 4

# The header level defects never make the loader look at a segment, so
# the file may be cut down to one page -- 34 of those on a small disk
# would otherwise be 500 KiB of image nobody reads.
HEADER_ONLY = {"magic", "class", "endian", "version", "type", "machine",
               "phent", "phnum", "phoff", "noload"}


def u16(d, at):
    return struct.unpack_from("<H", d, at)[0]


def u64(d, at):
    return struct.unpack_from("<Q", d, at)[0]


def p16(d, at, v):
    struct.pack_into("<H", d, at, v)


def p32(d, at, v):
    struct.pack_into("<I", d, at, v)


def p64(d, at, v):
    struct.pack_into("<Q", d, at, v & 0xFFFFFFFFFFFFFFFF)


def segments(d):
    off, n = u64(d, E_PHOFF), u16(d, E_PHNUM)
    return [off + i * PHENT for i in range(n)]


def entry_segment(d):
    e = u64(d, E_ENTRY)
    for ph in segments(d):
        if u64(d, ph + P_TYPE) & 0xFFFFFFFF != PT_LOAD:
            continue
        v, m = u64(d, ph + P_VADDR), u64(d, ph + P_MEMSZ)
        if v <= e < v + m:
            return ph
    return segments(d)[0]


def loads(d):
    return [ph for ph in segments(d)
            if struct.unpack_from("<I", d, ph + P_TYPE)[0] == PT_LOAD]


def main(argv):
    if len(argv) != 4:
        print(__doc__)
        return 2
    src, dst, what = argv[1], argv[2], argv[3]
    d = bytearray(open(src, "rb").read())

    if what == "short":
        d = d[:32]
    elif what == "magic":
        d[0] = 0
    elif what == "class":
        d[4] = 1
    elif what == "endian":
        d[5] = 2
    elif what == "version":
        d[6] = 0
    elif what == "type":
        p16(d, E_TYPE, 3)          # ET_DYN
    elif what == "machine":
        p16(d, E_MACHINE, 3)       # EM_386
    elif what == "phent":
        p16(d, E_PHENTSIZE, 32)
    elif what == "phnum":
        p16(d, E_PHNUM, 99)
    elif what == "phoff":
        p64(d, E_PHOFF, 0xFFFFFFFFFFFFFF00)
    elif what == "noload":
        for ph in loads(d):
            p32(d, ph + P_TYPE, PT_NOTE)
    elif what == "segfile":
        # The segment keeps its length and is moved to the LAST page of
        # the file, so that it reaches past the end. p_filesz stays where
        # it was on purpose -- raising it as well would trip the
        # `filesz > memsz` check first and measure the wrong thing.
        ph = loads(d)[0]
        p64(d, ph + P_OFFSET, len(d) & ~0xFFF)
    elif what == "bigoff":
        p64(d, loads(d)[0] + P_OFFSET, 0xFFFFFFFFFFFFF000)
    elif what == "memsz":
        # The FIRST loadable segment, not the last: a linker may leave an
        # empty PT_LOAD at the end (p_memsz 0), the loader skips those,
        # and a defect placed there would be a file that still works.
        ph = loads(d)[0]
        p64(d, ph + P_FILESZ, u64(d, ph + P_MEMSZ) + 1)
    elif what == "range":
        p64(d, loads(d)[0] + P_VADDR, 0x50000000)
    elif what == "bigmem":
        p64(d, loads(d)[0] + P_MEMSZ, 0xFFFFFFFFFFFF0000)
    elif what == "align":
        ph = loads(d)[0]
        p64(d, ph + P_VADDR, u64(d, ph + P_VADDR) + 8)
    elif what == "overlap":
        segs = loads(d)
        if len(segs) < 2:
            raise SystemExit("break: '%s' has only one PT_LOAD" % src)
        p64(d, segs[1] + P_VADDR, u64(d, segs[0] + P_VADDR))
    elif what == "noexec":
        ph = entry_segment(d)
        p32(d, ph + P_FLAGS, 4)    # PF_R only
    else:
        print("break: unknown defect '%s'" % what)
        return 2

    if what in HEADER_ONLY and what != "short":
        d = d[:4096]
    open(dst, "wb").write(bytes(d))
    print("break: %s -> %s (%s, %d octets)" % (src, dst, what, len(d)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
