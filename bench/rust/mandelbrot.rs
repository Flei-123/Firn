use std::hint::black_box;

const SIZE: i64 = 800;
const MAX_ITER: i64 = 500;

fn main() {
    let size: i64 = black_box(SIZE);
    let mut total: u64 = 0;
    let mut py: i64 = 0;
    while py < size {
        let ci = -1.5 + 3.0 * (py as f64) / (size as f64);
        let mut px: i64 = 0;
        while px < size {
            let cr = -2.0 + 3.0 * (px as f64) / (size as f64);
            let mut zr = 0.0f64;
            let mut zi = 0.0f64;
            let mut it: i64 = 0;
            while it < MAX_ITER {
                let zr2 = zr * zr;
                let zi2 = zi * zi;
                if zr2 + zi2 > 4.0 {
                    break;
                }
                zi = 2.0 * zr * zi + ci;
                zr = zr2 - zi2 + cr;
                it += 1;
            }
            total += it as u64;
            px += 1;
        }
        py += 1;
    }
    println!("{}", black_box(total));
}
