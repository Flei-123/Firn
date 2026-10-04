p = '/root/firn-wt-db/tools/db/check_dml.py'
s = open(p).read()
old = "# a random workload\n"
new = '''# DEFAULT CURRENT_TIMESTAMP and friends
S = ["CREATE TABLE ts(id INTEGER PRIMARY KEY, at TEXT DEFAULT CURRENT_TIMESTAMP, d TEXT DEFAULT CURRENT_DATE, t TEXT DEFAULT CURRENT_TIME, n INTEGER DEFAULT (strftime('%s', 'now')))",
     "INSERT INTO ts DEFAULT VALUES",
     "INSERT INTO ts(id) VALUES (5), (6)",
     "SELECT id, typeof(at), length(at), at LIKE '____-__-__ __:__:__', length(d), d = date(at), length(t), t = time(at), typeof(n), abs(n - strftime('%s', 'now')) < 5 FROM ts ORDER BY id",
     "SELECT count(DISTINCT at) FROM ts",
     "SELECT date('now') = CURRENT_DATE, datetime('now', 'localtime') IS NOT NULL, julianday('now') > 2460000"]
scenario("current timestamp defaults", S, True)

# a random workload
'''
assert old in s
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print('ok52')
