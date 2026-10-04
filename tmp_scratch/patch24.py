d = '/root/firn-wt-db/lib/db/'
p = open(d + 'pager.fi').read()
a = p.index("// Needs a read transaction. Takes the RESERVED lock")
b = p.index("fn end_write_state(pg: *mut Pager) {")
new = '''// Takes the RESERVED lock (starting the read transaction if there is none): from now on
// nobody else can write; readers still can, until the commit.
//
// DEADLOCK RULE (SQLite's too): a connection that holds a SHARED lock and waits for
// RESERVED would keep the writer that holds RESERVED from getting EXCLUSIVE. So when the
// transaction starts from nothing, a busy RESERVED lock means: give the SHARED lock back,
// wait, start again. When a read transaction is open already the answer is Busy at once.
fn pager_begin_write(pg: *mut Pager) -> DbError!bool {
    if (*pg).state == 2 {
        return true
    }
    if (*pg).readonly {
        return dberr.fail(DbError::ReadOnly, "attempt to write a readonly database")
    }
    if (*pg).nowrite {
        return dberr.fail(DbError::Unsupported, "auto-vacuum databases are read only here")
    }
    var waited: i64 = 0
    var step: i64 = 1
    var fresh: bool = (*pg).state == 0
    while true {
        if (*pg).state == 0 {
            try pager_begin_read(pg)
        }
        let r: i64 = lock_try_reserved(pg)
        if r == 0 {
            (*pg).lock = LOCK_RESERVED
            break
        }
        if r != vfsos.OS_BUSY {
            if fresh {
                pager_end_read(pg)
            }
            return os_fail(r, "cannot lock the database file")
        }
        if !fresh || waited >= (*pg).busy_ms {
            if fresh {
                pager_end_read(pg)
            }
            return dberr.fail(DbError::Busy, "database is locked")
        }
        pager_end_read(pg)
        vfsos.os_sleep_ms(step)
        waited = waited + step
        if step < 50 {
            step = step * 2
        }
    }
    (*pg).state = 2
    (*pg).orig_pages = (*pg).db_pages
    rt.buf_clear(&(*pg).jset)
    (*pg).db_touched = false
    (*pg).jcount = 0
    (*pg).jsynced = 0
    (*pg).sj_on = false
    return true
}

'''
p = p[:a] + new + p[b:]
open(d + 'pager.fi', 'w').write(p)
print('ok24')
