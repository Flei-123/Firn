# `bootstrap/` — the frozen way in

This directory answers one question: **how do you get a working Firn
compiler onto a machine that has no Firn compiler on it?**

Until round BOOTSTRAP the answer was "install Rust and run `cargo build`".
That made Rust a permanent dependency of a language that claims to carry
itself. It is not one any more. What you need now is a POSIX shell, `gzip`,
and GNU `as` and `ld` — the same two programs the Firn compiler calls for
every program it builds anyway.

```sh
sh bootstrap/build.sh          # -> ./firnc1
FIRNLIB=$PWD/lib ./firnc1 examples/tour.fi -o tour && ./tour
```

## What is in here

| file | what it is |
|---|---|
| `firnc1-seed.s.gz` | the **seed**: the x86-64 assembly text of `bin/firnc1.fi`, gzipped. Unpacked it is about 23 MB of text with roughly 780,000 lines. It is not a binary blob — you can read it. |
| `build.sh` | the three steps below |
| `SHA256SUMS` | checksum of the seed, and of the source file it was made from |

## The three steps

```
   bootstrap/firnc1-seed.s.gz
        |  gzip -dc | as --64 | ld
        v
   .seed/firnc-seed            a working Firn compiler
        |  compiles bin/firnc1.fi (SOURCE, in Firn)
        v
   .seed/firnc1
        |  compiles bin/firnc1.fi again
        v
   .seed/firnc2                and firnc1 == firnc2, byte for byte
```

The third step is not decoration. If the compiler built from the seed and
the compiler that one builds are byte identical, then the seed contains
nothing that is not also in `bin/firnc1.fi` and `lib/firnc1/` — no hidden
change, no trapdoor that survives a recompile. If they ever differ, the seed
does not match the source and `build.sh` stops.

## Where the seed comes from

It is the output of `tools/fixpoint.sh`, stage 2 — and stage 2 and stage 3
of that script are character identical, which is to say: the seed is what a
**Firn** compiler produces out of the Firn compiler, not what Rust produces.
Rust produced the very first link of that chain once, in 2026, and the
result is a fixpoint that no longer depends on it.

To renew the seed after a change to the compiler:

```sh
bash tools/fixpoint.sh                 # must end in FIXPOINT
bash tools/freeze_bootstrap.sh         # writes seed + SHA256SUMS
```

## And Rust?

`compiler/` stays where it is. It is the archive and the second opinion: two
independent implementations of the same language that can be held against
each other (`tools/self_compare.sh`, `tools/bootstrap_gaps.sh`), and that is
worth more than the disk space it costs. What changed is only that the
everyday build no longer goes through it — and `tools/no_rust.sh` proves it
by building the compiler in an environment where `cargo` and `rustc` do not
exist at all.

`docs/BOOTSTRAP-LUECKEN.md` lists, measured file by file, what the two
compilers still disagree about.
