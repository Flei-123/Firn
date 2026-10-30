use std::hint::black_box;

fn main() {
    let n: usize = black_box(1000000);
    let mut v: Vec<i32> = Vec::new();
    let mut x: u32 = black_box(12345);
    for _ in 0..n {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        v.push((x >> 1) as i32);
    }
    v.sort_unstable();
    let mut sum: u64 = 0;
    for k in 0..n {
        sum = sum.wrapping_add((v[k] as u64).wrapping_mul((k + 1) as u64));
    }
    println!("{}", black_box(sum));
}
