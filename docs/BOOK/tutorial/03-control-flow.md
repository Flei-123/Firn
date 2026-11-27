# 3. Control flow

```firn run
import std.io

fn classify(n: i32) -> str {
    if n < 0 {
        return "negative"
    } else if n == 0 {
        return "zero"
    }
    return "positive"
}

fn main() {
    var sum: i32 = 0
    for i in 1 as i32..6 as i32 {
        sum = sum + i
    }
    var n: i32 = 3
    while n > 0 {
        n = n - 1
    }
    io.print_line(io.fmt_string(f"{sum} {n}"))
    io.print_line(classify(0 - 4))
    io.print_line(classify(0))
    io.print_line(classify(9))
}
```

```output
15 0
negative
zero
positive
```

- `for i in a..b` is half-open: it runs from `a` up to `b - 1`.
- `if` conditions need no parentheses.
- A function needs a `return` on every path; the compiler tells you when one is missing.
