d = '/root/firn-wt-db/lib/db/'
t = open(d + 'datetime.fi').read()
t = t.replace("    if y < -4713 || y > 9999 {\n        (*x).is_error = true\n        return\n    }", "    if y < -4713 || y > 9999 || (*x).raw_s {\n        (*x).is_error = true\n        return\n    }")
old = '''    compute_jd(x)
    if (*x).is_error || !valid_jd((*x).ijd) {
        return false
    }
    return true
}'''
new = '''    compute_jd(x)
    if (*x).is_error || !valid_jd((*x).ijd) {
        return false
    }
    // a raw number has become a time: the later conversions (the day of the year) may start from it
    (*x).raw_s = false
    return true
}'''
assert old in t
t = t.replace(old, new)
open(d + 'datetime.fi', 'w').write(t)
print('ok48')
