p = '/root/firn-wt-db/tools/db/gen_test.py'
s = open(p).read()
old = '''    if isinstance(v, float):
        return repr(v) if v != int(v) or abs(v) >= 1e15 else str(v)
    return str(v)'''
new = '''    if isinstance(v, float):
        # SQLite's text form of a REAL: "%!.15g"
        t = "%.15g" % v
        if "e" in t:
            m, e = t.split("e")
            if "." not in m:
                m += ".0"
            return m + "e" + e
        if "." not in t and "inf" not in t and "nan" not in t:
            t += ".0"
        return t
    return str(v)'''
assert old in s
s = s.replace(old, new)
open(p, 'w').write(s)
print('ok34')
