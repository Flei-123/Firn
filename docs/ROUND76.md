# Round 76 — sockets, packet framing, NBT, and a Minecraft server as the hard test

**Branch** `r76-net` · **Date** 2026-08-21 · AMD EPYC 7571, 8 cores, 12 GiB,
Linux x86_64, Debian 12, `rustc` release build of `firnc0`, `java` 17.0.20,
`node` v18.20.8, `python` 3.11.2.

Everything below **was run**. Where a number stands, it was measured on this
machine on this day; where something does not work, it says so.

---

## 1. What the round was for

Firn had, up to round 75, **zero network code**. Every octet the compiler,
the browser stack or the JavaScript engine had ever read came out of a file:
a file does not close under you, does not hand you three octets where 300
were sent, and does not send you a signal when you write into it after the
other side went away.

That is a comfortable place to stand, and it hides things. This round was
supposed to put the language in an uncomfortable one — threads with
**blocking I/O**, a packet parser fed by an **untrusted peer**, and **octet
arithmetic** dense enough that the optimiser has to be right about it. The
yardstick chosen was the Minecraft protocol, because it is not negotiable:
either a real client walks into the world or it does not.

**It does.** Three independent clients get in, one of them written by
somebody who has never heard of this repository.

---

## 2. What was built

| | Lines | What |
|---|---|---|
| `lib/std/net.fi` | 580 | TCP over raw system calls: `socket`/`bind`/`listen`/`accept4`/`connect`/`setsockopt`/`getsockname`/`sendto`/`recvfrom`/`shutdown`/`close`. `sockaddr_in` correctly packed, errors as `NetError!T`, EINTR retried inside, `MSG_NOSIGNAL` instead of a SIGPIPE handler. |
| `lib/std/bytes.fi` | 630 | Byte buffer with a read and a write cursor. Big endian for every width, `f32`/`f64`, **VarInt/VarLong in the Minecraft encoding**, UTF-8 strings with a VarInt length prefix, the `Position` bit field, patching for length fields that stand in front of their content. |
| `lib/std/nbt.fi` | 575 | Named Binary Tag, writer and pull reader, all thirteen tag types, the named root of a file **and** the nameless one of 1.20.2, `skip` with a depth limit. |
| `lib/std/md5.fi` | 201 | MD5. Not a security primitive and the header says so — it is here because the offline UUID of Minecraft is prescribed to be `md5("OfflinePlayer:" + name)`. |
| `demos/mcserver/` | 1472 | A Minecraft server, protocol **765 (1.20.4)**, offline mode, no compression. |
| `tools/net/`, `tools/nbt/`, `tools/mcserver/` | 1231 | The proofs, plus three clients and two second parsers. |

Total: **6,759 lines added**, 29 files, five commits.

The version is written down in one place (`demos/mcserver/proto.fi`,
`PROTOCOL_VERSION = 765`) and the packet numbers were **not** guessed: they
come from the protocol tables of `minecraft-data` for 1.20.3/1.20.4 and were
cross-checked against a **running vanilla server 1.20.4** (see 4.3).

---

## 3. The measurements

### 3.1 Sockets (`tools/net/run.sh`, test.sh section 36)

| What | Measured |
|---|---|
| `nc` → Firn echo server → `nc`, 1 MiB of random octets | 1,048,576 octets back, **md5 identical**, in all three build stages |
| `curl` against the HTTP mode | **HTTP 200**, 24 octets, body as expected, status line and `Content-Length` accepted by curl |
| 16 connections at the same time, 1 MiB each, own data per connection | **48.1 MiB/s** payload = 96.3 MiB/s on the wire, 16 MiB in 0.33 s (`release-fast`; `no-opt` 51.0, `dev-fast` 48.8) |
| In-process, server thread + client, 1 MiB in 16 KiB chunks | passes in all three stages (`tests/1600_net_echo.fi`) |

The throughput number is **not** a socket benchmark — the other end is a
Python client with 16 threads, and Python is the limit. What it shows is that
16 blocking Firn threads move 16 MiB there and back without losing an octet
and without a stall.

Counter-checks, all of which have to fail:
`connect` to port 1 refuses · a server killed mid-transfer does not leave its
client hanging · a write to a peer that is gone does **not** kill the process
(`MSG_NOSIGNAL`; without it `tests/1600` would die of SIGPIPE before its
`return`).

