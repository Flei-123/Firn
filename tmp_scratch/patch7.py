d = '/root/firn-wt-db/lib/db/'
e = open(d + 'expr.fi').read()
old = '''        if x < 0.0 {
            return rc.val_real(0.0 - math.floor(((0.0 - x) + rounder) * m) / m)
        }
        return rc.val_real(math.floor((x + rounder) * m) / m)'''
new = '''        return rc.val_real(vl.round_decimal(x, digits))'''
assert old in e
e = e.replace(old, new)
e = e.replace('''            if et.n == 1 {
                esc = rt.ld8(et.p, 0) as i64
            }''', '''            if et.n != 1 {
                return dberr.fail(DbError::Sql, "ESCAPE expression must be a single character")
            }
            esc = rt.ld8(et.p, 0) as i64''')
open(d + 'expr.fi', 'w').write(e)

v = open(d + 'value.fi').read()
v = v.replace("export {\n    text_int_prefix,", "export {\n    round_decimal, text_int_prefix,")
v += '''
// x rounded to d decimals the way SQLite's printf does: half away from zero on the
// decimal digits of the number (15 significant digits), then back to a double
fn round_decimal(x: f64, d: i64) -> f64 {
    if x == 0.0 || x != x {
        return x
    }
    var ax: f64 = x
    var neg: bool = false
    if x < 0.0 {
        ax = 0.0 - x
        neg = true
    }
    let bits: u64 = rc.bits_of_f64(ax)
    var digits: [u8; 40] = [0; 40]
    var n10: i64 = 0
    var k: usize = num.dtoa_digits(bits, (&digits[0]) as *mut u8, &n10, ws_alloc() as *mut u8)
    if k > 15 {
        var carry: bool = digits[15] >= 53 as u8
        var q: usize = 15
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
            while z < 15 {
                digits[z] = 48 as u8
                z = z + 1
            }
            n10 = n10 + 1
        }
        k = 15
    }
    let keep: i64 = n10 + d
    if keep >= (k as i64) {
        return x
    }
    var res: f64 = 0.0
    if keep < 0 {
        res = 0.0
    } else {
        var carry2: bool = digits[keep as usize] >= 53 as u8
        var kk: usize = keep as usize
        var q2: usize = kk
        while carry2 && q2 > 0 {
            q2 = q2 - 1
            if digits[q2] == 57 as u8 {
                digits[q2] = 48 as u8
            } else {
                digits[q2] = digits[q2] + 1 as u8
                carry2 = false
            }
        }
        if carry2 {
            // all nines rolled over: 0.999.. -> 1.0 at one more digit
            var z2: usize = kk
            while z2 > 0 {
                digits[z2] = digits[z2 - 1]
                z2 = z2 - 1
            }
            digits[0] = 49 as u8
            n10 = n10 + 1
            kk = kk + 1
        }
        // text "<digits>e<exponent>"
        var b: rt.Buf = rt.buf_new()
        if kk == 0 {
            rt.buf_free(&b)
            res = 0.0
        } else {
            var i: usize = 0
            while i < kk {
                rt.buf_push(&b, digits[i])
                i = i + 1
            }
            rt.buf_push(&b, 101 as u8)
            rt.buf_push_dec_i64(&b, n10 - (kk as i64))
            var f: f64 = 0.0
            num.text_to_f64(rt.buf_data(&b), rt.buf_len(&b), &f)
            rt.buf_free(&b)
            res = f
        }
    }
    if neg {
        return -res
    }
    return res
}
'''
open(d + 'value.fi', 'w').write(v)
print("ok7")
