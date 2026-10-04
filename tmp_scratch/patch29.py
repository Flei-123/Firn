import re
d = '/root/firn-wt-db/lib/db/'
s = open(d + 'select.fi').read()

# compile_subs without an iterator
s = s.replace("compile_subs, sub_hook_addr, resolve_full,", "compile_subs, resolve_stmt, sub_hook_addr, resolve_full,")
old = '''fn resolve_full(it: *mut SelIter, sc: *mut ex.Scope, n: i32, agg_ok: bool) -> DbError!bool {
    if n < 0 {
        return true
    }
    try ex.resolve_expr(sc, (*it).ast, n, agg_ok, false)
    try compile_subs(it, sc, n)
    return true
}'''
new = '''fn resolve_full(it: *mut SelIter, sc: *mut ex.Scope, n: i32, agg_ok: bool) -> DbError!bool {
    if n < 0 {
        return true
    }
    try ex.resolve_expr(sc, (*it).ast, n, agg_ok, false)
    try compile_subs((*it).db, (*it).stmt, sc, n)
    return true
}

// the same for a statement that is not a SELECT (INSERT, UPDATE, DELETE)
fn resolve_stmt(db: *mut dc.Db, stmt: *mut dc.Stmt, sc: *mut ex.Scope, n: i32, agg_ok: bool) -> DbError!bool {
    if n < 0 {
        return true
    }
    try ex.resolve_expr(sc, &(*stmt).ast, n, agg_ok, false)
    try compile_subs(db, stmt, sc, n)
    return true
}'''
assert old in s
s = s.replace(old, new)
a = s.index("fn compile_subs(it: *mut SelIter, sc: *mut ex.Scope, n: i32) -> DbError!bool {")
b = s.index("fn has_subs(ast: *mut ax.Ast, n: i32) -> bool {")
body = s[a:b]
body = body.replace("fn compile_subs(it: *mut SelIter, sc: *mut ex.Scope, n: i32) -> DbError!bool {", "fn compile_subs(db: *mut dc.Db, stmt: *mut dc.Stmt, sc: *mut ex.Scope, n: i32) -> DbError!bool {")
body = body.replace("let ast: *mut ax.Ast = (*it).ast", "let ast: *mut ax.Ast = &(*stmt).ast")
body = body.replace("try compile_subs(it, sc,", "try compile_subs(db, stmt, sc,")
body = body.replace("sel_new((*it).db, (*it).stmt, ast)", "sel_new(db, stmt, ast)")
body = body.replace("let idx: usize = rt.buf_len(&(*(*it).stmt).subs) / 8\n        buf_push_u64(&(*(*it).stmt).subs, sub as u64)",
                    "let idx: usize = rt.buf_len(&(*stmt).subs) / 8\n        buf_push_u64(&(*stmt).subs, sub as u64)")
s = s[:a] + body + s[b:]
open(d + 'select.fi', 'w').write(s)

m = open(d + 'dml.fi').read()
# no dummy iterators
m = m.replace('''    var dummy_it: *mut sl.SelIter = sl.sel_new(db, st, ast)
    if (dummy_it as u64) == 0 {
        return dberr.fail(DbError::NoMemory, "out of memory")
    }
    // the sub-selects of the statement are compiled once and kept in the statement
    let r: DbError!bool = insert_rows(e, st, ins, &map[0], nmapped, dummy_it, upsert)
    sl.sel_free(dummy_it)
    try r
    return true''', '''    try insert_rows(e, st, ins, &map[0], nmapped, upsert)
    return true''')
m = m.replace('''fn insert_rows(e: *mut Dml, st: *mut dc.Stmt, ins: *mut ax.Node, map: *mut i32, nmapped: usize,
    it: *mut sl.SelIter, upsert: i32) -> DbError!bool {''', '''fn insert_rows(e: *mut Dml, st: *mut dc.Stmt, ins: *mut ax.Node, map: *mut i32, nmapped: usize,
    upsert: i32) -> DbError!bool {''')
m = m.replace("try sl.resolve_full(it, &es, x, false)", "try sl.resolve_stmt(db, st, &es, x, false)")
m = m.replace('''    let it: *mut sl.SelIter = sl.sel_new((*e).db, st, ast)
    var s: i32 = (*un).b
    while s >= 0 {
        let sn: *mut ax.Node = ax.ast_n(ast, s)
        try sl.resolve_full(it, &(*e).scope, (*sn).a, false)
        s = (*sn).next
    }
    try sl.resolve_full(it, &(*e).scope, (*un).c, false)
    sl.sel_free(it)
    return true''', '''    var s: i32 = (*un).b
    while s >= 0 {
        let sn: *mut ax.Node = ax.ast_n(ast, s)
        try sl.resolve_stmt((*e).db, st, &(*e).scope, (*sn).a, false)
        s = (*sn).next
    }
    try sl.resolve_stmt((*e).db, st, &(*e).scope, (*un).c, false)
    return true''')
