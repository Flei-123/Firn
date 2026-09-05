# `bench/tokenizer` -- the yardstick html5ever

A Cargo project of its own, **never** a dependency of the compiler
(`compiler/Cargo.toml` stays without external crates). It tokenizes the same
input corpus as the Firn tokenizer with `html5ever` and prints the number of
tokens and the time.

## Building and measuring

```
cargo build --release --manifest-path bench/tokenizer/Cargo.toml
bash tools/tokenizer/throughput.sh .tokenizer-work/tokenize
```

`throughput.sh` produces TWO corpora (`tools/tokenizer/korpus.py`), measures
the Firn tokenizer first each time and after that -- as soon as this binary is
built -- `html5ever` on **the same file**:

* `.tokenizer-work/korpus.html5lib.html` (4.08 MB) -- the inputs of the
  html5lib cases, **deliberately pathological** (almost only edge cases, very
  many state changes per byte, hardly any long runs of text): the worst case.
* `.tokenizer-work/korpus.realweb.html` (4.70 MB) -- eight stored real
  pages from `testdata/realweb/` (see `testdata/realweb/MANIFEST.md`):
  the everyday case.

Both times are taken from the outside (process start, reading, tokenizing),
so they are measured alike.

A single call by hand works as well:

```
bench/tokenizer/target/release/html5ever_bench .tokenizer-work/korpus.html5lib.html
tokens=771264 zeichen=1810815 bytes=4275135 sekunden=0.396089
```

## Measured here (14.08.2026, a shared machine under load)

Corpus: `.tokenizer-work/korpus.html5lib.html`, 4.08 MB, 302,912 `&`, 109,696 `<`.
Three runs each, CPU time (`user`+`sys`, more stable than the wall clock):

| | CPU time (best run) | factor |
|---|---:|---:|
| Firn (`lib/html/`, all 6,810 cases passed except 3) | 0.886 s | **2.6x** |
| html5ever 0.27, `--release` | 0.335 s | 1.0x |

The wall-clock values from `throughput.sh` scatter between 1.8x and 5.0x on
this machine depending on the foreign load -- the CPU time above is the more
reliable value. The goal of the acceptance (<= 2x) is therefore **missed**;
the real factor stands here.

On the second corpus (`realweb`, real pages) the distance is **larger**:
three runs on 14.08.2026 gave 5.72x / 7.72x / 7.84x (Firn 5.5-7.4 MB/s
against html5ever 42-45 MB/s). Long runs of text are html5ever's best case,
while the Firn tokenizer still works code point by code point and
additionally writes the html5lib JSON. The numbers are in ACCEPTANCE.md
item 3.

## What is compared -- and what is not

* The same corpus, the same kind of measurement (wall clock around the whole
  process).
* Both sides drive the full state machine including character references.
* The Firn driver additionally writes html5lib JSON to standard output;
  `html5ever` only counts tokens here. The factor therefore comes out rather
  too favourable for Firn than too bad -- it is reported all the same the way
  it was measured.
* The machine is shared: under load **both** values scatter. What counts is
  the ratio from one run, not the absolute MB/s value.
