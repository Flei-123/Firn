# Declarative macros (design draft, nothing implemented)

Goal: user-defined syntax sugar without a general compile-time language. `derive` (docs/DERIVE.md) covers the common cases; macros cover the rest.

## Form
```
macro unless(cond: expr, body: block) {
    if !(cond) body
}
macro min_of(a: expr, b: expr) -> expr {
    if a < b { a } else { b }
}
unless(x > 3, { return 1 })
```
- `macro name(pattern, ...) { template }`; fragment kinds: `expr`, `ident`, `ty`, `block`, `stmt`, `lit`, `tokens` (rest of the list).
- Repetition: `$(item: expr),*` in the pattern, `$(...)*` in the template, separator optional.
- Several rules per macro: `macro m { (a: expr) => { ... } (a: expr, b: expr) => { ... } }`, first matching rule wins.

## Expansion
- Runs after lexing and before parsing as a TOKEN pass (same place as derive), in firnc0 and firnc1 identically; the output is plain tokens.
- Hygiene: identifiers introduced by the template (`let t = ..`) are renamed with a per-expansion suffix (`t#7`), identifiers coming from the arguments keep their name. A template never captures a caller variable by accident.
- Expansion depth limit 64 and size limit 1 MiB of tokens, else an error at the call site.
- Macros are module-local; `export macro` makes one importable (`import m.mymacros` brings it in).

## Errors
- Every error carries the position of the CALL SITE and, in a note, the position inside the macro: `no rule of macro 'min_of' matches 3 arguments (--> call 12:5, rules at 4:1)`.
- Unknown fragment kind, unbound `$name`, recursion past the limit, unused pattern variable: errors at the macro definition.

## Plan (roadmap)
1. Token model shared by both compilers (derive already works on text; move to tokens for spans).
2. Pattern matcher + template expander for `expr/ident/ty/block/lit` without repetition (firnc0), tests + neg tests.
3. Repetition `$(...)*`, multiple rules.
4. Hygiene renaming + tests that prove no capture.
5. Port to firnc1 (`lib/firnc1/macro.fi`), tests/diff.
6. `export macro` across modules, LSP expansion preview, `firn expand FILE`.
