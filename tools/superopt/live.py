from x86 import REG, IDENT, split_line

PURE_WRITE = {"mov", "movzx", "movsx", "movsxd", "lea"}
EXPLICIT = {
    "mov", "movzx", "movsx", "movsxd", "lea", "add", "sub", "and", "or", "xor", "cmp", "test",
    "imul", "shl", "shr", "sar", "rol", "ror", "neg", "not", "inc", "dec", "movss", "movsd",
    "movq", "movd", "cvtsi2sd", "cvtsi2ss", "cvttsd2si", "cvttss2si", "addsd", "subsd", "mulsd",
    "divsd", "addss", "subss", "mulss", "divss", "ucomisd", "ucomiss", "movaps", "movapd",
    "xorps", "xorpd", "andps", "andpd", "sqrtsd", "sqrtss", "popcnt", "lzcnt", "tzcnt", "bswap",
}
FLAG_FULL = {"add", "sub", "and", "or", "xor", "cmp", "test", "neg"}
FLAG_NEUTRAL = {"mov", "movzx", "movsx", "movsxd", "lea", "not", "nop", "movss", "movsd", "movq",
                "movd", "movaps", "movapd", "bswap"}


def is_boundary(s):
    return s.endswith(":") or (s.startswith(".") and not s.startswith(".loc"))


def regs_in(op):
    return [REG[t][0] for t in IDENT.findall(op) if t in REG]


def reg_dead_after(lines, start, reg):
    for line in lines[start:]:
        s = line.strip()
        if not s or s.startswith("#") or s.startswith(".loc"):
            continue
        if is_boundary(s):
            return False
        mn, ops = split_line(s)
        if mn not in EXPLICIT or (mn == "imul" and len(ops) == 1):
            return False
        if not ops:
            continue
        rest = [r for o in ops[1:] for r in regs_in(o)]
        dst = ops[0]
        dregs = regs_in(dst)
        if mn == "xor" and len(ops) == 2 and dst == ops[1] and dst in REG and REG[dst][0] == reg and REG[dst][1] >= 32:
            return True
        if mn in PURE_WRITE and dst in REG and REG[dst][0] == reg and REG[dst][1] >= 32 and reg not in rest:
            return True
        if reg in rest or reg in dregs:
            return False
    return False


def flags_dead_after(lines, start):
    for line in lines[start:]:
        s = line.strip()
        if not s or s.startswith("#") or s.startswith(".loc"):
            continue
        if is_boundary(s):
            return False
        mn = split_line(s)[0]
        if mn in FLAG_FULL:
            return True
        if mn in FLAG_NEUTRAL:
            continue
        return False
    return False
