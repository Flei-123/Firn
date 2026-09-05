// SPDX-License-Identifier: GPL-2.0-only
// Counterpart to bench/firn/branchy.fi.
use std::hint::black_box;

fn main() {
    let n: u64 = black_box(60000000);
    let mut x: u64 = 88172645463325252;
    let (mut a, mut b, mut c, mut d): (u64, u64, u64, u64) = (0, 0, 0, 0);
    let mut i: u64 = 0;
    while i < n {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let v = x & 255;
        if v < 64 { a += v & 7; }
        else if v < 128 { b += v & 15; }
        else if v < 192 { c += v & 31; }
        else { d += v & 63; }
        i += 1;
    }
    println!("{}", black_box(a + b * 2 + c * 3 + d * 4));
}
