d = '/root/firn-wt-db/lib/db/'
v = open(d + 'value.fi').read()
old = '''    if neg {
        rt.buf_push(out, 45 as u8)
    }
    if (bits & 9223372036854775807) == 0 {
        ut.put_text(out, "0.0")
        return
    }'''
new = '''    if (bits & 9223372036854775807) == 0 {
        ut.put_text(out, "0.0")
        return
    }
    if neg {
        rt.buf_push(out, 45 as u8)
    }'''
assert old in v
v = v.replace(old, new)
open(d + 'value.fi', 'w').write(v)
print("ok3")
