# Threads

```firn run
import std.io

fn __thread_work(kind: u64, arg: u64) -> u64 {
    return arg * 2
}

fn main() {
    let first: u64 = thread_start(1, 21)
    let second: u64 = thread_start(1, 50)
    let a: u64 = thread_wait(first)
    let b: u64 = thread_wait(second)
    io.print_line(io.fmt_string(f"{a} {b}"))
}
```

```output
42 100
```

- `thread_start(kind, arg)` runs `__thread_work` on a real thread and returns a handle; `thread_wait` joins and yields the result.
- Share state with mutexes, atomics or channels (`docs/CHAN.md`, `docs/JOBS.md` for `parallel_for`).
