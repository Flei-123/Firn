use std::hint::black_box;

const N: usize = 2_000_000;

fn insertion(a: &mut [u32], lo: i64, hi: i64) {
    let mut i = lo + 1;
    while i <= hi {
        let v = a[i as usize];
        let mut j = i - 1;
        while j >= lo && a[j as usize] > v {
            a[(j + 1) as usize] = a[j as usize];
            j -= 1;
        }
        a[(j + 1) as usize] = v;
        i += 1;
    }
}

fn quicksort(a: &mut [u32], mut lo: i64, mut hi: i64) {
    while hi - lo > 16 {
        let mid = lo + (hi - lo) / 2;
        if a[mid as usize] < a[lo as usize] {
            a.swap(mid as usize, lo as usize);
        }
        if a[hi as usize] < a[lo as usize] {
            a.swap(hi as usize, lo as usize);
        }
        if a[hi as usize] < a[mid as usize] {
            a.swap(hi as usize, mid as usize);
        }
        let pivot = a[mid as usize];
        let mut i = lo;
        let mut j = hi;
        while i <= j {
            while a[i as usize] < pivot {
                i += 1;
            }
            while a[j as usize] > pivot {
                j -= 1;
            }
            if i <= j {
                a.swap(i as usize, j as usize);
                i += 1;
                j -= 1;
            }
        }
        if j - lo < hi - i {
            quicksort(a, lo, j);
            lo = i;
        } else {
            quicksort(a, i, hi);
            hi = j;
        }
    }
    insertion(a, lo, hi);
}

fn main() {
    let mut a = vec![0u32; N];
    let mut x: u32 = black_box(12345);
    for v in a.iter_mut() {
        x = x.wrapping_mul(1664525).wrapping_add(1013904223);
        *v = x;
    }
    quicksort(&mut a, 0, N as i64 - 1);
    let mut sum: u64 = 0;
    for k in 0..N {
        if k > 0 && a[k - 1] > a[k] {
            std::process::exit(1);
        }
        sum = sum.wrapping_add((a[k] as u64).wrapping_mul((k + 1) as u64));
    }
    println!("{}", black_box(sum));
}
