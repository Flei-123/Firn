use std::hint::black_box;

fn fib(n: u32) -> u64 {
    if n < 2 {
        return n as u64;
    }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    println!("{}", black_box(fib(black_box(40))));
}