m = m.replace('''    let it: *mut sl.SelIter = sl.sel_new(db, st, ast)
    // resolve SET targets and expressions, and WHERE''', '''    // resolve SET targets and expressions, and WHERE''')
m = m.replace('''        if ci == -1 {
            sl.sel_free(it)
            return dberr.fail(DbError::NoSuchColumn, text_msg2("no such column: ", (*sn).sp, (*sn).sn, "", 0, 0))
        }''', '''        if ci == -1 {
            return dberr.fail(DbError::NoSuchColumn, text_msg2("no such column: ", (*sn).sp, (*sn).sn, "", 0, 0))
        }''')
m = m.replace('''            if (*t).rowid_col < 0 {
                sl.sel_free(it)
                return dberr.fail(DbError::Unsupported, "UPDATE of the hidden rowid is not supported")
            }''', '''            if (*t).rowid_col < 0 {
                return dberr.fail(DbError::Unsupported, "UPDATE of the hidden rowid is not supported")
            }''')
m = m.replace('''        if nt >= 64 {
            sl.sel_free(it)
            return dberr.fail(DbError::TooBig, "too many columns in SET")
        }''', '''        if nt >= 64 {
            return dberr.fail(DbError::TooBig, "too many columns in SET")
        }''')
m = m.replace('''        let rr: DbError!bool = sl.resolve_full(it, &(*e).scope, (*sn).a, false)
        if dberr.failed_bool(rr) {
            sl.sel_free(it)
            try rr
        }
        s = (*sn).next
    }
    let wr: DbError!bool = sl.resolve_full(it, &(*e).scope, (*un).b, false)
    sl.sel_free(it)
    try wr''', '''        try sl.resolve_stmt(db, st, &(*e).scope, (*sn).a, false)
        s = (*sn).next
    }
    try sl.resolve_stmt(db, st, &(*e).scope, (*un).b, false)''')
m = m.replace('''    let it: *mut sl.SelIter = sl.sel_new(db, st, ast)
    let wr: DbError!bool = sl.resolve_full(it, &(*e).scope, (*dn).a, false)
    sl.sel_free(it)
    try wr''', '''    try sl.resolve_stmt(db, st, &(*e).scope, (*dn).a, false)''')
# the column map on the heap, sized by the table
m = m.replace("    var map: [i32; 2000] = [-1; 2000]\n    var nmapped: usize = 0\n    try column_map(e, ins, &map[0], &nmapped)\n    try insert_rows(e, st, ins, &map[0], nmapped, upsert)\n    return true",
              "    var mapbuf: rt.Buf = rt.buf_new()\n    rt.buf_reserve(&mapbuf, ((*t).ncols + 4) * 4)\n    var nmapped: usize = 0\n    let cm: DbError!bool = column_map(e, ins, (rt.buf_ptr(&mapbuf)) as *mut i32, &nmapped)\n    if dberr.failed_bool(cm) {\n        rt.buf_free(&mapbuf)\n        try cm\n    }\n    let ir: DbError!bool = insert_rows(e, st, ins, (rt.buf_ptr(&mapbuf)) as *mut i32, nmapped, upsert)\n    rt.buf_free(&mapbuf)\n    try ir\n    return true")
# sizes of the scratch memory follow the table
m = m.replace("(*e).vals = rt.heap_alloc(2100 * 40)", "(*e).vals = rt.heap_alloc(((*t).ncols + 4) * 40)")
m = m.replace("(*e).exc = rt.heap_alloc(2100 * 40)", "(*e).exc = rt.heap_alloc(((*t).ncols + 4) * 40)")
m = m.replace("    kv: u64, // key values\n", "    kv: u64, // key values\n    vals_n: usize, // bytes allocated for vals / exc\n")
m = m.replace("kv: 0, mode: ax.CF_ABORT, changes: 0, hook: 0 }", "kv: 0, vals_n: 0, mode: ax.CF_ABORT, changes: 0, hook: 0 }")
m = m.replace('''    if (*e).vals != 0 {
        rt.heap_free((*e).vals, 2100 * 40)
    }
    if (*e).exc != 0 {
        rt.heap_free((*e).exc, 2100 * 40)
    }''', '''    if (*e).vals != 0 {
        rt.heap_free((*e).vals, (*e).vals_n)
    }
    if (*e).exc != 0 {
        rt.heap_free((*e).exc, (*e).vals_n)
    }''')
m = m.replace("    (*e).vals = rt.heap_alloc(((*t).ncols + 4) * 40)", "    (*e).vals_n = ((*t).ncols + 4) * 40\n    (*e).vals = rt.heap_alloc((*e).vals_n)")
m = m.replace("    var srcbuf: rt.Buf = rt.buf_new()\n    rt.buf_reserve(&srcbuf, 2100 * 40)", "    var srcbuf: rt.Buf = rt.buf_new()\n    rt.buf_reserve(&srcbuf, ((*t).ncols + 8) * 40)")
open(d + 'dml.fi', 'w').write(m)
print('ok29')
