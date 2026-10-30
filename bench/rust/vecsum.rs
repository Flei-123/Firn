use std::hint::black_box;

fn sum_while(v: &Vec<i64>) -> i64 {
    let mut s: i64 = 0;
    let mut i: usize = 0;
    while i < v.len() {
        s = s.wrapping_add(v[i]);
        i += 1;
    }
    s
}

fn sum_pairs(v: &Vec<i64>) -> i64 {
    let mut s: i64 = 0;
    let mut i: usize = 0;
    while i + 1 < v.len() {
        s = s.wrapping_add(v[i] ^ v[i + 1]);
        i += 1;
    }
    s
}

fn main() {
    let mut v: Vec<i64> = Vec::new();
    let mut k: i64 = 0;
    while k < black_box(100000) {
        v.push(k);
        k += 1;
    }
    let mut total: i64 = 0;
    for _ in 0..50 {
        total = total.wrapping_add(sum_while(black_box(&v))).wrapping_add(sum_pairs(black_box(&v)));
    }
    if black_box(total) == 0 {
        std::process::exit(1);
    }
}
