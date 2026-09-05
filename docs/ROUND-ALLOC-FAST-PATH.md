# Round ANLEGEWEG -- the fast path of the allocator

Original German write-up (621 lines): commit `b4e21483b` in the archive
history (`Flei-123/FirnOld`, branch `sammeln`). This is the English summary
that goes with the code now on main; the numbers are the ones measured in that
round (04.09.2026, build host with foreign load, best of several runs).

## What was measured

Round TEMPO2 had found the one number everything else hangs on:

```
    allocate one object in the heap ......... 161 ns
    call an empty Firn function ............   3 ns
    read one column of the syntax tree ...    7 ns
    look up a name .......................   25 ns
```

The collector was NOT the cost (same value with the collector switched off).
The cost was about three dozen FUNCTION CALLS on the way, and a size class
that the runtime computed again on every call although the CALL SITE had known
it all along. `tools/anlegeweg/mikro.fi` takes the number apart in five
ablation steps.

## What was built

1. **`__gc_alloc_fast` in the runtime** (`lib/gc/gc.fi`) and, more important,
2. **the fast path at the call site** (`compiler/src/gc_lower.rs::alloc_fast`,
   mirrored in `lib/firnc1/lower.fi` for the self-hosting compiler): the size
   class and the block step are folded to constants, and the ordinary case --
   free list not empty, no cycle running, single thread -- is a handful of
   inline instructions. If one of the five conditions does not hold, the
   runtime is called exactly as before.
3. **The fast rejection** in `__gc_block_of`: the lowest and highest address the
   heap ever occupied (`S_HEAPLO`, `S_HEAPHI`). An address outside is refused
   in four instructions. This turned out to matter MORE than the fast
   allocation, because the CONSERVATIVE stack scan hands every word of the
   stack to `__gc_mark`, and most words are not pointers.

The offsets of the state block that the call site writes into are a CONTRACT
between `gc.rs` (`ST_*` constants) and `lib/gc/gc.fi` (`S_*`); the module test
`offsets_match_the_runtime` reads them back out of the embedded runtime text
and fails if one of them moves.

## Results

```
   Firn, tools/anlegeweg/mikro.fi, default build stage
       24 octets .................  153.9 ns ->  54.9 ns   2.80 x
       56 octets .................  190.4 ns ->  74.5 ns   2.55 x
       environment of four objects  566.9 ns -> 210.9 ns   2.69 x
       with 8192 live objects ....  179.1 ns ->  84.7 ns   2.11 x
       __gc_alloc_raw directly ...  142.3 ns ->  63.0 ns   2.26 x

   __gc_mark on a pointer ...........  29 ns ->  27 ns
   __gc_mark on a non-pointer ....... 279 ns ->   5 ns
   gc_soak throughput ............... 1.30 x
   gc_soak longest pause ............ 27.5 ms -> 10.0 ms

   twelve JS benches (Certus), geometric mean ..... 1.207 x
   page build xoffi.ai ................. 766.3 -> 674.7 ms (picture bit-identical)
```

The honest answer to "factor 2 to 3 on the whole machine" is NO: the page
build is 62 % rasterising, and rasterising hardly allocates.

## Certus extensions that ride on top (this port)

The runtime in `/root/firnc-gc` carried more than the round above. All of it
is in `lib/gc/gc.fi` now (see `docs/CERTUS-COMPILER-MERGE.md` for the map):

* a sorted address index of the chunks plus a direct-mapped cache in front of
  it (`S_IDX*`, `S_BOCACHE`): `__gc_block_of` was O(chunks) per marked pointer,
  which made marking quadratic in the heap (1300 chunks on youtube.com);
* `MAX_LIMIT` raised from 4 MiB to 256 MiB, growth factor `GROW_NUM/GROW_DEN`;
* the monotonic clock through the vDSO instead of a system call;
* phase time sums and counters (`gc_time_by_phase`, ...), `gc_cycle_finish`,
  `gc_bottom_swap`;
* the per-class census (`gc_count_classes`, `gc_class_*`, `gc_refs_to`, ...)
  and the class name word in the type table.

The census API got English names in this port. The old German names that
Certus calls are listed in `docs/CERTUS-COMPILER-MERGE.md` with a one-line
rename script.
