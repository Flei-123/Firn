# 5. Errors

```firn run
import std.io

error ParseError { Empty, TooBig }

fn check(n: i32) -> ParseError!i32 {
    if n == 0 {
        return ParseError::Empty
    }
    if n > 100 {
        return ParseError::TooBig
    }
    return n * 2
}

fn double_twice(n: i32) -> ParseError!i32 {
    let first = try check(n)
    return try check(first)
}

fn main() -> i32 {
    let a = check(21) catch 0 - 1
    let b = check(0) catch 0 - 1
    let c = double_twice(30) catch 0 - 1
    io.print_line(io.fmt_string(f"{a} {b} {c}"))
    return 0
}
```

```output
42 -1 120
```

- `E!T` is either an error of set `E` or a value of `T`.
- `try` passes an error up; `catch` supplies a substitute value.
- The result of an error union must not be ignored.
