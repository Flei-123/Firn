d = '/root/firn-wt-db/lib/db/'
# dberr helpers
e = open(d + 'dberr.fi').read()
e = e.replace("export { DbError, set_message, last_message, clear_message, fail, to_code, from_code }",
              "export { DbError, set_message, last_message, clear_message, fail, to_code, from_code, failed_bool, failed_i32, failed_i64, failed_usize }")
e += '''
fn mark_flag(f: *mut bool) -> bool {
    *f = true
    return false
}

fn mark_flag_i32(f: *mut bool) -> i32 {
    *f = true
    return 0
}

fn mark_flag_i64(f: *mut bool) -> i64 {
    *f = true
    return 0
}

fn mark_flag_usize(f: *mut bool) -> usize {
    *f = true
    return 0
}

// Did the call fail? (`r catch false` cannot tell a failure from a legitimate false)
fn failed_bool(r: DbError!bool) -> bool {
    var failed: bool = false
    let v: bool = r catch | e | (mark_flag(&failed))
    return failed
}

fn failed_i32(r: DbError!i32) -> bool {
    var failed: bool = false
    let v: i32 = r catch | e | (mark_flag_i32(&failed))
    return failed
}

fn failed_i64(r: DbError!i64) -> bool {
    var failed: bool = false
    let v: i64 = r catch | e | (mark_flag_i64(&failed))
    return failed
}

fn failed_usize(r: DbError!usize) -> bool {
    var failed: bool = false
    let v: usize = r catch | e | (mark_flag_usize(&failed))
    return failed
}
'''
open(d + 'dberr.fi', 'w').write(e)

# pager: hold_read
p = open(d + 'pager.fi').read()
p = p.replace("    n_syncs: u64,\n}\n\n// ---------------------------------------------------------------- frames",
              "    n_syncs: u64,\n    hold_read: i32, // readers of this connection are active: keep the read lock after a write ends\n}\n\n// ---------------------------------------------------------------- frames")
p = p.replace("crash: 0, n_reads: 0, n_writes: 0, n_syncs: 0,\n    }", "crash: 0, n_reads: 0, n_writes: 0, n_syncs: 0, hold_read: 0,\n    }")
old = '''    (*pg).db_touched = false
    (*pg).jcount = 0
    (*pg).jsynced = 0
    unlock_all(pg)
    (*pg).state = 0
}'''
new = '''    (*pg).db_touched = false
    (*pg).jcount = 0
    (*pg).jsynced = 0
    if (*pg).hold_read > 0 {
        unlock_to_shared(pg)
        (*pg).state = 1
    } else {
        unlock_all(pg)
        (*pg).state = 0
    }
}'''
assert old in p
p = p.replace(old, new)
open(d + 'pager.fi', 'w').write(p)

# sqlite.fi: fix the stepping and exec
s = open(d + 'sqlite.fi').read()
a = s.index("    let it2: *mut sl.SelIter = (*st).it as *mut sl.SelIter\n    let r3")
b = s.index("// Runs the statement to its next row.")
s = s[:a] + '''    let it2: *mut sl.SelIter = (*st).it as *mut sl.SelIter
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

''' + s[b:]
a = s.index("// Runs every statement of `sql`")
s = s[:a] + '''// Runs every statement of `sql` (no parameters), ignoring result rows.
fn db_exec(d: *mut dc.Db, sql: str) -> DbError!bool {
    var rest: str = sql
    while true {
        let st: *mut dc.Stmt = try db_prepare(d, rest)
        if (*st).root < 0 {
            stmt_finalize(st)
            break
        }
        var more: bool = true
        while more {
            let r: DbError!bool = stmt_step(st)
            if dberr.failed_bool(r) {
                stmt_finalize(st)
                return r
            }
            more = r catch false
        }
        let next: usize = (*st).pos
        stmt_finalize(st)
        if next >= rest.length() {
            break
        }
        rest = str { p: ((rest.p as u64) + (next as u64)) as *mut u8, n: rest.length() - next }
    }
    return true
}

// The first column of the first row as an integer (`dflt` if there is no row).
fn db_query_int(d: *mut dc.Db, sql: str, dflt: i64) -> DbError!i64 {
    let st: *mut dc.Stmt = try db_prepare(d, sql)
    let r: DbError!bool = stmt_step(st)
    if dberr.failed_bool(r) {
        stmt_finalize(st)
        try r
    }
    var out: i64 = dflt
    if (r catch false) {
        out = stmt_column_int(st, 0)
    }
    stmt_finalize(st)
    return out
}

fn db_query_text(d: *mut dc.Db, sql: str) -> DbError!str {
    let st: *mut dc.Stmt = try db_prepare(d, sql)
    let r: DbError!bool = stmt_step(st)
    if dberr.failed_bool(r) {
        stmt_finalize(st)
        try r
    }
    var out: str = ""
    if (r catch false) {
        out = __str_copy(stmt_column_text(st, 0))
    }
    stmt_finalize(st)
    return out
}
'''
open(d + 'sqlite.fi', 'w').write(s)

open(d + 'dml.fi', 'w').write('''// SPDX-License-Identifier: MPL-2.0
// lib/db/dml.fi -- INSERT, UPDATE, DELETE (stub)
import std.rt
import db.dbcore as dc
export { plan_free }
fn plan_free(st: *mut dc.Stmt) {
}
''')
open(d + 'ddl.fi', 'w').write('''// SPDX-License-Identifier: MPL-2.0
// lib/db/ddl.fi -- CREATE, DROP, ALTER, PRAGMA (stub)
import std.rt
import db.dbcore as dc
export { ddl_stub }
fn ddl_stub(st: *mut dc.Stmt) {
}
''')
print('ok12')
