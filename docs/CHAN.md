# lib/chan -- a bounded ring channel between threads

`import chan.chan` (module name `chan`). One file, `lib/chan/chan.fi`. Status: final for the first integration (23.10.2026); names and signatures below were checked against the source, the examples are tests 4830 (SPSC) and 4861 (MPSC).

Purpose: hand records between threads without locks and without the GC. Used by `lib/window/shellthread.fi` (four rings between app thread and window thread, `docs/SHELL_THREAD.md`).

A channel moves RECORDS of `rec_words` 64 bit words from threads that send to
threads that receive. Meant for MPSC (many producers, one consumer) and SPSC;
the ring is also correct with several consumers (tests 4833 D), but that is
not what it is tuned or promised for.

```
import chan.chan

static mut g_blk: u64 = 0

fn __thread_work(kind: u64, arg: u64) -> u64 {
    var c: chan.Chan = chan.Chan { blk: g_blk }
    var i: u64 = 0
    while i < 1000 {
        chan.chan_send1(&c, i)
        i = i + 1
    }
    return 0
}

fn main() -> i32 {
    var c: chan.Chan = chan.chan_new(64, 1)
    g_blk = c.blk
    let h: u64 = thread_start(1, 0)
    var v: u64 = 0
    var n: u64 = 0
    while n < 1000 {
        chan.chan_recv1(&c, &v)
        n = n + 1
    }
    thread_wait(h)
    chan.chan_free(&c)
    return 0
}
```

A `Chan` is one word (`blk`). Threads share the channel by sharing that word
(a `static mut`, or inside the argument block of `thread_start`) and rebuilding
`chan.Chan { blk: ... }`. `blk == 0` means "could not be created"; every call
on such a channel is a no-op that answers false / closed.

## API

| call | meaning |
|---|---|
| `chan_new(cap, rec_words) -> Chan` | `cap` rounded up to a power of two (minimum 2, maximum 2^30); `rec_words` 1..4096 (0 counts as 1). Ring and header come from `rt.heap_alloc`, outside the GC heap. |
| `chan_free(c)` | frees the memory; `c.blk` becomes 0. All users must be done. |
| `chan_try_send(c, rec) -> bool` | copies `rec_words` words from `rec`; false if full or closed. |
| `chan_send(c, rec) -> bool` | waits while the ring is full; false only if the channel is closed. |
| `chan_try_recv(c, out) -> bool` | copies a record into `out`; false if empty. |
| `chan_recv(c, out) -> bool` | waits while empty; false only if closed AND empty. |
| `chan_recv_timeout(c, out, ms) -> u64` | `RECV_OK` (0), `RECV_TIMEOUT` (1), `RECV_CLOSED` (2: closed and empty). `ms == 0` is a try. |
| `chan_close(c)` | wakes everybody who waits. Later sends fail; receivers still get what is in the ring. Idempotent. |
| `chan_len(c)`, `chan_cap(c)`, `chan_rec_words(c)`, `chan_is_closed(c)` | `chan_len` is a snapshot. |
| `chan_send1(c, v)`, `chan_recv1(c, out)` | one word channels (`rec_words == 1`) without a buffer of your own. |

`rec` and `out` are `*mut u64` (Firn has no `*const`); a send does not write to `rec`.

## Thread rules

* **Who may call what.** Any thread may call any function on a shared `Chan` (the ring is thread safe); `chan_new` and `chan_free` are called once, by the owner, while nobody else uses the channel.
* **No GC allocation in the channel code.** The ring and header are `rt.heap_alloc` memory. The `try_` calls never block, so they are the ones to use in a thread that is marked `thread_blocking_an()` (the window shell thread, `docs/SHELL_THREAD.md`). The waiting calls (`chan_send`, `chan_recv`, `chan_recv_timeout`) wrap their sleep in `thread_blocking_an()/thread_blocking_out()` themselves, so they are for ordinary GC threads.
* **Return values.** `bool` calls: `true` = the record moved. `chan_recv_timeout` returns `RECV_OK` 0, `RECV_TIMEOUT` 1, `RECV_CLOSED` 2.
* **Bad channel.** `blk == 0` (failed `chan_new`) makes every call a no-op that answers `false` / closed. After `chan_free` the handle word `c.blk` is 0 in that copy only; other copies of the word dangle, so free only when all users are done.

## Example 2: MPSC with timeout and close (test 4861)

Two producers send `(id, n)` records; the consumer uses `chan_recv_timeout`, closes after both producers are joined, and drains until `RECV_CLOSED`. Closing only after the join avoids C-3.

