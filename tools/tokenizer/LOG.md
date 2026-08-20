# Job protocol of the tokenizer driver

A contract between `lib/html/tokenize_main.fi` (Firn) and
`tools/tokenizer/harness.py` (the workbench). Whoever changes one side changes
the other with it -- and nothing else.

## Input (stdin, binary, little-endian)

A stream of jobs, without a header, without an end marker:

| field | type | meaning |
|---|---|---|
| `startzustand` | `u32` | 0 = Data, 1 = PLAINTEXT, 2 = RCDATA, 3 = RAWTEXT, 4 = Script data, 5 = CDATA section |
| `flaggen` | `u32` | bit field, see below. Bit 0 = XML adjustment. All other bits are 0 |
| `len_lasttag` | `u32` | length of `lasttag` in bytes |
| `lasttag` | `u8[len_lasttag]` | `lastStartTag` of the test case, WTF-8 |
| `len_input` | `u32` | length of `input` in bytes |
| `input` | `u8[len_input]` | input text, WTF-8 (keeps unpaired surrogates) |

### Flags

| bit | name | effect |
|---|---|---|
| 0 | `XML_MODUS` | XML adjustment of the token stream following the section "Coercing an HTML DOM into an infoset" of the HTML standard. The harness sets it exactly for the cases under the key `xmlViolationTests` (file `xmlViolation.test`, 4 cases); `--no-xml-mode` switches it off. |

Implemented in XML mode (`lib/html/tokens.fi`, `xml_cp`, `out_json_cpbuf_text`,
`out_json_comment`), as far as the token stream is concerned:

* Characters that XML 1.0 does not know (C0 control characters except TAB/LF,
  unpaired surrogates, the non-characters `U+FFFE`/`U+FFFF` of every plane) are
  replaced by `U+FFFD` in text, attribute values and comments.
* `U+000C` (FORM FEED) becomes `U+0020` (SPACE) instead of `U+FFFD`.
* In comment text a `U+0020` is inserted between two consecutive `U+002D`; if
  the comment ends in `U+002D`, a `U+0020` follows.

Element, attribute and DOCTYPE **names** stay untouched: adjusting those
concerns the DOM building, not the token stream.

Without the flag the driver behaves exactly as before (the plain HTML
standard).

The driver reads to the end of the input and answers **every** job
with **exactly one** line.

## Output (stdout, pure ASCII, one line per job)

* The normal case: the html5lib token array, e.g.
  `[["StartTag","div",{"a":"b"}],["Character","x"],["EndTag","div"]]`
* the unsupported marker: the tokenizer reached a state it does not
  implement yet, or a means was not available (for example the name table of
  the character references could not be created, see
  `lib/html/entities.fi`). The harness counts such a thing as a **failure** --
  never as "skipped".

### The second field: the parse errors

Behind the token array follows a **tab character** (`0x09`) and after it the
list of the parse errors of this job as JSON, in the order in which they
occurred:

```
[]\t[{"code":"eof-in-tag","line":1,"col":5}]        (input: <div)
```

* `code` is the WHATWG code name from 13.2 "Parse errors"
  (`lib/html/error_codes.fi` carries all the names the tokenizer can
  report).
* `line` counts from 1, `col` is the column **behind** the character that
  triggered the error -- the same counting as in the `errors` lists of the
  html5lib suite. Line breaks are those of the normalised input stream
  (`\r\n` and `\r` are already `\n` there).
* Without an error `[]` stands there. The field is never missing.
* The harness only compares it with `--with-errors`; without the switch only
  the token stream counts. `run.sh` reports **both** quotas.

All characters outside `0x20..0x7E` are written as `\uXXXX`
(code points > 0xFFFF as a surrogate pair). That makes the line ASCII and
`json.loads` gives unpaired surrogates back as well.

EOF produces **no** token (html5lib does not carry EOF in `output`).
Consecutive characters are merged into **one** `Character` token.