### 3.2 VarInt and the byte order (`tests/1601_bytes_protocol.fi`)

Checked against the **table** of the protocol documentation, not against
itself — a pure roundtrip test passes with little-endian integers and with
zigzag VarInts, and both produce a stream no client understands:

```
0 → 00 · 1 → 01 · 127 → 7f · 128 → 80 01 · 255 → ff 01 · 25565 → dd c7 01
2097151 → ff ff 7f · 2147483647 → ff ff ff ff 07
-1 → ff ff ff ff 0f          (FIVE octets, not one — no zigzag)
INT_MIN → 80 80 80 80 08
VarLong: -1 → ff ff ff ff ff ff ff ff ff 01 · LONG_MIN → 80 80 80 80 80 80 80 80 80 01
Position(18357644, 831, -20882616) → 46 07 63 2c 15 b4 83 3f
```

All of them reproduced independently in Python and found identical. On top:
**20,000 random values per width** through VarInt, VarLong, u16/u32/u64,
f32/f64 and the Position bit field, there and back.

**ZigZag is offered and is NOT used**, and the test says so out loud: Java
Edition encodes a negative VarInt as the two's complement pattern, which is
why `-1` costs five octets. Bedrock uses zigzag; this file does not speak
Bedrock.

### 3.3 NBT against Notch's own file (`tools/nbt/run.sh`, section 37)

The strongest single number of the round:

> `tools/nbt/bigtest.fi` rebuilds `bigtest.nbt` — the reference file of the
> NBT specification — out of `lib/std/nbt.fi`, and the result is
> **identical over all 1,543 octets** of the original content. The 1,544th
> is its `TAG_End`; the Firn file goes on there with the three tags bigtest
> predates.

That cannot be satisfied by a reader and a writer that are wrong in the same
way. The reference lies in `testdata/nbt/bigtest.nbt.gz`, gzip'ed exactly as
published.

Plus, in both directions and in three build stages:

* `tools/nbt/dump.fi` (Firn) and `tools/nbt/check.py` (Python, not one shared
  line) turn the same file into the same canonical text — **38 lines**
  identical for the Firn file, **35** for a file Python wrote with all
  thirteen tag types, the extremes of every width, an empty string, an empty
  compound, an empty list, a list of lists and three levels of nesting.
* the nameless root of 1.20.2 is read as such and the **named** reader
  refuses it (one octet of difference, and it is the whole difference).
* refusals: truncated file · tag number 13 · negative array length · an
  array that announces more than is there · 256 levels of nesting.

### 3.4 The Minecraft server (`tools/mcserver/run.sh`, section 38)

Three clients, none of which takes the server's word for anything.

**(a) `tools/mcserver/harness.py`** — own VarInt reader, own framing, own NBT
parser. It **logs into the vanilla server first**, so a failure against the
Firn server is the Firn server's fault:

```
ping: version='Firn 1.20.4' protocol=765 players=0/20 rtt=0.33ms
ping: json=192 octets, whole answer verified
login: Login Success uuid=b50ad385-829d-3141-a216-7e7d7539ba7f name='Notch'
login: the UUID is version 3, variant RFC 4122 -- offline mode, as it should be
config: Registry Data, 6 registries, 7619 octets
play: Join Game entity=1 dimension_type='minecraft:overworld' gamemode=1 view=2 flat=1
play: first chunk (-2,-2) heightmaps=['MOTION_BLOCKING'] data=2245 octets
play: chunk verified -- 24 sections, 1 of them not empty, 2245 octets consumed exactly
play: Synchronize Player Position x=8.5 y=-60.0 z=8.5 teleport id=1
play: 25 chunks, 1 keep alives, 1405373 octets in / 91 out
OK login: entity=1 position=(8.5, -60.0, 8.5) spawn=(8, -60, 8) chunks=25
```

`b50ad385-829d-3141-a216-7e7d7539ba7f` is **the UUID the real vanilla server
handed out for the same name on this machine**. Four such UUIDs are pinned in
`tests/1603_md5_uuid.fi`, next to the seven test vectors of RFC 1321.

**(b) The same harness with ONE OCTET PER WRITE** (`dribble`), half a
millisecond apart. This is what separates a server that reads a length prefix
from one that hopes a `read()` is a packet — on localhost the difference is
invisible, over a real link it is the whole game. It gets into the world.

