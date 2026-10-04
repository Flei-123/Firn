p = '/root/firn-wt-db/tools/db/check_expr.py'
s = open(p).read()
old = '''            print(f"DIFF {e!r}: sqlite={want} mine={got}")'''
new = '''            def show(v):
                if v.startswith("T:"):
                    try:
                        return "T:" + repr(bytes.fromhex(v[2:]).decode("utf-8"))
                    except Exception:
                        return v
                return v
            print(f"DIFF {e!r}: sqlite={show(want)} mine={show(got)}")'''
assert old in s
s = s.replace(old, new)
open(p, 'w').write(s)
print('ok46')
