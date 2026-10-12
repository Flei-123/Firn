# Browser build + WebSocket gateway (plan only, NOT built)

## Why a gateway
Browsers have no raw TCP. Minecraft Java protocol = TCP. So: **browser client <-WebSocket(binary)-> gateway <-TCP-> MC server.**
The gateway only copies bytes (frames of the MC protocol are already length-prefixed); optional compression/encryption stay end-to-end client<->server.

## Browser client
* Build target `web` (WASM, `lib/plat/@web`, `lib/fui/gpu.fi`); rendering via the WebGL2 backend of the GPU interface (r44). CPU raster as emergency fallback.
* Threads: Wasm threads need cross-origin isolation (COOP/COEP headers) + SharedArrayBuffer; otherwise single thread + Workers with message passing (mc_stream jobs).
* Assets: packs via file picker (File API) -> in-memory ZIP (mc_assets already works from bytes). Persistence: IndexedDB.
* Audio: WebAudio worklet fed from the synth bank.
* Online login: browser cannot hold the MS token safely -> device-code flow on a page, token kept in memory; the gateway must NOT see it (the MC session join is client -> sessionserver; CORS is the open point).

## Gateway
* Small server (Firn or any): `wss://gw/ws?target=host:port`, binary frames <-> TCP stream, backpressure both ways.
* Auth: signed short-lived token (JWT-like) issued by our site; gateway checks it before connecting. No open proxy.
* Target allow-list (default only configured servers; block private ranges 10/8, 127/8, 169.254/16, 192.168/16, ::1, fc00::/7 -> SSRF).
* Limits: connections per IP/account, bytes/s per connection, max frame size, idle + handshake timeouts, global connection cap.
* TLS terminated by reverse proxy; log metadata only (no payload, no tokens).
* Minimum MC awareness: none; optional status-ping cache.

## Effort
WASM build + WebGL2 backend: 4-6 d (GPU interface r44 is the prerequisite). Gateway: 1-2 d. Threads/COOP setup + IndexedDB + file picker: 1-2 d. Hardening/tests: 1-2 d.

## Open questions
* Hosted by us (cost, abuse, ToS of third-party servers) or self-host only? Which servers may be reached?
* Mojang session join from browser: CORS on sessionserver unknown -> may need a join proxy (sees the access token -> design!).
* Single thread performance target in WASM? Max view distance there?
* Offline-mode/own servers only first?
