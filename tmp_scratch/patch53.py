p = '/root/firn-wt-db/tools/db/check_dml.py'
s = open(p).read()
s = s.replace('''     "SELECT count(DISTINCT at) FROM ts",
     "SELECT date('now') = CURRENT_DATE, datetime('now', 'localtime') IS NOT NULL, julianday('now') > 2460000"]''', '''     "SELECT date('now') = CURRENT_DATE, datetime('now', 'localtime') IS NOT NULL, julianday('now') > 2460000",
     "UPDATE ts SET at = 'x', d = 'y', t = 'z', n = 0"]''')
open(p, 'w').write(s)
print('ok53')
