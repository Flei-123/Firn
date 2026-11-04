import re

GPR = [
    ("rax", "eax", "ax", "al"),
    ("rcx", "ecx", "cx", "cl"),
    ("rdx", "edx", "dx", "dl"),
    ("rbx", "ebx", "bx", "bl"),
    ("rsp", "esp", "sp", "spl"),
    ("rbp", "ebp", "bp", "bpl"),
    ("rsi", "esi", "si", "sil"),
    ("rdi", "edi", "di", "dil"),
    ("r8", "r8d", "r8w", "r8b"),
    ("r9", "r9d", "r9w", "r9b"),
    ("r10", "r10d", "r10w", "r10b"),
    ("r11", "r11d", "r11w", "r11b"),
    ("r12", "r12d", "r12w", "r12b"),
    ("r13", "r13d", "r13w", "r13b"),
    ("r14", "r14d", "r14w", "r14b"),
    ("r15", "r15d", "r15w", "r15b"),
]
WIDTHS = (64, 32, 16, 8)
SUFFIX = {64: "", 32: "d", 16: "w", 8: "b"}
REG = {}
for _i, _names in enumerate(GPR):
    for _w, _n in zip(WIDTHS, _names):
        REG[_n] = (_i, _w)
FORBIDDEN = {4, 5}

ALU2 = {"add", "sub", "and", "or", "xor"}
SHIFTS = {"shl", "shr", "sar", "rol", "ror"}
UNARY = {"neg", "not", "inc", "dec"}
MOVES = {"mov", "movzx", "movsx", "movsxd", "lea"}
WINDOW_OPS = ALU2 | SHIFTS | UNARY | MOVES | {"imul"}
FLAG_WRITERS = ALU2 | SHIFTS | {"neg", "inc", "dec", "imul"}

IDENT = re.compile(r"[A-Za-z_.$][A-Za-z0-9_.$]*")
VARTOK = re.compile(r"%(\d+)([dwb]?)")
NUM = re.compile(r"^-?(0x[0-9a-fA-F]+|0|[1-9][0-9]*)$")


def split_line(line):
    s = line.strip()
    if not s or s.startswith("#"):
        return None
    if " #" in s:
        s = s[: s.index(" #")].strip()
    parts = s.split(None, 1)
    mn = parts[0]
    ops = [o.strip() for o in parts[1].split(",")] if len(parts) > 1 else []
    return mn, ops


def normalize(lines):
    names = {}
    out = []
    for line in lines:
        sp = split_line(line)
        if sp is None:
            return None
        mn, ops = sp
        if mn not in WINDOW_OPS or "ptr" in line:
            return None
        if mn != "lea" and any("[" in o for o in ops):
            return None
        if mn == "lea" and (len(ops) != 2 or not ops[1].startswith("[")):
            return None
        if mn == "imul" and len(ops) not in (2, 3):
            return None
        nops = []
        for o in ops:
            if not o.startswith("[") and NUM.match(o):
                nops.append(str(int(o, 0)))
                continue
            bad = []

            def rep(m):
                t = m.group(0)
                if t not in REG:
                    bad.append(t)
                    return t
                idx, w = REG[t]
                if idx in FORBIDDEN:
                    bad.append(t)
                    return t
                if idx not in names:
                    names[idx] = len(names)
                return "%%%d%s" % (names[idx], SUFFIX[w])

            no = IDENT.sub(rep, o)
            if bad:
                return None
            nops.append(no)
        out.append(mn + (" " + ", ".join(nops) if nops else ""))
    regs = [None] * len(names)
    for idx, v in names.items():
        regs[v] = idx
    return out, regs


def parse_operand(o):
    m = VARTOK.fullmatch(o)
    if m:
        w = {"": 64, "d": 32, "w": 16, "b": 8}[m.group(2)]
        return ("r", int(m.group(1)), w)
    if NUM.match(o):
        return ("i", int(o, 0))
    if o.startswith("[") and o.endswith("]"):
        return ("m", parse_addr(o[1:-1]))
    raise ValueError("bad operand " + o)


def parse_addr(s):
    terms = []
    for sign, t in re.findall(r"([+-]?)([^+-]+)", s):
        t = t.strip()
        if NUM.match(t):
            v = int(t, 0)
            terms.append(("d", -v if sign == "-" else v))
            continue
        if sign == "-":
            raise ValueError("negative register term")
        if "*" in t:
            a, b = t.split("*")
            if VARTOK.fullmatch(b):
                a, b = b, a
            r = parse_operand(a)
            terms.append(("s", r[1], int(b), r[2]))
        else:
            r = parse_operand(t)
            terms.append(("s", r[1], 1, r[2]))
    return terms


def parse_insn(text):
    mn, ops = split_line(text)
    return mn, [parse_operand(o) for o in ops]


def writes_flags(insns):
    return any(mn in FLAG_WRITERS for mn, _ in insns)


def nvars(texts):
    n = 0
    for t in texts:
        for m in VARTOK.finditer(t):
            n = max(n, int(m.group(1)) + 1)
    return n
