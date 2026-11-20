# Read a file

```firn run
import std.io
import std.rt
import std.fs

fn main() -> i32 {
    let path: str = "/tmp/firn-book-sample.txt"
    let body: str = "line one\nline two\n"
    let written = fs.write_file(path, body.p as u64, body.n) catch 0
    if written != body.n {
        io.print_line("write failed")
        return 1
    }
    var data: rt.Buf = rt.buf_new()
    defer rt.buf_free(&data)
    let size = fs.read_file(path, &data) catch 0
    io.print_line(io.fmt_string(f"{size} bytes"))
    io.print_line(rt.buf_str(&data))
    let removed = fs.remove(path) catch false
    return 0
}
```

```output
18 bytes
line one
line two

```

- `fs.read_file` appends the content to a `rt.Buf`; the error set is `FsError`.
- `fs.exists(path)` checks a path first; `fs.remove(path)` deletes it.
