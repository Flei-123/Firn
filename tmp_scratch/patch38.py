p = '/root/firn-wt-db/lib/db/pager.fi'
s = open(p).read()
old = '''        if pgno != 0 && pgno <= (*pg).sj_pages {
            let i: usize = try pager_frame(pg, pgno, false)'''
new = '''        if pgno != 0 && pgno <= (*pg).sj_pages {
            pager_next_op(pg)
            let i: usize = try pager_frame(pg, pgno, false)'''
assert old in s
s = s.replace(old, new)
open(p, 'w').write(s)
print('ok38')
