use std::hint::black_box;

const LIMIT: usize = 20_000_000;

fn main() {
    let limit: usize = black_box(LIMIT);
    let mut sieve = vec![0u8; limit + 1];
    let mut i = 2;
    while i * i <= limit {
        if sieve[i] == 0 {
            let mut j = i * i;
            while j <= limit {
                sieve[j] = 1;
                j += i;
            }
        }
        i += 1;
    }
    let mut sum: u64 = 0;
    for k in 2..=limit {
        if sieve[k] == 0 {
            sum += k as u64;
        }
    }
    println!("{}", black_box(sum));
}
