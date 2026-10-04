#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/uiextras/check_human.py -- holds lib/i18n/human.fi against ICU (PyICU) and
# Python's zoneinfo, for all eight languages:
#   rel    the text of every time distance (all units, past and future, every
#          plural category) = ICU RelativeDateTimeFormatter.formatNumeric of the
#          unit/count the documented rule picks
#   bytes  sizes up to 2^63 = ICU NumberFormatter(unit byte..petabyte, short, max 1
#          fraction digit), the value passed to ICU as an exact decimal
#   dt     date styles x time styles (every combination, 2,000 random instants,
#          years 1..9999) = ICU DateFormat.createDateTimeInstance in GMT
#   pct    percent with 0..2 fraction digits = ICU percent
#   list   2..6 items, "and" and "or" = ICU ListFormatter
#   zone   instants in eight zones read from the system's TZif files = the offsets
#          of zoneinfo, the text formatted by ICU
# usage: check_human.py <humancli binary> [seed] [count]
import datetime
import random
import subprocess
import sys
from decimal import ROUND_HALF_EVEN, Decimal

import icu
from icu import (DateFormat, ListFormatter, Locale, MeasureUnit, NoUnit, NumberFormatter,
                 Precision, RelativeDateTimeFormatter, TimeZone, UNumberUnitWidth as UW,
                 URelativeDateTimeUnit as RU)

cli = sys.argv[1]
seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
count = int(sys.argv[3]) if len(sys.argv) > 3 else 400
rnd = random.Random(seed)
LANGS = ["en", "de", "fr", "it", "es", "pl", "cs", "ru"]
GMT = TimeZone.getGMT()

cmds = []
expect = []


def add(cmd, want):
    cmds.append(cmd)
    expect.append(want)


# ---------------------------------------------------------------- relative
RUNIT = [RU.SECOND, RU.MINUTE, RU.HOUR, RU.DAY, RU.WEEK, RU.MONTH, RU.YEAR]


