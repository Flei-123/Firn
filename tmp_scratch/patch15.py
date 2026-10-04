d = '/root/firn-wt-db/lib/db/'
s = open(d + 'select.fi').read()
# alias substitution walker
walker = '''
// An unqualified name that is no column of any source but the alias of a result column
// stands for that column's expression (SQLite allows this in WHERE, GROUP BY, HAVING).
fn subst_aliases(it: *mut SelIter, n: i32) {
    if n < 0 {
        return
    }
    let ast: *mut ax.Ast = (*it).ast
    let nd: *mut ax.Node = ax.ast_n(ast, n)
    let k: u32 = (*nd).kind
    if k == ax.N_COL {
        if (*nd).sn2 > 0 {
            return
        }
        var si: usize = 0
        while si < (*it).nsrc {
            if find_col_in(&(*it).scope, si, (*nd).sp, (*nd).sn) != -1 {
                return
            }
            let info: *mut ex.SrcInfo = &(*it).scope.src[si]
            if (*info).hidden_rowid && (dc.name_eq((*nd).sp, (*nd).sn, "rowid".p as u64, 5)
                || dc.name_eq((*nd).sp, (*nd).sn, "oid".p as u64, 3) || dc.name_eq((*nd).sp, (*nd).sn, "_rowid_".p as u64, 7)) {
                return
            }
            si = si + 1
        }
        let a: i64 = find_result_alias(it, (*nd).sp, (*nd).sn)
        if a >= 0 {
            let target: i32 = buf_i32(&(*it).res, a as usize)
            if target != n {
                let nxt: i32 = (*nd).next
                *nd = *ax.ast_n(ast, target)
                (*nd).next = nxt
            }
        }
        return
    }
    if k == ax.N_INT || k == ax.N_REAL || k == ax.N_STR || k == ax.N_BLOB || k == ax.N_NULL || k == ax.N_PARAM
        || k == ax.N_EXISTS || k == ax.N_SUBQ {
        return
    }
    if k == ax.N_FUNC {
        var a2: i32 = (*nd).a
        while a2 >= 0 {
            subst_aliases(it, a2)
            a2 = (*ax.ast_n(ast, a2)).next
        }
        return
    }
    if k == ax.N_IN_LIST {
        subst_aliases(it, (*nd).a)
        var b: i32 = (*nd).b
        while b >= 0 {
            subst_aliases(it, b)
            b = (*ax.ast_n(ast, b)).next
        }
        return
    }
    if k == ax.N_CASE {
        subst_aliases(it, (*nd).a)
        var w: i32 = (*nd).b
        while w >= 0 {
            subst_aliases(it, (*ax.ast_n(ast, w)).a)
            subst_aliases(it, (*ax.ast_n(ast, w)).b)
            w = (*ax.ast_n(ast, w)).next
        }
        subst_aliases(it, (*nd).c)
        return
    }
    subst_aliases(it, (*nd).a)
    subst_aliases(it, (*nd).b)
    subst_aliases(it, (*nd).c)
}
'''
s = s.replace("// Sets up one SELECT core (no compound): sources, columns, clauses, plan.", walker + "\n// Sets up one SELECT core (no compound): sources, columns, clauses, plan.")
s = s.replace('''    (*it).where_n = (*sel).c
    try resolve_full(it, sc, (*sel).c, false)''', '''    (*it).where_n = (*sel).c
    subst_aliases(it, (*sel).c)
    try resolve_full(it, sc, (*sel).c, false)''')
s = s.replace('''    (*it).having = (*sel).e
    try resolve_full(it, sc, (*sel).e, true)''', '''    (*it).having = (*sel).e
    subst_aliases(it, (*sel).e)
    try resolve_full(it, sc, (*sel).e, true)''')
open(d + 'select.fi', 'w').write(s)
print('ok15')
