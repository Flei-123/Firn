# Benchmark results (really measured)

Produced by `bench/run.sh` (`bench/bench.py`), 5 runs per program, **median**.
Every benchmark exists twice -- `bench/firn/<name>.fi` and `bench/rust/<name>.rs` -- and both print their result; the outputs have to match, otherwise the measurement stops.
The Rust side uses `std::hint::black_box` and the same unchecked pointer accesses as the Firn side, so that the same work is measured.

* CPU: AMD EPYC 7571 32-Core Processor
* system: Linux 7.0.14-5-pve x86_64
* rustc 1.99.0-nightly (c98d0cb27 2026-08-12)
* Firn: its own code generator, no external crates

| benchmark | Firn | Firn `--no-opt` | Rust `-O` | factor Firn/Rust | gain through the optimiser | result |
|---|---:|---:|---:|---:|---:|---:|
| fib | 0.056 s | 0.143 s | 0.032 s | **1.75x** | 2.54x | 4356618 |
| sieve | 0.119 s | 1.444 s | 0.032 s | **3.68x** | 12.10x | 697026 |
| matmul | 0.111 s | 2.168 s | 0.025 s | **4.48x** | 19.55x | 8291727 |
| bytecount | 0.507 s | 5.605 s | 0.193 s | **2.63x** | 11.05x | 1604208 |
| bubblesort | 0.103 s | 1.344 s | 0.039 s | **2.63x** | 13.10x | 12021846167 |
| statemachine | 0.253 s | 1.286 s | 0.085 s | **3.00x** | 5.08x | 6710880 |

Median over all benchmarks: **2.82x** slower than Rust `-O` (range 1.75x - 4.48x).
The optimiser (mem2reg, CSE, inlining, register allocation) brings **11.57x** in the median compared with `--no-opt`.

---

## A/B measurement of the optimiser round (14.08.2026)

The factors above scatter with the **Rust** time: on this machine the same
binary gives up to **40 % difference** between two runs. A codegen change of
5 % cannot be judged with that -- on the first attempt the very same
improvement showed up once as -18 % and once as +6 %.

That is why progress on the compiler has been measured over the
**executed instructions** since this round (`bench/instr.sh`, `valgrind
--tool=callgrind`). The number is reproducible down to the single instruction.

**State `e517942` (before the round) against `26861e3`+ (LICM, `lea`, inline limit):**

| program | instructions before | after | change |
|---|---:|---:|---:|
| matmul | 1,668,312,681 | 1,376,734,921 | **-17.48 %** |
| bubblesort | 811,682,925 | 667,321,089 | **-17.79 %** |
| bytecount | 2,579,216,109 | 2,148,310,351 | **-16.71 %** |
| sieve | 825,458,961 | 708,292,727 | **-14.19 %** |
| statemachine | 1,847,172,267 | 1,721,343,055 | **-6.81 %** |
| fib | 338,351,740 | 338,353,992 | +-0.00 % |

`fib` is pure recursion without loops and without field accesses -- there is
nothing to gain there for LICM and `lea`. The result is no error but the
proof of the point: the passes bite exactly where they are supposed to.

**An honest limit of the metric:** instructions are not run time. A `lea`
and a `div` both count as one. For the question "does the compiler emit
less work?" it is right, for the question "how fast is it?" it is not.

## Why the tokenizer does not get faster from it

Measured on the corpus `realweb` (4,931,819 bytes), both sides with callgrind:

| | instructions | per byte |
|---|---:|---:|
| Firn tokenizer | 4,033,688,605 | **818** |
| html5ever | 540,567,170 | **110** |

The ratio **7.46x** matches the measured time factor (**7.04x**) almost
exactly. That proves what the distance is NOT caused by: not by the quality
of the emitted code. Firn **executes seven and a half times as much work**.
A perfect code generator would change nothing about that.

The causes lie in the tokenizer and in the measuring setup, not in the compiler:

1. **Firn first decodes the input completely to UTF-32** (`mem.CpBuf`,
   4 bytes per character) and then tokenizes that buffer. html5ever works
   directly on the bytes. That is a complete additional pass over the
   input plus four times the memory traffic.
2. **No bulk path for runs of text.** html5ever looks for the next `<`, `&` or
   `\0` and emits everything in between as one block. Firn puts every character
   through the full state machine one by one -- which is exactly why the
   distance on `realweb` (long texts) is much larger at 7.0x than on
   `html5lib` (almost only edge cases) at 2.8x.
3. **The Firn run additionally writes the html5lib JSON**, html5ever only
   counts tokens. That work sits entirely in the 818 instructions per byte.

**Consequence for the roadmap:** the acceptance goal "<= 2x the reference"
cannot be reached with compiler work alone. The next step belongs to the
tokenizer (a byte path instead of a code point buffer, block processing for
runs of text) and to a fair measuring setup (the same output on both sides).
