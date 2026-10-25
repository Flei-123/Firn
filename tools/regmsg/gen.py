#!/usr/bin/env python3
import re
import sys

MESSAGES = [
    (101, "unknown error set '%n'"),
    (102, "the success type of an error union cannot be '()'"),
    (103, "an error union over the success type '%t' cannot be the field type of a struct"),
    (104, "type '%n' is already declared"),
    (201, "type '%n' is already declared"),
    (202, "unknown base class '%n'"),
    (203, "the inheritance chain of 'gc class %n' is cyclic"),
    (204, "field '%n' is already taken in the base of 'gc class %n'"),
    (205, "field type %t is not allowed in a gc class"),
    (206, "unknown gc class '%n'"),
    (207, "'%n' is a gc class and cannot be a value"),
    (301, "unknown interface '%n'"),
    (302, "the type '%n' is ambiguous"),
    (303, "unknown type '%n'"),
    (304, "'%n' is an interface and not a type"),
    (305, "'%n' already implements the interface '%n'"),
    (306, "'%n' does not implement the method '%n.%n'"),
    (307, "the receiver of '%n.%n' does not fit '%n'"),
    (308, "'%n.%n' has %d parameters, the interface '%n' requires %d"),
    (309, "parameter %d of '%n.%n' has type %t, the interface '%n' requires %t"),
    (310, "'%n.%n' returns %t, the interface '%n' requires %t"),
    (401, "type '%n' is already declared"),
    (402, "field '%n' has the enum type '%n' — that is not supported in this stage"),
    (403, "enum '%n' contains itself (directly or indirectly)"),
    (404, "the payload of a variant cannot have the type '()'"),
]

HEAD = """// SPDX-License-Identifier: MPL-2.0

import rt
import intern
import types
import msgfmt
import report
import regerr

export { regmsg_say }

fn regmsg_say(r: *mut regerr.RegErr, i: usize, pend: *mut report.Pend, dmsg: *mut rt.Buf,
        t: *mut types.Types, it: *mut intern.Interner) -> bool {
    let code: u32 = regerr.regerr_code(r, i)
    let pos: u64 = regerr.regerr_pos(r, i)
    let a0: u64 = regerr.regerr_arg(r, i, 0)
    let a1: u64 = regerr.regerr_arg(r, i, 1)
    let a2: u64 = regerr.regerr_arg(r, i, 2)
    let a3: u64 = regerr.regerr_arg(r, i, 3)
    let a4: u64 = regerr.regerr_arg(r, i, 4)
    let a5: u64 = regerr.regerr_arg(r, i, 5)
    rt.buf_clear(dmsg)
"""


def chunks(template):
    parts = re.split(r"(%[tnd])", template)
    out = []
    cur = ""
    used = 0
    for part in parts:
        if re.fullmatch(r"%[tnd]", part):
            if used == 4:
                out.append((cur, used))
                cur = ""
                used = 0
            used += 1
        cur += part
    out.append((cur, used))
    return out


def main(path):
    lines = [HEAD]
    for code, template in MESSAGES:
        lines.append("    if code == %d {" % code)
        arg = 0
        for n, (text, used) in enumerate(chunks(template)):
            size = len(text.encode("utf-8"))
            name = "m%d_%d" % (code, n)
            args = ["a%d" % (arg + k) for k in range(used)] + ["0"] * (4 - used)
            arg += used
            lines.append('        var %s: [u8; %d] = "%s"' % (name, size, text))
            lines.append("        msgfmt.fmt_msg(dmsg, (&%s[0]) as u64, %d, t, it, %s)" % (name, size, ", ".join(args)))
        lines.append("        report.pend_add(pend, pos, 1, dmsg)")
        lines.append("        return true")
        lines.append("    }")
    lines.append("    return false")
    lines.append("}")
    open(path, "w").write("\n".join(lines) + "\n")


main(sys.argv[1])
