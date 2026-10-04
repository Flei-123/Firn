d = '/root/firn-wt-db/lib/db/'
s = open(d + 'select.fi').read()
s = s.replace("rt.st32(rt.buf_ptr(b) + (at as u64), 0, v as u32)", "rt.st32(rt.buf_ptr(b) + (at as u64), 0, v as% u32)")
s = s.replace("return rt.ld32(rt.buf_ptr(b), i) as i32", "return rt.ld32(rt.buf_ptr(b), i) as% i32")
open(d + 'select.fi', 'w').write(s)
p = open(d + 'plan.fi').read()
old = '''    var i: usize = 0
    while i < got {
        let v: *mut rc.Val = vals + i
        if ((*s).used[i / 64] >> ((i % 64) as u64)) & 1 == 0 {
            *v = rc.val_null()
        } else if (*v).t == 3 || (*v).t == 4 {
            (*v).p = ar.arena_copy(&(*s).arena, (*v).p, (*v).n)
        }
        i = i + 1
    }'''
new = '''    var i: usize = 0
    while i < got {
        let v: *mut rc.Val = vals + i
        if ((*s).used[i / 64] >> ((i % 64) as u64)) & 1 == 0 {
            *v = rc.val_null()
        } else if (*v).t == 3 || (*v).t == 4 {
            (*v).p = ar.arena_copy(&(*s).arena, (*v).p, (*v).n)
        } else if (*v).t == 1 {
            // a whole REAL is stored as an integer: a REAL column reads it back as a REAL
            let col: *mut dc.Column = ((*t).cols as *mut dc.Column) + i
            if (*col).aff == 4 {
                *v = rc.val_real((*v).i as f64)
            }
        }
        i = i + 1
    }'''
assert old in p
p = p.replace(old, new)
open(d + 'plan.fi', 'w').write(p)
print('ok14')
