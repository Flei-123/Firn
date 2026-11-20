# HTTP server

```firn build
import std.rt
import std.io
import webfw.wf_types
import webfw.wf_web
import webfw.wf_ctx

fn hello(c: *mut wf_types.Ctx) {
    wf_ctx.text(c, 200, "hello " + wf_ctx.param(c, "name"))
}

fn health(c: *mut wf_types.Ctx) {
    wf_ctx.json_raw(c, 200, "{\"status\": \"ok\"}")
}

fn main() -> i32 {
    var w: wf_types.Web = wf_web.web_new(8080)
    wf_web.get(&w, "/health", health)
    wf_web.get(&w, "/hello/:name", hello)
    io.print_line(wf_web.url(&w))
    return wf_web.serve(&w)
}
```

- Start a ready project: `firn new web-api shop`.
- Routes take `:param` segments; groups and middleware are in `docs/WEBFRAMEWORK.md`.
- Try it: `curl localhost:8080/hello/firn`.
