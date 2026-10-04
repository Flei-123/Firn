#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/uiextras/gen_human.py -- writes the locale data block of lib/i18n/human.fi
# from ICU (PyICU; CLDR as that ICU ships it). The block sits between the lines
#     // BEGIN GENERATED (tools/uiextras/gen_human.py)
#     // END GENERATED
# in human.fi; run `gen_human.py --apply lib/i18n/human.fi` to replace it, or
# without arguments to print it. The data is plain strings ("|"-separated
# fields), no code, so that a reviewer can diff it against ICU.
import re
import sys

import icu
from icu import (DateFormat, DateFormatSymbols, ListFormatter, Locale, MeasureUnit,
                 NumberFormatter, NoUnit, PluralRules, Precision, RelativeDateTimeFormatter,
                 UNumberUnitWidth as UW, URelativeDateTimeUnit as RU)

LANGS = ["en", "de", "fr", "it", "es", "pl", "cs", "ru"]
CATS = ["zero", "one", "two", "few", "many", "other"]
UNITS = [("second", RU.SECOND), ("minute", RU.MINUTE), ("hour", RU.HOUR), ("day", RU.DAY),
         ("week", RU.WEEK), ("month", RU.MONTH), ("year", RU.YEAR)]
BYTE_UNITS = [MeasureUnit.createByte(), MeasureUnit.createKilobyte(), MeasureUnit.createMegabyte(),
              MeasureUnit.createGigabyte(), MeasureUnit.createTerabyte(), MeasureUnit.createPetabyte()]


def lit(s):
    """A Firn string literal for s (no-break spaces escaped so they are visible)."""
    out = []
    for ch in s:
        o = ord(ch)
        if ch == '"' or ch == "\\":
            out.append("\\" + ch)
        elif o in (0xA0, 0x202F, 0x2009, 0x200E, 0x200F, 0x061C):
            out.append("\\u{%X}" % o)
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def sample_for(rules, cat, limit=1000, fractions=False):
    """A sample number with plural category `cat` (never 0 unless the category is
    'zero'); an integer if there is one, else (with `fractions`) a decimal."""
    for n in range(0 if cat == "zero" else 1, limit):
        if rules.select(n) == cat:
            return n
    if fractions:
        for x in (1.5, 0.5, 2.5, 5.5, 10.5, 21.5, 0.1, 1.1, 12.3):
            if rules.select(x) == cat:
                return x
    return None


def rel_block(lang):
    f = RelativeDateTimeFormatter(Locale(lang))
    rules = PluralRules.forLocale(Locale(lang))
    fields = []
    for _name, u in UNITS:
        for sign in (-1, 1):
            for cat in CATS:
                n = sample_for(rules, cat)
                if n is None:
                    fields.append("")
                    continue
                s = f.formatNumeric(sign * n, u)
                digits = str(n)
                if s.count(digits) != 1:
                    # a number that also occurs in the text: take another sample
                    cands = [m for m in range(1, 1000) if rules.select(m) == cat and f.formatNumeric(sign * m, u).count(str(m)) == 1]
                    if not cands:
                        fields.append("")
                        continue
                    n = cands[0]
                    s = f.formatNumeric(sign * n, u)
                    digits = str(n)
                fields.append(s.replace(digits, "{0}"))
    now = f.format(0, RU.SECOND)
    return "|".join(fields), now


def lang_decimal(lang, x):
    """x (a decimal sample like 1.5) as the language writes it."""
    t = "%g" % x
    sep = str(NumberFormatter.withLocale(Locale(lang)).precision(Precision.fixedFraction(1)).formatDouble(1.5))
    return sep.replace("1", "%s" % t.split(".")[0]).replace("5", t.split(".")[1]) if "." in t else t


def byte_block(lang):
    rules = PluralRules.forLocale(Locale(lang))
    fields = []
    for u in BYTE_UNITS:
        nf = NumberFormatter.withLocale(Locale(lang)).unit(u).unitWidth(UW.SHORT).precision(Precision.maxFraction(1))
        for cat in CATS:
            n = sample_for(rules, cat, 200, fractions=True)
            if n is None:
                fields.append("")
                continue
            s = str(nf.formatDouble(float(n)))
            num = ("%d" % n) if isinstance(n, int) else lang_decimal(lang, n)
            fields.append(s.replace(num, "{0}", 1) if s.count(num) == 1 else "")
    return "|".join(fields)


def pct_block(lang):
    nf = NumberFormatter.withLocale(Locale(lang)).unit(NoUnit.percent()).precision(Precision.maxFraction(0))
    s = str(nf.formatDouble(57.0))
    return s.replace("57", "{0}")


