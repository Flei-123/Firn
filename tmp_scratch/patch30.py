d = '/root/firn-wt-db/lib/db/'
m = open(d + 'dml.fi').read()
m = m.replace('''                if cnt >= 2000 {
                    return dberr.fail(DbError::TooBig, "too many values")
                }''', '''                if cnt >= (*t).ncols + 8 {
                    return dberr.fail(DbError::Sql, text_msg2("table ", (*t).name_p, (*t).name_n, " has fewer columns than values supplied", 0, 0))
                }''')
m = m.replace("let got: DbError!usize = rs.rs_get(&rows, i, src, 2000)", "let got: DbError!usize = rs.rs_get(&rows, i, src, (*t).ncols + 8)")
# one Source for the deletes of a statement
m = m.replace("    vals_n: usize, // bytes allocated for vals / exc\n", "    vals_n: usize, // bytes allocated for vals / exc\n    dsrc: pl.Source, // reads the rows that are deleted or replaced\n")
m = m.replace("kv: 0, vals_n: 0, mode: ax.CF_ABORT, changes: 0, hook: 0 }", "kv: 0, vals_n: 0, dsrc: pl.source_new(), mode: ax.CF_ABORT, changes: 0, hook: 0 }")
m = m.replace('''fn dml_free(e: *mut Dml) {
    ar.arena_free(&(*e).arena)''', '''fn dml_free(e: *mut Dml) {
    pl.src_free(&(*e).dsrc)
    ar.arena_free(&(*e).arena)''')
old = '''    let t: *mut dc.Table = table_of(e)
    var s: pl.Source = pl.source_new()
    s.tbl = (*e).tidx
    s.ncols_table = (*t).ncols
    pl.mark_all_used(&s, (*t).ncols)
    let found: bool = try pl.src_load_row(&(*e).cx, &s, rowid)
    if !found {
        pl.src_free(&s)
        return false
    }
    let vals: *mut rc.Val = s.vals as *mut rc.Val
    var i: usize = 0
    while i < (*e).nidx {
        let ix: *mut dc.Index = dc.index_at(&(*db).schema, (*e).idx[i] as usize)
        let r: DbError!bool = index_remove(e, ix, vals, rowid)
        if dberr.failed_bool(r) {
            pl.src_free(&s)
            return r
        }
        i = i + 1
    }
    let r2: DbError!bool = bx.bt_table_delete(&(*db).bt, (*t).root, rowid)
    pl.src_free(&s)
    try r2
    return true'''
new = '''    let t: *mut dc.Table = table_of(e)
    let s: *mut pl.Source = &(*e).dsrc
    (*s).tbl = (*e).tidx
    (*s).ncols_table = (*t).ncols
    pl.mark_all_used(s, (*t).ncols)
    let found: bool = try pl.src_load_row(&(*e).cx, s, rowid)
    if !found {
        return false
    }
    let vals: *mut rc.Val = (*s).vals as *mut rc.Val
    var i: usize = 0
    while i < (*e).nidx {
        let ix: *mut dc.Index = dc.index_at(&(*db).schema, (*e).idx[i] as usize)
        try index_remove(e, ix, vals, rowid)
        i = i + 1
    }
    try bx.bt_table_delete(&(*db).bt, (*t).root, rowid)
    return true'''
assert old in m
m = m.replace(old, new)
open(d + 'dml.fi', 'w').write(m)

# a statement savepoint only where a failure can leave partial work
s = open(d + 'sqlite.fi').read()
old = '''    pgr.pager_stmt_begin(&(*d).pg)
    ar.arena_reset(&(*d).temp)'''
new = '''    // one VALUES row without ON CONFLICT cannot fail half way (its checks come first)
    var savepoint: bool = true
    if (*st).kind == ax.N_INSERT {
        let ins: *mut ax.Node = ax.ast_n(&(*st).ast, (*st).root)
        if (*ins).b >= 0 && (*ax.ast_n(&(*st).ast, (*ins).b)).next < 0 && (*ins).d < 0 && (*ins).c < 0 {
            savepoint = false
        }
    }
    if savepoint {
        pgr.pager_stmt_begin(&(*d).pg)
    }
    ar.arena_reset(&(*d).temp)'''
assert old in s
s = s.replace(old, new)
s = s.replace('''    if dberr.failed_bool(r) {
        pgr.pager_stmt_rollback(&(*d).pg) catch false
        if autocommit {''', '''    if dberr.failed_bool(r) {
        if savepoint {
            pgr.pager_stmt_rollback(&(*d).pg) catch false
        } else if !autocommit {
            // a single row failed before it wrote anything: nothing to undo
        }
        if autocommit {''')
open(d + 'sqlite.fi', 'w').write(s)
print('ok30')
