use std::hint::black_box;

const STEPS: u64 = 200_000_000;

fn popcount(v: u64) -> u64 {
    let mut x = v;
    x = x.wrapping_sub((x >> 1) & 0x5555555555555555);
    x = (x & 0x3333333333333333).wrapping_add((x >> 2) & 0x3333333333333333);
    x = x.wrapping_add(x >> 4) & 0x0f0f0f0f0f0f0f0f;
    x.wrapping_mul(0x0101010101010101) >> 56
}

fn rotl(v: u64, k: u64) -> u64 {
    (v << k) | (v >> (64 - k))
}

fn main() {
    let mut x: u64 = black_box(88172645463325252);
    let mut acc: u64 = 0;
    let mut i: u64 = 0;
    while i < STEPS {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        acc = acc.wrapping_add(popcount(x));
        acc ^= rotl(x, (i & 63) | 1);
        i += 1;
    }
    println!("{}", black_box(acc));
}
