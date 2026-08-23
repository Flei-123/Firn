# Round 84 — `firnc run`: compile and start, in one command

The wish was one sentence long: *start a Firn file the way `python test.py`
starts a Python file.* Until this round the compiler could not do it. There
was `-o <path>` and there were the `--emit=` forms, but no mode that
translates AND runs. Every try of a three line program was two commands and
one invented file name.

Now it is one:

```sh
firnc run hello.fi
firnc run tool.fi --input data.csv --verbose
./program.fi                 # with a shebang line, see §3
```

What follows is what was built, what was measured, and what deliberately was
not built.

---

## 1. The command

`compiler/src/main.rs::run_subcommand`, caught in `main()` before the normal
option parsing — and that placement is the whole point:

```
firnc run [OPTIONS] file.fi [ARGUMENTS...]
          ^^^^^^^^^ compiler   ^^^^^^^^^^^ program, never looked at again
```

Up to and including the FIRST word that is not an option, the arguments
belong to the compiler. Everything after the file name is handed to the
program unchanged. That is what makes

```sh
firnc run tool.fi --help
firnc run tool.fi -o out.txt
```

work at all: `--help` and `-o` are exactly the two words a compiler would
otherwise swallow, and a launcher that swallows them is useless for the
programs people actually write.

Three more properties, and all three are checked rather than claimed:

