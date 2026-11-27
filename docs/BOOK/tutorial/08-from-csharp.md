# 8. Coming from C#

The prelude makes `x.to_str()`, `parse.`, `format.`, `chars.`, `print`, `println` and `format(tpl, a, b, ...)` available without `import` (see `docs/PRELUDE.md`; `--no-prelude` turns it off). Your own names win, and the collector starts itself, so there is no `gc_init()` call.

Firn takes the C# names where they are free of traps: `int`, `long`, `byte`, `double`, `float`, `string` are aliases of `i32`, `i64`, `u8`, `f64`, `f32`, `str`. Same type, no cast.

| C# | Firn |
|---|---|
| `int x = 5;` | `let x: int = 5` |
| `x.ToString()` | `x.to_str()` |
| `int.Parse(s)` | `int.parse(s) catch 0` |
| `int.TryParse(s, out n)` | `int.try_parse(s, &n)` |
| `new List<int>()` | `Vec[i32].new()` (also `Map[u32, i32].new()`, `Set[u32].new()`) |
| `new Counter()` | `Counter.start()`, a function in `impl Counter` without `self` |
| `a.B().C()` | `a.b().c()`, also on results and on `(x + y)` |
| `string.Format("{0} + {1}", a, b)` | `format("{0} + {1}", a, b)` (long form: `format.format2("{0} + {1}", a.to_str(), b.to_str())`) |
| `int Sum(params int[] xs)` | `fn sum(xs: ..int) -> int`, called `sum(1, 2, 3)` or `sum()` |
| `char c = 'a';` | `let c: char = 'a'` |
| `foreach (char c in s)` | `chars.iter(s)` + `while chars.next(&it)` (`import std.chars`) |
| `$"x = {x}"` | `f"x = {x}"`, usable directly as `str` |

## ToString

```firn run
import std.io
import std.tostr

fn main() {
    let n: int = 42
    let big: ulong = 18446744073709551615
    let ratio: double = 0.1 + 0.2
    let flag: bool = true
    io.print_line(n.to_str() + " " + big.to_str() + " " + ratio.to_str() + " " + flag.to_str())
}
```

```output
42 18446744073709551615 0.30000000000000004 true
```

- `to_str()` exists on `i8..i64`, `u8..u64`, `isize`, `usize`, `f32`, `f64` and `bool`.
- Floats print the shortest text that reads back, exactly like an f-string.
- The result is a `str` you can join with `+`.

## Parse and TryParse

Firn has no exceptions. `int.parse` returns an error union `ParseError!i32`; `catch` gives a fallback, `try` passes the error up. The type name before the dot works for `int`, `long`, `uint`, `ulong`, `float`, `double`, `bool` and their short names (`i32`, `i64`, `u32`, `u64`, `f32`, `f64`).

```firn run
import std.io
import std.parse
import std.tostr

fn main() {
    let a = int.parse("123") catch 0 - 1
    let b = int.parse("12x") catch 0 - 1
    let c = f64.parse("2.5") catch 0.0
    let d = int.parse("3000000000") catch |e| 0 - 2
    var n: i64 = 0
    let ok: bool = i64.try_parse("77", &n)
    io.print_line(a.to_str() + " " + b.to_str() + " " + c.to_str() + " " + d.to_str() + " " + ok.to_str() + " " + n.to_str())
}
```

```output
123 -1 2.5 -2 true 77
```

- `ParseError` has `Empty`, `Invalid` and `Overflow`.
- The whole text must be the number: `" 4"` and `"4 "` fail; call `s.trim()` first.
- `int.parse(s)` is the function `i32__parse` that `impl i32` in `std.parse` declares; `parse.to_i32(s)` is the same call in module form.

## params

A last parameter `xs: ..T` collects every extra argument into a `Vec[T]`. The body iterates it with `for` and asks `xs.len()`.

```firn run
import std.io
import std.tostr

fn sum(xs: ..int) -> int {
    var total: int = 0
    for x in xs {
        total += x
    }
    return total
}

fn describe(label: str, xs: ..long) -> str {
    return label + ":" + xs.len().to_str()
}

fn main() {
    io.print_line(sum().to_str() + " " + sum(5).to_str() + " " + sum(1, 2, 3).to_str() + " " + sum(sum(1, 2), 4).to_str())
    io.print_line(describe("n") + " " + describe("n", 1, 2, 3, 4, 5, 6, 7, 8, 9, 10))
}
```

```output
0 5 6 7
n:0 n:10
```

- Zero arguments are fine; every argument is type-checked against `T`.
- The call is rewritten in the file that declares the function, so a variadic function is called with its bare name in that file. Across modules pass a `Vec[T]` built with `vec.vec_with`.
- A `Vec[T]` cannot be spread into the call (`sum(v)` is an error); the parameter itself is a plain `Vec[T]`.

## Functions on a type

An `impl` block may hold functions without a receiver. They are called on the type name, with a dot.

