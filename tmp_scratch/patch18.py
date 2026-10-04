d = '/root/firn-wt-db/lib/db/'
dd = open(d + 'ddl.fi').read()
dd = dd.replace("v = ut.rd32(d, 60) as i32 as i64", "v = (ut.rd32(d, 60) as% i32) as i64")
dd = dd.replace("v = ut.rd32(d, 68) as i32 as i64", "v = (ut.rd32(d, 68) as% i32) as i64")
dd = dd.replace("export { exec_ddl, pragma_query, pragma_set_nowrite, pragma_is_write, bump_cookie }",
                "export { exec_ddl, exec_pragma_set, pragma_query, pragma_set_nowrite, pragma_is_write, bump_cookie }")
open(d + 'ddl.fi', 'w').write(dd)

s = open(d + 'sqlite.fi').read()
# split step_select into setup and iterate
a = s.index("fn step_select(st: *mut dc.Stmt) -> DbError!bool {")
b = s.index("// Runs the statement to its next row.")
new_step = '''// the next row of the running select (or pragma); the read ends with the last row
fn iterate(st: *mut dc.Stmt) -> DbError!bool {
    let it2: *mut sl.SelIter = (*st).it as *mut sl.SelIter
    let r3: DbError!bool = sl.sel_next(it2)
    if dberr.failed_bool(r3) {
        stmt_end_read(st)
        (*st).state = dc.ST_DONE
        (*st).out = 0
        return r3
    }
    let got: bool = r3 catch false
    if got {
        (*st).out = (*it2).out
        return true
    }
    stmt_end_read(st)
    (*st).state = dc.ST_DONE
    (*st).out = 0
    return false
}

fn step_select(st: *mut dc.Stmt) -> DbError!bool {
    if (*st).state == dc.ST_FRESH {
        try begin_read_for(st)
        let r: DbError!bool = compile_select(st)
        let ok: bool = r catch false
        if !ok {
            stmt_end_read(st)
            try r
        }
        let it: *mut sl.SelIter = (*st).it as *mut sl.SelIter
        let r2: DbError!bool = sl.sel_start(it)
        let ok2: bool = r2 catch false
        if !ok2 {
            stmt_end_read(st)
            try r2
        }
        (*st).state = dc.ST_RUNNING
        (*st).changes = 0
    }
    if (*st).state != dc.ST_RUNNING {
        return false
    }
    return try iterate(st)
}

'''
s = s[:a] + new_step + s[b:]
# stmt_step: pragma and the other kinds
old = '''    if (*st).state == dc.ST_DONE {
        return false
    }
    (*st).state = dc.ST_DONE
    if k == ax.N_BEGIN {'''
new = '''    if k == ax.N_PRAGMA {
        if (*st).state == dc.ST_DONE {
            return false
        }
        if (*st).state == dc.ST_RUNNING {
            return try iterate(st)
        }
        let more0: bool = try run_other(st)
        if !more0 {
            (*st).state = dc.ST_DONE
            return false
        }
        (*st).state = dc.ST_RUNNING
        let it0: *mut sl.SelIter = (*st).it as *mut sl.SelIter
        try sl.sel_start(it0)
        return try iterate(st)
    }
    if (*st).state == dc.ST_DONE {
        return false
    }
    (*st).state = dc.ST_DONE
    if k == ax.N_BEGIN {'''
