# 4. Structs and enums

```firn run
import std.io

struct Point {
    x: i32,
    y: i32,
}

enum Shape {
    Circle(i32),
    Rect(i32, i32),
    Empty,
}

fn area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) => { return 3 * r * r }
        Shape::Rect(w, h) => { return w * h }
        Shape::Empty => { return 0 }
    }
}

fn main() {
    let p: Point = Point { x: 3, y: 4 }
    io.print_line(io.fmt_string(f"{p.x} {p.y}"))
    io.print_line(io.fmt_string(f"{area(Shape::Circle(2))}"))
    io.print_line(io.fmt_string(f"{area(Shape::Rect(3, 5))}"))
    io.print_line(io.fmt_string(f"{area(Shape::Empty)}"))
}
```

```output
3 4
12
15
0
```

- `match` must cover every variant.
- Generic enums: `enum Opt[T] { Some(T), None }` (see `docs/GENERICS.md`).
