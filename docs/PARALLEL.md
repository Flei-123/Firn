# Parallel compilation

## Optimizer (`compiler/src/parfn.rs`)
- `FIRN_OPT_THREADS=n`: per-function passes on up to n threads (default min(8, cores), 1 = serial).

## x86-64 code generator (`compiler/src/parcodegen.rs`)
- `FIRN_CODEGEN_THREADS=n`: functions are emitted by n workers (default min(8, cores), 1 = the serial loop in `codegen_x86::emit`). `FIRN_CODEGEN_MIN_FUNCS=n` lowers the size threshold (default 32) for tests.
- Active for x86-64 Linux/Android only. Windows (thunk and import state) and aarch64 stay serial.
- Worker state copied from the compiling thread: target switches, extfn tables, profile, check mode, gc runtime flag, panic handler.
- Numbering shared by the whole translation unit is made function-local and renumbered when the results are joined in function order, so the assembler text is byte identical to the serial run:
  - `.Lfconst<n>` (`fpool`), `.Lpanicmsg<n>` (`panic_rt`), `.Lmcpy_*<n>` (`memcpy_idiom`), `.Lcpuid_*_<n>` (`simd`).
- Frame-budget selection of recursive callers (`regalloc::frame_bytes`) runs in parallel too; its pool side effects are replayed before the function they belong to.
- Proof: `tools/parcodegen/run.sh` (threads 1 vs 8, assembler text compared).
- `FIRN_STAGE_TIMINGS=1` prints `cg ...` stage times and the parallel emit/renumber/relocate split.