**(c) `node-minecraft-protocol` 1.54.0** (PrismarineJS) — a third
implementation, with its own packet definitions out of `minecraft-data`,
which validates every field and throws when something does not fit:

```
nmp: login  entityId=3 dimension=minecraft:overworld gameMode=1 viewDistance=2 isFlat=true
nmp: chunk batch finished, 25 announced, 25 received
nmp: position x=8.5 y=-60 z=8.5 teleportId=1
OK nmp: the client is in the world
```

**Load:** 16 logins at the same time — 16 threads, ~22 MiB of chunk data —
in **0.13 s**, all 16 through. Sequentially: **2,529 pings/s** and
**1,402 full logins/s** (each of the latter builds the registry codec anew
and writes 25 chunks), RSS flat over 3,400 connections — see 4.5.
**Ping latency**, 30 pings each: Firn **median 0.43 ms** (min 0.37, p95 0.62,
max 2.29). The vanilla server on the same machine, idle, same harness:
**median 1.97 ms** (min 0.84, p95 18.44, max 21.36). Not a fair comparison —
the vanilla server does a great deal more per tick and pays for a JVM — but
it is a number rather than an adjective.

**Counter-checks:** garbage instead of a handshake (`ff ff ff ff ff ff`, a
zero-length packet, a VarInt announcing 2 GiB), and a client that sends a
handshake and then RSTs mid-login. After all of them the server still
answers a ping.

### 3.5 Sizes

| | Firn server | Vanilla 1.20.4 |
|---|---|---|
| Registry Data | 7,619 octets, 6 registries (1 / 1 / 1 / 44 / 1 / 1) | 39,307 octets, 6 registries (4 / 64 / 7 / 44 / 16 / 10) |
| Chunk section data | 2,245 octets, 24 sections | 2,245 octets, 24 sections |
| Chunk packet incl. light | ~54 KiB (26 × 2048 sky light) | comparable |
| Whole join, view distance 2 | 1,405,373 octets in | 512,048 (view 3, batched differently) |

The chunk section data is **the same size to the octet** as the vanilla
superflat — not because it was copied, but because a 24-section chunk with
one four-bit palette section and 23 single-valued ones has exactly that size.

---

## 4. What this cost the language — the findings

This is the part the round was actually for.

### 4.1 A compiler bug that had survived 75 rounds — FIXED

**A function of a MODULE could not return an error union over a struct of its
own module.**

```firn
// lib/std/net.fi
struct Listener { fd: i32, port: u16, err: i32 }
fn listen_tcp(addr: u32, port: u16) -> NetError!Listener { ... }
//                                              ^^^^^^^^ error: unknown type 'Listener'
```

In the **root** module the identical code worked, which is why nobody had run
into it: no module had ever needed it.

**The cause.** The parser does not leave the success type of `E!T` standing in
the syntax tree. `errors::hook_type` puts it aside into a side table and
leaves the placeholder `__eu#<n>` behind (that is what makes the two-phase
resolution work). `modules.rs::Resolver::ty` — the pass that qualifies every
type name of a module with the module name — walks the **tree**, and the
payload type is not in it. So `Listener` never became `net__Listener` and the
lookup failed.

**The fix** is nine lines: `errors::pending_inner`/`set_pending_inner` hand
the stored type expression out and take it back, and the resolver walks it
like any other. Out-and-back rather than a `&mut`, because the resolver calls
itself while it works on the type and a borrow held across that call would be
a second `borrow_mut` on the same `RefCell`.

**`firnc1` never had the bug** — it renames while parsing, so the payload is
already qualified when it goes into the side table. That is the first time in
this series that the compiler written in Firn was *right* where the one
written in Rust was wrong.

Without the fix the whole library would have had to fall back on the
out-parameter convention of `lib/std/rc.fi`
(`fn listen_tcp(..., out: *mut Listener) -> NetError!bool`), which works and
reads badly.

### 4.2 The optimiser costs a factor of 6.3 in recursion depth — MEASURED

Found by a counter-check, not by reading code: a file with 400 nested
compounds killed `tools/nbt/dump.fi` with a segmentation fault.

The recursion was not the problem; the **frame** was:

```
tools/nbt/dump.fi::value, --opt-level=release-fast : 25,904 octets of frame
maximum NBT nesting before the guard page, 8 MiB stack:
    release-fast   309
    no-opt        1962
    dev-fast      1969
```

