# Round 96 — a secret becomes a type, and a variable survives the optimizer

**State before this round.** `ACCEPTANCE.md` carried three items with a
named remainder each.

*Item 6, constant time.* Three primitives were built in round 4
(`compiler/src/ct.rs`): `select(b, a, c)` becomes a `cmov`, `barrier(x)`
survives constant folding, `secure_zero(p, n)` survives the optimizer.
**Not** built: `secret[T]` itself, the spreading of the marking,
`declassify`, `u128`, `mul_wide`, and any effect of `#[constant_time]`. Both
reported a clean error with a line and a column
(`tests/neg/int_secret_not_implemented.fi`, `tests/neg/attr_not_implemented.fi`).
The acceptance said it in one sentence: *"Without `secret[T]` there is no
type check for secret data; the item stays open."*

*Item 4, debugger.* Criterion A was satisfied in round 94 (a test is a
function, `#[test]`, `firnc --test`, 35 of 35). Open at B: variables were
visible **only** in `--no-opt`, because a frame offset is the one thing the
optimizer takes away. Round 94 wrote it down rather than half building it:
*"After `mem2reg` only 2 of 11 `alloca`s of `tools/dwarf/probe.fi` are left
… a fixed frame offset would be a lie in all three cases. That needs DWARF
location lists."*

*Item 5, packages.* The summary table of `ACCEPTANCE.md` said registry, lock
file and the two-machine proof were missing. That table was **two years of
rounds out of date** — see section 8.

**What is there now.**

| | |
|---|---|
| `secret[T]` | a type (`types::Type::Secret`), with the whole rule set of SPEC §9.1 in `ct.rs` |
| refusals, measured | **20 new negative tests**, every one with line and column; `tools/ct/run.sh` **65 checks, 0 failed** |
| the assembly of a secret comparison | **0 conditional jumps of control flow** at all four build levels; the public twins of the same functions carry **1 each** |
| our own AES asked the same question | refused at `aes_probe.fi:25:29` and `:31:17` — the two lines note A3 of `lib/std/crypto/aes.fi` predicted |
| what the constant-time way costs | one S-box substitution: **3 ns** by table against **1,595–1,816 ns** by `select` over all 256 entries (~530x) |
| `#[constant_time]` | implemented: the check in the code generator is fed at last, and it strikes (unit test, built FIR) |
| variables in an OPTIMIZED build | `--debug-vars`: **8 of 11** at `dev-fast`, **6 of 11** at the two release levels — before the round it was **0** at every level |
| the values gdb reads there | **4 of 4 right at `dev-fast`, 0 wrong at any level** — a variable that cannot be answered for says `<optimized out>` |
| `.debug_loc` | **3 location lists** at `dev-fast`, 1 at the release levels; a list starts where the value comes into being, not at the function |
| `tools/dwarf/run.sh` | **63 → 73 checks**, 0 failed |
| the debugger used as a test of the optimizer | `tools/dwarf/diff_levels.sh` over the whole corpus reported **exactly one** difference between the two build levels — and that one was the bug above |
| a real bug found with the debugger | **yes**, and fixed: a breakpoint on a function landed BEHIND the first conditional branch at every optimized level, so it was hit for only half the calls (section 7) |
| item 5, re-measured in person | `tools/packages/run.sh` **39 of 39**, `tools/repro/two_machines.sh` **PASS** — lock file and two-machine proof were built in round 93; the **registry is still missing** |
| the fixpoint | **stage 2 == stage 3, character-identical**, 781,717 lines of assembly, 4,657,448 octets each (`tools/fixpoint.sh`, run inside the full suite AFTER every change of this round) |
| the whole suite | `bash test.sh`, sections 1-17 of 63 measured green while this was written, **0 failures**: 333 programs x 4 build levels = **1,332 checks**, **211 negative tests**, the proofs of the optimizer, the result location, the architecture guards, the symbol scheme, the atomic primitive, the interface bounds and the function values, the HTML5 tokenizer **6,810/6,810**, the four comparisons against `firnc1` (lexer, parser, layout/ABI, type checker), `self_compare` **332 same / 0 differing / 0 faulty**, and the fixpoint above. The remaining sections (18-64: test262, the browser rounds, the Minecraft client, aarch64) were still running -- this machine shares its cores with another acceptance run and the stage 2 compiler needs minutes per JavaScript file |

