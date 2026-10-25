# firnc1 diagnostics: remaining fallbacks

`complain(s)` in `lib/firnc1/sema.fi` reports "the type checker error" at the current position. After r1003 the call sites below are the only ones left. Each is either unreachable from source or only follows a message that was already reported.

| Site | Why it stays |
|---|---|
| `lambda_check`: `!has_gc`, `!has_fe`, `AllocError` missing | The driver always registers the gc and error-set registries and the runtime declares `AllocError`; no source program reaches these. |
| `gc_as_check`: no receiver child | The parser always builds the receiver node. |
| `methods_call`: `cnt < 1` | The parser always puts the receiver first. |
| `slit` missing-fields list empty | Guarded by the caller: the list is only built when a field is missing. |
| array repeat `[x; n]` tail after element/length messages | Errors were already said, `complain` only ends the expression. |
| `const_eval` failed without `ce_say` text (static/const/array count) | Every failing branch of `const_eval` records its own text first; the fallback guards a future branch. |
| `match_check`: match index out of range | Internal registry invariant. |

Known remaining deviations from firnc0: none for f-strings (`!r`/`!s`, `"a" + f"{n}"`, lexer errors inside an interpolation fragment and closure bodies in `#[no_gc]` match firnc0).
