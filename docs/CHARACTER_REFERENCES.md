# Character references in the HTML5 tokenizer (`lib/html/entities.fi`)

Module `tokenizer-text` from `PLAN.md` §1. Implements the character
reference states of the WHATWG HTML standard (§13.2.5.72 – §13.2.5.80)
**in Firn** and provides the official name table with **2.231** entries.

## Files

| File | Role |
|---|---|
| `lib/html/entities.fi` | states, name lookup, numeric references (Firn, ~330 lines) |
| `lib/html/entities_data.fi` | the **generated** name table as u64 words (Firn, ~4.660 lines) |
| `tools/tokenizer/gen_entities.py` | generator of the table from `html.entities.html5` |
| `lib/html/entities_probe.fi` | test bench: run the character reference part only (Firn) |
| `tools/tokenizer/check_entities.py` | workbench: test bench against the html5lib data |

## Interface (contract with `lib/html/tokenizer.fi`, PLAN.md §2.3)

```
fn char_ref(input: *mut mem.CpBuf, pos: usize, in_attr: bool,
            s: *mut tokens.Sink) -> usize
fn char_ref_out(input: *mut mem.CpBuf, pos: usize, in_attr: bool,
                s: *mut tokens.Sink, out: u32) -> usize
```

`pos` points behind the `&`. The return value is the new position; output
goes through `tokens.sink_emit_char` resp. — with `in_attr` — through
`tokens.tok_attr_value_push`. The call cannot fail: in the
worst case the `&` itself is emitted. There is **no** state
„not supported" here.

`char_ref_out` is the same function with a separate output switch
(`out == 0` -> character stream, otherwise attribute value), in case the
special rule of the standard and the output target should ever come apart.

## Implemented rules

* **Character reference state**: `#` -> numeric, alphanumeric -> name,
  otherwise emit only `&`.
* **Named character reference state**: longest match first, names with and
  without a semicolon (`&amp` just like `&amp;`), replacement with one
  **or two** code points (93 of the 2.231 entries have two).
* **Special rule in an attribute value**: a name without a semicolon,
  followed by `=` or an alphanumeric character -> do not replace, text
  unchanged.
* **Ambiguous ampersand state**: no match -> `&` and the alphanumeric
  sequence unchanged; `;` is given back to the calling state.
* **Numeric character reference**: decimal and hexadecimal (`&#x…`/`&#X…`),
  a missing semicolon permitted, missing digits emit `&#`/`&#x` verbatim,
  overflow is caught (numbers > 0x10FFFF).
* **Numeric character reference end state**: `0`, values > 0x10FFFF and
  surrogates become U+FFFD; the C1 replacement table (0x80–0x9F, 27 values)
  is fully implemented.

## The name table: why generated source text

Stage 0 knows neither string literals nor global arrays (`const` only
scalar, see SPEC §14.1). The table is therefore written as a sequence of
u64 words into a memory area:

```
0            u64   the id
OFF_NAMES    u8[]  all 2,231 names one after another (16,641 byte), sorted
OFF_LEN      u8[]  the length per entry
OFF_VALUE    u64[] the substitute: cp1 | cp2 << 32   (cp2 == 0: one character)
OFF_POS      u32[] the start per name -- computed at the first access
BYTES        u32[] index by the first character (256 x start/end)
```

The area lies at the fixed address `0x6000_0000_0000` and is created on
first access with `mmap(MAP_FIXED_NOREPLACE)`; if the kernel reports
`EEXIST`, it is already there (the identifier is checked). The lookup is a
binary search inside the range that the directory gives for the first
character; the candidate stands in an array on the stack
(`[u32; 33]`) — a character reference requests **no** memory.

The table comes from `html.entities.html5` of the Python standard library
(the official WHATWG list), **not** from the test data:

```
python3 tools/tokenizer/gen_entities.py
```

## Proof

```
compiler/target/release/firnc -o .tokenizer-work/entities_probe lib/html/entities_probe.fi
python3 tools/tokenizer/check_entities.py
```

The test bench takes from the official html5lib data all cases that can be
decided with the character reference part alone (data state, no `<`,
expectation only character tokens) and compares character by character:

```
Zeichenreferenzen (lib/html/entities.fi), reine Data-state-Faelle
  bestanden: 4657 / 4657
```

That is a **module proof, not a balance** — the binding number over all
6.810 cases is delivered by `tools/tokenizer/run.sh` alone. There the
character references contribute `namedEntities.test` 4210/4210,
`numericEntities.test` 336/336 and `entities.test` 80/80 (the last one
contains the attribute value special cases as well that the narrow test
bench does not cover).

## Costs, honestly

* One `mmap` call per character reference (the check „is the table already
  there?" cannot be had without a system call in the absence of global
  variables). On the measuring machine around 1 µs; on the entity-heavy
  measuring corpus (302.912 `&` in 4,08 MB) that is about 0,12 s of around
  0,9 s total time. As soon as stage 0 knows global data, this falls away —
  until then it is stated here.
* The lookup itself costs around 0,9 µs per reference (binary search in the
  first-character range).
* Without character references the same corpus needs around 0,48 s, with
  them around 0,89 s of CPU time. `html5ever` needs 0,34 s for the same
  corpus — a factor of 2,6x for the whole Firn tokenizer (measurement and
  method: `bench/tokenizer/README.md`).

## Open

* No caching of the table pointer (see above).
* `char_ref` reports no parse errors to the outside; the html5lib balance
  compares only the token stream, and `ParseError` entries are removed in
  the harness anyway.