---

## 1. What `secret[T]` had to become, and what it must not become

SPEC §9.1 is one page, and every line of it is a rule that has to be
implemented somewhere:

> `secret[T]` is a type qualifier on integer and `bool` types. The type
> checker forbids on it: **Branching** … **Indexing** … **Division** …
> **Output or conversion to non-`secret`** … The marking propagates through
> expressions: `secret[u8] ^ u8` yields `secret[u8]`.

The first decision was the representation, and it decides how safe the rest
can be. `Type::Secret(Box<Type>)` is a variant of its own and is
**deliberately not transparent**: every `is_concrete_int()` in this compiler
— and there are about two hundred places that ask — now says **no** to a
secret value. A place that has not been taught about secrets therefore
REFUSES them with its own message instead of quietly treating them as public
data.

That is not a theoretical benefit. Three refusals in this round come from
places nobody wrote a rule for:

| written | what refuses it | message |
|---|---|---|
| `f"{s}"` with a secret `s` | the interpolation, which shows integers, bool, f32, f64 and str | `an interpolation f"{…}" cannot show a value of type secret[u8]` |
| `match s { … }` | `sema_match`, which knows enums, integers and bool | `'match' works on enums, integers and bool, not on secret[i32]` |
| `const K: secret[u8] = 5` | `check_consts`, which allows integer and bool | `'const' supports only integer and bool types in stage 0` |

Printing a secret is the plainest side channel there is, `match` is a jump
table indexed by its scrutinee, and a `const` is a number folded into every
use site. All three had to be refused, and all three were refused by rules
that already existed — because the type is not transparent.

## 2. The rules, and where each of them lives

Everything of SPEC §9.1 sits in `compiler/src/ct.rs`, in one file, so a
reader can check the language against the standard without a search:

| written | answer | why |
|---|---|---|
| `if s`, `while s`, `s && t`, `s \|\| t` | refused | a jump, and the processor takes two different amounts of time |
| `table[s]` | refused | the address lands in the cache (`C4`) |
| `a / s`, `a % s` | refused | division has a data dependent latency |
| `a + s`, `a - s`, `a * s` | refused | see section 3 — the overflow test IS a jump |
| `a +\| s` | refused | the saturating form clamps with `jo`/`jc` |
| `a << s` | refused | a secret shift AMOUNT is not constant time everywhere |
| `s as u8` | refused | that would be declassification by the back door |
| `s ^ t`, `s & t`, `s \| t`, `~s`, `-s` | allowed | one instruction, no jump, no memory |
| `a +% s` | allowed | bit for bit the unchecked path |
| `s << 3` | allowed | the VALUE may be secret, the amount may not |
| `s == t`, `s < t` | allowed, yield `secret[bool]` | `cmp` + `setcc`, no jump |
| `u8` where `secret[u8]` is wanted | allowed | classification leaks nothing |
| `declassify(s)` | allowed | and greppable, which is the point of it |

**Classification** sits at exactly one place: `sema::expr`, the funnel every
context passes through — declaration, assignment, argument, `return`, struct
field, array element. The wanted type travels down WITHOUT the marking and
what comes back public gets it. There is no second place that could be
forgotten, and there is no way back through a context: `declassify(x)` is a
call and stands out in the source text.

**The marking travels into the IR** at exactly one place as well
(`lower::lower_expr`): every value whose type is `secret[T]` lands in
`Func::secret`. From there the passes read it, and they have been reading it
since round 4 — `mem2reg` does not promote it, `opt` does not fold it,
`licm` does not move it, the register allocator leaves it in memory
(`regalloc.rs`: *"secret values stay at the stack slot"*), and the code
generator refuses a conditional jump on it. Round 4 built all of that
against an empty set. This round fills the set.

## 3. The find of the round: a `+` is a conditional jump

