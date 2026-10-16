#!/usr/bin/env python3
# tools/cal_cross/gen.py -- random recurrence rules answered by dateutil.rrule.
#   gen.py <seed> <cases> <out file>
# Line formats (tab separated, times are "local seconds" since 1970-01-01T00:00:00):
#   R  dtstart  rule  from  to  expected      (expected = first <=40 occurrences inside [from, to])
#   S  dtstart  rule  rdates  exdates  expected   (rruleset: rule + RDATEs - EXDATEs)
import sys, random, signal, datetime as dt
from itertools import islice
from dateutil import rrule as rr

seed, ncases, outpath = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
rng = random.Random(seed)
EPOCH = dt.datetime(1970, 1, 1)
WD = [rr.MO, rr.TU, rr.WE, rr.TH, rr.FR, rr.SA, rr.SU]
WDN = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"]
FREQN = ["YEARLY", "MONTHLY", "WEEKLY", "DAILY", "HOURLY", "MINUTELY", "SECONDLY"]
MAXN = 40


def secs(d):
    return int((d - EPOCH).total_seconds())


def fmt_dt(d):
    return d.strftime("%Y%m%dT%H%M%S")


class Timeout(Exception):
    pass


def alarm(*_):
    raise Timeout()


signal.signal(signal.SIGALRM, alarm)


def pick(lo, hi, k, nonzero=True):
    vals = set()
    for _ in range(k):
        v = rng.randint(lo, hi)
        if nonzero and v == 0:
            continue
        vals.add(v)
    return sorted(vals)


def gen_rule():
    freq = rng.choice([0] * 4 + [1] * 5 + [2] * 4 + [3] * 4 + [4] * 2 + [5] + [6])
    start = dt.datetime(rng.randint(1990, 2040), rng.randint(1, 12), rng.randint(1, 28), rng.randint(0, 23), rng.randint(0, 59), rng.randint(0, 59))
    if rng.random() < 0.3:
        start = start.replace(hour=0, minute=0, second=0)
    interval = rng.choice([1, 1, 1, 1, 2, 2, 3, 4, 5, 7, 10, 13])
    kw = {}
    text = ["FREQ=" + FREQN[freq]]
    if interval != 1:
        text.append("INTERVAL=%d" % interval)
    r = rng.random()
    if r < 0.45:
        c = rng.randint(1, 30)
        kw["count"] = c
        text.append("COUNT=%d" % c)
    elif r < 0.7:
        span = {0: 3000, 1: 900, 2: 300, 3: 120, 4: 4, 5: 1, 6: 1}[freq]
        u = start + dt.timedelta(days=rng.randint(0, span), seconds=rng.randint(0, 86399))
        if rng.random() < 0.3:
            u = u.replace(hour=0, minute=0, second=0)
            text.append("UNTIL=" + u.strftime("%Y%m%d"))
        else:
            text.append("UNTIL=" + fmt_dt(u))
        kw["until"] = u
    parts = []
    bysetbase = False
    if rng.random() < 0.25:
        v = pick(1, 12, rng.randint(1, 3))
        kw["bymonth"] = v
        parts.append("BYMONTH=" + ",".join(map(str, v)))
        bysetbase = True
    if rng.random() < (0.12 if freq == 0 else 0.02):
        v = pick(-53, 53, rng.randint(1, 3))
        kw["byweekno"] = v
        parts.append("BYWEEKNO=" + ",".join(map(str, v)))
        bysetbase = True
    if rng.random() < (0.1 if freq == 0 else 0.03):
        v = pick(-366, 366, rng.randint(1, 4))
        kw["byyearday"] = v
        parts.append("BYYEARDAY=" + ",".join(map(str, v)))
        bysetbase = True
    if rng.random() < (0.3 if freq in (0, 1) else 0.12):
        v = pick(-31, 31, rng.randint(1, 4))
        kw["bymonthday"] = v
        parts.append("BYMONTHDAY=" + ",".join(map(str, v)))
        bysetbase = True
    if rng.random() < 0.4:
        items = []
        for _ in range(rng.randint(1, 3)):
            w = rng.randint(0, 6)
            n = 0
            if freq in (0, 1) and rng.random() < 0.5:
                n = rng.choice([1, 2, 3, 4, 5, -1, -2, -3]) if freq == 1 or rng.random() < 0.7 else rng.randint(-53, 53)
            if freq >= 2 and rng.random() < 0.05:
                n = rng.choice([1, -1, 2])
            if n == 0 and freq in (0, 1) and w in [x[0] for x in items]:
                continue
            items.append((w, n))
        if items:
            kw["byweekday"] = [WD[w](n) if n else WD[w] for w, n in items]
            parts.append("BYDAY=" + ",".join(("%d" % n if n else "") + WDN[w] for w, n in items))
            bysetbase = True
    if rng.random() < 0.25:
        v = pick(0, 23, rng.randint(1, 3), False)
        kw["byhour"] = v
        parts.append("BYHOUR=" + ",".join(map(str, v)))
        bysetbase = True
    if rng.random() < 0.2:
        v = pick(0, 59, rng.randint(1, 3), False)
        kw["byminute"] = v
        parts.append("BYMINUTE=" + ",".join(map(str, v)))
        bysetbase = True
    if rng.random() < 0.15:
        v = pick(0, 59, rng.randint(1, 3), False)
        kw["bysecond"] = v
        parts.append("BYSECOND=" + ",".join(map(str, v)))
        bysetbase = True
    if bysetbase and rng.random() < 0.25:
        v = pick(-5, 5, rng.randint(1, 3))
        if rng.random() < 0.1:
            v = pick(-366, 366, 2)
        if v:
            kw["bysetpos"] = v
            parts.append("BYSETPOS=" + ",".join(map(str, v)))
    if rng.random() < 0.3:
        w = rng.randint(0, 6)
        kw["wkst"] = WD[w]
        parts.append("WKST=" + WDN[w])
    parts = [x for x in parts if not x.endswith("=")]
    kw = {k: v for k, v in kw.items() if not (isinstance(v, list) and len(v) == 0)}
    rng.shuffle(parts)
    text += parts
    return freq, start, kw, ";".join(text), interval


