from x86 import REG, IDENT, split_line

PURE_WRITE = {"mov", "movzx", "movsx", "movsxd", "lea"}
EXPLICIT = {
    "mov", "movzx", "movsx", "movsxd", "lea", "add", "sub", "and", "or", "xor", "cmp", "test",
    "imul", "shl", "shr", "sar", "rol", "ror", "neg", "not", "inc", "dec", "movss", "movsd",
    "movq", "movd", "cvtsi2sd", "cvtsi2ss", "cvttsd2si", "cvttss2si", "addsd", "subsd", "mulsd",
    "divsd", "addss", "subss", "mulss", "divss", "ucomisd", "ucomiss", "movaps", "movapd",
    "xorps", "xorpd", "andps", "andpd", "sqrtsd", "sqrtss", "popcnt", "lzcnt", "tzcnt", "bswap",
    "nop", "endbr64",
}
FLAG_FULL = {"add", "sub", "and", "or", "xor", "cmp", "test", "neg"}
FLAG_NEUTRAL = {"mov", "movzx", "movsx", "movsxd", "lea", "not", "nop", "movss", "movsd", "movq",
                "movd", "movaps", "movapd", "bswap"}
JCC = {"jo", "jno", "jb", "jc", "jnae", "jae", "jnb", "jnc", "je", "jz", "jne", "jnz", "jbe", "jna",
       "ja", "jnbe", "js", "jns", "jp", "jpe", "jnp", "jpo", "jl", "jnge", "jge", "jnl", "jle", "jng",
       "jg", "jnle"}
QUIET_DIRECTIVES = (".loc", ".p2align", ".align", ".cfi_")
LIVE_AT_RET = {0, 2, 3, 4, 5, 12, 13, 14, 15}
STEP_BUDGET = 4096

DEAD, LIVE, NEXT, CUT = 0, 1, 2, 3


def label_map(lines):
    out = {}
    for i, line in enumerate(lines):
        s = line.strip()
        if s.endswith(":") and " " not in s and "\t" not in s:
            out.setdefault(s[:-1], i)
    return out


def regs_in(op):
    return [REG[t][0] for t in IDENT.findall(op) if t in REG]


def classify(s):
    if not s or s.startswith("#") or s.startswith(QUIET_DIRECTIVES):
        return "skip", None, None
    if s.endswith(":"):
        return "label", None, None
    if s.startswith("."):
        return "barrier", None, None
    mn, ops = split_line(s)
    return "insn", mn, ops


def control(mn, ops, labels, stack):
    if mn == "ret":
        return CUT
    if mn == "jmp":
        if len(ops) == 1 and ops[0] in labels:
            stack.append(labels[ops[0]])
            return CUT
        return LIVE
    if mn in JCC:
        if len(ops) == 1 and ops[0] in labels:
            stack.append(labels[ops[0]])
            return NEXT
        return LIVE
    return None


def reg_step(mn, ops, reg):
    if mn == "ret":
        return LIVE if reg in LIVE_AT_RET else DEAD
    if mn == "jmp" or mn in JCC:
        return NEXT
    if mn not in EXPLICIT or (mn == "imul" and len(ops) == 1):
        return LIVE
    if not ops:
        return NEXT
    rest = [r for o in ops[1:] for r in regs_in(o)]
    dst = ops[0]
    full = dst in REG and REG[dst][0] == reg and REG[dst][1] >= 32
    if mn == "xor" and len(ops) == 2 and dst == ops[1] and full:
        return DEAD
    if mn in PURE_WRITE and full and reg not in rest:
        return DEAD
    if reg in rest or reg in regs_in(dst):
        return LIVE
    return NEXT


def flags_step(mn, ops):
    if mn == "ret" or mn in FLAG_FULL:
        return DEAD
    if mn in JCC:
        return LIVE
    if mn in FLAG_NEUTRAL or mn == "jmp":
        return NEXT
    return LIVE


def dead_after(lines, start, labels, step):
    stack = [start]
    seen = set()
    budget = STEP_BUDGET
    while stack:
        i = stack.pop()
        if i in seen:
            continue
        seen.add(i)
        while True:
            if i >= len(lines):
                return False
            budget -= 1
            if budget < 0:
                return False
            kind, mn, ops = classify(lines[i].strip())
            i += 1
            if kind in ("skip", "label"):
                continue
            if kind == "barrier":
                return False
            r = step(mn, ops)
            if r == LIVE:
                return False
            if r == DEAD:
                break
            c = control(mn, ops, labels, stack)
            if c == LIVE:
                return False
            if c == CUT:
                break
    return True


def reg_dead_after(lines, start, reg, labels):
    return dead_after(lines, start, labels, lambda mn, ops: reg_step(mn, ops, reg))


def flags_dead_after(lines, start, labels):
    return dead_after(lines, start, labels, flags_step)
