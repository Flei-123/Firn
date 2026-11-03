from x86 import FLAG_WRITERS, parse_insn

SCALES = (1, 2, 4, 8)
NONFLAG = {"mov", "movzx", "movsx", "movsxd", "lea"}


def reg(v, w):
    return "%%%d%s" % (v, {64: "", 32: "d", 16: "w", 8: "b"}[w])


def fmt_disp(d):
    return "+%d" % d if d >= 0 else "-%d" % -d


def lea_forms(k, disps):
    for a in range(k):
        for d in disps:
            if d:
                yield "[%s%s]" % (reg(a, 64), fmt_disp(d))
        for s in SCALES[1:]:
            yield "[%s*%d]" % (reg(a, 64), s)
            for d in disps:
                if d:
                    yield "[%s*%d%s]" % (reg(a, 64), s, fmt_disp(d))
        for b in range(k):
            for s in SCALES:
                if s == 1 and b < a:
                    continue
                yield "[%s+%s%s]" % (reg(a, 64), reg(b, 64), "" if s == 1 else "*%d" % s)
                if s == 1:
                    for d in disps:
                        if d:
                            yield "[%s+%s%s]" % (reg(a, 64), reg(b, 64), fmt_disp(d))


def texts(k, imms, allow_flags):
    small = sorted({i for i in imms if -(1 << 31) <= i < (1 << 31)} | {0, 1, -1})
    disps = [i for i in small if i]
    for w in (32, 64):
        for d in range(k):
            for s in range(k):
                if s != d:
                    yield "mov %s, %s" % (reg(d, w), reg(s, w))
            for i in small:
                yield "mov %s, %d" % (reg(d, w), i)
            for f in lea_forms(k, disps):
                yield "lea %s, %s" % (reg(d, w), f)
    for d in range(k):
        for s in range(k):
            yield "movzx %s, %s" % (reg(d, 32), reg(s, 8))
            yield "movzx %s, %s" % (reg(d, 32), reg(s, 16))
            yield "movsxd %s, %s" % (reg(d, 64), reg(s, 32))
            for w in (32, 64):
                yield "movsx %s, %s" % (reg(d, w), reg(s, 8))
                yield "movsx %s, %s" % (reg(d, w), reg(s, 16))
    if not allow_flags:
        return
    for w in (32, 64):
        for d in range(k):
            for op in ("add", "sub", "and", "or", "xor"):
                for s in range(k):
                    yield "%s %s, %s" % (op, reg(d, w), reg(s, w))
                for i in small:
                    if i or op in ("and",):
                        yield "%s %s, %d" % (op, reg(d, w), i)
            for op in ("shl", "shr", "sar", "rol", "ror"):
                for c in range(1, w):
                    yield "%s %s, %d" % (op, reg(d, w), c)
            for op in ("neg", "not", "inc", "dec"):
                yield "%s %s" % (op, reg(d, w))
            for s in range(k):
                yield "imul %s, %s" % (reg(d, w), reg(s, w))
                for i in small:
                    if i not in (0, 1):
                        yield "imul %s, %s, %d" % (reg(d, w), reg(s, w), i)


def build(k, imms, allow_flags):
    out = []
    seen = set()
    for t in texts(k, imms, allow_flags):
        if t in seen:
            continue
        seen.add(t)
        out.append((t, parse_insn(t)))
    return out


def weight(insn):
    mn, ops = insn
    if mn == "imul":
        return 3
    if mn == "lea" and len(ops[1][1]) >= 3:
        return 2
    return 1


def writes_flags(insn):
    return insn[0] in FLAG_WRITERS