def list_block(lang):
    """(and-joiners, or-joiners). "and" comes from ICU; PyICU 2.10 cannot ask ICU for
    an "or" list, so that one comes from Babel (CLDR as Babel ships it)."""
    from babel.lists import format_list as babel_list

    def joiners(two, three, four):
        m2 = re.fullmatch(r"A(.*)B", two)
        m4 = re.fullmatch(r"A(.*)B(.*)C(.*)D", four)
        m3 = re.fullmatch(r"A(.*)B(.*)C", three)
        return "|".join([m2.group(1), m4.group(1), m4.group(2), m4.group(3), m3.group(2)])

    lf = ListFormatter.createInstance(Locale(lang))
    and_j = joiners(lf.format(["A", "B"]), lf.format(["A", "B", "C"]), lf.format(["A", "B", "C", "D"]))
    or_j = joiners(babel_list(["A", "B"], style="or", locale=lang), babel_list(["A", "B", "C"], style="or", locale=lang),
                   babel_list(["A", "B", "C", "D"], style="or", locale=lang))
    return [and_j, or_j]


def date_block(lang):
    loc = Locale(lang)
    row = []
    for d in [DateFormat.FULL, DateFormat.LONG, DateFormat.MEDIUM, DateFormat.SHORT]:
        row.append(DateFormat.createDateInstance(d, loc).toPattern())
    for t in [DateFormat.MEDIUM, DateFormat.SHORT]:
        row.append(DateFormat.createTimeInstance(t, loc).toPattern())
    for d in [DateFormat.FULL, DateFormat.LONG, DateFormat.MEDIUM, DateFormat.SHORT]:
        for t in [DateFormat.MEDIUM, DateFormat.SHORT]:
            row.append(DateFormat.createDateTimeInstance(d, t, loc).toPattern())
    sym = DateFormatSymbols(loc)
    wd = sym.getWeekdays()[1:8]  # Sunday .. Saturday
    wa = sym.getShortWeekdays()[1:8]
    ap = sym.getAmPmStrings()
    return "|".join(row), "|".join(wd), "|".join(wa), "|".join(ap)


def block():
    lines = []
    lines.append("// BEGIN GENERATED (tools/uiextras/gen_human.py, ICU %s)" % icu.ICU_VERSION)
    # one function per kind, one branch per language
    def func(name, doc, mapping):
        lines.append("")
        lines.append("// " + doc)
        lines.append("fn %s(l: u32) -> str {" % name)
        for i, lang in enumerate(LANGS[1:], start=1):
            lines.append("    if l == %d { return %s }" % (i, lit(mapping[lang])))
        lines.append("    return %s" % lit(mapping["en"]))
        lines.append("}")

    rel = {}
    now = {}
    for lang in LANGS:
        rel[lang], now[lang] = rel_block(lang)
    func("data_rel", "relative time: 7 units (second..year) x past/future x 6 plural categories (zero one two few many other), `{0}` = the count", rel)
    func("data_now", "the word for \"now\"", now)
    func("data_bytes", "byte units: B kB MB GB TB PB x 6 plural categories, `{0}` = the number (ICU short width)", {l: byte_block(l) for l in LANGS})
    func("data_percent", "percent: `{0}` = the number", {l: pct_block(l) for l in LANGS})
    lists = {l: list_block(l) for l in LANGS}
    func("data_list_and", "\"and\" list: pair | start | middle | end(of 3+) | end(of 3)  -- the text BETWEEN items", {l: lists[l][0] for l in LANGS})
    func("data_list_or", "\"or\" list, the same (from Babel: PyICU cannot ask ICU for it)", {l: lists[l][1] for l in LANGS})
    dates = {l: date_block(l) for l in LANGS}
    func("data_patterns", "date/time patterns: date FULL LONG MEDIUM SHORT | time MEDIUM SHORT | date x time (FULL, LONG, MEDIUM, SHORT) x (MEDIUM, SHORT)", {l: dates[l][0] for l in LANGS})
    func("data_weekdays", "weekday names, Sunday first", {l: dates[l][1] for l in LANGS})
    func("data_weekdays_abbr", "abbreviated weekday names, Sunday first", {l: dates[l][2] for l in LANGS})
    func("data_ampm", "AM|PM", {l: dates[l][3] for l in LANGS})
    lines.append("// END GENERATED")
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    b = block()
    if len(sys.argv) >= 3 and sys.argv[1] == "--apply":
        path = sys.argv[2]
        s = open(path, encoding="utf-8").read()
        a = s.index("// BEGIN GENERATED")
        e = s.index("// END GENERATED") + len("// END GENERATED\n")
        open(path, "w", encoding="utf-8").write(s[:a] + b + s[e:])
    else:
        sys.stdout.write(b)
