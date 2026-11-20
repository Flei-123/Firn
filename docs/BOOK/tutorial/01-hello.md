# 1. Hello

```firn run
import std.io

fn main() -> i32 {
    io.print_line("hello, firn")
    return 0
}
```

```output
hello, firn
```

- `import std.io` brings in the module `io`; call its functions as `io.print_line`.
- `main` returns the exit code.
- Run a single file with `firn run hello.fi`; start a project with `firn new cli hello`.
