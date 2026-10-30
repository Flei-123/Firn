use std::hint::black_box;

fn main() {
    let n: usize = black_box(16777216);
    let words = n / 8;
    let mut a = vec![0u64; words];
    let mut b = vec![0u64; words];
    let mut c = vec![0u64; words];
    let mut acc: u64 = 0;
    for r in 0..12u64 {
        for j in 0..words {
            a[j] = (j as u64).wrapping_add(r).wrapping_mul(0x9E3779B97F4A7C15);
        }
        let ab: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(a.as_mut_ptr() as *mut u8, n) };
        let bb: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr() as *mut u8, n) };
        for i in 0..n {
            bb[i] = ab[i];
        }
        for i in 0..n / 2 {
            ab[i] = ((i as u64).wrapping_add(r) & 255) as u8;
        }
        for j in 0..words {
            c[j] = b[j];
        }
        let mut j = 0;
        while j < words {
            acc = acc.wrapping_mul(31).wrapping_add(c[j]);
            j += 4099;
        }
    }
    for j in 0..words {
        acc = acc.wrapping_mul(31).wrapping_add(a[j] ^ c[j]);
    }
    println!("{}", black_box(acc));
}
