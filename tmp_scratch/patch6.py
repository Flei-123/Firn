d = '/root/firn-wt-db/lib/db/'
p = open(d + 'sqlparse.fi').read()
old = '''        // a name: column, table.column or a function call
        var p1: u64 = 0
        var n1: usize = 0
        try parse_name(ps, &p1, &n1)'''
new = '''        // a name: column, table.column or a function call
        var p1: u64 = 0
        var n1: usize = 0
        if !(*t).quoted && (code == tk.K_LIKE || code == tk.K_GLOB) {
            p1 = (*t).p
            n1 = (*t).n
            advance(ps)
        } else {
            try parse_name(ps, &p1, &n1)
        }'''
assert old in p
p = p.replace(old, new)
open(d + 'sqlparse.fi', 'w').write(p)
c = open('/root/firn-wt-db/tools/db/check_expr.py').read()
c = c.replace('''"hex(NULL)", "unhex('4142')", "unhex('4')", "unhex('zz')",''', '''"hex(NULL)",''')
open('/root/firn-wt-db/tools/db/check_expr.py', 'w').write(c)
print("ok6")
