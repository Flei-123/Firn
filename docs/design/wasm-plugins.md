# WASM plugin contract with per-plugin capabilities (M8c)

Status: design note, Q2 2027, no code.

## Goal

A host app (Certus page, fUi app, OrientOS app) loads a third-party WASM module as a plugin. The plugin gets exactly the
capabilities the host grants it and nothing else.

## Building blocks that exist

| block | what it gives | where |
|---|---|---|
| Firn-page contract (v1: 6 `firn` imports, 1 `env`, 5 optional `certus.fetch*`; v2 = async fetch ABI `m3fetch`) | the only doors a Firn WASM page has; enforced by the linker (`tools/wasm/contract_check.py`) | Certus repo `docs/wasm/FIRN-PAGES-CONTRACT*.md`, `lib/wasm/*` |
| Compile-time refusal of syscalls that have no import | a module cannot even name files/sockets/processes | docs/ROUND-WASM.md section 3 |
| Services contract v1 (file, clipboard, dialog, storage, dir, mut) with status codes and `svc.caps()` | one API for platform services, `NO_BACKEND` when not allowed | docs/services-contract.md, `lib/services/` |
| `#[export_c]` = WASM export, `extern fn` = import from `env` | the two doors | docs/ROUND-WASM.md |

## What a plugin contract adds

1. **Manifest** (custom section `firn.plugin`): name, version, `contract = 2`, required caps, optional caps. Same
   vocabulary as `svc.caps()` (`file.open`, `file.save`, `clipboard.write`, `clipboard.read`, `dialog`, `store`, `dir`,
   `fetch`, `draw`).
2. **Import gating:** the host instantiates the module with an import object that contains ONLY the functions of granted
   caps. A missing import is a link error for required caps, a stub that returns `svc.NO_BACKEND` for optional caps.
   No ambient authority: nothing outside the manifest is reachable, also not by accident.
3. **Plugin ABI:** the plugin exports `plugin_init(api_version) -> i32`, `plugin_call(id, ptr, len) -> i32`,
   `plugin_free(ptr)`; data crosses as `(ptr, len)` into the plugin's linear memory (the host copies in and out, never
   shares host memory). Async results come back through `plugin_event(kind, id, ptr, len)`, matching the v2 fetch model
   (`firn_web_fetch_done`) and the services reply queue (`svc.poll`).
4. **Grants are per plugin and per origin:** the host keeps a table `plugin -> caps`; `fetch` is further limited by the
   same-origin / `data-contract` rule of contract v2; `store` keys are namespaced `plugin/<name>/`; `dir` / `file` only
   through host pickers (the plugin never sees a path).
5. **Limits:** memory pages cap, fuel / time slice per call (Certus interpreter and AOT both need a hook), call depth.
   Over the limit = trap, host kills the instance and returns `svc.FAILED`.
6. **Validation before instantiate:** the existing WASM validator (Certus) plus contract check of imports against the
   manifest; reject undeclared imports with the name.

## Open questions

* Where does the services bridge live: in the host (Certus/fUi/Osum each own an adapter, like `lib/fui/services*.fi`) or
  as one `lib/services/svcplugin.fi` that all hosts share? Proposal: one shared module + thin host adapters.
* Fuel metering: Certus interpreter can count; AOT code needs inserted counters (cost?). Browsers (Chromium) have none: plugin time limits there
  rely on a worker + terminate.
* Plugin-to-plugin calls: out of scope for v1 (host mediates).
* Signing / provenance of plugins: separate topic.

## Steps

1. Spec `docs/design/wasm-plugins.md` -> `docs/wasm-plugin-contract.md` (host side imports table per cap).
2. Manifest section + `tools/wasm/plugin_check.py` (import set vs manifest).
3. `svcplugin.fi` gating layer over `lib/services` with caps table; test with a hostile plugin (imports everything).
4. Fuel/limits in the Certus interpreter; then Certus AOT.
5. Host adapters: Certus page host, fUi, OrientOS.
