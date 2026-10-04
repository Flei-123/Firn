p = '/root/firn-wt-db/lib/db/btree.fi'
s = open(p).read()
old = '''        if total + ci.size > u {
            return dberr.fail(DbError::Corrupt, "cells overlap")
        }'''
new = '''        if total + ci.size > u || total + ci.size + (*info).arr_end > u {
            return dberr.fail(DbError::Corrupt, "cells overlap")
        }'''
assert old in s
s = s.replace(old, new)
open(p, 'w').write(s)
print('ok27')