The inliner gives **every inlined call site room of its own** instead of
reusing the slots of callees whose lifetimes do not overlap. `value()` calls
a dozen small `nbt_read_*` and `bytes_*` helpers; each one is inlined, each
one brings its locals, and none of the space is shared. The source shows a
function with four local variables.

Consequences, both of them real:

* The **depth limit in `lib/std/nbt.fi` is 256, not the 512 the vanilla
  reader uses.** A limit the optimised build cannot keep is worse than no
  limit at all, so the number sits below the *smallest* measured value, not
  above the largest.
* Any recursive descent parser in Firn is roughly **six times shallower**
  when optimised than when not. That is a property the source does not show
  and that nobody would look for.

**Not fixed in this round.** The fix is stack-slot colouring for inlined
frames in `opt.rs`/`regalloc.rs` and is a round of its own. It is written
down here so it is not rediscovered a fourth time.

### 4.3 What is missing in the language — the list

1. **No `_` as a wildcard.** `let _ = x` twice in one block is
   `error: '_' is already declared in this block`. Every language with
   pattern matching has a hole; this one has an ordinary identifier. Worked
   around by making `try f()` a bare expression statement (which works and is
   nicer), but the wildcard is missing everywhere else too — in `match` arms
   and in tuple destructuring, when those come.
2. **Error positions point into the merged module text.** A syntax error in
   a 25-line file reported `--> /tmp/nt.fi:545:17`. The line number belongs
   to the concatenation of all modules, the file name to the root. That makes
   an error in an imported module very hard to find, and it is the kind of
   thing that costs a beginner an evening.
3. **String literals count their own length badly for the reader.**
   `var s: [u8; 106] = "…"` has to be counted by hand, and `\u{C5}` counts as
   two octets while `\xC5` is refused outright ("does not yield valid UTF-8,
   write `\u{...}`" — a good message). Four of the fifteen new files needed a
   script to fix the lengths. A `[u8; _]` or a `sizeof`-style inference on a
   literal initializer would remove a whole class of edit-compile cycles.
4. **`__atomic_swap` does not take integer literals.**
   `__atomic_swap(p, 0, 1)` is `comparison between different types u64 and
   i32`; it has to be `0 as u64, 1 as u64`. Ordinary calls infer the literal
   type from the parameter; the intrinsic does not.
5. **Error sets are program-wide, module types are not**, and the two look
   the same at the call site. `proto.ProtoError::Closed` is a syntax error
   (the set is global: `ProtoError::Closed`), while `proto.Bytes` is right.
   SPEC 14.1 F2 documents it; it still catches you every time.
6. **`catch` needs a value, and an error union carries no payload**, so
   "which error was it" has to be smuggled out by hand — here through a
   `fn err_code(e: ProtoError) -> i32` in the catch expression, and through
   an `err` field in `net.Conn` for the raw errno. It works. A payload on the
   error, or `catch` with a block, would remove a lot of scaffolding.
7. **No `defer` in a loop body without cost**, and no destructors: every
   `Bytes` and every `Conn` is freed by hand. That is a known and deliberate
   property of the language (SPEC 3.3), and in a server with one thread per
   connection it is exactly where a leak would live. `tools/strsoak` measures
   this for `str`; the socket path has **no soak measurement yet** — see 6.

None of these blocked the round. All of them cost time.

### 4.5 A bug in this round's own server, found by counting — FIXED

Not a language finding, but the most instructive failure of the round, and
it is written down because the counter-check that found it is now permanent.

**`demos/mcserver` stopped answering after exactly 63 connections.** Not
slowly, not under load: connection 64 got a closed socket, every time, at the
same number.

The cause is in `lib/gc/gc.fi`. `thread_start` takes an entry out of a table
of `THREAD_MAX` = 64, `__thread_slot_new` only hands out entries whose state
is `Z_FREE`, and **the only thing that sets `Z_FREE` is `thread_wait`**. A
worker that runs to its end reaches `Z_DEAD` — its stack stays mapped, its
table entry stays taken. The main thread holds the first entry, so 63 workers
and then nothing.

`thread_wait` is therefore not "wait for the result" in a server; it is
`free`. The fix is a `reap()` that walks the connection slots before every
accept and joins whatever has finished — cheap, because a worker sets its
slot free as its last statement and is at its final instructions by then.

**What is embarrassing is how nearly it was missed.** Every test of section
3.4 above — ping, login, dribble, node, sixteen at once, three junk
connections, one RST — comes to about 25 connections. The bug sits at 64.
Two more tests and it would have shipped.

So the round has an endurance run now (`tools/mcserver/soak.py`, part of
section 38), with **two** counter-checks:

From the full `./test.sh` run, `release-fast`:

```
ping : 3000 connections in 1.19 s = 2529/s, RSS 460 -> 460 KiB (+0 KiB over 2700)
login:  400 connections in 0.29 s = 1402/s, RSS 568 -> 572 KiB (+4 KiB over 360)
soak: RSS flat (ping +0 KiB, login +4 KiB, limit 2048)
counter-check A: the same server WITHOUT reap() dies in the sixties, as it must
counter-check B: the same server WITHOUT the bytes_free climbs
                 RSS 2252 -> 18452 KiB over 1350 connections
```

`--no-opt` and `dev-fast` are the same picture (+0 / +272 KiB and +0 / +8 KiB).

Counter-check A is the important one: without it the endurance run would pass
with a server that leaks thread table entries, as long as the leak is not
RSS. Counter-check B is the classic one — a leak measurement that cannot show
a leak measures nothing.

### 4.4 What did NOT go wrong

Worth writing down, because it was not obvious:

* **Threads with blocking I/O just work.** 16 threads, each sitting in
  `recvfrom` on its own connection, plus a main thread in `accept`. No
  interference, no lost wakeups, `thread_blocking_an`/`out` around the
  blocking calls keeps the collector's stop-the-world honest (the server
  allocates nothing on the GC heap, so it never runs — but the calls are
  there and correct).
