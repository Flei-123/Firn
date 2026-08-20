# Round 63 -- the measurements of the JavaScript path

Produced by `bash tools/js/run.sh` / `tools/js/report.py`.
Nothing here is typed in by hand.

Reference: tc39/test262 @ `3655e7464de3d52643ecddd4b5f9f4f3e7f62398`, the subset of `testdata/test262/MANIFEST.md` (32,893 files).

## The parser

Does every case parse -- or fail to parse -- the way its metadata says?

| runs | passed | failed | quota |
|---:|---:|---:|---:|
| 63364 | 44341 | 19023 | 69.98% |

## The engine

Every case really executed. A case that uses a feature this engine does not have counts as a FAILURE like any other; nothing is filtered.

| runs | passed | failed | quota |
|---:|---:|---:|---:|
| 51664 | 21414 | 30250 | 41.45% |

### The failures by cause

| cause | cases |
|---|---:|
| not-reached | 12432 |
| unsupported-syntax | 9046 |
| throw | 7408 |
| crash | 572 |
| parse | 449 |
| unsupported-builtin | 300 |
| timeout | 40 |
| wrong | 3 |

`unsupported-syntax` is a program that the parser rejects because the feature is deliberately absent (generators, async, BigInt, private methods). `throw` is an exception the test did not expect -- usually a built in that does not exist. `wrong` is a case that ran through without the expected exception or delivered a wrong value: that is where the real bugs are.

### Per directory

| directory | runs | passed | quota |
|---|---:|---:|---:|
| built-ins/Boolean | 101 | 89 | 88.12% |
| built-ins/Error | 186 | 84 | 45.16% |
| built-ins/Function | 893 | 351 | 39.31% |
| built-ins/Infinity | 10 | 10 | 100.00% |
| built-ins/JSON | 330 | 162 | 49.09% |
| built-ins/Map | 405 | 204 | 50.37% |
| built-ins/Math | 654 | 352 | 53.82% |
| built-ins/NaN | 10 | 10 | 100.00% |
| built-ins/NativeErrors | 188 | 150 | 79.79% |
| built-ins/Number | 680 | 586 | 86.18% |
| built-ins/Object | 6802 | 4887 | 71.85% |
| built-ins/Set | 764 | 328 | 42.93% |
| built-ins/String | 2443 | 1499 | 61.36% |
| built-ins/Symbol | 192 | 56 | 29.17% |
| built-ins/isFinite | 30 | 26 | 86.67% |
| built-ins/isNaN | 30 | 26 | 86.67% |
| built-ins/parseFloat | 108 | 104 | 96.30% |
| built-ins/parseInt | 110 | 108 | 98.18% |
| built-ins/undefined | 12 | 10 | 83.33% |
| language/arguments-object | 460 | 139 | 30.22% |
| language/asi | 204 | 204 | 100.00% |
| language/block-scope | 287 | 263 | 91.64% |
| language/comments | 81 | 61 | 75.31% |
| language/computed-property-names | 96 | 66 | 68.75% |
| language/destructuring | 37 | 30 | 81.08% |
| language/directive-prologue | 62 | 50 | 80.65% |
| language/expressions | 21286 | 9648 | 45.33% |
| language/function-code | 281 | 240 | 85.41% |
| language/future-reserved-words | 85 | 85 | 100.00% |
| language/global-code | 75 | 33 | 44.00% |
| language/identifier-resolution | 22 | 20 | 90.91% |
| language/identifiers | 535 | 463 | 86.54% |
| language/keywords | 50 | 50 | 100.00% |
| language/line-terminators | 82 | 64 | 78.05% |
| language/literals | 1037 | 540 | 52.07% |
| language/punctuators | 22 | 22 | 100.00% |
| language/reserved-words | 53 | 53 | 100.00% |
| language/rest-parameters | 22 | 22 | 100.00% |
| language/source-text | 2 | 0 | 0.00% |
| language/statementList | 160 | 64 | 40.00% |
| language/types | 211 | 203 | 96.21% |
| language/white-space | 134 | 52 | 38.81% |
| language/statements | 9350 | 0 | not reached |
| built-ins/Array | 3082 | 0 | not reached |

