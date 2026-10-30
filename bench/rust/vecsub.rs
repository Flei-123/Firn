use std::hint::black_box;

fn diffs(v: &Vec<i64>) -> i64 {
    let mut s: i64 = 0;
    let mut i: i32 = 1;
    let n = v.len() as i32;
    while i < n {
        s = s.wrapping_add(v[i as usize] ^ v[(i - 1) as usize]);
        i += 1;
    }
    s
}

fn smear(v: &mut Vec<i64>) {
    let mut i: usize = 1;
    while i < v.len() {
        v[i] = v[i - 1].wrapping_add(v[i]) & 1023;
        i += 1;
    }
}

fn main() {
    let mut v: Vec<i64> = Vec::new();
    let mut k: i64 = 0;
    while k < black_box(100000) {
        v.push(k);
        k += 1;
    }
    let mut r: i64 = 0;
    for _ in 0..20 {
        smear(black_box(&mut v));
        r ^= diffs(black_box(&v));
    }
    if black_box(r) == 123456789 {
        std::process::exit(1);
    }
}
