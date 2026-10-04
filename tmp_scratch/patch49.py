d = '/root/firn-wt-db/lib/db/'
t = open(d + 'datetime.fi').read()
old = '''    } else if !valid_jd((*x).ijd) {
        (*x).is_error = true
    } else {
        let z: i64 = ((*x).ijd + 43200000) / 86400000'''
new = '''    } else if !valid_jd((*x).ijd) {
        (*x).is_error = true
    } else {
        // a raw number that is a valid Julian day has become a date
        (*x).raw_s = false
        let z: i64 = ((*x).ijd + 43200000) / 86400000'''
assert old in t
t = t.replace(old, new)
open(d + 'datetime.fi', 'w').write(t)
print('ok49')
