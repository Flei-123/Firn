p = '/root/firn-wt-db/tools/db/check_expr.py'
s = open(p).read()
old = "# unique, in order\n"
new = '''# ---- date and time functions (no 'now': the answers must not depend on the clock)
bases = ["'2024-02-29'", "'2023-12-31 23:59:59'", "'2000-01-01 00:00:00.500'", "'1999-03-15T12:30'", "'12:34:56'", "'2024-06-15 08:00:00+02:00'",
         "'2024-06-15 08:00:00Z'", "'0000-01-01'", "'9999-12-31 23:59:59'", "'2024-13-01'", "'2024-02-30'", "'abc'", "NULL", "2460000.5", "2460000", "0",
         "'2024-01-31'", "'1970-01-01 00:00:00'", "1700000000", "'-0001-05-05'"]
mods = ["", ", '+1 day'", ", '-1 month'", ", '+1 year'", ", '+3 months'", ", '+36 hours'", ", '-90 minutes'", ", '+1.5 days'", ", 'start of month'",
        ", 'start of year'", ", 'start of day'", ", 'weekday 0'", ", 'weekday 3'", ", 'unixepoch'", ", '+00:30'", ", '-01:15:30'",
        ", 'start of month', '+1 month', '-1 day'", ", '+1 year', 'start of year'", ", 'bogus'", ", '+1 fortnight'", ", 'auto'", ", 'julianday'",
        ", '+10000000 days'", ", '+1 month', '+1 month', '+1 month'", ", '+12 months'", ", '-13 months'", ", '+0.5 month'"]
for b in bases:
    for m in mods:
        for f in ["date", "time", "datetime", "julianday", "unixepoch"]:
            exprs.append(f"{f}({b}{m})")
        exprs.append(f"strftime('%Y-%m-%d %H:%M:%S|%f|%j|%J|%s|%w|%W|%%|%d|%m|%M|%S|%H', {b}{m})")
exprs += ["strftime('%Q', '2024-01-01')", "strftime('', '2024-01-01')", "strftime(NULL, '2024-01-01')", "strftime('%Y')", "date('2024-01-01', NULL)",
          "datetime('2024-01-01', '+1 day', NULL)", "date(1e20)", "date(-5)", "date('1e3')", "datetime(2460000.5, 'unixepoch')",
          "datetime(1700000000, 'unixepoch', 'localtime') IS NOT NULL", "typeof(date('now'))", "typeof(CURRENT_TIMESTAMP)", "CURRENT_DATE = date('now')",
          "length(CURRENT_TIME)", "date('2024-03-10', 'weekday 6', '+7 days')", "julianday('2024-02-29 12:00')", "unixepoch('2024-02-29 12:00:00')"]
# unique, in order
'''
assert old in s
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print('ok44')
