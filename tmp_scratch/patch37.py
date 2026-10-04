p = '/root/firn-wt-db/tests/2160_db_btree.fi'
s = open(p).read()
s = s.replace('    var gone: bx.Payload = bx.Payload { p: 0, n: 0 }\n    check(t, "a deleted row is gone"', '    check(t, "a deleted row is gone"')
s = s.replace('    var irec: rt.Buf = rt.buf_new()\n    // ---- fill', '    var irec: rt.Buf = rt.buf_new()\n    var gone: bx.Payload = bx.Payload { p: 0, n: 0 }\n    // ---- fill')
open(p, 'w').write(s)
print('ok37')
