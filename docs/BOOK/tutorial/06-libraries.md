# 6. Libraries

```firn run
import std.io
import std.args
import std.vec

fn main() {
    var items: Vec[i32] = vec.vec_new[i32]()
    vec.vec_push[i32](&items, 10)
    vec.vec_push[i32](&items, 20)
    io.print_line(io.fmt_string(f"{vec.vec_len[i32](&items)}"))
    var cmd: args.Cmd = args.cmd("demo", "demo tool")
    args.opt(&cmd, "--name", args.STR, "world", "who")
    var parsed: args.Result = args.parse_words(&cmd, "--name firn")
    io.print_line(args.get(&parsed, "name"))
    vec.vec_free[i32](&items)
}
```

```output
2
firn
```

- `lib/` holds the standard library; `docs/LIBS.md` lists every module, `firn doc std.args` prints one API.
- Your own library: `firn new lib geo`, then in the app `firn add geo ../geo` and `import geo`.
- Registry packages: `firn add NAME` (see `docs/PACKAGES.md`).
- Search before writing: `firn doc std.json --search parse`.
