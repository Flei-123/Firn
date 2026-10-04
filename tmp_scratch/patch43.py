d = '/root/firn-wt-db/lib/db/'
t = open(d + 'datetime.fi').read()
old = '''            tx.y = 0
            tx.m = 0
            tx.d = 0
            tx.valid_ymd = true
            tx.valid_jd = false
            compute_jd(&tx)
            let day_ms: i64 = tx.ijd - 211812451200000 + 0
            compute_jd(x)'''
new = '''            compute_jd(x)'''
assert old in t
t = t.replace(old, new)
old = '''                var y: DT = dt_blank()
                y.ijd = x.ijd
                y.valid_jd = true
                y.valid_ymd = true
                y.y = x.y
                y.m = 1
                y.d = 1
                y.valid_jd = false
                compute_jd(&y)'''
new = '''                var y: DT = dt_blank()
                y.valid_ymd = true
                y.y = x.y
                y.m = 1
                y.d = 1
                compute_jd(&y)'''
assert old in t
t = t.replace(old, new)
open(d + 'datetime.fi', 'w').write(t)

c = open(d + 'dbcore.fi').read()
c = c.replace("    cookie_seen: u32,\n}", "    cookie_seen: u32,\n    now_ms: i64, // the clock at the start of the running statement (date functions)\n}")
c = c.replace("max_sql: 1000000, cookie_seen: 0 }", "max_sql: 1000000, cookie_seen: 0, now_ms: 0 }")
open(d + 'dbcore.fi', 'w').write(c)

e = open(d + 'expr.fi').read()
e = e.replace("import db.vfsos as os\n", "import db.vfsos as os\nimport db.datetime as dtm\n", 1)
e = e.replace("const F_UNIXEPOCH: u32 = 35\n", "const F_UNIXEPOCH: u32 = 35\nconst F_DATE: u32 = 36\nconst F_TIME: u32 = 37\nconst F_DATETIME: u32 = 38\nconst F_JULIANDAY: u32 = 39\nconst F_STRFTIME: u32 = 40\n")
old = '''    if fname_is(p, n, "unixepoch") { if argc == 1 { return F_UNIXEPOCH as i64 } return -2 }'''
new = '''    if fname_is(p, n, "unixepoch") { return F_UNIXEPOCH as i64 }
    if fname_is(p, n, "date") { return F_DATE as i64 }
    if fname_is(p, n, "time") { return F_TIME as i64 }
    if fname_is(p, n, "datetime") { return F_DATETIME as i64 }
    if fname_is(p, n, "julianday") { return F_JULIANDAY as i64 }
    if fname_is(p, n, "strftime") { if argc >= 1 { return F_STRFTIME as i64 } return -2 }'''
assert old in e
e = e.replace(old, new)
old = '''    if f == F_UNIXEPOCH {
        return rc.val_null()
    }'''
new = '''    if f >= F_UNIXEPOCH && f <= F_STRFTIME {
        var kind: u32 = dtm.DT_UNIXEPOCH
        if f == F_DATE {
            kind = dtm.DT_DATE
        } else if f == F_TIME {
            kind = dtm.DT_TIME
        } else if f == F_DATETIME {
            kind = dtm.DT_DATETIME
        } else if f == F_JULIANDAY {
            kind = dtm.DT_JULIAN
        } else if f == F_STRFTIME {
            kind = dtm.DT_STRFTIME
        }
        var res: rc.Val = rc.val_null()
        if dtm.date_func((*cx).db, kind, ar0, argc, cxa, &res) {
            return res
        }
        return rc.val_null()
    }'''
assert old in e
e = e.replace(old, new)
# not constant: they read the clock
old = '''        if (*nd).op >= 100 || (*nd).op == F_RANDOM || (*nd).op == F_RANDOMBLOB || (*nd).op == F_LAST_INSERT_ROWID
            || (*nd).op == F_CHANGES || (*nd).op == F_TOTAL_CHANGES {
            return false
        }'''
new = '''        if (*nd).op >= 100 || (*nd).op == F_RANDOM || (*nd).op == F_RANDOMBLOB || (*nd).op == F_LAST_INSERT_ROWID
            || (*nd).op == F_CHANGES || (*nd).op == F_TOTAL_CHANGES || ((*nd).op >= F_UNIXEPOCH && (*nd).op <= F_STRFTIME) {
            return false
        }'''
assert old in e
e = e.replace(old, new)
open(d + 'expr.fi', 'w').write(e)

p = open(d + 'sqlparse.fi').read()
old = '''            if code == tk.K_CURRENT_TIME || code == tk.K_CURRENT_DATE || code == tk.K_CURRENT_TIMESTAMP {
                return unsupported("CURRENT_TIME / CURRENT_DATE / CURRENT_TIMESTAMP")
            }'''
new = '''            if code == tk.K_CURRENT_TIME || code == tk.K_CURRENT_DATE || code == tk.K_CURRENT_TIMESTAMP {
                // CURRENT_TIMESTAMP is datetime('now'), and so on
                let f: i32 = try mk(ps, ax.N_FUNC)
                let arg: i32 = try mk(ps, ax.N_STR)
                let an: *mut ax.Node = ax.ast_n((*ps).ast, arg)
                (*an).sp = "now".p as u64
                (*an).sn = 3
                let fnn: *mut ax.Node = ax.ast_n((*ps).ast, f)
                if code == tk.K_CURRENT_TIMESTAMP {
                    (*fnn).sp = "datetime".p as u64
                    (*fnn).sn = 8
                } else if code == tk.K_CURRENT_DATE {
                    (*fnn).sp = "date".p as u64
                    (*fnn).sn = 4
                } else {
                    (*fnn).sp = "time".p as u64
                    (*fnn).sn = 4
                }
                (*fnn).a = arg
                (*fnn).ival = 1
                advance(ps)
                return f
            }'''
assert old in p
p = p.replace(old, new)
old = '''    if is_kw(ps, tk.K_CURRENT_TIME) || is_kw(ps, tk.K_CURRENT_DATE) || is_kw(ps, tk.K_CURRENT_TIMESTAMP) {
        return unsupported("DEFAULT CURRENT_TIMESTAMP")
    }'''
new = '''    if is_kw(ps, tk.K_CURRENT_TIME) || is_kw(ps, tk.K_CURRENT_DATE) || is_kw(ps, tk.K_CURRENT_TIMESTAMP) {
        let pc: i32 = try parse_primary(ps)
        return pc
    }'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)

# the statement clock
s = open(d + 'sqlite.fi').read()
old = '''fn stmt_step(st: *mut dc.Stmt) -> DbError!bool {
    let d: *mut dc.Db = (*st).db
    if !(*d).is_open {
        return dberr.fail(DbError::Misuse, "the database is closed")
    }'''
new = '''fn stmt_step(st: *mut dc.Stmt) -> DbError!bool {
    let d: *mut dc.Db = (*st).db
    if !(*d).is_open {
        return dberr.fail(DbError::Misuse, "the database is closed")
    }
    if (*st).state == dc.ST_FRESH {
        (*d).now_ms = 0
    }'''
assert old in s
s = s.replace(old, new)
open(d + 'sqlite.fi', 'w').write(s)
print('ok43')
