# Benchmark results (really measured)

Produced by `bench/run.sh` (`bench/bench.py`), 9 runs per program, **median**.
Every benchmark exists twice -- `bench/firn/<name>.fi` and `bench/rust/<name>.rs` -- and both print their result; the outputs have to match, otherwise the measurement stops.
The Rust side uses `std::hint::black_box` and the same unchecked pointer accesses as the Firn side, so that the same work is measured.

* CPU: AMD EPYC 7571 32-Core Processor
* system: Linux 7.0.14-5-pve x86_64
* rustc 1.99.0-nightly (c98d0cb27 2026-08-12)
* Firn: its own code generator, no external crates

| benchmark | Firn `release-fast` | Firn `release-safe` | Firn `dev-fast` (default) | `rustc -O` | factor fast | factor safe | factor devf | result |
|---|---|---|---|---|---|---|---|
| fib | 0.053 s | 0.051 s | 0.051 s | 0.033 s | **1.64x** | **1.56x** | **1.57x** | 4356618 |
| sieve | 0.033 s | 0.048 s | 0.139 s | 0.033 s | **1.01x** | **1.47x** | **4.24x** | 697026 |
| matmul | 0.044 s | 0.095 s | 0.253 s | 0.022 s | **2.01x** | **4.30x** | **11.42x** | 8291727 |
| bytecount | 0.268 s | 0.230 s | 0.860 s | 0.205 s | **1.30x** | **1.12x** | **4.18x** | 1604208 |
| bubblesort | 0.076 s | 0.106 s | 0.190 s | 0.041 s | **1.84x** | **2.57x** | **4.62x** | 12021846167 |
| statemachine | 0.162 s | 0.138 s | 0.235 s | 0.095 s | **1.71x** | **1.46x** | **2.48x** | 6710880 |

Median Firn `release-fast` against `rustc -O`: **1.67x** (range 1.01x - 2.01x).
Median Firn `release-safe` against `rustc -O`: **1.52x** (range 1.12x - 4.30x).
Median Firn `dev-fast` (default) against `rustc -O`: **4.21x** (range 1.57x - 11.42x).

`release-fast` is the like-for-like comparison: all passes, and integer arithmetic unchecked exactly as `rustc -O` leaves it. `release-safe` runs the same passes and CHECKS every integer operation, so it is Firn doing strictly more work than Rust. `dev-fast` is what a plain `firnc` gives you: checked, and without the one pass that would make the call stack unreadable.

## Round SPEED (27.08.2026) — the two passes, and what changed

The table above is pass 2. The machine is shared, so a second pass is
printed next to it; the honest statement is the pair, not one of them.

| benchmark | factor `release-fast` pass 1 | pass 2 |
|---|---:|---:|
| fib | 1.91x | 1.64x |
| sieve | **0.85x** | **1.01x** |
| matmul | 2.15x | 2.01x |
| bytecount | 1.30x | 1.30x |
| bubblesort | 1.82x | 1.84x |
| statemachine | 1.71x | 1.71x |
| **median** | **1.76x** | **1.67x** |

`release-safe`: **1.55x** / **1.52x** median. `dev-fast`: 4.14x / 4.21x.

### Against the state this round started from

`bench/RESULTS.md` said 2.08x median and `sieve` 4.16x, and
`orientos/ROADMAP.md` point 4.9 quoted those numbers. **Round 1 of
`docs/ROUNDSPEED.md` found that they were measured at `dev-fast`** — the
default level since round 72, which checks every integer operation and does
not inline — while the Rust side was `rustc -O`. The comparison Rust does
with Rust is `release-fast`, and this file now names the level in every
column.

| benchmark | old table (`dev-fast`, called "Firn") | now `release-fast` |
|---|---:|---:|
| fib | 1.52x | 1.64x |
| bytecount | 1.43x | 1.30x |
| statemachine | 1.99x | 1.71x |
| bubblesort | 2.17x | 1.84x |
| matmul | 2.90x | 2.01x |
| **sieve** | **4.16x** | **1.01x** |
| **median** | **2.08x** | **1.67x** |

Two things did the work, and they are separate. The **naming** of the build
level is what moves `sieve` from 4.16x to about 1x — that number was never a
codegen result, it was a checked, uninlined build measured against an
unchecked, inlined one. The **optimiser rounds 2 to 11** are what moved the
rest: block layout, exact loop depth, loops laid out in one piece, division
by a constant without `div`, the range analysis that removes checks which
provably cannot fire, `lea` for scaling, the bool cells of `&&` / `||` that
had been stuck in memory since round 92, and the same threading through a
bool phi. Every one of them is in `docs/ROUNDSPEED.md` with its own before
and after.

**The target of this round was median under 1.5x and `sieve` under 2.5x.**
`sieve` is at **1.01x**, so that one is met with room to spare. The median
is **1.67x** at `release-fast` and **1.52x** at `release-safe` — close, and
not there. `matmul` at 2.01x is the one that now carries it, and its cause
is named and not guessed: no vector instructions are emitted, and the
register allocation is linear rather than colouring.

**A note on the conditions.** These two passes were measured on a machine
that was doing other work at the same time (load average 14 to 17 on
12 cores) and with the root file system at 98 %. That widens the scatter of
BOTH sides — `fib` at `release-safe` came out faster than at `release-fast`
in pass 1, which cannot be true of the code and is the clearest available
statement about what this wall clock can resolve. For differences under
about 5 %, `tools/bench90/icount.py` (executed instructions, exact) is the
instrument, not this table.
