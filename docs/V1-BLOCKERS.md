# Open questions before Firn 1.0

Found while writing `docs/STABILITY.md`. Each item is a roadmap point (group "v1.0-Stabilitaet"). Fixing later is compatible only if marked (C); (B) means it breaks programs and must be settled before the freeze.

| # | Question | Why it blocks | Kind |
|---|---|---|---|
| 1 | **Reference implementation**: `SPEC.md` and the code diverge (section 10.5 still lists WASM/aarch64 as postponed; v0.x text). Which one decides? | A promise needs one normative text. Today the frozen corpus is the only authority. | B |
| 2 | **Overflow and `release-fast` semantics**: checked levels panic, `release-fast` wraps. Is wrap defined or unspecified? | Programs relying on either behave differently per level. Decide and document in SPEC 13. | B |
| 3 | **`Option[T]` is a struct** because there are no generic enums (`lib/std/option.fi`). Will generic enums come? | Changing `Option` to an enum breaks every user. | B |
| 4 | **Unary `*` vs `as` precedence** (`*p as u64` parses as `*(p as u64)`), negative literals needing `0 - 1`, statics limited to number/byte-array literals, `&x as u64` rejected. | Surprising grammar quirks must be fixed (or declared final) before the grammar freezes. Needs verification against SPEC 12. | B |
| 5 | **`std.rt` surface** is imported by 729 tests but is runtime internals. Split into a stable part and an internal part. | Otherwise the core list cannot include it and corpus programs depend on an unstable module. | B |
| 6 | **Module names are global** (`Modulnamen global eindeutig`). Package-qualified names? | Package ecosystem cannot grow without clashes; changing import resolution later breaks code. | B |
| 7 | **Package management** (SPEC 15.6): manifest, lock file and local `needs` exist; registry, version resolution rules and per-package editions do not. | Edition of dependencies is ignored today; manifest keys must be final. | B |
| 8 | **SIMD surface** (SPEC 15.4): intrinsics or vector types? | Intrinsics stay experimental until decided. | C |
| 9 | **Conditional moves** (SPEC 15.5): rejected conservatively. | Loosening is compatible; document the rule. | C |
| 10 | **Coroutine stack size** and growth (SPEC 15.2). | Affects `Io` async behavior limits, not syntax. | C |
| 11 | **GC: conservative, non-moving** (SPEC 15.1). Promise "objects never move"? | Needed for FFI pointers; precise scanning would be an internal change. | B |
| 12 | **12 open non-guarantees** in `docs/SAFETY.md`. Fix or declare permanent each one. | The safety claim "fast and secure" must say exactly what is not covered. | B |
| 13 | **Two compilers** (`firnc0` Rust, `firnc1` Firn). `tools/diff_c0c1.sh` must stay at 0 differences; firnc1 needs the same stable flags. | Stable behavior must not depend on which compiler built the program. | C |
| 14 | **ABI** (`L13`): the calling convention for Firn-to-Firn and `#[export_c]` is stable only on x86-64 and aarch64 Linux; Windows and kernel profiles are not listed. | Stable targets list in STABILITY.md. | C |
| 15 | **Name**: rename Firn to Fission is not approved. Keep the manifest file and CLI names as they are (`brand_compat`). | Renaming after 1.0 breaks every project. | B |
| 16 | **Diagnostics**: error text is unstable by design; tools (LSP, editors) need a machine-readable code per error. | Without error codes tools parse text. | C |
| 17 | **Edition differences list** does not exist yet: only `2026` is defined. The first change that needs a second edition starts the list. | Mechanism is in place (`--edition`, manifest key), content is empty. | C |
| 18 | **`unsafe fn` in the std core**: N16 marked `std.math.frexp/ldexp` as `unsafe fn`; the API gate flagged it as a signature change. Is `unsafe` part of the stable signature? Callers break under `--unsafe=error`. | Decide before the std core list is final; the frozen list was refreshed once for this (pre-1.0). | B |
