use std::hint::black_box;

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn push_dec(buf: &mut Vec<u8>, v: u64) {
    let mut tmp = [0u8; 24];
    let mut n = 0;
    let mut x = v;
    if x == 0 {
        tmp[0] = 48;
        n = 1;
    }
    while x > 0 {
        tmp[n] = (x % 10) as u8 + 48;
        n += 1;
        x /= 10;
    }
    while n > 0 {
        n -= 1;
        buf.push(tmp[n]);
    }
}

fn is_digit(c: u8) -> bool {
    c >= 48 && c <= 57
}

fn scan(p: &[u8]) -> u64 {
    let len = p.len();
    let mut i = 0;
    let mut objects: u64 = 0;
    let mut arrays: u64 = 0;
    let mut numbers: u64 = 0;
    let mut strings: u64 = 0;
    let mut sum: u64 = 0;
    while i < len {
        let c = p[i];
        if c == 123 {
            objects += 1;
            i += 1;
        } else if c == 91 {
            arrays += 1;
            i += 1;
        } else if c == 34 {
            strings += 1;
            i += 1;
            while i < len {
                let d = p[i];
                if d == 92 {
                    i += 2;
                } else if d == 34 {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
        } else if is_digit(c) || c == 45 {
            let mut neg = false;
            if c == 45 {
                neg = true;
                i += 1;
            }
            let mut v: u64 = 0;
            while i < len {
                let d = p[i];
                if is_digit(d) {
                    v = v.wrapping_mul(10).wrapping_add((d - 48) as u64);
                    i += 1;
                } else {
                    break;
                }
            }
            numbers += 1;
            if neg {
                sum = sum.wrapping_sub(v);
            } else {
                sum = sum.wrapping_add(v);
            }
        } else {
            i += 1;
        }
    }
    let mut r = objects;
    r = r.wrapping_mul(1000003).wrapping_add(arrays);
    r = r.wrapping_mul(1000003).wrapping_add(numbers);
    r = r.wrapping_mul(1000003).wrapping_add(strings);
    r = r.wrapping_mul(1000003).wrapping_add(sum);
    r
}

fn main() {
    let count: u64 = black_box(350000);
    let mut buf: Vec<u8> = Vec::with_capacity(count as usize * 128);
    let mut s: u64 = 21;
    for i in 0..count {
        s = lcg(s);
        buf.extend_from_slice(b"{\"id\":");
        push_dec(&mut buf, i);
        buf.extend_from_slice(b",\"name\":\"it\\\"em");
        push_dec(&mut buf, (s >> 40) % 100000);
        buf.extend_from_slice(b"\\\"\",\"ok\":");
        if ((s >> 20) & 1) == 1 {
            buf.extend_from_slice(b"true");
        } else {
            buf.extend_from_slice(b"false");
        }
        buf.extend_from_slice(b",\"v\":[");
        push_dec(&mut buf, (s >> 8) % 1000);
        buf.push(44);
        buf.push(45);
        push_dec(&mut buf, (s >> 30) % 1000);
        buf.push(44);
        push_dec(&mut buf, (s >> 50) % 100000);
        buf.extend_from_slice(b"],\"o\":{\"p\":");
        push_dec(&mut buf, (s >> 12) % 100);
        buf.extend_from_slice(b"}},");
    }
    let mut total: u64 = 0;
    for _ in 0..4 {
        total = total.wrapping_mul(1000003).wrapping_add(scan(black_box(&buf)));
    }
    println!("{}", black_box(total));
}