At the build levels `dev`, `dev-fast` and `release-safe` an addition carries
an overflow check (SPEC §13, `L9`, round 72). The check is a conditional
jump, and its condition is the carry flag — which comes out of the operands.
On secret operands that is a **branch on secret data**, and the code
generator's own check (`f.constant_time && f.is_secret(cond)`) would not
catch it: it looks at the condition of a `Term::BrCond`, and the overflow
arm is not one. It is `Op::CheckedBin`, and the backend turns it into
`add` + `jc` on its own (`panic_rt.rs`).

The same holds for the saturating forms `+| -| *|`: `codegen_x86::emit_wrap_sat`
clamps with `jo`/`jc`.

The answer is a type rule, not a code generator special case, and it holds at
**every** build level:

```
error: operator '+' is not allowed on a secret value
  --> tests/neg/ct_secret_add_checked.fi:10:25
   |
10 |     let c: secret[u8] = a + b
   |                         ^^^^^ here
   = note: the checked form jumps on the overflow flag of a secret value;
           write '+%', '-%' or '*%' — wrapping is what crypto code means anyway
```

A program must not compile at one level and leak at another. Wrapping is
what crypto code means in the first place — every one of the four positive
tests of this round uses `+%` and reads no worse for it.

## 4. The proof on the assembly (ACCEPTANCE item 6, second half)

The acceptance asks for the assembly TEXT of a comparison of two secret
values to be free of conditional jumps. A text cannot tell a jump belonging
to a public loop counter from one belonging to a secret, so the four
functions that are measured (`tools/ct/probe.fi`) are free of loops on
purpose and the claim is the strongest one a text can carry: **not one
conditional jump of control flow**.

`tools/ct/jumps.py` splits the two kinds by their target: a jump to
`.Lchkidx…`, `.Lchk…`, `.Lpanic…` or `.Lsat…` belongs to the checking
machinery and is decided by PUBLIC data (an index that stands in the source,
a length that stands in the type); everything else is control flow.

| build level | `ct_eq` | `ct_choose` | `ct_mask` | `ct_eq4` | `public_eq` | `public_choose` |
|---|---|---|---|---|---|---|
| release-fast | 0 | 0 (1 cmov) | 0 | 0 | **1** | **1** |
| dev (`--no-opt`) | 0 | 0 (1 cmov) | 0 | 0 of 8 jcc | **1** | **1** |
| dev-fast | 0 | 0 (1 cmov) | 0 | 0 | **1** | **1** |
| release-safe | 0 | 0 (1 cmov) | 0 | 0 | **1** | **1** |

The one interesting cell is `ct_eq4` at `--no-opt`: **eight** conditional
jumps, and every one of them is `jb .Lchkidxct_eq4_N` — the bounds check of
the eight array accesses, on indices that stand in the source text as `0`,
`1`, `2`, `3`. They always go the same way and they betray nothing; with the
optimizer on they are folded away entirely. That is written here rather than
filtered out, because a proof that hides its own awkward number is worth
nothing.

The counter-checks, without which the table above would measure nothing:
the public twins of the same two functions DO carry a conditional jump at
every level (last two columns), the extracted bodies are checked for a
minimum length (a "no jump" in an empty string is not a measurement), and a
deliberately wrong expectation (`99:99`) has to fail to match.

## 5. Our own AES, asked the question

`lib/std/crypto/aes.fi` carries a note that the round which wrote it put
there in full honesty, long before there was a type to check it with:

> A3 NOT CONSTANT TIME. This implementation uses S-BOX LOOKUPS, and a table
> lookup indexed by secret data leaks through the cache. SPEC 9 promises
> `secret` types and `#[constant_time]` for exactly this problem and stage 0
> does not implement them for tables.

`tools/ct/aes_probe.fi` is that very step — SubBytes — with the state
declared as what it is. The compiler refuses it, twice, at the two lines the
note predicted:

```
error: a secret value cannot be converted to the public type usize
   --> tools/ct/aes_probe.fi:25:29
error: an index must not be a secret value, found secret[usize]
   --> tools/ct/aes_probe.fi:31:17
```

And the way out that SPEC §9.1 names really works: `tools/ct/aes_ct.fi`
reads the WHOLE table and keeps the entry that was meant with `select`. It
gives **the same answer for all 256 inputs** — and it costs, measured over
20,000 substitutions each:

| way | per substitution |
|---|---|
| `sbox[x]`, one load | **3 ns** |
| `select` over all 256 entries | **1,595 / 1,570 / 1,816 ns** in three runs |

