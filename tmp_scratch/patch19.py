d = '/root/firn-wt-db/lib/db/'
e = open(d + 'expr.fi').read()
e = e.replace("    hidden_rowid: bool, // the rowid can be named (tables, not sub-selects)\n}",
              "    hidden_rowid: bool, // the rowid can be named (tables, not sub-selects)\n    qual_only: bool, // only found when written with its name (`excluded`)\n}")
e = e.replace("rowid_col: -1, hidden_rowid: false }", "rowid_col: -1, hidden_rowid: false, qual_only: false }")
old = '''            if (*nd).sn2 > 0 {
                if !(dc.name_eq((*si).alias_p, (*si).alias_n, (*nd).sp2, (*nd).sn2)) {
                    i = i + 1
                    continue
                }
            }'''
new = '''            if (*nd).sn2 > 0 {
                if !(dc.name_eq((*si).alias_p, (*si).alias_n, (*nd).sp2, (*nd).sn2)) {
                    i = i + 1
                    continue
                }
            } else if (*si).qual_only {
                i = i + 1
                continue
            }'''
assert old in e
e = e.replace(old, new)
open(d + 'expr.fi', 'w').write(e)
m = open(d + 'dml.fi').read()
m = m.replace('''        (*ix).hidden_rowid = true
        (*e).scope.n = 2''', '''        (*ix).hidden_rowid = true
        (*ix).qual_only = true
        (*e).scope.n = 2''')
open(d + 'dml.fi', 'w').write(m)
print('ok19')
