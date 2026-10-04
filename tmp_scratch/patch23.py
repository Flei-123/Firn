p = '/root/firn-wt-db/tools/db/sql_probe.fi'
s = open(p).read()
old = '''        var out: rt.Buf = rt.buf_new()
        let r: DbError!bool = run_line(d, line, &out)'''
new = '''        if line.chars(0) == 46 as u8 {
            // meta commands: .wait  .sleep N  .busy N  .echo TEXT
            if line.length() >= 5 && line.chars(1) == 119 as u8 {
                var b: [u8; 1] = [0]
                while true {
                    let got: i64 = syscall(0, 0, ((&b[0]) as u64) as i64, 1, 0, 0, 0)
                    if got <= 0 || b[0] == 10 as u8 {
                        break
                    }
                }
                io.print_line("WAITED")
            } else if line.chars(1) == 115 as u8 {
                var ms: i64 = 0
                var q: usize = 7
                while q < line.length() {
                    ms = ms * 10 + ((line.chars(q) as i64) - 48)
                    q = q + 1
                }
                var ts: [i64; 2] = [ms / 1000, (ms % 1000) * 1000000]
                syscall(35, ((&ts[0]) as u64) as i64, 0, 0, 0, 0, 0)
            } else if line.chars(1) == 98 as u8 {
                var ms2: i64 = 0
                var q2: usize = 6
                while q2 < line.length() {
                    ms2 = ms2 * 10 + ((line.chars(q2) as i64) - 48)
                    q2 = q2 + 1
                }
                sq.db_set_busy_timeout(d, ms2)
            } else {
                io.print_bytes((line.p as u64) + 6, line.length() - 6)
                io.new_line()
            }
            continue
        }
        var out: rt.Buf = rt.buf_new()
        let r: DbError!bool = run_line(d, line, &out)'''
assert old in s
s = s.replace(old, new)
open(p, 'w').write(s)
print('ok23')
