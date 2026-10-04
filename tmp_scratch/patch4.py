d = '/root/firn-wt-db/lib/db/'
e = open(d + 'expr.fi').read()
old = "        var p2: i64 = 9223372036854775807\n"
new = "        var p2: i64 = 1000000000\n"
assert old in e
e = e.replace(old, new)
open(d + 'expr.fi', 'w').write(e)
v = open(d + 'value.fi').read()
old = '''        if x.t == 2 {
            var i: i64 = 0
            if real_to_int_exact(x.f, &i) {
                x = rc.val_int(i)
            }
        }
        *out = x
        return
    }'''
new = '''        if x.t == 2 && (*v).t == 3 {
            var i: i64 = 0
            if real_to_int_exact(x.f, &i) {
                x = rc.val_int(i)
            }
        }
        *out = x
        return
    }'''
assert old in v
v = v.replace(old, new)
open(d + 'value.fi', 'w').write(v)
print("ok4")
