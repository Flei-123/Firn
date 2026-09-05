# Third party material in Firn

Everything below keeps its own licence and is NOT covered by the MPL-2.0
of the rest of this repository.

| Path | Origin | Licence |
|---|---|---|
| `tools/ucd/UnicodeData.txt`, `tools/ucd/DerivedCoreProperties.txt` | Unicode Character Database 17.0.0, unicode.org | Unicode Licence v3 (permissive, attribution) |
| `tests/data/fonts/Ahem.ttf` | CSS working group test font, via web-platform-tests | Public domain (Todd Fahrner, 1995); WPT itself BSD-3-Clause |
| `tests/data/fonts/FirnSans.ttf`, `tools/layout/FirnMetric.ttf` | Subsets of DejaVu Sans, made here with fontTools | DejaVu / Bitstream Vera licence (permissive, attribution) |

Provenance, versions and checksums are documented in `tools/ucd/SOURCE.md`
and `tests/data/fonts/PROVENANCE.md`.

No third party *source code* is vendored in this repository. The compiler,
the standard library and the code generator were written from scratch.
