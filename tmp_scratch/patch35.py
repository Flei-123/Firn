p = '/root/firn-wt-db/lib/db/dml.fi'
s = open(p).read()
helper = '''
// "table t has 3 columns but 2 values were supplied" / "2 values for 3 columns"
fn count_msg(t: *mut dc.Table, explicit: bool, expected: usize, got: usize) -> str {
    var b: rt.Buf = rt.buf_new()
    if explicit {
        rt.buf_push_dec_u64(&b, got as u64)
        ut.put_text(&b, " values for ")
        rt.buf_push_dec_u64(&b, expected as u64)
        ut.put_text(&b, " columns")
    } else {
        ut.put_text(&b, "table ")
        rt.buf_push_bytes(&b, (*t).name_p, (*t).name_n)
        ut.put_text(&b, " has ")
        rt.buf_push_dec_u64(&b, expected as u64)
        ut.put_text(&b, " columns but ")
        rt.buf_push_dec_u64(&b, got as u64)
        ut.put_text(&b, " values were supplied")
    }
    let s: str = __str_copy(str { p: rt.buf_data(&b), n: rt.buf_len(&b) })
    rt.buf_free(&b)
    return s
}
'''
s = s.replace("// ------------------------------------------------------------------ INSERT\n", "// ------------------------------------------------------------------ INSERT\n" + helper, 1)
s = s.replace('''            if cnt != nmapped {
                return dberr.fail(DbError::Sql, text_msg2("table ", (*t).name_p, (*t).name_n, " has a different number of columns than values supplied", 0, 0))
            }''', '''            if cnt != nmapped {
                return dberr.fail(DbError::Sql, count_msg(t, (*ins).a >= 0, nmapped, cnt))
            }''')
s = s.replace('''                    return dberr.fail(DbError::Sql, text_msg2("table ", (*t).name_p, (*t).name_n, " has fewer columns than values supplied", 0, 0))''',
              '''                    return dberr.fail(DbError::Sql, count_msg(t, (*ins).a >= 0, nmapped, cnt + 1))''')
s = s.replace('''            return dberr.fail(DbError::Sql, text_msg2("table ", (*t).name_p, (*t).name_n, " has a different number of columns than the SELECT supplies", 0, 0))''',
              '''            return dberr.fail(DbError::Sql, count_msg(t, (*ins).a >= 0, nmapped, ncol))''')
open(p, 'w').write(s)
print('ok35')