* **The `syscall` intrinsic is enough for a whole socket layer.** Eleven
  system calls, no libc, no shim, and `struct sockaddr_in` falls out of the
  natural Firn layout — measured against the field addresses, not assumed.
* **The optimiser did not break the byte arithmetic.** Every test passes
  identically in `release-fast`, `--no-opt` and `dev-fast`, including 20,000
  random VarInts and the 1,543-octet comparison against `bigtest.nbt`.
  The only difference the optimiser makes anywhere in this round is the frame
  size of 4.2.

---

### 4.6 The state of `test.sh` — and the two failures that are not this round's

Full run on this machine, `./test.sh`: **1,169 checks, 2 failed**, and the
three new sections (**36, 37, 38**) pass in **every** build stage —
`release-fast`, `--no-opt` and `dev-fast`.

Two checks fail. Neither belongs to round 76, and that is a claim, so each
one was **reproduced on `main`**:

| Section | Symptom | Established how |
|---|---|---|
| **34** round 66, JS | the promise endurance run `jobs` ends with `rc=-11` (SIGSEGV) | (a) round 76 changed exactly one thing in the compiler (4.1), so the **whole JavaScript engine was translated with both compilers and the assembly compared**: `lib/js/run_main.fi`, ~9 MB, **byte identical** once the source path is normalised (the two worktrees have different directory names). A change that emits the same octets cannot cause a different crash. (b) `bash tools/js/round66.sh --fast` **on `main`**: `jobs rc=-11 1.3s`, the same failure. |
| **23** layout | `1082 of 1087 boxes equal to Chromium (0.46 % off)`, limit 1087 | `bash tools/layout/run.sh` **on `main`**: `1082 / 1087`, deviation 0.46 %, and the **same five cases** (`a4_abs_icb` 2 boxes, `a2_fixed_bottom_right`, `a3_fixed_percent`, `a7_sticky_bottom` 1 each). Round 76 touches nothing in `lib/layout` or `lib/css`. |

Two other failures of the first run **were** fixed here, and both are
inherited rather than caused:

* **Section 24, the formatter.** `lib/std/io.fi` had one blank line too many
  at line 705. The file is **byte identical to `main`** (`md5sum` on both)
  and `main`'s own compiler-built `firnfmt -c` calls it unformatted too, so
  section 24 was red before this branch existed. One line, fixed.
* **Section 23 did not even run.** There was **no Chromium on this machine at
  all** — an environment prerequisite like `gdb` for section 25 and QEMU for
  section 22, and the check reported `0 / 0 boxes, deviation 100 %`. Chromium
  151.0.7922.137 is installed now, so the section **runs** and reports a real
  number for the first time on this machine. That number is 99.54 % against a
  recorded limit of 100 %.

