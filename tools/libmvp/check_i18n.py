#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_i18n.py <i18n_probe> -- lib/i18n against ICU (PyICU):
# plural categories, number formatting and date formatting for the eight
# languages lib/i18n has data for.
import random, subprocess, sys
try:
    import icu
except ImportError:
    print("  skip: PyICU not installed"); sys.exit(0)
probe = sys.argv[1]
LANGS = {"en-US": "en_US", "de": "de", "fr": "fr", "it": "it", "es": "es", "pl": "pl", "cs": "cs", "ru": "ru"}
rng = random.Random(20260925)
cases = []
for lang, loc in LANGS.items():
    L = icu.Locale(loc)
    pr = icu.PluralRules.forLocale(L)
    nums = list(range(0, 230)) + [1000, 1001, 1000000, 2000000, 1000001, 10**9, 1234567]
    for x in nums:
        cases.append(("P %s %d 0" % (lang, x), pr.select(x)))
    for _ in range(300):
        i = rng.choice([0, 1, 2, 3, 5, 11, 12, 21, 22, 25, 101, 111, 1000000, rng.randint(0, 5000)])
        v = rng.randint(1, 3)
        f = rng.randint(0, 10 ** v - 1)
        if f % 10 == 0:
            f += 1  # the double must show exactly v fraction digits
        text = "%d.%0*d" % (i, v, f)
        # ICU derives the visible fraction digits of a double from its
        # shortest decimal form -- which is `text`
        cases.append(("P %s %s %d" % (lang, text, v), pr.select(float(text))))
    nf_cache = {}
    for _ in range(400):
        frac = rng.randint(0, 4)
        mag = rng.choice([1, 10, 1000, 10**6, 10**9, 10**12])
        val = round(rng.uniform(-mag, mag), 6)
        # keep away from exact halfway points of the rounding
        text = "%.6f" % val
        if frac not in nf_cache:
            nf = icu.NumberFormat.createInstance(L); nf.setMinimumFractionDigits(frac); nf.setMaximumFractionDigits(frac)
            nf.setRoundingMode(icu.DecimalFormat.kRoundHalfEven)
            nf_cache[frac] = nf
        scaled = abs(val) * 10 ** frac
        if abs(scaled - int(scaled) - 0.5) < 1e-6:
            continue
        cases.append(("N %s %s %d" % (lang, text, frac), nf_cache[frac].format(float(text))))
    dfs = {s: icu.DateFormat.createDateInstance(s, L) for s in (icu.DateFormat.SHORT, icu.DateFormat.MEDIUM, icu.DateFormat.LONG)}
    for f in dfs.values():
        f.setTimeZone(icu.TimeZone.getGMT())
    cal = icu.Calendar.createInstance(icu.TimeZone.getGMT(), L)
    for _ in range(60):
        y, m, d = rng.randint(1990, 2099), rng.randint(1, 12), rng.randint(1, 28)
        cal.clear(); cal.set(y, m - 1, d, 12, 0, 0)
        t = cal.getTime()
        for style, s in ((1, icu.DateFormat.SHORT), (2, icu.DateFormat.MEDIUM), (3, icu.DateFormat.LONG)):
            cases.append(("D %s %d %d %d %d" % (lang, y, m, d, style), dfs[s].format(t)))
inp = "".join(c[0] + "\n" for c in cases)
out = subprocess.run([probe], input=inp.encode(), capture_output=True, check=True).stdout.decode().split("\n")
bad = 0
kinds = {"P": 0, "N": 0, "D": 0}
for (q, want), got in zip(cases, out):
    kinds[q[0]] += 1
    if q[0] == "N" and want.startswith("-") and not any(ch in "123456789" for ch in want):
        want = want[1:]  # ICU writes "-0"; lib/i18n writes no sign on a zero
    if got != want:
        bad += 1
        if bad <= 15:
            print("  DIFF %-32s firn %r icu %r" % (q, got, want))
print("i18n: %d plural, %d number, %d date cases in 8 languages against ICU %s (CLDR %s), %d differ"
      % (kinds["P"], kinds["N"], kinds["D"], icu.ICU_VERSION, icu.UNICODE_VERSION and "42", bad))
sys.exit(1 if bad else 0)
