use std::hint::black_box;

fn add_into(dst: &mut Vec<i64>, src: &Vec<i64>) {
    let mut i: usize = 0;
    while i < dst.len() && i < src.len() {
        dst[i] = dst[i].wrapping_add(src[i]);
        dst[i] = dst[i] ^ src[i];
        i += 1;
    }
}

fn main() {
    let mut a: Vec<i64> = Vec::new();
    let mut b: Vec<i64> = Vec::new();
    let mut k: i64 = 0;
    while k < black_box(100000) {
        a.push(k);
        b.push(k * 3);
        k += 1;
    }
    for _ in 0..50 {
        add_into(black_box(&mut a), black_box(&b));
    }
    if black_box(a[5]) == 12345 {
        std::process::exit(1);
    }
}
