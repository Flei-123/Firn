d = '/root/firn-wt-db/lib/db/'
t = open(d + 'datetime.fi').read()
old = '''    if kind == DT_DATE || kind == DT_DATETIME {
        put_year(&b, x.y)'''
new = '''    if kind == DT_DATE || kind == DT_DATETIME {
        if x.y < 0 {
            rt.buf_push(&b, 45 as u8)
            put_n(&b, 0 - x.y, 4)
        } else {
            put_n(&b, x.y, 4)
        }'''
assert old in t
t = t.replace(old, new)
open(d + 'datetime.fi', 'w').write(t)
print('ok51')
