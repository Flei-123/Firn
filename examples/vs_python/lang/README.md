# vs_python/lang -- language and tooling level, Firn vs Python (17.10.2026, vspy-lang)

- `01_script.{py,fi}`      script mode: `./01_script.fi` with `#!/usr/bin/env firn-run`
- `d/*.probe`              syntax probes (type inference, defaults, f-strings, slicing, ...)
- `e/*.probe`              idiom probes (comprehensions, closures, defer, enums, errors, maps, ...)
- `f/fNN_*.{probe,py}`     17 typical mistakes: firnc message vs Python message
- `p0*/task.{py,fi}`       three small programs, measured with tools/vspy/measure.py (`results_vspy-lang.tsv`)
- `PROBES.txt`             output of `bash tools/vspy/langprobe.sh <probes>` (OK = compiled+ran, ERR = firnc message)

`.probe` is used on purpose: many probes are *meant* to be rejected by firnc and must not be picked up by
the tree-wide `.fi` scans (firnfmt, parser compares). Run one with `bash tools/vspy/langprobe.sh [--msg] file`.
