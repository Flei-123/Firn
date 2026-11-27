# 1. Hello

```firn run
import std.io

fn main() {
    io.print_line("hello, firn")
}
```

```output
hello, firn
```

- `import std.io` brings in the module `io`; call its functions as `io.print_line`.
- `main` needs no return type: the exit code is 0. Write `fn main() -> i32` and `return code` to choose another exit code.
- Run a single file with `firn run hello.fi`; start a project with `firn new cli hello`.
