p = '/root/firn-wt-db/tools/db/bt_probe.fi'
s = open(p).read()
s = s.replace('''fn rowid_of(i: i64) -> i64 {
    return ((i * 2654435761) % 4294967296) + 1
}''', '''static mut seq_mode: bool = false

fn rowid_of(i: i64) -> i64 {
    if seq_mode {
        return i + 1
    }
    return ((i * 2654435761) % 4294967296) + 1
}''')
s = s.replace('''    let cmd: str = arg(start, 1)
    let path: str = arg(start, 2)''', '''    var cmd: str = arg(start, 1)
    // inss / dels: the same, with the rowids in sequence (appends to the right-most leaf)
    if cmd.equal("inss") {
        seq_mode = true
        cmd = "ins"
    } else if cmd.equal("dels") {
        seq_mode = true
        cmd = "del"
    }
    let path: str = arg(start, 2)''')
open(p, 'w').write(s)

c = '/root/firn-wt-db/tools/db/check_bt.py'
t = open(c).read()
t = t.replace('''def rowid_of(i):
    return (i * 2654435761) % 4294967296 + 1''', '''SEQ = False

def rowid_of(i):
    if SEQ:
        return i + 1
    return (i * 2654435761) % 4294967296 + 1''')
t = t.replace('''with tempfile.TemporaryDirectory() as d:
    for page_size in (512, 1024, 4096, 65536):''', '''def cmdname(c):
    return c + "s" if SEQ else c

with tempfile.TemporaryDirectory() as d:
  for SEQ in (False, True):
    for page_size in (512, 1024, 4096, 65536):''')
# indent the body of the loops by two spaces: easier to rewrite the loop explicitly
open(c, 'w').write(t)
print('ok42')
