# mc_text -- chat components, legacy codes, styled spans

`lib/mc/mc_text.fi`: flattens a JSON chat component (string, object, array) into a `TextDoc` = plain text + spans, each span
with one effective style (colour 0xRRGGBB or `NO_COLOR`, flags bold / italic / underlined / strikethrough / obfuscated).

```
text_new / text_free
text_from_json(&doc, ptr, n, &translations) -> bool      false = not JSON (nothing added)
text_from_legacy(&doc, ptr, n)                            section-sign codes (§0-9a-f colours, §k §l §m §n §o, §r)
text_plain / text_legacy / text_ansi (&doc, &buf)         render
text_span_count / _off / _len / _color / _flags, text_ptr, text_len
tr_new / tr_load(&tr, ptr, n) / tr_count / tr_free        flat JSON object key -> format (minecraft-data's language.json works)
```

Rules (cross-checked against an independent python implementation): style inheritance through `extra`, `translate` arguments and array
tails (`[a, b, c]` is `a` with extra `b`, `c`); named and `#RRGGBB` colours, `"reset"` clears the colour, explicit `false` switches a
flag off; `translate` with `%s`, `%n$s`, `%%`, `fallback`, an unknown key shows as itself; `keybind` / `score` / `selector` are shown as
their placeholder (not resolved); a colour code in legacy text resets the formatting (as in the game). Limits: nesting 32, 1 MiB of text,
262144 spans (the `failed` flag marks a cut-off).

Notes: Firn string literals are NOT NUL terminated (use `"...".n` for the length, not `rt.c_length`); a literal `{` has to be written `\x7b`.
Tests: `bash tools/mc/run.sh text` (200 random components + 9 legacy strings: spans, plain, legacy and ANSI output identical to python;
1500-round fuzz x 3; the real language.json loads as translation table; counter-check), `tests/2742_mc_text_basic.fi`.
