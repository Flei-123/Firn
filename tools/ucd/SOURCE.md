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