That is a factor of about 530, and it is the honest price of this pattern
for a 256-entry table. It is also the number that says why the second path
of round 82 exists: on a processor with AES-NI the whole round is ONE
instruction (`lib/std/crypto/accel.fi`), constant time by construction, and
no table is read at all.

## 6. Variables in an optimized build (ACCEPTANCE item 4 B)

Round 94 left the exact reason: after `mem2reg` the storage of most
variables is gone, the register allocator puts what is left into registers,
and `remove_dead_stores` makes a frame slot stale. A fixed offset would be a
lie in all three cases.

Three answers can be given truthfully, and this round gives them
(`dwarf::VarPlace`):

* the `alloca` still has a frame address → `DW_OP_fbreg -off`, as before
* the register allocator holds the cell in a REGISTER
  (`regalloc::promotable_cells`: one register for the whole function, and
  the address never escapes) → `DW_OP_reg<N>`
* the storage is gone and `mem2reg` left a **trail** → a **location list**
  in `.debug_loc`: from the label where the value comes into being to the
  end of the function

The trail is written only where it has ONE answer: a cell written **exactly
once** carries one value from its store onwards, no phi, no ambiguity. That
is every parameter and every `let`, which is most of what a reader asks a
debugger about. A cell written in a loop (`sum`, `k` of `tools/dwarf/probe.fi`)
has a different value per block; the honest answer for it is none at all,
and gdb then says `<optimized out>`.

A constant that the code generator folded into its use sites gets
`DW_OP_consts <n> DW_OP_stack_value` — the value itself, with no place.

**Measured** (`bash tools/dwarf/run.sh`, section 9, over
`tools/dwarf/probe.fi`, which declares 11 names):

| build level | variable DIEs | with a location | location lists | values checked | wrong |
|---|---|---|---|---|---|
| dev (`--no-opt`) | 11 | 11 | 0 | 4 of 4 | **0** |
| dev-fast | **8** | 8 | **3** | 4 of 4 | **0** |
| release-safe | **6** | 6 | 1 | 2 of 4 | **0** |
| release-fast | **6** | 6 | 1 | 2 of 4 | **0** |

Before this round the three optimized levels had **0** — variable
information was tied to `--no-opt`. The values are held against what the
program really computes: `shift(&p, 3)` makes `p = {x = 8, y = 10}` and
returns 18, `total(2)` computes 1+2+3+4+2 = 12, `flag` is true because
18 > 10. A gdb session at `probe.fi:33` and `:35`, in the `dev-fast` build:

```
Breakpoint 1, main () at tools/dwarf/probe.fi:33
p = {x = 8, y = 10}
moved = 18
flag = true
sum = <optimized out>          <- correct: sum does not exist yet
Breakpoint 2, main () at tools/dwarf/probe.fi:35
sum = 12
```

That `<optimized out>` at the first breakpoint is the location list doing
its work: the list begins at the label where the value is defined, and
before it gdb says nothing instead of reading a register that still holds
something else.

**Why this needed a flag.** `--debug-vars` (`-g`) is off by default, and
that is deliberate: debug information carries the working directory
(`DW_AT_comp_dir`), so a build with it is not reproducible octet for octet —
round 93 wrote that down as a known gap. Whoever wants to debug an optimized
program asks for it; whoever wants the same artifact on two machines does
not. The counter-check is in the proof: an optimized build **without** `-g`
carries 0 variables, exactly as before the round.

**What is still open here.** A variable written more than once (a loop
counter, an accumulator) gets no entry. Doing it needs the trail through the
phi nodes of the SSA construction of round 92 and a location list with one
entry per range — that is the next step and it is named, not pretended.
`aarch64` gets nothing of this: the places come out of the x86 register
allocator, and the second machine has its own (section 43 of `test.sh`
measures it separately).

## 7. The debugger as a test of the optimizer — and the bug it found

Criterion B of item 4 asks for more than a working debugger: *"a real bug was
found with it"*. The variable information of this round makes a method
possible that could not be run before (`tools/dwarf/diff_levels.sh`): build
the same file twice, `--no-opt -g` and `--opt-level=dev-fast -g`, stop at the
same function in both, and hold the PARAMETERS against each other. A
parameter has an exactly defined value at a function entry — the one the
caller passed — so two builds owe each other the same one.