```
import chan.chan

static mut g_blk: u64 = 0

fn __thread_work(kind: u64, arg: u64) -> u64 {
    var c: chan.Chan = chan.Chan { blk: g_blk }
    var rec: [u64; 2] = [0; 2]
    var n: u64 = 0
    while n < 1000 {
        rec[0] = arg
        rec[1] = n
        if !chan.chan_send(&c, &rec[0]) {
            return 1
        }
        n = n + 1
    }
    return 0
}

fn main() -> i32 {
    var c: chan.Chan = chan.chan_new(16, 2)
    if c.blk == 0 {
        return 1
    }
    g_blk = c.blk
    let a: u64 = thread_start(1, 0)
    let b: u64 = thread_start(1, 1)
    var next: [u64; 2] = [0; 2]
    var out: [u64; 2] = [0; 2]
    var total: u64 = 0
    var closed: bool = false
    while true {
        let r: u64 = chan.chan_recv_timeout(&c, &out[0], 20)
        if r == chan.RECV_OK {
            if out[0] > 1 || out[1] != next[out[0] as usize] {
                return 2
            }
            next[out[0] as usize] = out[1] + 1
            total = total + 1
        } else if r == chan.RECV_CLOSED {
            break
        } else if !closed && total == 2000 {
            // both producers are done: nobody sends any more, so closing loses nothing
            thread_wait(a)
            thread_wait(b)
            chan.chan_close(&c)
            closed = true
        }
    }
    chan.chan_free(&c)
    if total != 2000 {
        return 3
    }
    return 0
}
```

Build and run: `firnc tests/4861_chan_doc_example.fi -o x && ./x` (exit 0); also `--target=x86_64-windows` under Wine.

## Integration checklist for Certus

1. `import chan.chan`; nothing else needed, no compiler support beyond threads (`thread_start`, `thread_wait`, futex seam).
2. Windows target: needs the thread support of the WIN-THREADS patch (`WaitOnAddress`, `WakeByAddressSingle/All`; for `firnc-gc`: `patches-pending/compiler-0027-win-threads.patch`). Windows 8 or newer.
3. Share a channel by sharing the `blk` word (`static mut`, or a field of the `thread_start` argument block) and rebuild `chan.Chan { blk: ... }` in the other thread. Do not call `chan_free` twice.
4. Use `rec_words` words per message: ids, indexes, numbers. Never a Gc pointer (C-G1).
5. In a thread without GC rights use only `chan_try_send` / `chan_try_recv`.
6. Close the channel to stop consumers; join producers first when every record matters (C-3).

## Model

* **Ring of slots with sequence numbers** (Vyukov bounded queue, the same
  scheme as the ring in `lib/jobs`, J8). A slot is `1 + rec_words` words: the
  sequence word, then the record. Positions `enq` / `deq` only grow. A slot is
  free for position `p` when its sequence is `p`, full when it is `p + 1`, free
  for the next lap when it is `p + cap`.
* **The CAS** is `__atomic_swap(p, expected, new)`. A sender claims a position
  by moving `enq`, copies the record, then publishes it with one locked add on
  the sequence word. A receiver does the same on `deq`. The locked add is a
  full barrier on x86-64 and aarch64, so the record is visible before the
  sequence changes. The sequence is READ with a plain load and used only to
  decide; the compare-and-swap that follows is the barrier that makes the
  payload reads safe (on aarch64 as well).
* **Waiting.** A call that finds the ring full / empty first tries 1024 times
  (the short spin), then sleeps on a futex word (`__thread_futex_wait`: Linux
  futex, Windows `WaitOnAddress`). Two futex words: DATA (receivers sleep on
  it) and SPACE (senders sleep on it), each with a waiter count. The waiter
  first counts itself in (locked add), then reads the futex word, then tries
  once more, then sleeps only if that failed. The other side publishes first,
  then looks at the waiter count; only if somebody waits does it bump the word
  and wake one. So a channel that nobody waits on does no system call at all.
* **GC.** A blocking wait is wrapped in `thread_blocking_an()` /
  `thread_blocking_out()`: a sleeping thread never holds up a collection (W-T6).
  The records are plain words in non-GC memory; see C-G1.
* **Close.** `chan_close` sets the flag, bumps both futex words and wakes all
  sleepers. A sleeping receiver then drains what is left and answers false.

## HONEST

* **C-G1 The collector does not look into a channel.** A Gc pointer sent as a
  word does NOT keep its object alive, and a moved/freed object leaves a
  dangling word. Send an index into a table the SENDER keeps alive (a stack
  array or a field of a rooted object), a serial number, or memory from
  `rt.heap_alloc` that the receiver frees. Test 4834 does the first: the sender
  holds 32 `Gc[Cell]` in a stack array, sends `(index, serial, check)`, a
  second thread allocates garbage and forces `gc_collect()` while it
  acknowledges over a second channel; the sender checks every acknowledged
  object. (Same rule as J2 in jobs and Q1 in std.pool.)
* **C-1 recv_timeout polls.** The seam has a futex WAIT without a timeout
  (Windows `WaitOnAddress` is called with an infinite wait). So after the
  spin `chan_recv_timeout` sleeps in steps of 1 ms (`thread_sleep(1)`) and
  looks again: the answer can come up to ~1 ms (Linux) or one timer tick
  (Windows, up to ~16 ms with the default timer) late, and the timeout itself
  is measured with `monotonic_ns`. Blocking `chan_recv` has no such delay.
