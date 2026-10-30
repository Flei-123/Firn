use std::hint::black_box;

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn main() {
    let n: usize = black_box(67108864);
    let mut buf = vec![0u8; n];
    let mut s: u64 = 1;
    for b in buf.iter_mut() {
        s = lcg(s);
        *b = (s >> 56) as u8;
    }
    let mut h: u64 = 14695981039346656037;
    for _ in 0..4 {
        for i in 0..n {
            h = (h ^ buf[i] as u64).wrapping_mul(1099511628211);
        }
    }
    println!("{}", black_box(h));
}
