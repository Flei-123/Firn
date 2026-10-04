d = '/root/firn-wt-db/lib/db/'
s = open(d + 'sqlite.fi').read()
s = s.replace("    db_query_int, db_query_text,\n", "    db_query_int, db_query_text, db_query_all,\n")
s += '''
// All rows of a query as text: columns joined by "|", rows by ";". NULL is written NULL, a
// BLOB as x'..' in hex, a REAL like SQLite's text. For tests, logs and quick looks.
fn db_query_all(d: *mut dc.Db, sql: str) -> DbError!str {
    let st: *mut dc.Stmt = try db_prepare(d, sql)
    var out: rt.Buf = rt.buf_new()
    var first: bool = true
    while true {
        let r: DbError!bool = stmt_step(st)
        if dberr.failed_bool(r) {
            stmt_finalize(st)
            rt.buf_free(&out)
            try r
        }
        if !(r catch false) {
            break
        }
        if !first {
            rt.buf_push(&out, 59 as u8)
        }
        first = false
        var c: usize = 0
        while c < stmt_column_count(st) {
            if c > 0 {
                rt.buf_push(&out, 124 as u8)
            }
            let t: i64 = stmt_column_type(st, c)
            if t == COL_NULL {
                ut.put_text(&out, "NULL")
            } else if t == COL_BLOB {
                var p: u64 = 0
                var n: usize = 0
                stmt_column_blob(st, c, &p, &n)
                ut.put_text(&out, "x'")
                var i: usize = 0
                while i < n {
                    rt.buf_push_hex_u64(&out, rt.ld8(p, i) as u64, 2)
                    i = i + 1
                }
                rt.buf_push(&out, 39 as u8)
            } else {
                let tx: str = stmt_column_text(st, c)
                rt.buf_push_bytes(&out, tx.p as u64, tx.length())
            }
            c = c + 1
        }
    }
    stmt_finalize(st)
    let s: str = __str_copy(str { p: rt.buf_data(&out), n: rt.buf_len(&out) })
    rt.buf_free(&out)
    return s
}
'''
open(d + 'sqlite.fi', 'w').write(s)
print('ok32')