**Two things the round learned about its own method**, and both cost a run:

* the LOCAL variables of a function are not comparable at its entry.
  `break <function>` stops after the prologue and before the first
  statement, so a local holds whatever was in its storage — garbage, and
  legitimately different garbage in two builds whose frames look nothing
  alike. Comparing them reported three "differences" in `examples/tour.fi`
  that were nothing of the sort.
* an ADDRESS is not a value. A pointer to a local names a frame offset, the
  frames differ, and a `u64` holding the address of a static differs as well
  (`__gc_alloc_in(st = 4831632)` against `4647520` is the state block of the
  collector). Both kinds are counted separately now instead of compared.

**And then the method found something.**

```
DIFFERENT  tests/1002_js_interp.fi  val__realm_bool  on:
           --no-opt='false'  dev-fast='true'
```

`val.fi:739` is four lines long:

```firn
fn realm_bool(r: Gc[Realm], on: bool) -> Gc[JsVal] {
    if on {
        return r.yes
    }
    return r.no
}
```

Neither value was wrong. The BREAKPOINT was in a different place — the
backtraces say it plainly:

```
--no-opt   #1 builtin__install_number   (builtin.fi:3561)   on = false
dev-fast   #1 builtin2__install2_array  (builtin2.fi:3851)  on = true
```

Two builds of the same deterministic program stopped at two different CALLS.
The reason is that `break <function>` does not stop at the first instruction
— the frame is not set up there and the parameters are not where the debug
information says. The debugger has to find the end of the prologue, and if
the line table does not SAY where it is, gdb guesses: it takes the second
line entry of the function. That guess is right without the optimizer and
wrong with it. Here it landed **behind the conditional jump**, inside one arm
of the `if`, so the breakpoint was hit only for the calls that took that arm
and the other half ran straight past it.

A debugger that silently misses half the calls to a function is worth less
than none, and nothing about it looks broken while it happens.

**The cause, exactly.** Round 94 put the source position ON THE INSTRUCTIONS
(`fir::Inst.loc`) and that is why the line table is right at every build
level. A TERMINATOR is not an instruction and carries no position. As long as
the body begins with something else, that does not show: the load of `on`
carries line 740 and the branch inherits it. After `mem2reg` the load is
gone, the entry block of `realm_bool` consists of nothing but the branch, and
not one `.loc` announces the body — the whole block still counts as line 739,
the `fn` line. Minimal reproduction, six lines, at `--opt-level=dev-fast`:

```
_F0.pick:
    .loc 1 1 0          <- the fn line, and the only entry in the function
    push rbp
    …
.Lpick__bb0:
    test r8b, r8b       <- `if on`, line 2, announced by nobody
    jnz .Lpick__bb1
```

**The fix** is the flag DWARF has for exactly this and GNU `as` writes:
`.loc <file> <line> <col> prologue_end`. Where the body has a position of its
own the marker goes ON that instruction and not one line earlier — between
the prologue and it lie the stores that put the parameters into the places
the debug information names, and a breakpoint in front of them reads a
parameter that has not arrived yet. That is the second trap and it was
measured too: with the marker at the frame setup, `docs/gdb_example.fi`
answered `n = 0` where the caller had passed 10. Where the entry block has no
position at all — the case that started all this — the marker goes right
after the frame setup, with the `fn` line.

**After the fix**, in the `dev-fast` build:

```
Breakpoint 1, val__realm_bool (r=0x7ffff7d81e70, on=false) at lib/js/val.fi:739
#1  0x51cb25 in builtin__install_number (…) at lib/js/builtin.fi:3561
```

The same call, the same value as at `--no-opt`. `tools/dwarf/run.sh` stays at
**73 of 73** — the sessions of round 64 and round 94 are unchanged by it,
which is what had to be checked: the marker must not move a breakpoint that
was already right.

**What this does NOT claim.** The underlying gap is still there: a terminator
carries no source position. The fix answers the question a breakpoint asks
("where does the prologue end") and does not give the branch its line back.
Stepping through a condition at an optimized level still shows the `fn` line.
Putting a position on `fir::Term` is the next step and is named here rather
than pretended.