**On those five boxes, honestly:** whether they are a regression in
`lib/layout` or a change in Chromium is **not established here**. The limit
1087 was recorded against the Chromium of the round that set it; 151 is newer
than anything this repository has been measured against, and the four cases
are absolute, fixed and sticky positioning — exactly where browsers have
moved. Establishing it needs the old Chromium, and that is not round 76's
job. What round 76 did was turn "cannot run" into "runs and disagrees by five
boxes", which is strictly more information than the repository had before.

## 5. What is NOT proven

* **No real vanilla client has connected.** The Minecraft client needs a GPU
  and a display; this machine has neither. What has connected is
  `node-minecraft-protocol` — a full third-party implementation of the same
  protocol with its own packet tables — plus a Python harness that logs into
  the *vanilla server* as a control. That is strong evidence and it is not
  the same thing, and the round does not claim it is.
* **No compression.** `network-compression-threshold: -1`, on purpose: the
  client only compresses after a Set Compression packet, so not sending one
  is a complete configuration. But this repository has **no deflate**, and a
  server that talks to a proxy which forces compression cannot be built with
  it.
* **No encryption, no Mojang authentication.** Offline mode only. That needs
  RSA, AES-CFB8 and a HTTPS request to the session server — none of which
  exists here.
* **No game.** Nobody can break a block, no entity moves, there is no
  inventory, no chat and no persistence. The world is 5×5 chunks of superflat
  and every player gets the same one.
* **The endurance run is 3,400 connections, not 100,000.** It is flat over
  those (4.5) and it has both counter-checks, but a leak of a few octets per
  connection would need a longer run to show. `MC_SOAK_PINGS` and
  `MC_SOAK_LOGINS` turn it up.
* **IPv4 only, blocking only.** No `AF_INET6`, no `epoll`, no non-blocking
  sockets, no name resolution. One thread per connection scales to **63
  connections at the same time** — the thread table of the runtime holds 64
  entries and the main thread has the first (4.5). The 64th concurrent
  connection is refused, cleanly; connection number 64 *in sequence* is fine
  since `reap()` exists.
* **NBT strings are plain UTF-8, not Java's modified UTF-8.** They differ for
  `U+0000` and outside the BMP; nothing in this round contains either, and
  `lib/std/nbt.fi` says so under NBT2.

---

## 6. What comes next, in the order it should come

1. **Stack-slot colouring for inlined frames** (4.2). It is a six-fold loss
   of recursion depth that nobody can see in the source.
2. **A bigger thread table, or threads that release themselves.** 63 at a
   time is a hard ceiling that comes from `THREAD_MAX` in `lib/gc/gc.fi`, and
   `thread_wait` doubling as `free` is a sharp edge that will cut somebody
   again (4.5).
3. **`_` as a wildcard** and **error positions per file** (4.3, 1 and 2) —
   both small, both felt on every single day.
4. **deflate**, and with it compression, gzip'ed NBT and a real `.mca` region
   reader.
5. **Non-blocking sockets and `epoll`**, when a thread per connection stops
   being enough. Not before.

---

## 7. Reproducing it

```sh
bash test.sh                    # 1169 checks; sections 36, 37, 38 among them
bash tools/net/run.sh           # nc, curl, 16 connections, throughput
bash tools/nbt/run.sh           # bigtest.nbt octet for octet, Python cross-read
MC_FAST=1 bash tools/mcserver/run.sh   # only the optimised stage, ~40 s

# by hand:
compiler/target/release/firnc -o /tmp/mcserver demos/mcserver/main.fi
/tmp/mcserver 25565 3 0                       # port, view distance, seconds (0 = forever)
python3 tools/mcserver/harness.py ping  127.0.0.1 25565
python3 tools/mcserver/harness.py login 127.0.0.1 25565 Notch
```

`tools/mcserver/run.sh` fetches `node-minecraft-protocol` itself if `npm` is
there and `tools/mcserver/node_modules` is missing (453 MiB of
`minecraft-data`, deliberately not in the repository). If that fails, point
(c) is **skipped and said so out loud**, not silently passed.

The vanilla server used as ground truth is not in the repository either. It
was fetched from `piston-data.mojang.com` (1.20.4 server, 49,150,256 octets),
run with `online-mode=false` and `level-type=flat`, and its registry data and
offline UUIDs were captured with the same harness. The four UUIDs it produced
are pinned in `tests/1603_md5_uuid.fi`.
