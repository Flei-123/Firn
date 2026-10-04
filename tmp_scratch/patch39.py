p = '/root/firn-wt-db/compiler/src/syscalls.rs'
s = open(p).read()
old = "    (74, A64::Direct(82)), // fsync (Firn r64)\n"
assert old in s
s = s.replace(old, old + "    (77, A64::Direct(46)),           // ftruncate (lib/db: a database file shortened after a rollback)\n", 1)
open(p, 'w').write(s)
print('ok39')
