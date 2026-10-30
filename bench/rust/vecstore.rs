use std::hint::black_box;

fn scale(v: &mut Vec<i64>) {
    for i in 0..v.len() {
        v[i] = (v[i] ^ 5).wrapping_add(1);
    }
}

fn sum_signed(v: &Vec<i64>) -> i64 {
    let mut s: i64 = 0;
    let mut i: i32 = 0;
    let n = v.len() as i32;
    while i < n {
        s ^= v[i as usize];
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
        scale(black_box(&mut v));
        total ^= sum_signed(black_box(&v));
    }
    if black_box(total) == 12345 {
        std::process::exit(1);
    }
}
