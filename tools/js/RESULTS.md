# Round 66 -- the measurements of the JavaScript path

Produced by `bash tools/js/run.sh` / `tools/js/report.py`.
Nothing here is typed in by hand.

Reference: tc39/test262 @ `3655e7464de3d52643ecddd4b5f9f4f3e7f62398`, the subset of `testdata/test262/MANIFEST.md` (32,893 files).

## The parser

Does every case parse -- or fail to parse -- the way its metadata says?

| runs | passed | failed | quota |
|---:|---:|---:|---:|
| 63364 | 58259 | 5105 | 91.94% |

## The engine

Every case really executed. A case that uses a feature this engine does not have counts as a FAILURE like any other; nothing is filtered.

| runs | passed | failed | quota |
|---:|---:|---:|---:|
| 63364 | 45033 | 18331 | 71.07% |

### The failures by cause

| cause | cases |
|---|---:|
| throw | 12317 |
| parse | 3169 |
| unsupported-syntax | 1184 |
| async-incomplete | 1021 |
| unsupported-builtin | 310 |
| timeout | 264 |
| crash | 57 |
| wrong | 6 |
| unsupported-module | 3 |

`unsupported-syntax` is a program that the parser rejects because the feature is deliberately absent (after round 66: `eval`, the `Function` constructor, regular expressions, modules). `throw` is an exception the test did not expect -- usually a built in that does not exist. `async-incomplete` is a case with `flags: [async]` that ran through without ever printing `Test262:AsyncTestComplete`: its promise never settled. `wrong` is a case that ran through without the expected exception or delivered a wrong value: that is where the real bugs are.

### Per directory

| directory | runs | passed | quota |
|---|---:|---:|---:|
| built-ins/Array | 6117 | 4066 | 66.47% |
| built-ins/Boolean | 101 | 89 | 88.12% |
| built-ins/Error | 186 | 84 | 45.16% |
| built-ins/Function | 893 | 351 | 39.31% |
| built-ins/Infinity | 10 | 10 | 100.00% |
| built-ins/JSON | 330 | 162 | 49.09% |
| built-ins/Map | 405 | 208 | 51.36% |
| built-ins/Math | 654 | 352 | 53.82% |
| built-ins/NaN | 10 | 10 | 100.00% |
| built-ins/NativeErrors | 188 | 150 | 79.79% |
| built-ins/Number | 680 | 584 | 85.88% |
| built-ins/Object | 6802 | 4897 | 71.99% |
| built-ins/Set | 764 | 372 | 48.69% |
| built-ins/String | 2443 | 1503 | 61.52% |
| built-ins/Symbol | 192 | 58 | 30.21% |
| built-ins/isFinite | 30 | 26 | 86.67% |
| built-ins/isNaN | 30 | 26 | 86.67% |
| built-ins/parseFloat | 108 | 104 | 96.30% |
| built-ins/parseInt | 110 | 108 | 98.18% |
| built-ins/undefined | 12 | 10 | 83.33% |
| language/arguments-object | 460 | 359 | 78.04% |
| language/asi | 204 | 204 | 100.00% |
| language/block-scope | 287 | 197 | 68.64% |
| language/comments | 81 | 61 | 75.31% |
| language/computed-property-names | 96 | 74 | 77.08% |
| language/destructuring | 37 | 34 | 91.89% |
| language/directive-prologue | 62 | 50 | 80.65% |
| language/expressions | 21286 | 15433 | 72.50% |
| language/function-code | 281 | 240 | 85.41% |
| language/future-reserved-words | 85 | 85 | 100.00% |
| language/global-code | 75 | 43 | 57.33% |
| language/identifier-resolution | 22 | 20 | 90.91% |
| language/identifiers | 535 | 463 | 86.54% |
| language/keywords | 50 | 50 | 100.00% |
| language/line-terminators | 82 | 64 | 78.05% |
| language/literals | 1037 | 580 | 55.93% |
| language/punctuators | 22 | 22 | 100.00% |
| language/reserved-words | 53 | 53 | 100.00% |
| language/rest-parameters | 22 | 22 | 100.00% |
| language/source-text | 2 | 0 | 0.00% |
| language/statementList | 160 | 64 | 40.00% |
| language/statements | 18015 | 13490 | 74.88% |
| language/types | 211 | 203 | 96.21% |
| language/white-space | 134 | 52 | 38.81% |

