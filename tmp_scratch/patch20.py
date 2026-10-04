d = '/root/firn-wt-db/lib/db/'
p = open(d + 'sqlparse.fi').read()
old = '''    if is_kw(ps, tk.K_AS) {
        return unsupported("CREATE TABLE ... AS SELECT")
    }'''
new = '''    if is_kw(ps, tk.K_AS) {
        advance(ps)
        let sel: i32 = try parse_select(ps)
        let sn2: *mut ax.Node = ax.ast_n((*ps).ast, s)
        (*sn2).c = sel
        (*sn2).d = start as i32
        (*sn2).e = (*ps).last_end as i32
        return s
    }'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)

s = open(d + 'select.fi').read()
s = s.replace("sel_fixed_new, sel_fixed_add_name, sel_fixed_row,", "sel_fixed_new, sel_fixed_add_name, sel_fixed_row, sel_col_aff,")
s += '''
// the affinity of result column i (a column's own, a CAST's), -1 if none
fn sel_col_aff(it: *mut SelIter, i: usize) -> i64 {
    var core: *mut SelIter = it
    if (*it).ncore > 0 {
        core = rt.ld64(rt.buf_ptr(&(*it).cores), 0) as *mut SelIter
    }
    if i >= (*core).nout {
        return -1
    }
    let n: i32 = buf_i32(&(*core).res, i)
    if n < 0 {
        return -1
    }
    (*core).cx.scope = &(*core).scope
    (*core).cx.ast = (*core).ast
    return ex.expr_aff(&(*core).cx, n)
}
'''
open(d + 'select.fi', 'w').write(s)

dd = open(d + 'ddl.fi').read()
old = '''fn exec_create_table(st: *mut dc.Stmt) -> DbError!bool {
    let db: *mut dc.Db = (*st).db
    let ast: *mut ax.Ast = &(*st).ast
    let nd: *mut ax.Node = ax.ast_n(ast, (*st).root)
    let np: u64 = (*nd).sp
    let nn: usize = (*nd).sn'''
new = '''// CREATE TABLE name AS SELECT ...: the columns are the result columns (typed by affinity)
fn exec_create_table_as(st: *mut dc.Stmt) -> DbError!bool {
    let db: *mut dc.Db = (*st).db
    let ast: *mut ax.Ast = &(*st).ast
    let nd: *mut ax.Node = ax.ast_n(ast, (*st).root)
    let np: u64 = (*nd).sp
    let nn: usize = (*nd).sn
    if starts_with_ci(np, nn, "sqlite_") {
        return dberr.fail(DbError::Sql, msg3("object name reserved for internal use: ", np, nn, ""))
    }
    let it: *mut sl.SelIter = sl.sel_new(db, st, ast)
    if (it as u64) == 0 {
        return dberr.fail(DbError::NoMemory, "out of memory")
    }
    let sr: DbError!bool = sl.sel_setup(it, (*nd).c, 0 as *mut ex.Scope)
    if dberr.failed_bool(sr) {
        sl.sel_free(it)
        try sr
    }
    let ncols: usize = sl.sel_ncols(it)
    var sqlb: rt.Buf = rt.buf_new()
    ut.put_text(&sqlb, "CREATE TABLE ")
    rt.buf_push_bytes(&sqlb, np, nn)
    rt.buf_push(&sqlb, 40 as u8)
    var affs: rt.Buf = rt.buf_new()
    var i: usize = 0
    while i < ncols {
        if i > 0 {
            rt.buf_push(&sqlb, 44 as u8)
        }
        var cp: u64 = 0
        var cn: usize = 0
        sl.sel_name(it, i, &cp, &cn)
        var simple: bool = cn > 0
        var k: usize = 0
        while k < cn {
            let c: u8 = rt.ld8(cp, k)
            let letter: bool = (c >= 65 as u8 && c <= 90 as u8) || (c >= 97 as u8 && c <= 122 as u8) || c == 95 as u8
            let digit: bool = c >= 48 as u8 && c <= 57 as u8
            if !(letter || (digit && k > 0)) {
                simple = false
            }
            k = k + 1
        }
        if simple {
            rt.buf_push_bytes(&sqlb, cp, cn)
        } else {
            rt.buf_push(&sqlb, 34 as u8)
            k = 0
            while k < cn {
                let c: u8 = rt.ld8(cp, k)
                rt.buf_push(&sqlb, c)
                if c == 34 as u8 {
                    rt.buf_push(&sqlb, 34 as u8)
                }
                k = k + 1
            }
            rt.buf_push(&sqlb, 34 as u8)
        }
        let aff: i64 = sl.sel_col_aff(it, i)
        if aff == 3 {
            ut.put_text(&sqlb, " INT")
        } else if aff == 1 {
            ut.put_text(&sqlb, " TEXT")
        } else if aff == 2 {
            ut.put_text(&sqlb, " NUM")
        } else if aff == 4 {
            ut.put_text(&sqlb, " REAL")
        }
        rt.buf_push(&affs, (if aff < 0 { 0 } else { aff }) as u8)
        i = i + 1
    }
    rt.buf_push(&sqlb, 41 as u8)
    let sqlp: u64 = ar.arena_copy(&(*db).temp, rt.buf_ptr(&sqlb), rt.buf_len(&sqlb))
    let sqln: usize = rt.buf_len(&sqlb)
    rt.buf_free(&sqlb)
    let root: u32 = try bx.bt_create(&(*db).bt, false)
    try master_add(db, "table", np, nn, np, nn, root, sqlp, sqln)
    // the rows
    let r: DbError!bool = ctas_rows(db, it, root, ncols, &affs)
    rt.buf_free(&affs)
    sl.sel_free(it)
    try r
    try bump_cookie(db)
    (*db).schema.loaded = false
    return true
}

fn ctas_rows(db: *mut dc.Db, it: *mut sl.SelIter, root: u32, ncols: usize, affs: *mut rt.Buf) -> DbError!bool {
    try sl.sel_start(it)
    var rowid: i64 = 0
    var rec: rt.Buf = rt.buf_new()
    var vbuf: rt.Buf = rt.buf_new()
    rt.buf_reserve(&vbuf, (ncols + 1) * 40)
    let vals: *mut rc.Val = (rt.buf_ptr(&vbuf)) as *mut rc.Val
    var arena: ar.Arena = ar.arena_new()
    while true {
        let more: DbError!bool = sl.sel_next(it)
        if dberr.failed_bool(more) {
            rt.buf_free(&rec)
            rt.buf_free(&vbuf)
            ar.arena_free(&arena)
            try more
        }
        if !(more catch false) {
            break
        }
        var i: usize = 0
        while i < ncols {
            var v: rc.Val = *(((*it).out as *mut rc.Val) + i)
            vl.apply_affinity(&v, rt.ld8(rt.buf_ptr(affs), i) as i64, &arena)
            *(vals + i) = v
            i = i + 1
        }
        rt.buf_clear(&rec)
        rc.rec_encode(vals, ncols, &rec)
        rowid = rowid + 1
        let ir: DbError!bool = bx.bt_table_insert(&(*db).bt, root, rowid, rt.buf_ptr(&rec), rt.buf_len(&rec))
        if dberr.failed_bool(ir) {
            rt.buf_free(&rec)
            rt.buf_free(&vbuf)
            ar.arena_free(&arena)
            try ir
        }
        ar.arena_reset(&arena)
    }
    rt.buf_free(&rec)
    rt.buf_free(&vbuf)
    ar.arena_free(&arena)
    return true
}

fn exec_create_table(st: *mut dc.Stmt) -> DbError!bool {
    let db: *mut dc.Db = (*st).db
    let ast: *mut ax.Ast = &(*st).ast
    let nd: *mut ax.Node = ax.ast_n(ast, (*st).root)
    let np: u64 = (*nd).sp
    let nn: usize = (*nd).sn'''
assert old in dd
dd = dd.replace(old, new)
old = '''    if starts_with_ci(np, nn, "sqlite_") {
        return dberr.fail(DbError::Sql, msg3("object name reserved for internal use: ", np, nn, ""))
    }
    // columns: no duplicates, AUTOINCREMENT only on an INTEGER PRIMARY KEY'''
new = '''    if starts_with_ci(np, nn, "sqlite_") {
        return dberr.fail(DbError::Sql, msg3("object name reserved for internal use: ", np, nn, ""))
    }
    if (*nd).c >= 0 {
        return try exec_create_table_as(st)
    }
    // columns: no duplicates, AUTOINCREMENT only on an INTEGER PRIMARY KEY'''
assert old in dd
dd = dd.replace(old, new)
open(d + 'ddl.fi', 'w').write(dd)
print('ok20')
