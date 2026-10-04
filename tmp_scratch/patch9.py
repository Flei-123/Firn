d = '/root/firn-wt-db/lib/db/'
s = open(d + 'select.fi').read()
s = s.replace("ex.expr_text_name((*(*it).stmt).sql_p(), (*(*it).stmt).sql_n(), cn, &np2, &nn2)",
              "ex.expr_text_name(rt.buf_ptr(&(*(*it).stmt).sql), rt.buf_len(&(*(*it).stmt).sql), cn, &np2, &nn2)")
s = s.replace("*(vp + G + argcols + b) = try ex.eval_col_raw(cx, buf_i32(&(*it).bare, b))",
              "*(vp + G + argcols + b) = try ex.eval(cx, buf_i32(&(*it).bare, b))")
s = s.replace('''        n = (*nd).ival as i32
        if n == 0 - 0 && (*nd).ival == 0 {
            n = -1
        }''', '''        n = (*nd).ival as i32''')
s = s.replace('''fn dberr_code_of(r: DbError!bool) -> i64 {
    var code: i64 = 18
    let tmp: bool = r catch | e | (code_set(e))
    return last_code
}''', '''fn dberr_code_of(r: DbError!bool) -> i64 {
    last_code = 18
    let tmp: bool = r catch | e | (code_set(e))
    return last_code
}''')
s = s.replace('''    rs.rs_free(&input)
    rt.buf_free(&vals)
    rt.buf_free(&accs)''', '''    var fa: usize = 0
    while fa < A {
        rt.buf_free(&(((rt.buf_ptr(&accs)) as *mut Acc) + fa).cat)
        fa = fa + 1
    }
    rs.rs_free(&input)
    rt.buf_free(&vals)
    rt.buf_free(&accs)''')
open(d + 'select.fi', 'w').write(s)

p = open(d + 'sqlparse.fi').read()
old = '''    let s: i32 = try mk(ps, ax.N_SELECT)
    if accept_kw(ps, tk.K_DISTINCT) {'''
new = '''    let s: i32 = try mk(ps, ax.N_SELECT)
    (*ax.ast_n((*ps).ast, s)).ival = -1
    if accept_kw(ps, tk.K_DISTINCT) {'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)
print('ok9')
