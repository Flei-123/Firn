p = '/root/firn-wt-db/tools/db/bench.fi'
s = open(p).read()
old = '''    report("insert_100k_one_txn", t0)
'''
new = '''    report("insert_100k_one_txn", t0)
    if rt.arg_count(start) > 3 {
        // a third argument: only the first test (for the profiler)
        sq.db_close(d)
        return 0
    }
'''
assert old in s
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print('ok28')
