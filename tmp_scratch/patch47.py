d = '/root/firn-wt-db/lib/db/'
t = open(d + 'datetime.fi').read()
t = t.replace("    if y < -4713 || y > 9999 || (*x).raw_s {\n        (*x).is_error = true\n        return\n    }", "    if y < -4713 || y > 9999 {\n        (*x).is_error = true\n        return\n    }")
# %J: exact for a Julian day with seven integer digits (every date 0000..9999)
old = "                vl.fmt_g16(&b, (x.ijd as f64) / 86400000.0)"
new = "                put_julian(&b, (x.ijd as f64) / 86400000.0)"
assert old in t
t = t.replace(old, new)
t = t.replace("fn put2(b: *mut rt.Buf, v: i64) {", '''// "%.16g" of a Julian day. A value of seven integer digits (every date from the year 0 to 9999)
// has nine decimals; they are cut from the EXACT binary value with integer arithmetic (rounded to
// nearest, ties to even, like printf), so the last digit is right. Other values: the shortest form.
fn put_julian(b: *mut rt.Buf, v: f64) {
    if v >= 1000000.0 && v < 10000000.0 {
        let bits: u64 = rc.bits_of_f64(v)
        let e: u64 = (bits >> 52) & 2047
        let m: u64 = (bits & 4503599627370495) | 4503599627370496
        let shift: u64 = 1075 - e
        var whole: u64 = m >> shift
        let fnum: u64 = m & ((1 << shift) - 1)
        let t: u64 = fnum * 1000000000
        var q: u64 = t >> shift
        let r: u64 = t & ((1 << shift) - 1)
        let half: u64 = 1 << (shift - 1)
        if r > half || (r == half && (q & 1) == 1) {
            q = q + 1
        }
        if q >= 1000000000 {
            q = q - 1000000000
            whole = whole + 1
        }
        rt.buf_push_dec_u64(b, whole)
        if q != 0 {
            var digs: [u8; 9] = [48; 9]
            var i: usize = 9
            var z: u64 = q
            while i > 0 {
                i = i - 1
                digs[i] = (48 + z % 10) as u8
                z = z / 10
            }
            var last: usize = 9
            while last > 0 && digs[last - 1] == 48 as u8 {
                last = last - 1
            }
            rt.buf_push(b, 46 as u8)
            var k: usize = 0
            while k < last {
                rt.buf_push(b, digs[k])
                k = k + 1
            }
        }
        return
    }
    vl.fmt_g16(b, v)
}

fn put2(b: *mut rt.Buf, v: i64) {''', 1)
open(d + 'datetime.fi', 'w').write(t)
print('ok47')
