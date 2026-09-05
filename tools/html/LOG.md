# Job protocol of the tree building driver

A contract between `lib/browser/parse_main.fi` (Firn) and
`tools/html/harness_tree.py` resp. `tools/html/realweb.py` (the workbench).
Whoever changes one side changes the other with it -- and nothing else.

## Input (stdin, binary, little-endian)

A stream of jobs, without a header, without an end marker:

| field | type | meaning |
|---|---|---|
| `len_input` | `u32` | length of `input` in bytes |
| `input` | `u8[len_input]` | the HTML document, WTF-8 (keeps unpaired surrogates) |

Line endings are normalised by the driver (`\r\n` and `\r` -> `\n`), exactly
as in the tokenizer driver (`lib/html/tokenize_main.fi`).

## Output (stdout, UTF-8)

Per job the tree in the **`.dat` format of the html5lib `tree-construction`
tests**, followed by a line `#ENDE`:

```
| <!DOCTYPE html>
| <html>
|   <head>
|   <body>
|     <p>
|       "x"
#ENDE
```

Rules of the format (`lib/browser/write.fi`):

* Every line begins with `| `, then **two spaces per level**. The children of
  the document stand at level 0.
* Element: `<name>`. Outside the HTML namespace with a prefix: `<svg circle>`,
  `<math mi>`.
* Attributes stand **before** the children, one level deeper than their
  element, and are **sorted by name** -- the DOM keeps the order of the source
  text, only the output sorts. Namespace attributes with a prefix:
  `xlink href="..."`.
* Text: `"data"`, without escaping.
* Comment: `<!-- data -->`.
* Doctype: `<!DOCTYPE name>` resp. `<!DOCTYPE name "public" "system">`.

If the driver does not get through, a line `#KAPUTT <code>` stands **before**
the tree; the runner then counts the case as a failure. The codes are in
`lib/browser/driver.fi` (`lauf_dokument_bauen`):

| code | meaning |
|---|---|
| 1 | the tokenizer reached a state it does not implement |
| 2 | the tree building stopped (stack overflow or out of memory) |
| 3 | too many tokenizer switch-overs (can only be a bug in the code) |
| 9 | no document (out of memory) |

## Why the detour through a buffer

The tokenizer is completely `#[no_gc]` (SPEC 3.5.4) -- it must not call a
function that asks for GC memory. The tree building does exactly that (every
node is a GC object). Between the two there is therefore a **binary token
protocol** (`lib/html/tokens.fi`, `tb_*`; read by
`lib/browser/token_stream.fi`).

Besides the token, every record carries the **source position immediately
behind the token**. The tree building needs it to send the tokenizer into a
different start state at `<title>`, `<style>`, `<script>`, `<textarea>` and
`<plaintext>` (the WHATWG "generic raw text element parsing algorithm"). How
that is done -- and what it costs -- is written in the head of
`lib/browser/driver.fi`.

## Round B1 — one case of this suite was rewritten

`06_selection_frames.dat`, the case `<select><div>x</div></select>`, used to
expect `<select>"x"` — the div was dropped. WHATWG has since removed the
insertion modes "in select" and "in select in table" (relaxed select
parsing): a `select` takes arbitrary content, so the expectation is now
`<select><div>"x"`. The same case stands with the same expectation in
`tests/data/html5lib/webkit02.dat`. The note lives here and not in the
`.dat` file, because the `.dat` format has no comment lines — a `#` line
outside a section starts a new case.

The three gaps of `tools/html/gaps/` (foreign content, `<template>`,
fragment parsing) are closed in round B1; see `docs/ROUNDB1.md`.
