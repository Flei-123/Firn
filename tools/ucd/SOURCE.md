# Where `UnicodeData.txt` comes from

| | |
|---|---|
| **File** | `UnicodeData.txt` of the Unicode Character Database |
| **Version** | **Unicode 17.0.0** (`ReadMe.txt` of the same directory, dated 2025-08-15) |
| **URL** | <https://www.unicode.org/Public/UCD/latest/ucd/UnicodeData.txt> |
| **Fetched** | 2026-08-23 |
| **Size** | 2,198,209 octets, 40,575 lines |
| **sha256** | `2e1efc1dcb59c575eedf5ccae60f95229f706ee6d031835247d843c11d96470c` |

Fetch it again and compare:

```sh
curl -sO https://www.unicode.org/Public/UCD/latest/ucd/UnicodeData.txt
sha256sum -c tools/ucd/UnicodeData.sha256
```

`tools/ucd/run.sh` checks the sum as step 0 and stops on any deviation --
a table generated from a changed input would prove nothing about the UCD.

The file lies **next to** `ucd_real.fi` and not in `testdata/`, because
compile-time file access is deliberately restricted to the directory of the
root source file: no `..`, no absolute path (SPEC 14.1.comptime, supply
chain security; negative tests `tests/neg/comptime_file_absolute.fi` and
`comptime_file_parent.fi`).

Terms of use of the data: <https://www.unicode.org/terms_of_use.html>.

# Die Bidi-Dateien (seit 23.09.2026)

`tools/ucd/build_bidi.sh` baut daraus `generated/bidi_tables.fi` (die
Bidi-Klasse, die arabische Verbindungsart, Spiegel- und Klammerpaare, die
arabischen Darstellungsformen). Alle vier sind **Unicode 17.0.0**, geholt
am 2026-09-23, die Summen stehen in `tools/ucd/UCD_BIDI.sha256`.

| Datei | URL | Oktette |
|---|---|---|
| `DerivedBidiClass.txt` | <https://www.unicode.org/Public/17.0.0/ucd/extracted/DerivedBidiClass.txt> | 173,433 |
| `BidiMirroring.txt` | <https://www.unicode.org/Public/17.0.0/ucd/BidiMirroring.txt> | 26,827 |
| `BidiBrackets.txt` | <https://www.unicode.org/Public/17.0.0/ucd/BidiBrackets.txt> | 8,891 |
| `ArabicShaping.txt` | <https://www.unicode.org/Public/17.0.0/ucd/ArabicShaping.txt> | 41,441 |

Neu holen und vergleichen: `bash tools/ucd/build_bidi.sh --fetch`
(bricht ab, wenn eine Summe nicht mehr passt).
