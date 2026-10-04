d = '/root/firn-wt-db/lib/db/'
p = open(d + 'pager.fi').read()
p = p.replace("    pager_purge_cache, pager_is_open, pager_header_valid,", "    pager_purge_cache, pager_is_open, pager_header_valid, pager_set_cache_size,")
p += '''
// A new cache size in pages. While no transaction is open the cache is dropped and
// made again at the next one; during one the new size is used from then on.
fn pager_set_cache_size(pg: *mut Pager, n: usize) {
    (*pg).cache_req = n
    if (*pg).state == 0 && (*pg).cache_mem != 0 {
        cache_free(pg)
        (*pg).have_header = false
    }
}
'''
open(d + 'pager.fi', 'w').write(p)
dd = open(d + 'ddl.fi').read()
old = '''        if n < 64 {
            n = 64
        }
        (*db).pg.cache_req = n as usize
        return true'''
new = '''        if n < 64 {
            n = 64
        }
        pgr.pager_set_cache_size(&(*db).pg, n as usize)
        return true'''
assert old in dd
dd = dd.replace(old, new)
open(d + 'ddl.fi', 'w').write(dd)
s = open(d + 'sqlite.fi').read()
s = s.replace("    (*d).pg.cache_req = n\n}", "    pgr.pager_set_cache_size(&(*d).pg, n)\n}")
open(d + 'sqlite.fi', 'w').write(s)
print('ok21')