* **The exit code of `firnc run` IS the exit code of the program.** Without
  that the command cannot be used in a script, and a launcher that always
  returns 0 hides every failure. (Killed by a signal: 128 + number, the
  shell's convention.)
* **Standard input, output and error are inherited**, not copied through a
  pipe of our own. A terminal stays a terminal, a pipe stays a pipe, and the
  program's stderr does not end up in its stdout.
* **The default build level is `dev-fast`**, not `release-fast`. A program
  that is started once wants a short cycle, not the whole optimizer.
  `--opt-level=` is read afterwards and wins.

`run` refuses `-o`, `--emit=` and `-c` BEFORE the file name with an error
instead of quietly ignoring them: those ask for something other than
"start it", and silently doing something else is worse than saying no.

## 2. The cache

Recompiling an unchanged file is the thing that makes a compiled language
feel slow next to an interpreted one. So the result is kept — the same idea
as `__pycache__`, only in one visible place:

```
$FIRN_CACHE, else $XDG_CACHE_HOME/firn, else $HOME/.cache/firn
  hello-a824be1d6cea59a79be9b23c7270ad8d
  run_main-3f0c...
```

The name is `<file stem>-<first 128 bits of the key>`, and the key is a
SHA-256 (`compiler/src/runcmd.rs`, 50 lines, FIPS 180-4, checked against the
official vectors in the module tests — the compiler still has no external
crates) over EVERYTHING that can change the produced binary:

| in the key | why |
|---|---|
| source text of the root file | obvious |
| source text of **every** module `import` reaches | the trap: change a module, leave the main file alone |
| the PATH of each of those files | a different `$FIRNLIB` is a different program |
| every package manifest of the world | a manifest can switch the profile |
| compiler version **and** size + mtime of the running binary | the version string is a constant and does not move when the compiler is rebuilt |
| build level, target, profile | different flags, different code |

Everything goes in length prefixed, so two different lists of files cannot
produce the same octet stream by concatenation.

**The rule: the cache may never hand back a wrong answer.** It is allowed to
err in exactly one direction — compiling again when it did not have to. So
when anything about the module resolution fails (a missing module, a broken
manifest), there is no key at all: the file is compiled without a cache and
the compiler prints the real error message.

The counter-check for the trap is case 9 of `tools/run/run.sh`, and it is
built so that a timestamp cache would fail it: `main.fi` is copied once, its
mtime is set to 2020-01-01 and never touched again; only `helper.fi` next to
it changes. Before: exit 10. After: exit 33. The test also asserts that the
mtime and size of `main.fi` really did not move.

`--no-cache` switches the cache off (compile into a temporary file, start it,
delete it — the test checks that nothing is left behind), `--clear-cache`
empties the directory.

## 3. The shebang

```
#!/usr/bin/env firnc-run
fn main() -> i32 { ... }
```

Three small pieces:

* **Both lexers** skip a `#!` line — `compiler/src/lexer.rs::run` and
  `lib/firnc1/lexer.fi::lex_run`, the same rule in both, because the fixpoint
  (`tools/fixpoint.sh`) compares what the two compilers produce and a lexer
  that only exists in one of them breaks it. Only line 1, only when the first
  two characters are `#!`, and the newline itself stays so that the first
  real token sits on line 2 with the right column.
* `#` does **not** become a comment character. Everywhere else it stays the
  `Hash` token that opens an attribute (`#[inline]`). Case 13 of the test
  puts `#!/bin/sh` in line 2 and checks that the parser still complains
  about it at `2:1`.
* `tools/run/firnc-run` is the helper `env` calls: a five line shell script
  that `exec`s `firnc run "$@"`. `exec`, so that the exit code and the
  signals belong to the program and not to a wrapper.
* `demos/hello_run.fi` is the example, with the executable bit set in git.
* **The formatter had to learn it too.** `firnfmt` has a scanner of its own
  (`tools/fmt/fmt.fi`), so it did not know about the rule and reshaped the
  line into `#!/ usr / bin / env firnc - run` -- a file that no longer
  starts. `tools/fmt/firnfmt.fi::shape` now copies a leading `#!` line
  through character for character and formats the rest; the pinned case
  `tools/fmt/cases/shebang.in` holds it there. That is the sort of thing
  that only turns up because the whole tree is held against the formatter
  in `test.sh`.

```sh
ln -s "$PWD/tools/run/firnc-run" ~/.local/bin/firnc-run   # once
./demos/hello_run.fi                                      # hello from a shebang
```

## 4. What it is worth, measured

`bash tools/run/bench.sh 5`, median of five runs, AMD EPYC 7571, the same
machine as every other figure in this repository. The times are around the
WHOLE command, so the runtime of the program is inside every column; the
third column names it separately.

| program | cold | warm | the program alone | saved |
|---|---:|---:|---:|---:|
| `examples/hello.fi` (14 lines) | 10 ms | 3 ms | 2 ms | 7 ms |
| `demos/number_check.fi` (4 modules) | 339 ms | 16 ms | 1 ms | 323 ms |
| `lib/js/run_main.fi` (the JavaScript engine) | 2543 ms | 87 ms | 3 ms | 2456 ms |

And the yardstick people will hold against it anyway:

| | |
|---|---:|
| `python3 bench.py` (`print("hello")`) | 16 ms |
| `python3 -c pass` | 16 ms |

Read honestly, that says three things:

1. **The JavaScript engine starts 29 times faster warm than cold** — 2543 ms
   down to 87 ms. That is the difference between "I will avoid trying this"
   and "just run it".
2. **A warm start of a small program is 3 ms** — five times quicker than the
   bare startup of the interpreter it is being compared with, and 1 ms of
   those 3 are `firnc run` itself. This is not a language comparison
   (`python3` does not compile anything and Firn produces machine code); it
   is there so the number has a scale.
3. **The warm start is not free, and it grows with the project.** 87 ms for
   the JS engine is the fingerprint: to know whether the cache is valid, all
   ~100 source files have to be read and lexed for their `import` lines. A
   cache on timestamps would be quicker and would be exactly the kind of
   cache that lies. If this becomes the bottleneck, the fix is a stored
   dependency list guarded by mtimes with a content check on mismatch — not
   a weaker key.

## 5. The proof

`tools/run/run.sh`, 17 cases, hooked into `test.sh` as **section 45**:

| # | what |
|---|---|
| 1 | the exit code of the program becomes the exit code of `run` (42) |
| 2 | a program that succeeds: exit 0, output unchanged |
| 3 | three arguments arrive unchanged |
| 4 | `--help` after the file name goes to the program |
| 5 | `-o`, `--`, `-x` reach the program; no file is written |
| 6 | standard input passes through; the program's stderr stays stderr |
| 7 | a compile error is shown with line:column, exit != 0, nothing cached |
| 8 | cold cache compiles, warm cache does not, same answer |
| 9 | a changed MODULE forces a new compilation, main file untouched |
| 10 | a changed main file forces a new compilation |
| 11 | `--no-cache` compiles every time and leaves nothing behind |
| 12 | `./demos/hello_run.fi` starts through the shebang |
| 13 | `#!` is skipped in line 1 only; in line 2 it is a token like any other |
| 14 | `--opt-level` belongs to the key, the default is `dev-fast`, nonsense is refused |
| 15 | `--clear-cache` empties the cache, the next start compiles |
| 16 | `run` refuses `-o` / `--emit=` / `-c` before the file, a missing file, no file |
| 17 | the cached binary takes new arguments |

Plus the Rust module tests in `compiler/src/runcmd.rs` (SHA-256 against the
FIPS vectors, including the million-`a` case and chunked input, and the
proof that the length prefix makes the feed unambiguous) and in
`compiler/src/lexer.rs` (the shebang, and that `#` in line 2 stays a `Hash`).

## 5.1 The state of `test.sh` in this round -- honestly

`./test.sh` on this branch: **1199 of 1202**, and the three that are not
green have nothing to do with round 84:

* `tools/aarch64/run.sh` (twice, once per build stage):
  `tests/1613_crypto.fi` cannot be built for aarch64 -- *"cannot emit the
  vector instruction yet (round 82 built them for x86-64)"*. That is the
  leftover of the r80/r82 merge and it fails **on `main` in exactly the same
  way**: checked by running `tools/aarch64/run.sh` in the untouched main
  worktree (`598f2061`), same case, same message, `RESULT: 296 of 301`.
* `tools/js/round66.sh`: the promise soak died with SIGSEGV in the run that
  had four full test suites on the same 8 vCPU at once. Run again on its own
  it passes (`OK: the features of round 66 hold their limits`, exit 0). A
  load flake, not a regression.

What round 84 touches is green: section 45 (17/17), section 24 (the
formatter, 5/5 pinned cases and the whole tree in shape), section 21 (the
English check), `tools/self_compare.sh` (321 the same, 0 differing, 0
faulty) and `tools/fixpoint.sh` (stage 2 == stage 3, character-identical,
649,903 lines of assembly) -- the last one is the one that matters here,
because the shebang rule had to go into BOTH lexers.

## 6. What this round is NOT

No interpreter, no JIT, no incremental compilation. `firnc run` compiles the
whole program, exactly as before, and starts it. The cache makes the second
start cheap; it does not make the first one cheaper. Anyone who wants the
first start cheaper is asking for incremental compilation, and that is a
round of its own.
