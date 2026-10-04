d = '/root/firn-wt-db/lib/db/'
e = open(d + 'expr.fi').read()
# Ctx gets the statement
e = e.replace("    subq: u64, // hook for sub-selects: address of the statement executor (exec.fi)\n    err_node: i32,\n}",
              "    subq: u64, // hook for sub-selects: address of the statement executor (select.fi)\n    stmt: u64, // the statement (address) the sub-select registry lives in\n    err_node: i32,\n}")
e = e.replace("agg: 0, bare: 0, parent: 0 as *mut Ctx, subq: 0, err_node: -1 }", "agg: 0, bare: 0, parent: 0 as *mut Ctx, subq: 0, stmt: 0, err_node: -1 }")
# outer depth tracking
e = e.replace("fn resolve_col(sc: *mut Scope, ast: *mut ax.Ast, n: i32) -> DbError!bool {",
"""// the deepest outer scope a name was found in since the last reset (correlation)
static mut outer_depth_max: i32 = 0

fn outer_depth_get() -> i32 {
    return outer_depth_max
}

fn outer_depth_set(v: i32) {
    outer_depth_max = v
}

fn resolve_col(sc: *mut Scope, ast: *mut ax.Ast, n: i32) -> DbError!bool {""")
e = e.replace("""            (*nd).d = depth
            return true
        }
        scope = (*scope).parent""", """            (*nd).d = depth
            if depth > outer_depth_max {
                outer_depth_max = depth
            }
            return true
        }
        scope = (*scope).parent""")
e = e.replace("expr_aff, expr_coll, compare_aff, is_aggregate_id, ctx_new, node_is_const,", "expr_aff, expr_coll, compare_aff, is_aggregate_id, ctx_new, node_is_const, outer_depth_get, outer_depth_set,")
open(d + 'expr.fi', 'w').write(e)

c = open(d + 'dbcore.fi').read()
c = c.replace("    started_read: bool,\n    pos: usize, // offset of the next statement in the text (for exec)\n}",
              "    started_read: bool,\n    pos: usize, // offset of the next statement in the text (for exec)\n    subs: rt.Buf, // addresses of the sub-select iterators of this statement\n    sub_arena: ar.Arena, // values cached from uncorrelated sub-selects\n    plan: u64, // the compiled plan of a DML statement (address)\n}")
c = c.replace("    ast_root: i32,\n}", "    ast_root: i32,\n    mrowid: i64, // the rowid of its row in sqlite_master\n}")
c = c.replace("    bad: bool, // an index this library cannot maintain (collation, expression, partial)\n    msg_p: u64,\n    msg_n: usize,\n}",
              "    bad: bool, // an index this library cannot maintain (collation, expression, partial)\n    msg_p: u64,\n    msg_n: usize,\n    mrowid: i64,\n    sql_p: u64,\n    sql_n: usize,\n}")
c = c.replace("MAX_SRC, MAX_ICOLS,", "sz_of_column, MAX_SRC, MAX_ICOLS,")
c += '''
fn sz_of_column() -> usize {
    return core_sz_of[Column]()
}
'''
open(d + 'dbcore.fi', 'w').write(c)

p = open(d + 'sqlparse.fi').read()
old = '''            if !done {
                let e: i32 = try parse_expr(ps, 1)
                (*ax.ast_n((*ps).ast, c)).a = e'''
new = '''            if !done {
                let estart: usize = (*ps).tok.start
                let e: i32 = try parse_expr(ps, 1)
                (*ax.ast_n((*ps).ast, c)).a = e
                (*ax.ast_n((*ps).ast, c)).d = estart as i32
                (*ax.ast_n((*ps).ast, c)).e = (*ps).last_end as i32'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)

pl = open(d + 'plan.fi').read()
pl = pl.replace("(*s).used[i] = 18446744073709551615", "(*s).used[i] = ~(0 as u64)")
open(d + 'plan.fi', 'w').write(pl)
print("ok8")
