import re
d = '/root/firn-wt-db/lib/db/'
v = open(d + 'value.fi').read()
v = v.replace('''// CAST(.. AS INTEGER): reals truncate (saturating), text uses its prefix
fn val_to_int(v: *mut rc.Val) -> i64 {
    var x: rc.Val = rc.val_null()
    if !to_number(v, &x) {
        return 0
    }
    if x.t == 1 {
        return x.i
    }
    let f: f64 = x.f''', '''// the integer prefix of a text (sign and digits, saturating): "12abc" is 12, "1e2" is 1
fn text_int_prefix(p: u64, n: usize) -> i64 {
    var i: usize = 0
    while i < n && is_ws(rt.ld8(p, i)) {
        i = i + 1
    }
    var neg: bool = false
    if i < n && (rt.ld8(p, i) == 43 as u8 || rt.ld8(p, i) == 45 as u8) {
        neg = rt.ld8(p, i) == 45 as u8
        i = i + 1
    }
    var v: u64 = 0
    var over: bool = false
    while i < n && is_dig(rt.ld8(p, i)) {
        let dg: u64 = (rt.ld8(p, i) as u64) - 48
        if v > 922337203685477580 || (v == 922337203685477580 && dg > 7) {
            over = true
        } else if !over {
            v = v * 10 + dg
        }
        i = i + 1
    }
    if over {
        if neg {
            return int_min()
        }
        return 9223372036854775807
    }
    if neg {
        return 0 - (v as i64)
    }
    return v as i64
}

// CAST(.. AS INTEGER): reals are cut towards zero (saturating), text uses its integer prefix
fn val_to_int(v: *mut rc.Val) -> i64 {
    if (*v).t == 3 || (*v).t == 4 {
        return text_int_prefix((*v).p, (*v).n)
    }
    var x: rc.Val = rc.val_null()
    if !to_number(v, &x) {
        return 0
    }
    if x.t == 1 {
        return x.i
    }
    let f: f64 = x.f''')
v = v.replace('''        var ia: i64 = 0
        var ib: i64 = 0
        var real: bool = false
        if a.t == 1 {
            ia = a.i
        } else {
            real = true
            ia = val_to_int(&a)
        }
        if b.t == 1 {
            ib = b.i
        } else {
            real = true
            ib = val_to_int(&b)
        }''', '''        var ia: i64 = 0
        var ib: i64 = 0
        var real: bool = false
        if a.t != 1 {
            real = true
        }
        if b.t != 1 {
            real = true
        }
        ia = val_to_int(x)
        ib = val_to_int(y)''')
v = v.replace('''    *out = rc.val_real(0.0 - a.f)
    return true''', '''    *out = rc.val_real(-a.f)
    return true''')
v = v.replace("export {\n    parse_num,", "export {\n    text_int_prefix, parse_num,")
open(d + 'value.fi', 'w').write(v)

p = open(d + 'sqlparse.fi').read()
p = p.replace("(*en).fval = 0.0 - (*en).fval", "(*en).fval = -(*en).fval")
p = p.replace("(*en).fval = -(*en).fval", "(*en).fval = -(*en).fval")
open(d + 'sqlparse.fi', 'w').write(p)
print("patched")
