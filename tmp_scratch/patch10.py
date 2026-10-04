d = '/root/firn-wt-db/lib/db/'
s = open(d + 'select.fi').read()
s = s.replace("        rt.buf_free(&(((rt.buf_ptr(&accs)) as *mut Acc) + fa).cat)\n",
              "        let fap: *mut Acc = ((rt.buf_ptr(&accs)) as *mut Acc) + fa\n        rt.buf_free(&(*fap).cat)\n")
open(d + 'select.fi', 'w').write(s)
print('ok10')
