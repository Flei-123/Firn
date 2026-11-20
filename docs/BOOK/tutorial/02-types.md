# 2. Types and variables

```firn run
import std.io

fn main() -> i32 {
    let answer: i32 = 42
    var count: i64 = 0
    count = count + 8
    let ratio: f64 = 2.5
    let name: str = "firn"
    let flag: bool = true
    let wide: i64 = answer as i64 + count
    io.print_line(io.fmt_string(f"{name} {answer} {count} {wide} {ratio} {flag}"))
    return 0
}
```

```output
firn 42 8 50 2.5 true
```

- `let` is fixed, `var` can change.
- Integer types: `i8 i16 i32 i64 u8 u16 u32 u64 usize isize`; floats: `f32 f64`.
- No implicit conversion between numbers: write `x as i64`.
- `f"..."` strings interpolate values; wrap them in `io.fmt_string`.
- Negative literals are written `0 - 1`.
