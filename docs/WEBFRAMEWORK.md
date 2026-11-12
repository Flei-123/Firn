# Web framework (lib/webfw) -- the ASP.NET / Express layer on top of lib/http/server.fi

`lib/http/server.fi` is the transport (HTTP/1.1, WebSocket, SSE, TLS 1.3, one poll thread). `lib/webfw/` adds routing,
middleware and helpers. Example app: `examples/webfw/app.fi`. Proof: `bash tools/webfw/run.sh` (Python requests/http.client
cross-check + request fuzz), tests 6710 (units) and 6711 (in-process end to end).

```
import webfw.wf_types
import webfw.wf_web
import webfw.wf_ctx

fn hello(c: *mut wf_types.Ctx) { wf_ctx.text(c, 200, "hello " + wf_ctx.param(c, "name")) }

#[allow_escape]
fn main() -> i32 {
    var w: wf_types.Web = wf_web.web_new(8080)
    wf_web.get(&w, "/hello/:name", hello)
    return wf_web.serve(&w)
}
```

## Modules (module names are global: all prefixed `wf_`)
| File | What |
|---|---|
| `wf_types.fi` | `Web`, `Ctx`, `Route`, `Group`, `Mw`, `After` |
| `wf_web.fi` | app lifecycle (`web_new listen serve serve_background stop_wait`), routes (`get post put patch del any add`), groups (`group sub group_use g_get ...`), `use_before use_after set_not_found mount_static` |
| `wf_router.fi` | pattern matching: `/users/:id`, `/files/*rest`, trailing slash ignored, params percent-decoded |
| `wf_ctx.fi` | request: `method path header param query form cookie body client_ip json_in json_str json_int json_bool_or`; response: `set_status set_header set_cookie text html json_raw json_out json_error status_error redirect send_bytes` |
| `wf_cors.fi` | `enable_cors(w, origin, methods, headers, max_age)`: Origin check, preflight 204/403 |
| `wf_limit.fi` | `enable_rate_limit(w, max, window_ms)`: fixed window per client IP, 429 + Retry-After, X-RateLimit-* |
| `wf_compress.fi` | `enable_compress(w, min_size, level)`: zstd or gzip by Accept-Encoding, text-like types only |
| `wf_session.fi` | `enable_sessions(w, secret, ttl_s)`, `session_get/set/destroy`: HMAC-SHA256 signed cookie id, in-memory values |
| `wf_auth.fi` | `set_auth_hook`, `require_auth` (middleware, 401), `basic_credentials`, `bearer_token` |
| `wf_static.fi` | `mount_static(w, "/static", dir)`: ETag (weak) + 304, single Range (206/416), index.html, traversal refused |
| `wf_tpl.fi` | templates: `{{x}}` (HTML-escaped), `{{{x}}}` raw, `{{#if x}}..{{else}}..{{/if}}`, `{{#unless x}}`, `{{#each list}}{{.}}{{/each}}` |
| `wf_mime.fi`, `wf_text.fi` | content types; percent coding, HTML escape, pair parsing |

## Order of one request
1. global `before` middleware (CORS, rate limit, ...) in registration order; one returning false ends the request
2. route lookup (first match in registration order; HEAD matches GET); group middleware, then the handler
3. no route: static mounts, then 405 + `Allow` (OPTIONS: 204) when the path exists for other methods, else the not-found handler
4. `after` hooks (compression, session cookie), then the response is written

Middleware is `fn(*mut Ctx) -> bool`, after hooks are `fn(*mut Ctx)`. State lives in `Web` (config) or the program's own
record behind `http.set_userdata`. WebSocket/SSE routes are registered on `wf_web.web_app(&w)` with `http.route_ws/route_sse`
(exact routes win over the framework).

## Honest
- W1 Everything of server.fi stays: one poll thread, handlers synchronous, no HTTP/2, IPv4 only.
- W2 `client_ip` is the TCP peer (no X-Forwarded-For); behind a proxy every client shares one rate-limit bucket.
- W3 Sessions live in memory: lost on restart, expired ids are removed only when presented, one process only.
- W4 Static files are read whole (limit 64 MiB), one Range per request, no Last-Modified, no directory listing.
- W5 Request body limit is `set_limits` (default 1 MiB, 413 above); multipart/form-data is not parsed.
- W6 The ctx strings (path, query, header values) point into the connection buffer: copy them to keep them past the handler.
- W7 server.fi stays one file (1.6k lines) -- splitting out WebSocket/SSE is open.
