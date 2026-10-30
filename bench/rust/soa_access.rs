use std::hint::black_box;

#[derive(Clone, Copy)]
struct P {
    x: u64,
    y: u64,
    z: u64,
    w: u64,
}

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn main() {
    let n: usize = black_box(1000000);
    let mut aos = vec![P { x: 0, y: 0, z: 0, w: 0 }; n];
    let mut sx = vec![0u64; n];
    let mut sy = vec![0u64; n];
    let mut sz = vec![0u64; n];
    let mut sw = vec![0u64; n];
    let mut s: u64 = 5;
    for i in 0..n {
        s = lcg(s);
        let x = s >> 20;
        s = lcg(s);
        let y = s >> 20;
        s = lcg(s);
        let z = s >> 20;
        s = lcg(s);
        let w = s >> 20;
        aos[i] = P { x, y, z, w };
        sx[i] = x;
        sy[i] = y;
        sz[i] = z;
        sw[i] = w;
    }
    for _ in 0..8 {
        for i in 0..n {
            let nx = aos[i].x.wrapping_add(aos[i].y.wrapping_mul(3)).wrapping_add(aos[i].z >> 1);
            aos[i].x = nx;
            aos[i].w ^= nx;
        }
        for i in 0..n {
            let nx = sx[i].wrapping_add(sy[i].wrapping_mul(3)).wrapping_add(sz[i] >> 1);
            sx[i] = nx;
            sw[i] ^= nx;
        }
    }
    for _ in 0..24 {
        for i in 0..n {
            aos[i].w = aos[i].w.wrapping_add(aos[i].x);
        }
        for i in 0..n {
            sw[i] = sw[i].wrapping_add(sx[i]);
        }
    }
    let mut fa: u64 = 0;
    let mut fs: u64 = 0;
    for i in 0..n {
        fa = fa.wrapping_mul(31).wrapping_add(aos[i].x ^ aos[i].w);
        fs = fs.wrapping_mul(31).wrapping_add(sx[i] ^ sw[i]);
    }
    println!("{}", black_box(fa.wrapping_mul(1000003).wrapping_add(fs)));
}