```firn run
import std.io
import std.tostr

struct Counter { n: i32 }

impl Counter {
    fn start() -> Counter { return Counter { n: 0 } }
    fn next(*self) -> Counter { return Counter { n: self.n + 1 } }
    fn bump(*self) { self.n += 1 }
    fn get(*self) -> i32 { return self.n }
}

#[derive(Default)]
struct Config { width: i32, name: str }

fn main() {
    let c: Counter = Counter.start()
    io.print_line(Counter.start().next().next().get().to_str() + " " + c.get().to_str())
    let cfg: Config = Config.default()
    io.print_line(cfg.width.to_str() + "[" + cfg.name + "]")
    var d: Counter = Counter.start()
    d.bump()
    d.bump()
    io.print_line(d.get().to_str())
}
```

```output
2 0
0[]
2
```

- `self.n` reads and writes through the pointer, `(*self).n` means the same. It works for every `*T` / `*mut T` to a struct, also outside methods (`p.x = 3`).
- A method call works on any value: `Counter.start().next().next().get()` needs no `let` in between. A `*self` method on a temporary changes the temporary only.
- A local variable, `const` or function with the same name as the type wins over the type.
- `::` is only for enum variants (`Shape::Box(1)`).
- Generic types take the same call: `Vec[i32].new()`, `Map[u32, i32].new()`, `Set[u32].new()` (`import std.vec` / `std.map`). A module name in front works too: `geo.Point.origin()`.
- A wrong name says what is missing: `Counter.star()` gives `type 'Counter' has no function 'star'` with `did you mean 'start'?`.

## Format

`format(tpl, a, b, ...)` takes up to 8 arguments of any printable type (see the prelude below). The long form `format.format0` to `format.format8` takes the template and that many `str` arguments; `{0}`, `{1}` count from zero, `{{` and `}}` are literal braces.

```firn run
import std.io
import std.format
import std.tostr

fn main() {
    let x: int = 3
    let y: int = 4
    let z: int = x + y
    io.print_line(format.format3("z = {0} + {1} = {2}", x.to_str(), y.to_str(), z.to_str()))
    io.print_line(format.format2("{1} {0} {{braces}}", "a", "b"))
}
```

```output
z = 3 + 4 = 7
b a {braces}
```

- A placeholder without an argument stays as written (`{5}`); format never traps.

## Interpolation

```firn run
fn main() {
    let name: string = "firn"
    let n: int = 7
    println(f"{name} has {n} apples, {n * 2} pears, {2.5:.2f} units")
}
```

```output
firn has 7 apples, 14 pears, 2.50 units
```

- `f"..."` builds a `Fmt`. Where a `str` is expected (`let s: str = f".."`, an argument, `return`, a struct field, `s + f".."`, `==`) it becomes a `str` by itself. `io.fmt_string(f"..")` still works, and functions that take a `Fmt` (`io.fmt_print`) keep getting one.
- Format specs follow Python: `{x:>8}`, `{x:08.3f}`, `{x:x}`.

## Characters

`char` is a second spelling of `u32` (the Unicode scalar value), `'a'` and `'\u{1F600}'` are character literals.

```firn run
import std.io
import std.chars
import std.tostr

fn main() {
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
    let cs: Vec[u32] = chars.collect(text)
    var sum: u32 = 0
    for c in cs {
        sum += c
    }
    io.print_line(sum.to_str())
}
```

```output
104 233 108 108 111 32 8364 
7 h H é
9060
```

- `char` is `u32`, so `c.to_str()` is the number; `c.char_str()` is the text.
- `for c in s` walks the BYTES (`u8`) of a `str`. For characters: `chars.collect(s)` gives a `Vec[u32]` that `for c in cs` walks, or use `chars.iter` and `chars.next` without a copy.
- `chars.count(s)` counts characters, `s.length()` counts bytes.
- A method works on a call result: `chars.count(s).to_str()`.

## Indexes

An array index may have any integer type up to 64 bits, so no `as usize` is needed. A negative value fails the bounds check.

```firn run
import std.io
import std.tostr

fn main() {
    var v: [i32; 4] = [0, 0, 0, 0]
    for i in 0..4 {
        v[i] = i * 3
    }
    let k: u8 = 2
    io.print_line(v[3].to_str() + " " + v[k].to_str())
}
```

```output
9 6
```

## No imports needed

`to_str`, `parse.`, `format.`, `chars.`, `print`, `println`, `format(...)` and every `std` module (`io.`, `time.`, `math.`, ...) work without `import`. The collector starts itself too.

```firn run
fn main() {
    let n: int = 42
    let m: int = parse.to_i32("8") catch 0
    println(format("{0} + {1} = {2}", n, m, n + m))
    print("chars: ")
    let total: usize = chars.count("h\u{e9}llo")
    println(total.to_str())
    let t0: u64 = time.millis()
    let label: str = f"abs = {math.abs(0 - 3)}"
    io.print_line(label)
    println(f"fast: {time.millis() - t0 < 1000}")
}
```

```output
42 + 8 = 50
chars: 5
abs = 3
fast: true
```

- `format(tpl, a, b, ...)` takes up to 8 arguments of any printable type; no `to_str()` needed.
- Your own names win: a `fn println` or a variable called `parse` in your file switches that entry off.
- Every std module works without `import` when you write `name.member`: `io.print_line(..)`, `time.millis()`, `math.abs(x)`, `fs.read_all(..)`.
- Names inside f-strings count too: `f"took {time.millis() - t0} ms"`.
- If your file has its own `time` (variable, field, function, parameter) or imports a module of that name, nothing is added.
- `--no-prelude` turns the whole mechanism off. See `docs/PRELUDE.md`.
