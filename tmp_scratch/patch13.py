d = '/root/firn-wt-db/lib/db/'
s = open(d + 'sqlite.fi').read()
s = s.replace('''    if !ok {
        rt.heap_free(p, dc.sz_of_db() + 64)
        return r
    }''', '''    if !ok {
        rt.heap_free(p, dc.sz_of_db() + 64)
        try r
        return dberr.fail(DbError::Misuse, "unreachable")
    }''')
s = s.replace('''    if !ok2 {
        pgr.pager_close(&(*d).pg)
        rt.heap_free(p, dc.sz_of_db() + 64)
        return r2
    }''', '''    if !ok2 {
        pgr.pager_close(&(*d).pg)
        rt.heap_free(p, dc.sz_of_db() + 64)
        try r2
        return dberr.fail(DbError::Misuse, "unreachable")
    }''')
s = s.replace('''    if !ok {
        stmt_free(st)
        return r
    }
    let root: i32 = r catch -1''', '''    if !ok {
        stmt_free(st)
        try r
        return dberr.fail(DbError::Misuse, "unreachable")
    }
    let root: i32 = r catch -1''')
open(d + 'sqlite.fi', 'w').write(s)
print('ok13')
