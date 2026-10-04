d = '/root/firn-wt-db/lib/db/'
b = open(d + 'btree.fi').read()

# ---- C. the greedy packing reads the prefix sums instead of asking the list again and again
old = '''    while true {
        var sum: usize = 0
        var t: usize = s
        while t < total_n && sum + cl_cost(g, t) <= cap {
            sum = sum + cl_cost(g, t)
            t = t + 1
        }
        if t == total_n {
            break
        }'''
new = '''    while true {
        var t: usize = s
        let base: u64 = util.rd64(pre, s * 8)
        while t < total_n && (util.rd64(pre, (t + 1) * 8) - base) <= (cap as u64) {
            t = t + 1
        }
        if t == total_n {
            break
        }'''
assert old in b
b = b.replace(old, new)

# ---- A. a lean key reader for the binary search of a table tree
old = '''// The whole payload of a cell: a pointer into the page if it is all local,'''
new = '''// The rowid of cell `off` of a table page (leaf or interior) without the rest of the parse.
// Interior: [child 4][rowid varint]; leaf: [payload size varint][rowid varint]...
#[inline]
fn table_key_at(bt: *mut Bt, info: *mut PageInfo, off: usize, key: *mut i64) -> bool {
    let u: usize = usable_of(bt)
    let d: u64 = (*info).d
    var p: usize = off
    if !(*info).leaf {
        p = off + 4
    } else {
        // skip the payload size
        var i: usize = 0
        while p < u && i < 9 {
            let b: u64 = b8(d, p)
            p = p + 1
            i = i + 1
            if b < 128 {
                break
            }
        }
    }
    if p >= u {
        return false
    }
    let b0: u64 = b8(d, p)
    if b0 < 128 {
        *key = b0 as i64
        return true
    }
    var v: u64 = 0
    let n: usize = util.varint_get(d + (p as u64), u - p, &v)
    if n == 0 {
        return false
    }
    *key = v as i64
    return true
}

// The whole payload of a cell: a pointer into the page if it is all local,'''
assert old in b
b = b.replace(old, new, 1)

# descend_table and bt_table_seek use it
old = '''        while lo < hi {
            let mid: usize = (lo + hi) / 2
            let o: usize = try cell_off(bt, info, mid)
            try cell_parse(bt, info, o, &ci)
            if ci.key < rowid {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        if (*info).leaf {
            (*cur).idx[dp] = lo as i32
            *found = false'''
new = '''        while lo < hi {
            let mid: usize = (lo + hi) / 2
            let o: usize = try cell_off(bt, info, mid)
            var key: i64 = 0
            if !table_key_at(bt, info, o, &key) {
                return dberr.fail(DbError::Corrupt, "cell runs past the end of the page")
            }
            if key < rowid {
                lo = mid + 1
            } else {
                hi = mid
            }
        }
        if (*info).leaf {
            (*cur).idx[dp] = lo as i32
            *found = false'''
assert old in b
b = b.replace(old, new)

old = '''            var key: i64 = 0
            if info.leaf {
                try cell_parse(bt, &info, o, &ci)
                key = ci.key
            } else {
                try cell_parse(bt, &info, o, &ci)
                key = ci.key
            }
            var before: bool = key < rowid'''
new = '''            var key: i64 = 0
            if !table_key_at(bt, &info, o, &key) {
                return dberr.fail(DbError::Corrupt, "cell runs past the end of the page")
            }
            var before: bool = key < rowid'''
assert old in b
b = b.replace(old, new)

# ---- B. balance_quick: appending past the last cell of the right-most leaf of a table
old = '''    // overflow: the page content plus the new cell becomes the pending list
    cl_clear(&(*bt).pend)'''
new = '''    // appending to the right-most leaf of a table tree: the new cell gets a page of its own and
    // the old page stays full (SQLite's balance_quick); the divider goes to the parent
    if (*info).flags == 13 && idx == (*info).ncells && dp > 0 {
        var rightmost: bool = true
        var lv: usize = 0
        while lv < dp {
            let ppno: u32 = (*cur).pages[lv]
            let pdq: u64 = try pager.pager_get(pg, ppno)
            var pinfq: PageInfo = pinfo_new()
            try page_info(bt, ppno, pdq, &pinfq)
            if (*cur).idx[lv] as usize != pinfq.ncells {
                rightmost = false
            }
            lv = lv + 1
        }
        if rightmost {
            let ppno2: u32 = (*cur).pages[dp - 1]
            let pd2: u64 = try pager.pager_get(pg, ppno2)
            var pinf2: PageInfo = pinfo_new()
            try page_info(bt, ppno2, pd2, &pinf2)
            // the divider: [old leaf][rowid of its last cell]
            var lastkey: i64 = 0
            let lo: usize = try cell_off(bt, info, (*info).ncells - 1)
            if !table_key_at(bt, info, lo, &lastkey) {
                return dberr.fail(DbError::Corrupt, "cell runs past the end of the page")
            }
            var dv: [u8; 16] = [0; 16]
            w32((&dv[0]) as u64, 0, (*info).pgno as u64)
            let dn: usize = util.varint_put(((&dv[0]) as u64) + 4, lastkey as u64)
            let pwd: u64 = try pager.pager_write(pg, ppno2)
            pinf2.d = pwd
            let room: bool = try page_try_insert(bt, &pinf2, pinf2.ncells, (&dv[0]) as u64, 4 + dn)
            if room {
                let np: u32 = try pager.pager_alloc_page(pg)
                // the new leaf holds just the new cell
                var one: CellList = cl_new()
                cl_push(&one, cell, len)
                let nd: u64 = try pager.pager_write(pg, np)
                let br: DbError!bool = page_build(bt, np, nd, 13, &one, 0, 1, 0)
                cl_free(&one)
                try br
                // the parent's right pointer
                let pwd2: u64 = try pager.pager_write(pg, ppno2)
                w32(pwd2, pinf2.h + 8, np as u64)
                return true
            }
        }
    }
    // overflow: the page content plus the new cell becomes the pending list
    cl_clear(&(*bt).pend)'''
assert old in b
b = b.replace(old, new)
open(d + 'btree.fi', 'w').write(b)
print('ok41')