* **C-2 There is no fairness.** Several senders waiting on a full ring wake in
  no promised order; per producer the order is kept (4833 checks it), across
  producers it is the order of the claims.
* **C-3 `chan_close` racing a send in flight.** A sender that has passed the
  closed check and is about to publish when the channel is closed may publish a
  record the receiver already decided not to wait for (it saw closed + empty).
  Close after the producers are done (join them, or have them stop first) when
  every record matters.
* **C-4 A claimed but not yet published slot looks empty.** A sender that is
  descheduled between claiming a position and publishing it stalls the
  receiver at that slot (the ring is lock-free for the claim, not wait-free);
  the receiver answers "empty" and waits for the wake-up that publish sends.
* **C-5 `chan_len` is a snapshot** and counts claimed slots.
* **C-6 Counters in futex words are 32 bit.** DATA / SPACE are bumped only when
  someone waits; after 2^32 bumps a sleeper could see an equal value. Not
  reachable in practice (a sleeper wakes at the next bump).
* **C-7 Only numbers.** Strings, structs with pointers and Gc objects are the
  caller's job (indexes into a table, or `rt.heap_alloc` blocks).
* **C-8 aarch64 only under qemu.** Tests 4830-4834 pass with
  `--target=aarch64-linux` under `qemu-aarch64`; qemu does not reorder like
  real hardware, so the memory ordering on a real aarch64 CPU is argued (locked
  add = full barrier, plain read of the sequence followed by a CAS), not
  measured.
* **C-9 Windows.** Tests pass under Wine 8 and on a real Windows machine
  (HOLO-LAPTOP-3, 8 logical cores). The timer resolution caveat of C-1 applies.
* **C-10 A thread that blocks in `chan_send` / `chan_recv` cannot be
  interrupted** except by `chan_close`.

## Numbers (bench/chan/chan_bench.fi, release-fast)

Linux x86-64, 64 core server that was busy with other jobs (load 12-25): best
and worst of three runs; one word per message; the consumer is the main thread.

| case | messages / s |
|---|---|
| SPSC, ring 1024 | 12.7 M .. 23.7 M |
| SPSC, ring 64 | 15.3 M .. 24.3 M |
| SPSC, ring 2 (almost every call waits) | 2.6 M .. 3.2 M |
| MPSC, 4 producers, ring 1024 | 2.7 M .. 3.1 M |
| MPSC, 2 producers, ring 64 | 4.2 M .. 8.1 M |
| ping-pong round trip (two channels, ring 2) | 0.6 .. 0.9 us |

One thread, send + receive in turn: ~67 ns per pair. The MPSC numbers are
bounded by the producers fighting over one cache line (`enq`) and the slot
lines (a producer-only run without consumer: 4 producers ~2.5 M/s).

Re-run 23.10.2026 (same server, load 18, one run of `bench/chan/chan_bench.fi`, release-fast): SPSC ring 1024 13.4 M/s, ring 64 15.7 M/s, ring 2 3.1 M/s, MPSC 4 producers ring 1024 3.7 M/s, MPSC 2 producers ring 64 9.6 M/s, ping-pong round trip 196 ns (the table above says 0.6 .. 0.9 us for the first runs; the spread is mostly machine load).

Windows (HOLO-LAPTOP-3, 8 logical cores, real hardware, one run): SPSC ring
1024 12.2 M/s, ring 64 10.8 M/s, ring 2 4.9 M/s, MPSC 4 producers 3.4 M/s, MPSC
2 producers 5.4 M/s, ping-pong 304 ns. Wine on the server: SPSC 12.9 M/s,
ping-pong 961 ns.

## Tests

* `tests/4830_chan_basic.fi` -- order, full / empty, `try_*`, wrap-around,
  records of 3 words, `send1/recv1`, capacity rounding, bad arguments.
* `tests/4831_chan_close_timeout.fi` -- close wakes blocked receivers and
  senders, send after close, drain after close, `recv_timeout` (time limits,
  result, closed, zero wait).
* `tests/4832_chan_spsc_stress.fi` -- 5 M messages (order + sum), ring of 2,
  1 M records of 3 words, a sleeping producer.
* `tests/4833_chan_mpsc_stress.fi` -- 4 producers x 500 k (order per producer,
  sum), ring of 4, full ring + sleeping consumer, 2 consumers (bonus).
* `tests/4834_chan_gc.fi` -- the GC interaction of C-G1.
* `tests/4861_chan_doc_example.fi` -- the MPSC example above (recv_timeout, close, drain).
* All of them in dev-fast, release-safe, release-fast and `--no-opt`; also x86_64-windows (Wine + real Windows), aarch64 (qemu).
