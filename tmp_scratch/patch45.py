d = '/root/firn-wt-db/lib/db/'
v = open(d + 'value.fi').read()
v = v.replace("export {\n    round_decimal,", "export {\n    fmt_g16, round_decimal,")
v += '''
// C's "%.16g" (what strftime's %J prints): 16 significant digits, no forced ".0"
fn fmt_g16(out: *mut rt.Buf, f: f64) {
    if f != f || f > 1.7976931348623157e308 || f < -1.7976931348623157e308 {
        fmt_real(out, f)
        return
    }
    let bits: u64 = rc.bits_of_f64(f)
    if (bits & 9223372036854775807) == 0 {
        rt.buf_push(out, 48 as u8)
        return
    }
    if (bits >> 63) != 0 {
        rt.buf_push(out, 45 as u8)
    }
    var digits: [u8; 40] = [0; 40]
    var n10: i64 = 0
    var k: usize = num.dtoa_digits(bits, (&digits[0]) as *mut u8, &n10, ws_alloc() as *mut u8)
    var e10: i64 = n10 - 1
    if k > 16 {
        var carry: bool = digits[16] >= 53 as u8
        var q: usize = 16
        while carry && q > 0 {
            q = q - 1
            if digits[q] == 57 as u8 {
                digits[q] = 48 as u8
            } else {
                digits[q] = digits[q] + 1 as u8
                carry = false
            }
        }
        if carry {
            digits[0] = 49 as u8
            var z: usize = 1
            while z < 16 {
                digits[z] = 48 as u8
                z = z + 1
            }
            e10 = e10 + 1
        }
        k = 16
    }
    while k > 1 && digits[k - 1] == 48 as u8 {
        k = k - 1
    }
    if e10 < -4 || e10 >= 16 {
        rt.buf_push(out, digits[0])
        if k > 1 {
            rt.buf_push(out, 46 as u8)
            var q: usize = 1
            while q < k {
                rt.buf_push(out, digits[q])
                q = q + 1
            }
        }
        rt.buf_push(out, 101 as u8)
        var ee: i64 = e10
        if ee < 0 {
            rt.buf_push(out, 45 as u8)
            ee = 0 - ee
        } else {
            rt.buf_push(out, 43 as u8)
        }
        if ee < 10 {
            rt.buf_push(out, 48 as u8)
        }
        rt.buf_push_dec_i64(out, ee)
        return
    }
    if e10 >= 0 {
        var q: usize = 0
        var i: i64 = 0
        while i <= e10 {
            if q < k {
                rt.buf_push(out, digits[q])
                q = q + 1
            } else {
                rt.buf_push(out, 48 as u8)
            }
            i = i + 1
        }
        if q < k {
            rt.buf_push(out, 46 as u8)
            while q < k {
                rt.buf_push(out, digits[q])
                q = q + 1
            }
        }
        return
    }
    ut.put_text(out, "0.")
    var z: i64 = 0
    while z < (0 - e10) - 1 {
        rt.buf_push(out, 48 as u8)
        z = z + 1
    }
    var q: usize = 0
    while q < k {
        rt.buf_push(out, digits[q])
        q = q + 1
    }
}
'''
open(d + 'value.fi', 'w').write(v)

t = open(d + 'datetime.fi').read()
t = t.replace("vl.fmt_real(&b, (x.ijd as f64) / 86400000.0)", "vl.fmt_g16(&b, (x.ijd as f64) / 86400000.0)")
old = '''                var y: DT = dt_blank()
                y.valid_ymd = true
                y.y = x.y
                y.m = 1
                y.d = 1
                compute_jd(&y)'''
new = '''                var y: DT = x
                y.valid_jd = false
                y.m = 1
                y.d = 1
                compute_jd(&y)'''
assert old in t
t = t.replace(old, new)
old = '''                var y2: DT = dt_blank()
                y2.valid_ymd = true
                y2.y = x.y
                y2.m = 1
                y2.d = 1
                compute_jd(&y2)'''
new = '''                var y2: DT = x
                y2.valid_jd = false
                y2.m = 1
                y2.d = 1
                compute_jd(&y2)'''
assert old in t
t = t.replace(old, new)
# modifier index: auto, julianday, unixepoch only as the first modifier
t = t.replace("fn parse_modifier(p: u64, n: usize, x: *mut DT) -> bool {", "fn parse_modifier(p: u64, n: usize, x: *mut DT, idx: usize) -> bool {")
t = t.replace("if !parse_modifier((*a).p, (*a).n, x) {", "if !parse_modifier((*a).p, (*a).n, x, i - first) {")
old = '''    if c0 == 97 as u8 && eq_ci(p, n, "auto") {
        if (*x).raw_s {
            let r: f64 = (*x).s * 1000.0 + 210866760000000.0
            if r >= 0.0 && r < 464269060800000.0 {
                clear_ymd_hms_tz(x)
                (*x).ijd = (r + 0.5) as i64
                (*x).valid_jd = true
                (*x).raw_s = false
                return true
            }
            if (*x).s >= 0.0 && (*x).s < 5373484.5 {
                (*x).raw_s = false
                return true
            }
        }
        return false
    }
    if c0 == 106 as u8 && eq_ci(p, n, "julianday") {
        if (*x).raw_s && (*x).valid_jd {
            (*x).raw_s = false
            return true
        }
        return false
    }'''
new = '''    if c0 == 97 as u8 && eq_ci(p, n, "auto") {
        if idx > 1 {
            return false
        }
        if !(*x).raw_s || (*x).valid_jd {
            (*x).raw_s = false
            return true
        }
        if (*x).s >= -210866760000.0 && (*x).s <= 253402300799.0 {
            let r: f64 = (*x).s * 1000.0 + 210866760000000.0
            clear_ymd_hms_tz(x)
            (*x).ijd = (r + 0.5) as i64
            (*x).valid_jd = true
            (*x).raw_s = false
            return true
        }
        return false
    }
    if c0 == 106 as u8 && eq_ci(p, n, "julianday") {
        if idx > 1 {
            return false
        }
        if (*x).raw_s && (*x).valid_jd {
            (*x).raw_s = false
            return true
        }
        return false
    }'''
assert old in t
t = t.replace(old, new)
old = '''    if c0 == 117 as u8 && eq_ci(p, n, "unixepoch") {
        if (*x).raw_s {'''
new = '''    if c0 == 117 as u8 && eq_ci(p, n, "unixepoch") {
        if idx > 1 {
            return false
        }
        if (*x).raw_s {'''
assert old in t
t = t.replace(old, new)
open(d + 'datetime.fi', 'w').write(t)
print('ok45')
