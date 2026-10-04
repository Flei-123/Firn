d = '/root/firn-wt-db/lib/db/'
c = open(d + 'dbcore.fi').read()
c += '''
fn schema_add_table(s: *mut Schema, t: *mut Table) {
    vec_push[Table](&(*s).tables, *t)
}

fn schema_add_index(s: *mut Schema, x: *mut Index) {
    vec_push[Index](&(*s).indexes, *x)
}
'''
c = c.replace("sz_of_column, MAX_SRC,", "sz_of_column, schema_add_table, schema_add_index, MAX_SRC,")
open(d + 'dbcore.fi', 'w').write(c)
s = open(d + 'schema.fi').read()
a = s.index("fn mark_bad_table")
b = s.index("fn table_blank")
s = s[:a] + s[b:]
s = s.replace('''    // the master table itself
    {
        var nm: rc.Val = rc.val_text("sqlite_master".p as u64, 13)
        var sq: rc.Val = rc.val_text(MASTER_SQL.p as u64, MASTER_SQL.length())
        try load_table_row(db, 0, &nm, 1, &sq)
        var nm2: rc.Val = rc.val_text("sqlite_schema".p as u64, 13)
        try load_table_row(db, 0, &nm2, 1, &sq)
    }''', '''    // the master table itself
    var nm: rc.Val = rc.val_text("sqlite_master".p as u64, 13)
    var sq: rc.Val = rc.val_text(MASTER_SQL.p as u64, MASTER_SQL.length())
    try load_table_row(db, 0, &nm, 1, &sq)
    var nm2: rc.Val = rc.val_text("sqlite_schema".p as u64, 13)
    try load_table_row(db, 0, &nm2, 1, &sq)''')
s = s.replace("fn ieq(p: u64, n: usize, w: str) -> bool {", "fn ieq(p: u64, n: usize, w: str) -> bool {")
open(d + 'schema.fi', 'w').write(s)
print('ok11')
