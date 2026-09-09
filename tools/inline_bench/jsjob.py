#!/usr/bin/env python3
"""ROUND INLINE -- build a job stream for lib/js/run_main.fi.

Job format (little endian), from the header of run_main.fi:
    u32 mode        0 = script, 1 = module (parse only), 2 = script + GC report
    u32 n ; u8[n]   the source text (UTF-8)

Usage: jsjob.py <rounds> > job.bin
The load is the allocating loop round SAMMLER used to stand in for Certus:
an object literal per iteration, so the collector sees a stream of small
short lived objects, plus a little arithmetic so the interpreter loop itself
is represented.
"""
import struct, sys

rounds = int(sys.argv[1]) if len(sys.argv) > 1 else 20000
src = (
    "var s = 0;\n"
    "for (var i = 0; i < %d; i++) {\n"
    "  var o = {a: i, b: i + 1};\n"
    "  s = s + o.a + o.b;\n"
    "}\n"
    "print(s);\n" % rounds
)
b = src.encode()
sys.stdout.buffer.write(struct.pack("<II", 0, len(b)) + b)
