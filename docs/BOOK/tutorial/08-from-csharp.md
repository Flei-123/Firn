# 8. Coming from C#

Firn takes the C# names where they are free of traps: `int`, `long`, `byte`, `double`, `float`, `string` are aliases of `i32`, `i64`, `u8`, `f64`, `f32`, `str`. Same type, no cast.

| C# | Firn |
|---|---|
| `int x = 5;` | `let x: int = 5` |
| `x.ToString()` | `x.to_str()` (`import std.tostr`) |
| `int.Parse(s)` | `parse.to_i32(s) catch 0` (`import std.parse`) |
| `int.TryParse(s, out n)` | `parse.try_i32(s, &n)` |
| `string.Format("{0} + {1}", a, b)` | `format.format2("{0} + {1}", a.to_str(), b.to_str())` (`import std.format`) |
| `char c = 'a';` | `let c: char = 'a'` |
| `foreach (char c in s)` | `chars.iter(s)` + `while chars.next(&it)` (`import std.chars`) |
| `$"x = {x}"` | `f"x = {x}"` wrapped in `io.fmt_string(...)` |

## ToString

```firn run
import std.io
import std.tostr

fn main() -> i32 {
    let n: int = 42
    let big: ulong = 18446744073709551615
    let ratio: double = 0.1 + 0.2
    let flag: bool = true
    io.print_line(n.to_str() + " " + big.to_str() + " " + ratio.to_str() + " " + flag.to_str())
    return 0
}
```

```output
42 18446744073709551615 0.30000000000000004 true
```

- `to_str()` exists on `i8..i64`, `u8..u64`, `isize`, `usize`, `f32`, `f64` and `bool`.
- Floats print the shortest text that reads back, exactly like an f-string.
- The result is a `str` you can join with `+`.

## Parse and TryParse

Firn has no exceptions. `parse.to_*` returns an error union `ParseError!T`; `catch` gives a fallback, `try` passes the error up.

```firn run
import std.io
import std.parse
import std.tostr

fn main() -> i32 {
    let a = parse.to_i32("123") catch 0 - 1
    let b = parse.to_i32("12x") catch 0 - 1
    let c = parse.to_f64("2.5") catch 0.0
    let d = parse.to_i32("3000000000") catch |e| 0 - 2
    var n: i64 = 0
    let ok: bool = parse.try_i64("77", &n)
    io.print_line(a.to_str() + " " + b.to_str() + " " + c.to_str() + " " + d.to_str() + " " + ok.to_str() + " " + n.to_str())
    return 0
}
```

```output
123 -1 2.5 -2 true 77
```

- `ParseError` has `Empty`, `Invalid` and `Overflow`.
- The whole text must be the number: `" 4"` and `"4 "` fail; call `s.trim()` first.
- Why `parse.to_i32(s)` and not `int.parse(s)`: a type name is not a value in Firn, so `int.parse` would need static methods on built-in types; the module function says the same.

## Format

Firn has no variadic calls. `format.format0` to `format.format5` take the template and that many `str` arguments; `{0}`, `{1}` count from zero, `{{` and `}}` are literal braces.

```firn run
import std.io
import std.format
import std.tostr

fn main() -> i32 {
    let x: int = 3
    let y: int = 4
    let z: int = x + y
    io.print_line(format.format3("z = {0} + {1} = {2}", x.to_str(), y.to_str(), z.to_str()))
    io.print_line(format.format2("{1} {0} {{braces}}", "a", "b"))
    return 0
}
```

```output
z = 3 + 4 = 7
b a {braces}
```

- A placeholder without an argument stays as written (`{5}`); format never traps.

## Interpolation

```firn run
import std.io

fn main() -> i32 {
    let name: string = "firn"
    let n: int = 7
    io.print_line(io.fmt_string(f"{name} has {n} apples, {n * 2} pears, {2.5:.2f} units"))
    return 0
}
```

```output
firn has 7 apples, 14 pears, 2.50 units
```

- `f"..."` builds a `Fmt`, not a `str`; `io.fmt_string` turns it into a `str`.
- Format specs follow Python: `{x:>8}`, `{x:08.3f}`, `{x:x}`.

## Characters

`char` is a second spelling of `u32` (the Unicode scalar value), `'a'` and `'\u{1F600}'` are character literals.

```firn run
import std.io
import std.chars
import std.tostr

fn main() -> i32 {
    let text: string = "h\u00e9llo \u20ac"
    var it: chars.CharIter = chars.iter(text)
    var codes: str = ""
    while chars.next(&it) {
        codes = codes + it.cp.to_str() + " "
    }
    let first: char = 'h'
    io.print_line(codes)
    let total: usize = chars.count(text)
    let upper: char = chars.to_upper(first)
    let second: char = chars.at(text, 1)
    io.print_line(total.to_str() + " " + first.char_str() + " " + upper.char_str() + " " + second.char_str())
    return 0
}
```

```output
104 233 108 108 111 32 8364 
7 h H é
```

- `char` is `u32`, so `c.to_str()` is the number; `c.char_str()` is the text.
- `for c in s.chars()` does not exist: a `for` loop needs an array variable. Use `chars.iter` and `chars.next`.
- `chars.count(s)` counts characters, `s.length()` counts bytes.
- A method needs a named receiver: `let n = chars.count(s)` first, then `n.to_str()`.
