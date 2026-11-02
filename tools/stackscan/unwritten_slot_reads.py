#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys

FUNC_RE = re.compile(r'^([0-9a-f]+) <(.+)>:$')
INSN_RE = re.compile(r'^\s*([0-9a-f]+):\s+(\S+)\s*(.*)$')
SLOT_RE = re.compile(r'(-0x[0-9a-f]+|0x[0-9a-f]+)?\(%rbp\)')
TARGET_RE = re.compile(r'^([0-9a-f]+)\s+<')

REG_SIZES = {}
for r in ('ax', 'bx', 'cx', 'dx', 'si', 'di', 'bp', 'sp'):
    REG_SIZES['r' + r] = 8
    REG_SIZES['e' + r] = 4
    REG_SIZES[r] = 2
for r in ('al', 'bl', 'cl', 'dl', 'sil', 'dil', 'bpl', 'spl', 'ah', 'bh', 'ch', 'dh'):
    REG_SIZES[r] = 1
for n in range(8, 16):
    REG_SIZES['r%d' % n] = 8
    REG_SIZES['r%dd' % n] = 4
    REG_SIZES['r%dw' % n] = 2
    REG_SIZES['r%db' % n] = 1
for n in range(16):
    REG_SIZES['xmm%d' % n] = 16

SUFFIX_SIZES = {'b': 1, 'w': 2, 'l': 4, 'q': 8}
SUFFIXED_OPS = {'mov', 'add', 'sub', 'and', 'or', 'xor', 'inc', 'dec', 'not', 'neg', 'shl', 'shr', 'sar', 'adc', 'sbb'}
NO_WRITE_PREFIXES = ('cmp', 'test', 'bt', 'ucomis', 'comis', 'push', 'prefetch')
TERMINATORS = {'ret', 'retq', 'ud2', 'hlt'}


def split_operands(ops):
    out, depth, cur = [], 0, ''
    for ch in ops:
        if ch == '(':
            depth += 1
        elif ch == ')':
            depth -= 1
        if ch == ',' and depth == 0:
            out.append(cur.strip())
            cur = ''
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def slot_offset(op):
    m = SLOT_RE.fullmatch(op)
    if not m:
        return None
    return int(m.group(1), 16) if m.group(1) else 0


def write_size(mnem, operands):
    for op in operands[:-1]:
        if op.startswith('%'):
            sz = REG_SIZES.get(op[1:])
            if sz:
                return sz
    if mnem[:-1] in SUFFIXED_OPS and mnem[-1] in SUFFIX_SIZES:
        return SUFFIX_SIZES[mnem[-1]]
    if mnem.startswith('set'):
        return 1
    return 8


def parse(lines):
    funcs, cur = [], None
    for line in lines:
        line = line.rstrip('\n')
        m = FUNC_RE.match(line)
        if m:
            cur = {'name': m.group(2), 'insns': []}
            funcs.append(cur)
            continue
        if cur is None:
            continue
        m = INSN_RE.match(line)
        if not m:
            continue
        rest = m.group(3).split('#')[0].strip()
        cur['insns'].append((int(m.group(1), 16), m.group(2), rest))
    return funcs


def reachability(insns):
    index = {a: i for i, (a, _, _) in enumerate(insns)}
    succ = [[] for _ in insns]
    has_pred = [False] * len(insns)
    indirect = False
    for i, (_, mnem, ops) in enumerate(insns):
        nxt = i + 1 if i + 1 < len(insns) else None
        if mnem.startswith('j'):
            if ops.startswith('*'):
                indirect = True
                continue
            m = TARGET_RE.match(ops)
            t = index.get(int(m.group(1), 16)) if m else None
            if t is not None:
                succ[i].append(t)
                has_pred[t] = True
            if mnem not in ('jmp', 'jmpq') and nxt is not None:
                succ[i].append(nxt)
                has_pred[nxt] = True
        elif mnem in TERMINATORS:
            continue
        elif nxt is not None:
            succ[i].append(nxt)
            has_pred[nxt] = True
    roots = [0]
    if indirect:
        roots += [i for i in range(1, len(insns)) if not has_pred[i]]
    reach = [False] * len(insns)
    stack = list(roots)
    while stack:
        i = stack.pop()
        if reach[i]:
            continue
        reach[i] = True
        stack.extend(succ[i])
    return reach, indirect


def slot_accesses(insns):
    writes, leas, reads = [], [], []
    for i, (_, mnem, ops) in enumerate(insns):
        operands = split_operands(ops)
        if mnem.startswith('lea') and operands:
            off = slot_offset(operands[0])
            if off is not None:
                leas.append(off)
            continue
        if mnem == 'cmpb' and len(operands) == 2 and operands[0] == '$0x0':
            off = slot_offset(operands[1])
            if off is not None:
                reads.append((off, i))
            continue
        if not operands or mnem.startswith(NO_WRITE_PREFIXES):
            continue
        off = slot_offset(operands[-1])
        if off is not None:
            writes.append((off, write_size(mnem, operands), i))
    return writes, leas, reads


def escaped_ranges(writes, leas, reads):
    direct = {o for o, _, _ in writes} | {o for o, _ in reads}
    lea_offs = sorted(set(leas))
    out = []
    for o in lea_offs:
        above = [x for x in list(direct) + lea_offs if x > o]
        out.append((o, (min(above) - o) if above else 0x400))
    return out


def analyze(func):
    insns = func['insns']
    if not insns:
        return []
    reach, indirect = reachability(insns)
    writes, leas, reads = slot_accesses(insns)
    escaped = escaped_ranges(writes, leas, reads)

    def covered(off, live_only):
        if any(o <= off < o + sz for o, sz in escaped):
            return True
        return any((reach[i] or not live_only) and o <= off < o + sz for o, sz, i in writes)

    findings = []
    for off, i in reads:
        if not reach[i] or covered(off, True):
            continue
        kind = 'dead-writer' if covered(off, False) else 'no-writer'
        findings.append((kind, insns[i][0], off, indirect))
    return findings


def fmt_off(off):
    return hex(off) if off >= 0 else '-' + hex(-off)


def main():
    ap = argparse.ArgumentParser(description='Report reachable `cmpb $0x0,OFF(%rbp)` bool tests whose stack slot is never written on a reachable path.')
    ap.add_argument('binary')
    ap.add_argument('--func', help='only functions whose name contains this text')
    ap.add_argument('--objdump', default='objdump')
    args = ap.parse_args()
    cmd = [args.objdump, '-d', '--no-show-raw-insn', args.binary]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, text=True, bufsize=1 << 20)
    funcs = parse(proc.stdout)
    proc.wait()
    total = nfuncs = 0
    for func in funcs:
        if args.func and args.func not in func['name']:
            continue
        nfuncs += 1
        for kind, addr, off, indirect in analyze(func):
            total += 1
            mark = ' jump-table' if indirect else ''
            print('%s %x %s %s(%%rbp)%s' % (kind, addr, func['name'], fmt_off(off), mark))
    print('SUMMARY functions=%d findings=%d' % (nfuncs, total), file=sys.stderr)
    return 1 if total else 0


if __name__ == '__main__':
    sys.exit(main())
