d = '/root/firn-wt-db/lib/db/'
t = open(d + 'datetime.fi').read()
old = '''        if x.y < 0 {
            rt.buf_push(&b, 45 as u8)
            put_n(&b, 0 - x.y, 4)
        } else {
            put_n(&b, x.y, 4)
        }'''
new = '''        put_year(&b, x.y)'''
assert old in t
t = t.replace(old, new)
t = t.replace('''            } else if k == 89 as u8 {
                put_n(&b, x.y, 4)''', '''            } else if k == 89 as u8 {
                put_year(&b, x.y)''')
t = t.replace("fn put2(b: *mut rt.Buf, v: i64) {", '''// "%04d" of a year: a minus sign counts into the width (-001)
fn put_year(b: *mut rt.Buf, y: i64) {
    if y < 0 {
        rt.buf_push(b, 45 as u8)
        put_n(b, 0 - y, 3)
    } else {
        put_n(b, y, 4)
    }
}

fn put2(b: *mut rt.Buf, v: i64) {''', 1)
open(d + 'datetime.fi', 'w').write(t)
print('ok50')
