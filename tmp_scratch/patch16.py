d = '/root/firn-wt-db/lib/db/'
s = open(d + 'dml.fi').read()
s = s.replace('''    var srcbuf: rt.Buf = rt.buf_new()
    rt.buf_reserve(&srcbuf, 2100 * 40)''', '''    var es: ex.Scope = ex.scope_new()
    var srcbuf: rt.Buf = rt.buf_new()
    rt.buf_reserve(&srcbuf, 2100 * 40)''')
s = s.replace("try sl.resolve_full(it, &(*e).scope0(), x, false)", "try sl.resolve_full(it, &es, x, false)")
s = s.replace('''fn val_wrap(r: DbError!rc.Val) -> DbError!bool {
    let v: rc.Val = try r
    return true
}''', '''fn mark_val(f: *mut bool) -> rc.Val {
    *f = true
    return rc.val_null()
}

fn failed_val(r: DbError!rc.Val) -> bool {
    var f: bool = false
    let v: rc.Val = r catch | e | (mark_val(&f))
    return f
}''')
s = s.replace('''        let ev: DbError!rc.Val = ex.eval(&(*e).cx, (*sno).a)
        if dberr.failed_bool(val_wrap(ev)) {
            (*e).rr[0].vals = (*e).vals
            rt.buf_free(&old)
            rt.buf_free(&sets)
            let vv: rc.Val = try ev
            return 1
        }''', '''        let ev: DbError!rc.Val = ex.eval(&(*e).cx, (*sno).a)
        if failed_val(ev) {
            (*e).rr[0].vals = (*e).vals
            rt.buf_free(&old)
            rt.buf_free(&sets)
            let vv: rc.Val = try ev
            return 1
        }''')
s = s.replace('''            if dberr.failed_bool(val_wrap(ev)) {
                var tmp: DbError!bool = val_wrap(ev)
                r = tmp
                failed = true
                break
            }''', '''            if failed_val(ev) {
                var tmp: rc.Val = rc.val_null()
                let vv0: rc.Val = ev catch | er | (dml_note_err(er))
                r = dml_err_result()
                failed = true
                break
            }''')
s += '''
static mut dml_last_err: i64 = 18

fn dml_note_err(er: DbError) -> rc.Val {
    dml_last_err = dberr.to_code(er)
    return rc.val_null()
}

fn dml_err_result() -> DbError!bool {
    return dberr.from_code(dml_last_err)
}
'''
open(d + 'dml.fi', 'w').write(s)
print('ok16')