**The run this came out of**: `tools/dwarf/diff_levels.sh` over
`tests/*.fi`, `examples/*.fi` and `tools/dwarf/probe.fi` — 319 files, each
built twice and walked function by function. It reported **exactly one**
difference, `val__realm_bool`, and that one was the bug of this section. Two
earlier runs of the same script reported seven more, and every one of them
was the method's own fault (locals at a function entry, addresses); they are
what the two rules above are made of.

That is the honest shape of the result: the method is worth having, it found
one real thing, and most of the work of building it went into learning what
it must NOT compare.

## 8. Item 5: the summary table was out of date

The task of this round named registry, lock file and the two-machine proof
as missing for item 5. The **summary table** at the end of `ACCEPTANCE.md`
does say so — and it has said so since round 3. The item's own section says
something else, and round 93 measured it:

* the lock file `firn.lock` exists (`compiler/src/lock.rs`,
  `lib/firnc1/lock.fi`, SHA-256 written out on both sides)
* `--lock` writes it, `--locked` insists on it
* the two-machine proof exists and runs: `tools/repro/two_machines.sh`,
  with another working directory, `$HOME`, `$TMPDIR`, `$TZ`, `$LC_ALL`,
  `$PATH`, umask, wall clock, source time stamps, creation order, compiler
  path, and the second run under `qemu-x86_64`

Re-measured in person for this round, before a line of it was touched:

| | |
|---|---|
| `bash tools/packages/run.sh` | **39 of 39** |
| `bash tools/repro/two_machines.sh` | **PASS** — same artifact out of both compilers on both machines, same `firn.lock` in all four runs |

**What is really missing is the registry**, and it is missing in the sense
the acceptance itself already states: *"What is deliberately NOT claimed: no
network and no registry (`needs` knows local paths)."* The criterion of item
5 does not ask for one — it asks for the bit-identical artifact — but the
summary table promised three things and delivered a stale sentence. It is
corrected in `ACCEPTANCE.md` with the numbers above.

Building a registry was not attempted in this round. It would have to be
built in **both** compilers with octet-identical messages, like everything
else in the package system, and half of it would be worth less than none.

## 9. What this round did not do

* **`u128`/`i128` and `mul_wide`** (SPEC §9.3, `C5`). They are arithmetic,
  not a security property, and a half-built 128-bit type would be worse than
  none. Named as open in `ACCEPTANCE.md`.
* **`secret[T]` in `firnc1`.** The compiler in Firn does not know the type.
  It counts a file that uses `secret` or `declassify` as NOT CORE LANGUAGE,
  by spelling, exactly as it does for the SIMD words of round 82 —
  `tools/ct/run.sh` section 5 measures it: of 23 refused files, firnc1
  refuses 18 as not core and 5 otherwise, and **accepts none of them**. That
  is the important number: a compiler that silently dropped the marking
  would build a program that leaks.
* **A generic over a secret.** `Vec[secret[u8]]` is refused: not one line of
  a generic container has been read against SPEC §9, and a `sort` would put
  an `if` on a secret. Fail closed; `[secret[u8]; N]` is the way for now.
* **The pass table still lies a little.** `opt::PASSES` tags `mem2reg` as
  `debug_preserving: true`, and the tag's own comment says *"every named
  variable still shows its correct value at every breakpoint"*. After this
  round that is true for 8 of 11 names instead of 0 of 11 — better, and
  still not what the tag claims. It is not silently corrected here either:
  turning the tag to `false` would take `mem2reg` out of `dev-fast`, which
  is the DEFAULT build level, and that is a performance decision and not a
  documentation one.
* **The loop variables** of section 6, and **aarch64**.

## 10. Reproduction

```sh
bash tools/ct/run.sh          # constant time: 65 checks
bash tools/dwarf/run.sh       # debug information: 73 checks
bash tools/dwarf/diff_levels.sh   # the two build levels against each other
bash tools/packages/run.sh    # 39
bash tools/repro/two_machines.sh
bash tools/fixpoint.sh        # stage 2 == stage 3
bash test.sh                  # everything
```
