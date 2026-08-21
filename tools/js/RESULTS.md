# Round 74 -- the measurements of the JavaScript path

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
| 63364 | 48155 | 15209 | 76.00% |

### The failures by cause

| cause | cases |
|---|---:|
| throw | 9672 |
| parse | 3169 |
| unsupported-syntax | 1184 |
| async-incomplete | 830 |
| timeout | 282 |
| crash | 55 |
| unsupported-builtin | 8 |
| wrong | 6 |
| unsupported-module | 3 |

`unsupported-syntax` is a program that the parser rejects because the feature is deliberately absent (after round 74: `eval`, the `Function` constructor, modules -- the regular expressions moved into the engine in this round). `throw` is an exception the test did not expect -- usually a built in that does not exist. `async-incomplete` is a case with `flags: [async]` that ran through without ever printing `Test262:AsyncTestComplete`: its promise never settled. `wrong` is a case that ran through without the expected exception or delivered a wrong value: that is where the real bugs are.

### Per directory

| directory | runs | passed | quota |
|---|---:|---:|---:|
| built-ins/Array | 6117 | 4505 | 73.65% |
| built-ins/Boolean | 101 | 91 | 90.10% |
| built-ins/Error | 186 | 104 | 55.91% |
| built-ins/Function | 893 | 367 | 41.10% |
| built-ins/Infinity | 10 | 10 | 100.00% |
| built-ins/JSON | 330 | 170 | 51.52% |
| built-ins/Map | 405 | 319 | 78.77% |
| built-ins/Math | 654 | 530 | 81.04% |
| built-ins/NaN | 10 | 10 | 100.00% |
| built-ins/NativeErrors | 188 | 164 | 87.23% |
| built-ins/Number | 680 | 622 | 91.47% |
| built-ins/Object | 6802 | 5695 | 83.73% |
| built-ins/Set | 764 | 612 | 80.10% |
| built-ins/String | 2443 | 2171 | 88.87% |
| built-ins/Symbol | 192 | 136 | 70.83% |
| built-ins/isFinite | 30 | 26 | 86.67% |
| built-ins/isNaN | 30 | 26 | 86.67% |
| built-ins/parseFloat | 108 | 104 | 96.30% |
| built-ins/parseInt | 110 | 108 | 98.18% |
| built-ins/undefined | 12 | 10 | 83.33% |
| language/arguments-object | 460 | 361 | 78.48% |
| language/asi | 204 | 204 | 100.00% |
| language/block-scope | 287 | 197 | 68.64% |
| language/comments | 81 | 61 | 75.31% |
| language/computed-property-names | 96 | 74 | 77.08% |
| language/destructuring | 37 | 34 | 91.89% |
| language/directive-prologue | 62 | 50 | 80.65% |
| language/expressions | 21286 | 15638 | 73.47% |
| language/function-code | 281 | 240 | 85.41% |
| language/future-reserved-words | 85 | 85 | 100.00% |
| language/global-code | 75 | 51 | 68.00% |
| language/identifier-resolution | 22 | 21 | 95.45% |
| language/identifiers | 535 | 463 | 86.54% |
| language/keywords | 50 | 50 | 100.00% |
| language/line-terminators | 82 | 64 | 78.05% |
| language/literals | 1037 | 636 | 61.33% |
| language/punctuators | 22 | 22 | 100.00% |
| language/reserved-words | 53 | 53 | 100.00% |
| language/rest-parameters | 22 | 22 | 100.00% |
| language/source-text | 2 | 0 | 0.00% |
| language/statementList | 160 | 80 | 50.00% |
| language/statements | 18015 | 13664 | 75.85% |
| language/types | 211 | 203 | 96.21% |
| language/white-space | 134 | 102 | 76.12% |

