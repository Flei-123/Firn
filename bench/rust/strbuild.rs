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

fn main() {
    let mut total: u64 = 0;
    let mut s: u64 = black_box(7);
    for _ in 0..3 {
        let mut buf: Vec<u8> = Vec::with_capacity(16);
        for _ in 0..1000000u32 {
            s = lcg(s);
            push_dec(&mut buf, s >> 40);
            buf.push(44);
        }
        let mut h: u64 = 14695981039346656037;
        for &b in buf.iter() {
            h = (h ^ b as u64).wrapping_mul(1099511628211);
        }
        total = total.wrapping_mul(1000003).wrapping_add(h ^ buf.len() as u64);
    }
    println!("{}", black_box(total));
}
