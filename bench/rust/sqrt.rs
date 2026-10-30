use std::hint::black_box;

fn main() {
    let mut sum: f64 = 0.0;
    for _ in 0..4 {
        let mut y: i64 = 0;
        while y < black_box(1000) {
            let mut x: i64 = 0;
            while x < 1000 {
                let dx = (x - 500) as f64 + 0.5;
                let dy = (y - 400) as f64 + 0.25;
                let d2 = dx * dx + dy * dy;
                sum = sum + d2.sqrt();
                x += 1;
            }
            y += 1;
        }
    }
    println!("{}", black_box(sum as u64));
}