assert old in s
s = s.replace(old, new)
old = '''    let more: bool = try run_other(st)
    if more {
        (*st).state = dc.ST_RUNNING
    }
    return more
}

fn run_other(st: *mut dc.Stmt) -> DbError!bool {
    return dberr.fail(DbError::Unsupported, "statement kind not available yet")
}
'''
new = '''    let more: bool = try run_other(st)
    return false
}

// PRAGMA with rows, and the statements that write.
fn run_other(st: *mut dc.Stmt) -> DbError!bool {
    let d: *mut dc.Db = (*st).db
    let k: u32 = (*st).kind
    if k == ax.N_PRAGMA {
        let handled: bool = try dd.pragma_set_nowrite(st)
        if handled {
            return false
        }
        if dd.pragma_is_write(st) {
            try run_write(st)
            return false
        }
        try begin_read_for(st)
        if (*st).it != 0 {
            sl.sel_free((*st).it as *mut sl.SelIter)
            (*st).it = 0
        }
        let r: DbError!u64 = dd.pragma_query(st)
        if dberr.failed_bool(pr_wrap(r)) {
            stmt_end_read(st)
            let ig: u64 = try r
            return false
        }
        let ita: u64 = r catch 0
        if ita == 0 {
            stmt_end_read(st)
            return false
        }
        (*st).it = ita
        (*st).ncols = sl.sel_ncols(ita as *mut sl.SelIter)
        return true
    }
    if k == ax.N_VACUUM {
        return dberr.fail(DbError::Unsupported, "VACUUM is not supported")
    }
    if k == ax.N_ANALYZE {
        return false
    }
    try run_write(st)
    return false
}

fn pr_wrap(r: DbError!u64) -> DbError!bool {
    let v: u64 = try r
    return true
}

// A statement that changes the database: its own transaction unless BEGIN was given;
// a failed statement is undone (savepoint), and in autocommit mode so is the transaction.
fn run_write(st: *mut dc.Stmt) -> DbError!bool {
    let d: *mut dc.Db = (*st).db
    if (*d).readonly {
        return dberr.fail(DbError::ReadOnly, "attempt to write a readonly database")
    }
    let autocommit: bool = !(*d).in_txn
    (*d).pg.hold_read = (*d).readers
    let br: DbError!bool = pgr.pager_begin_write(&(*d).pg)
    if dberr.failed_bool(br) {
        if autocommit && (*d).pg.state == 1 && (*d).readers == 0 {
            pgr.pager_end_read(&(*d).pg)
        }
        try br
    }
    // a new file: page 1 and the empty sqlite_master
    var prep: DbError!bool = true
    if pgr.pager_page_count(&(*d).pg) == 0 {
        prep = pgr.pager_init_new(&(*d).pg)
    }
    let pr2: DbError!bool = prep
    if !dberr.failed_bool(pr2) {
        prep = sch.ensure_schema(d)
    }
    if dberr.failed_bool(prep) {
        if autocommit {
            pgr.pager_rollback(&(*d).pg) catch false
        }
        try prep
    }
    if (*d).in_txn {
        (*d).txn_write = true
    }
    pgr.pager_stmt_begin(&(*d).pg)
    ar.arena_reset(&(*d).temp)
    let r: DbError!bool = exec_write_kind(st)
    if dberr.failed_bool(r) {
        pgr.pager_stmt_rollback(&(*d).pg) catch false
        if autocommit {
            (*d).pg.hold_read = (*d).readers
            pgr.pager_rollback(&(*d).pg) catch false
        }
        (*d).schema.loaded = false
        try r
    }
    pgr.pager_stmt_end(&(*d).pg)
    if autocommit {
        (*d).pg.hold_read = (*d).readers
        let cr: DbError!bool = pgr.pager_commit(&(*d).pg)
        if dberr.failed_bool(cr) {
            pgr.pager_rollback(&(*d).pg) catch false
            (*d).schema.loaded = false
            try cr
        }
    }
    return true
}

fn exec_write_kind(st: *mut dc.Stmt) -> DbError!bool {
    let k: u32 = (*st).kind
    if k == ax.N_INSERT {
        try dm.exec_insert(st)
        return true
    }
    if k == ax.N_UPDATE {
        try dm.exec_update(st)
        return true
    }
    if k == ax.N_DELETE {
        try dm.exec_delete(st)
        return true
    }
    if k == ax.N_PRAGMA {
        try dd.exec_pragma_set(st)
        return true
    }
    try dd.exec_ddl(st)
    return true
}
'''
assert old in s
s = s.replace(old, new)
open(d + 'sqlite.fi', 'w').write(s)
print('ok18')
