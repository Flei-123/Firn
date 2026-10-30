use std::hint::black_box;

const N: usize = 1500;
const OFF_U: usize = 0;
const OFF_V: usize = 1500;
const OFF_T: usize = 3000;

fn eval_a(i: i64, j: i64) -> f64 {
    1.0 / (((i + j) * (i + j + 1) / 2 + i + 1) as f64)
}

fn mul_av(buf: &mut [f64], src: usize, dst: usize) {
    for i in 0..N {
        let mut s = 0.0;
        for j in 0..N {
            s = s + eval_a(i as i64, j as i64) * buf[src + j];
        }
        buf[dst + i] = s;
    }
}

fn mul_atv(buf: &mut [f64], src: usize, dst: usize) {
    for i in 0..N {
        let mut s = 0.0;
        for j in 0..N {
            s = s + eval_a(j as i64, i as i64) * buf[src + j];
        }
        buf[dst + i] = s;
    }
}

fn mul_atav(buf: &mut [f64], src: usize, dst: usize) {
    mul_av(buf, src, OFF_T);
    mul_atv(buf, OFF_T, dst);
}

fn main() {
    let mut buf = vec![0.0f64; 4500];
    for i in 0..black_box(N) {
        buf[OFF_U + i] = 1.0;
    }
    for _ in 0..10 {
        mul_atav(&mut buf, OFF_U, OFF_V);
        mul_atav(&mut buf, OFF_V, OFF_U);
    }
    let mut vbv = 0.0;
    let mut vv = 0.0;
    for k in 0..N {
        vbv = vbv + buf[OFF_U + k] * buf[OFF_V + k];
        vv = vv + buf[OFF_V + k] * buf[OFF_V + k];
    }
    println!("{}", black_box((((vbv / vv) as f64).sqrt() * 1000000000.0) as i64));
}
