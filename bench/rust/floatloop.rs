use std::hint::black_box;

const STEPS: i64 = 100_000_000;

fn main() {
    let mut x: f64 = black_box(1.0);
    let mut s: f64 = 0.0;
    let mut i: i64 = 0;
    while i < STEPS {
        let t = (i as f64) * 0.0000001 + 1.0;
        x = (x * x * 0.5 + t).sqrt();
        s = s + x * 0.5 + 1.0 / (x + t);
        i += 1;
    }
    println!("{}", black_box((s + 0.5) as i64));
}
