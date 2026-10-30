use std::hint::black_box;

const N: usize = 700;

fn main() {
    let n: usize = black_box(N);
    let mut ma = vec![0i64; N * N];
    let mut mb = vec![0i64; N * N];
    let mut mc = vec![0i64; N * N];
    for i in 0..n {
        for j in 0..n {
            ma[i * N + j] = ((i * 31 + j * 17 + 3) % 101) as i64;
            mb[i * N + j] = ((i * 13 + j * 7 + 5) % 97) as i64;
        }
    }
    for r in 0..n {
        for c in 0..n {
            let mut s: i64 = 0;
            for k in 0..n {
                s = s.wrapping_add(ma[r * N + k].wrapping_mul(mb[k * N + c]));
            }
            mc[r * N + c] = s;
        }
    }
    let mut h: u64 = 0;
    for d in 0..N * N {
        h = h.wrapping_mul(1000003).wrapping_add(mc[d] as u64);
    }
    println!("{}", black_box(h));
}
