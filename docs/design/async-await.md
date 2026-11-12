# Design note: async/await in Firn

Status: design, not implemented. Date 2026-11-12.

## Decision already taken

`SPEC.md` section 7 and `DESIGN_GOALS.md` 1: **no `async` colour, no `await` keyword.** The model is Zig's: an `Io` value is passed as a parameter like an allocator; `io.async(f, args)` returns a `Future[T]` (`#[must_consume]`, cancellable). This note keeps that decision and plans the library side. A keyword-style `await` is a separate product decision at the end (see open question).

## What exists

| Piece | File | Gives |
|---|---|---|
| Event loop (epoll/poll, timers, defer, wake) | `lib/async/loop.fi` | one thread, many descriptors, callbacks `fn(*mut Loop, u64)` |
| Streams, HTTP, WebSocket, DNS clients on the loop | `lib/async/stream.fi`, `ahttp.fi`, `wsc.fi`, `adns.fi` | non-blocking IO with completion callbacks |
| Hand work to the loop / run blocking work on workers | `lib/async/post.fi` (`post`, `bridge_call`), `std.pool` | thread to loop and back |
| Channels | `lib/chan/chan.fi` | blocking communication between threads |
| Interfaces with `dyn` dispatch | `docs/ROUND46.md` | the carrier for an `Io` interface |

Missing: `Future[T]`, an `Io` interface, coroutines (a stack per task), composition (`all`, `race`, timeout), structured scopes.

## Comparison

| | .NET | Rust | Go | Firn today | Firn after this plan |
|---|---|---|---|---|---|
| Unit of concurrency | `Task` | `Future` (state machine) | goroutine | callback | `Future[T]` on a coroutine |
| Suspension point | `await` | `.await` | any blocking call | callback split | `io.read(..)` etc. suspend the coroutine |
| Colour | yes | yes | no | no | no |
| Cancellation | `CancellationToken` | drop | `context` | `timer_cancel` | `Future.cancel`, `#[must_consume]` |

## Plan (each phase shippable on its own)

1. **`async/future.fi`: `Future[T]` as a library** (small, no compiler change). A completion cell `{state, value, error, waiter}`, `future_new`, `future_set`, `future_then(f, cb, ud)`, `future_cancel`; adapters from the existing callback APIs (`ahttp`, `stream`, timers). Composition: `future_all`, `future_race`, `with_timeout`. Tests on `Loop` with fake timers. **Roadmap r-async-1.**
2. **`lib/coro`: stackful coroutines.** mmap'ed stack (guard page, 64 KiB default), context switch in a few instructions of assembly for x86-64 and aarch64 (callee-saved registers + stack pointer), `coro_spawn`, `coro_yield`, `coro_resume`. The GC must scan every live coroutine stack as a root (`lib/gc`, `tools/stackscan`) and `#[no_gc]` code must stay valid while parked. Needs a compiler/runtime review for the frame-pointer and stack-limit checks, so this is the risky phase. **Roadmap r-async-2.**
3. **`Io` interface plus `Io.SingleThread` (deterministic) and `Io.Evented`.** `interface Io { fn async(..); fn sleep(*self, ms); fn read(..); fn write(..) }`, `dyn Io` passed through. `Io.Evented` runs coroutines on one `Loop`: a blocking-looking `io.read` registers a watch, yields, and the watch callback resumes it. `Io.Threaded` maps `io.async` to `std.pool`. **Roadmap r-async-3.**
4. **Structured scopes**: `scope.fi` pattern (`std/scope.fi` exists) so a scope cannot end while its futures run; the checker rule is `#[must_consume]`. **Roadmap r-async-4.**
5. **Open question for Justin:** is a keyword-free `f.await(io)` method enough, or does he want sugar (`await f`)? Sugar would have to lower to exactly that call so no colour appears; it is a parser-only change in both compilers.

## Why not a state-machine transform (Rust style)

It needs a new compiler pass in firnc0 and firnc1, a captured-variable analysis across suspension points, and it colours functions. Stackful coroutines cost one stack per task but touch only the runtime; the two compilers stay equal (differential test unchanged).

## Risks

* GC root scanning of parked stacks (phase 2) and stack overflow checks inside coroutine stacks.
* Windows/Android: context switch must exist per target (`@linux @windows @android` twins); fall back to `Io.Threaded` where it does not.
* Thread-affinity of `Gc[T]` (SPEC 3.5: no hand-over between threads): an `Io.Evented` task never migrates threads.
