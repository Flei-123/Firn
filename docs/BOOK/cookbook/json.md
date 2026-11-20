# Read JSON

```firn run
import std.io
import std.rt
import std.json
import std.jsonstr

fn main() -> i32 {
    let text: str = "{\"name\": \"firn\", \"version\": 3, \"tags\": [\"fast\", \"safe\"]}"
    var doc: json.Json = json.json_new()
    defer json.json_free(&doc)
    let parsed = json.json_parse(&doc, text.p as u64, text.n) catch false
    if !parsed {
        io.print_line("invalid json")
        return 1
    }
    let root: u32 = json.json_root(&doc)
    let name: u32 = jsonstr.get(&doc, root, "name")
    let version: u32 = jsonstr.get(&doc, root, "version")
    let tags: u32 = jsonstr.get(&doc, root, "tags")
    io.print_line(jsonstr.string(&doc, name))
    io.print_line(io.fmt_string(f"{json.json_int(&doc, version)}"))
    io.print_line(io.fmt_string(f"{json.json_count(&doc, tags)}"))
    io.print_line(jsonstr.string(&doc, json.json_child_at(&doc, tags, 1)))
    return 0
}
```

```output
firn
3
2
safe
```

- The document is parsed once into a flat node array; `jsonstr.get` finds a key, `json_child_at` an array element.
- A broken document makes `json_parse` fail; `json_err_line` and `json_err_col` say where.
- Strings are views into the text: keep the text alive while you use them.
