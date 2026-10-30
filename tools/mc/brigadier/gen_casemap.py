#!/usr/bin/env python3
import sys

lower, upper, cased = [], [], []
for line in sys.stdin:
    tag, a, b = line.split()
    {"L": lower, "U": upper, "C": cased}[tag].append((int(a), int(b)))


def table(name, rows, col):
    vals = [r[col] for r in rows]
    lines = []
    for i in range(0, len(vals), 12):
        lines.append("    " + ", ".join(str(v) for v in vals[i:i + 12]))
    return "static %s: [u32; %d] = [\n%s]\n" % (name, len(vals), ",\n".join(lines))


out = ["// SPDX-License-Identifier: MIT",
       "// lib/brigadier/casemap.fi -- brigadier.casemap: generated Java Character case tables (tools/mc/brigadier/gen_casemap.sh), do not edit.",
       "",
       "export { cm_lower, cm_upper, cm_cased }",
       ""]
out.append(table("LOWER_KEYS", lower, 0))
out.append(table("LOWER_VALS", lower, 1))
out.append(table("UPPER_KEYS", upper, 0))
out.append(table("UPPER_VALS", upper, 1))
out.append(table("CASED_LO", cased, 0))
out.append(table("CASED_HI", cased, 1))
out.append("""fn cm_lower(cp: u32) -> u32 {
    var lo: usize = 0
    var hi: usize = %d
    while lo < hi {
        let mid: usize = (lo + hi) / 2
        if LOWER_KEYS[mid] < cp {
            lo = mid + 1
        } else {
            hi = mid
        }
    }
    if lo < %d && LOWER_KEYS[lo] == cp {
        return LOWER_VALS[lo]
    }
    return cp
}

fn cm_upper(cp: u32) -> u32 {
    var lo: usize = 0
    var hi: usize = %d
    while lo < hi {
        let mid: usize = (lo + hi) / 2
        if UPPER_KEYS[mid] < cp {
            lo = mid + 1
        } else {
            hi = mid
        }
    }
    if lo < %d && UPPER_KEYS[lo] == cp {
        return UPPER_VALS[lo]
    }
    return cp
}

fn cm_cased(cp: u32) -> bool {
    var lo: usize = 0
    var hi: usize = %d
    while lo < hi {
        let mid: usize = (lo + hi) / 2
        if CASED_HI[mid] < cp {
            lo = mid + 1
        } else {
            hi = mid
        }
    }
    return lo < %d && CASED_LO[lo] <= cp
}
""" % (len(lower), len(lower), len(upper), len(upper), len(cased), len(cased)))
sys.stdout.write("\n".join(out))
