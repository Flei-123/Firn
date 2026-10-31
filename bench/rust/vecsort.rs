// SPDX-License-Identifier: MPL-2.0
use std::hint::black_box;

#[inline(never)]
fn sort_it(v: &mut Vec<i32>) {
    v.sort_unstable();
}

fn main() {
    let n: usize = black_box(1000000);
    let mut v: Vec<i32> = Vec::new();
    let mut seed: u64 = 12345;
    let mut i = 0usize;
    while i < n {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        v.push((seed >> 33) as i32);
        i += 1;
    }
    sort_it(&mut v);
    let mut bad = 0usize;
    let mut k = 1usize;
    while k < n {
        if v[k - 1] > v[k] {
            bad += 1;
        }
        k += 1;
    }
    if black_box(bad) != 0 {
        std::process::exit(2);
    }
}