def bucket(a_ms):
    secs = a_ms // 1000
    if secs < 60:
        return 0, secs
    mins = secs // 60
    if mins < 60:
        return 1, mins
    hours = mins // 60
    if hours < 24:
        return 2, hours
    days = hours // 24
    if days < 7:
        return 3, days
    if days < 30:
        return 4, days // 7
    if days < 360:
        return 5, days // 30
    return 6, max(1, days // 365)


fmts = {l: RelativeDateTimeFormatter(Locale(l)) for l in LANGS}
for lang in LANGS:
    f = fmts[lang]
    for i in range(count):
        mag = rnd.choice([3, 4, 5, 6, 7, 8, 9, 10, 11])
        a = rnd.randrange(0, 10 ** mag) if i % 3 else rnd.choice([0, 4999, 5000, 59999, 60000, 3599999, 3600000, 86399999,
                                                                      86400000, 604799999, 604800000, 2591999999, 2592000000,
                                                                      31103999999, 31104000000, 31535999999, 31536000000])
        d = a if rnd.random() < 0.5 else -a
        if abs(d) < 5000:
            want = f.format(0, RU.SECOND)
        else:
            u, n = bucket(abs(d))
            want = f.formatNumeric(n if d > 0 else -n, RUNIT[u])
        add("rel %s %d" % (lang, d), want)

# ------------------------------------------------------------------- bytes
BU = [MeasureUnit.createByte(), MeasureUnit.createKilobyte(), MeasureUnit.createMegabyte(),
      MeasureUnit.createGigabyte(), MeasureUnit.createTerabyte(), MeasureUnit.createPetabyte()]
bfmt = {}
for lang in LANGS:
    bfmt[lang] = [NumberFormatter.withLocale(Locale(lang)).unit(u).unitWidth(UW.SHORT).precision(Precision.maxFraction(1)) for u in BU]


def bytes_expect(lang, n, binary=False):
    if binary:
        return None
    k = 0
    while k < 5 and n >= 1000 ** (k + 1):
        k += 1
    if k > 0:
        v = (Decimal(n) / Decimal(1000 ** k)).quantize(Decimal("0.1"), rounding=ROUND_HALF_EVEN)
        if v >= 1000 and k < 5:
            k += 1
            v = (Decimal(n) / Decimal(1000 ** k)).quantize(Decimal("0.1"), rounding=ROUND_HALF_EVEN)
        exact = format(Decimal(n) / Decimal(1000 ** k), "f")
    else:
        exact = str(n)
    return str(bfmt[lang][k].formatDecimal(exact.encode()))


for lang in LANGS:
    for i in range(count):
        if i % 5 == 0:
            n = rnd.choice([0, 1, 2, 999, 1000, 1049, 1050, 1051, 1950, 2250, 2350, 999949, 999950, 999999, 1000000,
                            1234567, 999949999, 999950000, 10 ** 12 - 1, 10 ** 15, 10 ** 15 * 7 + 123, 2 ** 63 - 1, 2 ** 64 - 1])
        else:
            n = rnd.randrange(0, 10 ** rnd.randint(1, 19))
        add("bytes %s %d 0" % (lang, n), bytes_expect(lang, n))

# ---------------------------------------------------------- date and time
DSTYLE = {4: DateFormat.FULL, 3: DateFormat.LONG, 2: DateFormat.MEDIUM, 1: DateFormat.SHORT}
TSTYLE = {2: DateFormat.MEDIUM, 1: DateFormat.SHORT}


def icu_dt(lang, ds, ts, ms):
    loc = Locale(lang)
    if ds == 0:
        return None
    if ds == 9 and ts == 9:
        return ""
    if ts == 9:
        df = DateFormat.createDateInstance(DSTYLE[ds], loc)
    elif ds == 9:
        df = DateFormat.createTimeInstance(TSTYLE[ts], loc)
    else:
        df = DateFormat.createDateTimeInstance(DSTYLE[ds], TSTYLE[ts], loc)
    df.setTimeZone(GMT)
    # ICU's calendar switches to the Julian one before 1582-10-15; std.time is
    # proleptic Gregorian, so ask ICU for that
    cal = icu.GregorianCalendar(GMT, loc)
    cal.setGregorianChange(-1e18)
    df.setCalendar(cal)
    return df.format(ms / 1000.0)


def rand_ms():
    if rnd.random() < 0.7:
        y = rnd.randint(1900, 2100)
    else:
        y = rnd.randint(1, 9999)
    d = datetime.datetime(y, 1, 1) + datetime.timedelta(days=rnd.randrange(365), seconds=rnd.randrange(86400))
    epoch = datetime.datetime(1970, 1, 1)
    return int((d - epoch).total_seconds()) * 1000


for lang in LANGS:
    for ds in (9, 4, 3, 2, 1):
        for ts in (9, 2, 1):
            for i in range(max(8, count // 20)):
                ms = rand_ms()
                want = icu_dt(lang, ds, ts, ms)
                add("dt %s %d %d %d 0" % (lang, ds, ts, ms), want)

# ------------------------------------------------------------------ percent
for lang in LANGS:
    for fr in (0, 1, 2):
        nf = NumberFormatter.withLocale(Locale(lang)).unit(NoUnit.percent()).precision(Precision.fixedFraction(fr))
        for i in range(count // 8):
            r = rnd.randrange(0, 3000000)  # 0 .. 3.0 in millionths
            exact = format(Decimal(r) / Decimal(10000), "f")  # r / 1e6 * 100
            add("pct %s %d %d" % (lang, r, fr), str(nf.formatDecimal(exact.encode())))

# -------------------------------------------------------------------- lists
WORDS = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Äpfel", "Öl", "Яблоко"]
for lang in LANGS:
    for kind in (0, 1):
        if kind == 0:
            lf = ListFormatter.createInstance(Locale(lang))
            fmt = lf.format
        else:
            # PyICU 2.10 cannot ask ICU for an "or" list; Babel (CLDR 47) is the witness
            from babel.lists import format_list as babel_list
            fmt = lambda items, lang=lang: babel_list(items, style="or", locale=lang)
        for n in range(1, 7):
            for _ in range(3):
                items = rnd.sample(WORDS, n)
                add("list %s %d %s" % (lang, kind, "|".join(items)), fmt(items))

# --------------------------------------------------------------------- zones
ZONES = ["Europe/Vienna", "America/New_York", "Asia/Kolkata", "Asia/Kathmandu", "Australia/Lord_Howe",
         "Pacific/Auckland", "America/Sao_Paulo", "UTC"]
try:
    import zoneinfo
    have_zi = True
except ImportError:
    have_zi = False
if have_zi:
    for z in ZONES:
        try:
            zi = zoneinfo.ZoneInfo(z)
        except Exception:
            continue
        for i in range(max(20, count // 10)):
            ms = rand_ms()
            if rnd.random() < 0.8:
                # keep to 1950..2037, where the zone files carry the transitions
                y = rnd.randint(1950, 2037)
                d = datetime.datetime(y, 1, 1, tzinfo=datetime.timezone.utc) + datetime.timedelta(
                    days=rnd.randrange(365), seconds=rnd.randrange(86400))
                ms = int(d.timestamp()) * 1000
            lang = rnd.choice(LANGS)
            ds = rnd.choice([4, 3, 2, 1])
            ts = rnd.choice([2, 1])
            try:
                off = int(datetime.datetime.fromtimestamp(ms / 1000, zi).utcoffset().total_seconds()) // 60
            except Exception:
                continue
            want = icu_dt(lang, ds, ts, ms + off * 60000)
            add("zone %s /usr/share/zoneinfo/%s %d %d %d" % (lang, z, ms, ds, ts), want)

# -------------------------------------------------------------------- run
inp = "".join(c + "\n" for c in cmds)
out = subprocess.run([cli], input=inp.encode(), capture_output=True, check=True).stdout.decode().split("\n")
assert out[-1] == ""
out = out[:-1]
assert len(out) == len(cmds), (len(out), len(cmds))
bad = 0
n_checked = 0
kinds = {}
for c, got, want in zip(cmds, out, expect):
    if want is None:
        continue
    n_checked += 1
    k = c.split()[0]
    kinds.setdefault(k, [0, 0])
    kinds[k][0] += 1
    if got == want:
        kinds[k][1] += 1
    else:
        bad += 1
        if bad <= 25:
            print("DIFF %-60s ours %r  icu %r" % (c[:60], got, want))
for k, (n, ok) in kinds.items():
    print("%-6s %6d cases %6d identical" % (k, n, ok))
print("human: %d cases, %d differences (ICU %s)" % (n_checked, bad, icu.ICU_VERSION))
sys.exit(1 if bad else 0)
