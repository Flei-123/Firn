d = '/root/firn-wt-db/lib/db/'
p = open(d + 'sqlparse.fi').read()
# name token start for CREATE TABLE / CREATE INDEX (g), column definition span for ALTER
old = '''    var p1: u64 = 0
    var n1: usize = 0
    try parse_name(ps, &p1, &n1)
    if is_op(ps, tk.P_DOT) {
        advance(ps)
        var p2: u64 = 0
        var n2: usize = 0
        try parse_name(ps, &p2, &n2)
        p1 = p2
        n1 = n2
    }
    let sn0: *mut ax.Node = ax.ast_n((*ps).ast, s)
    (*sn0).sp = p1
    (*sn0).sn = n1'''
new = '''    var p1: u64 = 0
    var n1: usize = 0
    let name_start: usize = (*ps).tok.start
    try parse_name(ps, &p1, &n1)
    if is_op(ps, tk.P_DOT) {
        advance(ps)
        var p2: u64 = 0
        var n2: usize = 0
        try parse_name(ps, &p2, &n2)
        p1 = p2
        n1 = n2
    }
    let sn0: *mut ax.Node = ax.ast_n((*ps).ast, s)
    (*sn0).sp = p1
    (*sn0).sn = n1
    (*sn0).g = name_start as i32'''
assert old in p
p = p.replace(old, new)
old = '''    var p1: u64 = 0
    var n1: usize = 0
    try parse_name(ps, &p1, &n1)
    if is_op(ps, tk.P_DOT) {
        advance(ps)
        var p2: u64 = 0
        var n2: usize = 0
        try parse_name(ps, &p2, &n2)
        p1 = p2
        n1 = n2
    }
    try expect_kw(ps, tk.K_ON)'''
new = '''    var p1: u64 = 0
    var n1: usize = 0
    let iname_start: usize = (*ps).tok.start
    try parse_name(ps, &p1, &n1)
    if is_op(ps, tk.P_DOT) {
        advance(ps)
        var p2: u64 = 0
        var n2: usize = 0
        try parse_name(ps, &p2, &n2)
        p1 = p2
        n1 = n2
    }
    try expect_kw(ps, tk.K_ON)'''
assert old in p
p = p.replace(old, new)
old = '''    let sn: *mut ax.Node = ax.ast_n((*ps).ast, s)
    (*sn).sp = p1
    (*sn).sn = n1
    (*sn).sp2 = t1
    (*sn).sn2 = tn
    (*sn).a = cols'''
new = '''    let sn: *mut ax.Node = ax.ast_n((*ps).ast, s)
    (*sn).sp = p1
    (*sn).sn = n1
    (*sn).sp2 = t1
    (*sn).sn2 = tn
    (*sn).a = cols
    (*sn).g = iname_start as i32'''
assert old in p
p = p.replace(old, new)
old = '''        if accept_kw(ps, tk.K_ADD) {
            accept_kw(ps, tk.K_COLUMN)
            let c: i32 = try parse_coldef(ps)
            let snb: *mut ax.Node = ax.ast_n((*ps).ast, s)
            (*snb).op = 1
            (*snb).a = c
            (*snb).d = (*ps).last_end as i32
            return s
        }'''
new = '''        if accept_kw(ps, tk.K_ADD) {
            accept_kw(ps, tk.K_COLUMN)
            let cstart: usize = (*ps).tok.start
            let c: i32 = try parse_coldef(ps)
            let snb: *mut ax.Node = ax.ast_n((*ps).ast, s)
            (*snb).op = 1
            (*snb).a = c
            (*snb).e = cstart as i32
            (*snb).d = (*ps).last_end as i32
            return s
        }'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)

# fixed result sets for PRAGMA
s = open(d + 'select.fi').read()
s = s.replace("    set_up: bool,\n}\n\nfn sub_hook_addr", "    set_up: bool,\n    fixed: bool, // the rows are in `rs` already (PRAGMA results)\n}\n\nfn sub_hook_addr")
s = s.replace("tail: true, have_text: false, text_p: 0, text_n: 0, set_up: false }", "tail: true, have_text: false, text_p: 0, text_n: 0, set_up: false, fixed: false }")
old = '''fn sel_start(it: *mut SelIter) -> DbError!bool {
    setup_ctx(it)'''
new = '''fn sel_start(it: *mut SelIter) -> DbError!bool {
    if (*it).fixed {
        (*it).state = 2
        (*it).pos = 0
        (*it).skipped = 0
        (*it).emitted = 0
        (*it).limit = -1
        (*it).offset = 0
        (*it).has_limit = false
        return true
    }
    setup_ctx(it)'''
assert old in s
s = s.replace(old, new)
s = s.replace("compile_subs, sub_hook_addr, resolve_full, sel_nout, sel_col_type_hint,", "compile_subs, sub_hook_addr, resolve_full, sel_nout, sel_col_type_hint, sel_fixed_new, sel_fixed_add_name, sel_fixed_row,")
s += '''
// ------------------------------------------------------ fixed result sets
// An iterator over rows that were computed elsewhere (PRAGMA): names first, then rows.
fn sel_fixed_new(db: *mut dc.Db, stmt: *mut dc.Stmt, ncols: usize) -> *mut SelIter {
    let it: *mut SelIter = sel_new(db, stmt, &(*stmt).ast)
    if (it as u64) == 0 {
        return it
    }
    (*it).fixed = true
    (*it).ncols = ncols
    (*it).nout = ncols
    (*it).out = rt.heap_alloc((ncols + 1) * 40)
    (*it).set_up = true
    return it
}

fn sel_fixed_add_name(it: *mut SelIter, s: str) {
    let p: u64 = ar.arena_copy(&(*it).arena, s.p as u64, s.length())
    buf_push_u64(&(*it).names, p)
    buf_push_u64(&(*it).names, s.length() as u64)
}

fn sel_fixed_row(it: *mut SelIter, vals: *mut rc.Val) -> DbError!bool {
    return rs.rs_add(&(*it).rs, vals, (*it).ncols)
}
'''
open(d + 'select.fi', 'w').write(s)
print('ok17')
