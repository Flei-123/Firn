p = '/root/firn-wt-db/tests/2160_db_btree.fi'
s = open(p).read()
s = s.replace('''    check(t, "a deleted row is gone", !(bx.bt_table_find(&bt, troot, rowid_of(1), &Payload0()) catch true))''', '''    var gone: bx.Payload = bx.Payload { p: 0, n: 0 }
    check(t, "a deleted row is gone", !(bx.bt_table_find(&bt, troot, rowid_of(1), &gone) catch true))''')
s = s.replace('''        let pl2: bx.Payload = bx.bt_payload(&bt, &icur) catch bx.Payload { p: 0, n: 0 }''', '''        let pl2: bx.Payload = bx.bt_payload(&bt, &icur) catch gone''')
s = s.replace('''fn Payload0() -> bx.Payload {
    return bx.Payload { p: 0, n: 0 }
}

''', '')
open(p, 'w').write(s)
print('ok36')