def build(freq, start, kw, interval):
    return rr.rrule(freq, dtstart=start, interval=interval, **kw)


def run_limited(f, limit=1.5):
    signal.setitimer(signal.ITIMER_REAL, limit)
    try:
        return f()
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)


stats = {"R": 0, "S": 0, "skipped": 0, "empty": 0}
out = open(outpath, "w")
attempts = 0
while stats["R"] + stats["S"] < ncases and attempts < ncases * 6:
    attempts += 1
    freq, start, kw, text, interval = gen_rule()
    try:
        rule = build(freq, start, kw, interval)
        full = run_limited(lambda: list(islice(rule, 300)))
    except Exception:
        stats["skipped"] += 1
        continue
    secs_list = [secs(d) for d in full]
    if not secs_list:
        stats["empty"] += 1
    if rng.random() < 0.35 and len(full) >= 3:
        # rule + rdates - exdates
        rdates = [start + dt.timedelta(seconds=rng.randint(-86400 * 30, 86400 * 400)) for _ in range(rng.randint(0, 4))]
        exd = [rng.choice(full) for _ in range(rng.randint(0, 3))] + [start + dt.timedelta(days=rng.randint(0, 50)) for _ in range(rng.randint(0, 2))]
        rs = rr.rruleset()
        rs.rrule(rule)
        for d in rdates:
            rs.rdate(d)
        for d in exd:
            rs.exdate(d)
        try:
            exp = run_limited(lambda: [secs(d) for d in islice(rs, MAXN)])
        except Exception:
            stats["skipped"] += 1
            continue
        out.write("S\t%d\t%s\t%s\t%s\t%s\n" % (secs(start), text, ",".join(str(secs(d)) for d in rdates), ",".join(str(secs(d)) for d in exd), ",".join(map(str, exp))))
        stats["S"] += 1
        continue
    lo, hi = -400000000000000, 400000000000000
    exp = secs_list[:MAXN]
    if len(secs_list) >= 4 and rng.random() < 0.4:
        a = rng.randint(0, len(secs_list) - 2)
        b = rng.randint(a, len(secs_list) - 1)
        lo = secs_list[a] - rng.randint(0, 3)
        hi = secs_list[b] + rng.randint(0, 3)
        exp = [x for x in secs_list if lo <= x <= hi][:MAXN]
        if len(secs_list) == 300 and hi > secs_list[-1]:
            hi = secs_list[-1]
            exp = [x for x in secs_list if lo <= x <= hi][:MAXN]
    out.write("R\t%d\t%s\t%d\t%d\t%s\n" % (secs(start), text, lo, hi, ",".join(map(str, exp))))
    stats["R"] += 1
out.close()
print("gen: %d rule cases, %d set cases, %d empty results, %d skipped (python error/timeout)" % (stats["R"], stats["S"], stats["empty"], stats["skipped"]))
