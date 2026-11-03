import json
import random
import sys
import time

import numpy as np

import candidates
import sem
from x86 import nvars, parse_insn, writes_flags

np.seterr(all="ignore")
NB = sem.NumpyBackend()
SPECIAL = [0, 1, 2, 0x7F, 0x80, 0xFF, 0x7FFF, 0x8000, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF,
           0x100000000, 0x7FFFFFFFFFFFFFFF, 0x8000000000000000, sem.M64, sem.M64 - 1]


def inputs(k, n, rng):
    cols = []
    for v in range(k):
        vals = [rng.choice(SPECIAL) if rng.random() < 0.3 else rng.getrandbits(rng.choice((8, 16, 32, 64)))
                for _ in range(n)]
        cols.append(np.array(vals, dtype=np.uint64))
    return cols


def imm_pool(insns):
    out = set()
    for _, ops in insns:
        for o in ops:
            if o[0] == "i":
                out.add(o[1])
            elif o[0] == "m":
                out.update(t[1] for t in o[1] if t[0] == "d")
    return out


def mismatch(st, target, live):
    return [v for v in live if not np.array_equal(st[v], target[v])]


def prove(orig, cand, k, live):
    import z3
    zb = sem.Z3Backend()
    xs = [z3.BitVec("x%d" % i, 64) for i in range(k)]
    a = sem.run(zb, xs, orig)
    b = sem.run(zb, xs, cand)
    s = z3.Solver()
    s.set("timeout", 20000)
    s.add(z3.Or([a[v] != b[v] for v in live]))
    r = s.check()
    return r == z3.unsat


def search_row(row, budget_s):
    texts = row["pattern"]
    orig = [parse_insn(t) for t in texts]
    k = nvars(texts)
    dead = {v for v in range(k) if row["dead"] >> v & 1}
    live = [v for v in range(k) if v not in dead]
    flags_ok = row["flags_dead"]
    if writes_flags(orig) and not flags_ok:
        return None
    rng = random.Random(len(texts) * 7919 + k)
    init = inputs(k, 64, rng)
    target = sem.run(NB, init, orig)
    pool = candidates.build(k, imm_pool(orig), flags_ok)
    maxlen = len(orig) - 1
    t0 = time.time()
    found = []
    if not mismatch(list(init), target, live):
        found.append(([], []))
    level1 = []
    seen = set()
    for t, ins in ([] if found or maxlen < 1 else pool):
        try:
            st = sem.run(NB, init, [ins])
        except ValueError:
            continue
        mm = mismatch(st, target, live)
        if not mm:
            found.append(([t], [ins]))
            continue
        if len(mm) == 1 and maxlen >= 2:
            fp = tuple(st[v].tobytes() for v in range(k))
            if fp not in seen:
                seen.add(fp)
                level1.append((t, ins, st, mm[0]))
    if not found and maxlen >= 2 and level1:
        by_dest = {}
        for t, ins in pool:
            ops = ins[1]
            if ops and ops[0][0] == "r":
                by_dest.setdefault(ops[0][1], []).append((t, ins))
        for t1, i1, st1, v in level1:
            if time.time() - t0 > budget_s:
                break
            for t2, i2 in by_dest.get(v, ()):
                try:
                    st2 = sem.run(NB, st1, [i2])
                except ValueError:
                    continue
                if np.array_equal(st2[v], target[v]):
                    found.append(([t1, t2], [i1, i2]))
    found.sort(key=lambda c: (len(c[1]), sum(candidates.weight(i) for i in c[1]), c[0]))
    for ctexts, cins in found[:40]:
        if not prove(orig, cins, k, live):
            continue
        w_orig = sem.written(orig)
        w_cand = sem.written(cins)
        need_dead = sorted(dead & (w_orig | w_cand))
        need_flags = writes_flags(orig) or any(candidates.writes_flags(i) for i in cins)
        return {"pattern": texts, "replacement": ctexts, "dead": sum(1 << v for v in need_dead),
                "flags_dead": need_flags, "count": row["count"], "gain": len(orig) - len(cins),
                "proof": "z3"}
    return None


def main():
    src, out = sys.argv[1], sys.argv[2]
    limit = int(sys.argv[3]) if len(sys.argv) > 3 else 200
    min_count = int(sys.argv[4]) if len(sys.argv) > 4 else 3
    budget = float(sys.argv[5]) if len(sys.argv) > 5 else 15
    rows = [r for r in json.load(open(src)) if r["count"] >= min_count]
    rows.sort(key=lambda r: -r["count"] * len(r["pattern"]))
    rules = []
    tried = 0
    for row in rows[:limit]:
        tried += 1
        try:
            r = search_row(row, budget)
        except ValueError:
            continue
        if r:
            rules.append(r)
            print("RULE", r["count"], " | ".join(r["pattern"]), "=>", " | ".join(r["replacement"]),
                  "dead=%d flags=%s" % (r["dead"], r["flags_dead"]), flush=True)
    json.dump(rules, open(out, "w"), indent=1)
    print("tried", tried, "rules", len(rules))


if __name__ == "__main__":
    main()
