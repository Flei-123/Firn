use std::hint::black_box;

fn lcg(s: u64) -> u64 {
    s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

fn slot_of(k: u64) -> usize {
    (k.wrapping_mul(0x9E3779B97F4A7C15) >> 42) as usize
}

fn put(t: &mut [u64], k: u64, v: u64) {
    let mut s = slot_of(k);
    loop {
        let cur = t[s * 2];
        if cur == 0 || cur == k {
            t[s * 2] = k;
            t[s * 2 + 1] = v;
            return;
        }
        s = (s + 1) & 4194303;
    }
}

fn get(t: &[u64], k: u64) -> u64 {
    let mut s = slot_of(k);
    loop {
        let cur = t[s * 2];
        if cur == k {
            return t[s * 2 + 1];
        }
        if cur == 0 {
            return 0;
        }
        s = (s + 1) & 4194303;
    }
}

fn main() {
    let n: u64 = black_box(2000000);
    let mut t = vec![0u64; 4194304 * 2];
    let mut s: u64 = 12345;
    for i in 0..n {
        s = lcg(s);
        put(&mut t, s | 1, i + 1);
    }
    let mut sum: u64 = 0;
    s = 12345;
    for _ in 0..n {
        s = lcg(s);
        sum = sum.wrapping_add(get(&t, s | 1));
    }
    let mut s2: u64 = 777;
    let mut misses: u64 = 0;
    for _ in 0..n {
        s2 = lcg(s2);
        let v = get(&t, s2 | 1);
        if v == 0 {
            misses += 1;
        } else {
            sum = sum.wrapping_add(v.wrapping_mul(3));
        }
    }
    println!("{}", black_box(sum.wrapping_mul(1000003).wrapping_add(misses)));
}
