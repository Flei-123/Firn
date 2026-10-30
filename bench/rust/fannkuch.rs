use std::hint::black_box;

const N: usize = 10;

fn main() {
    let mut perm = [0i32; N];
    let mut perm1 = [0i32; N + 1];
    let mut count = [0i32; N];
    for i in 0..N {
        perm1[i] = i as i32;
    }
    let mut r: usize = black_box(N);
    let mut perm_count: i64 = 0;
    let mut checksum: i64 = 0;
    let mut max_flips: i64 = 0;
    loop {
        while r != 1 {
            count[r - 1] = r as i32;
            r -= 1;
        }
        for c in 0..N {
            perm[c] = perm1[c];
        }
        let mut flips: i64 = 0;
        let mut k = perm[0];
        while k != 0 {
            let mut lo: usize = 0;
            let mut hi: usize = k as usize;
            while lo < hi {
                perm.swap(lo, hi);
                lo += 1;
                hi -= 1;
            }
            flips += 1;
            k = perm[0];
        }
        if flips > max_flips {
            max_flips = flips;
        }
        if perm_count % 2 == 0 {
            checksum += flips;
        } else {
            checksum -= flips;
        }
        let mut more = true;
        while more {
            if r == N {
                println!("{}", black_box(checksum * 1000 + max_flips));
                return;
            }
            let p0 = perm1[0];
            for m in 0..r {
                perm1[m] = perm1[m + 1];
            }
            perm1[r] = p0;
            count[r] -= 1;
            if count[r] > 0 {
                more = false;
            } else {
                r += 1;
            }
        }
        perm_count += 1;
    }
}
