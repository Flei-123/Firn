# gpu.spirv -- SPIR-V reader + interpreter (G7)

`lib/gpu/spirv.fi`, tests `2920`-`2925`, test shaders `lib/gpu/spv/test/*.frag` (+ committed `.spv`, rebuild with `build.sh`
there; the tests need no tool). First step of "interpreter first, JIT later" from `docs/GRAPHICS-STACK-E-REQUIREMENTS.md`.

## API (module `gpu.spirv`)

| call | meaning |
|---|---|
| `spv_new(code_ptr, octets, &m) -> bool` | reads + validates the structure, copies the module, evaluates constants. On `false`: `m.err`, `m.err_op` |
| `spv_find_entry(&m, model) -> u32` | function id of the first `OpEntryPoint` of the model (0 Vertex, 4 Fragment, 5 GLCompute); 0 = none |
| `spv_find_location(&m, storage, loc)` / `spv_find_builtin(&m, storage, builtin)` | word offset of an Input (1) / Output (3) / ... variable; `SPV_NONE` = not found. Built-ins inside a block struct (`gl_Position`) work |
| `spv_mem_set/get(&m, base, k[, v])`, `spv_mem_set_f32/get_f32` | host side of the I/O: word `k` of the variable at `base` |
| `spv_run(&m, fn_id, max_steps) -> bool` | one invocation; `false` + `m.err` (`E_STEPS` after `max_steps` instructions, `E_OP` = unsupported opcode, ...) |
| `spv_reset`, `spv_free`, `spv_killed` | zero memory + re-apply initialisers / free / `OpKill` seen |

Model: every id with a result type owns a fixed region of 32-bit words, every `OpVariable` a region of memory; both use the PACKED
layout (scalar 1 word, vector n, matrix = columns, array/struct = elements/members in order). A pointer is a word offset. Nothing
is allocated per run. Every access of a module is bounds checked: a damaged module sets `err`, it does not crash (test 2925).

## Supported
* Binary: header, instruction boundaries, id bound, types (bool, 32-bit int/float, vector, matrix, array, struct, pointer, function, void), constants (incl. `Spec*` with their default), composite constants, `OpConstantNull`, `OpUndef` (zero).
* Memory: `OpVariable` (initialisers), `OpLoad`, `OpStore`, `OpAccessChain`/`InBounds` (constant and dynamic indices, range checked), `OpCopyMemory`.
* Arithmetic: `IAdd ISub IMul UDiv SDiv UMod SRem SMod SNegate`, `FAdd FSub FMul FDiv FRem FMod FNegate`, shifts, `And Or Xor Not`, all integer / float (ordered + unordered) comparisons, logical ops, `Select`, `Any/All`, `IsNan/IsInf`, conversions (`ConvertFToS/U`, `SToF/UToF` saturating, `Bitcast`, `CopyObject`).
* Vectors/matrices/composites: `CompositeConstruct/Extract/Insert`, `VectorShuffle`, `VectorExtractDynamic/InsertDynamic`, `VectorTimesScalar`, `MatrixTimesScalar`, `MatrixTimesVector`, `VectorTimesMatrix`, `MatrixTimesMatrix`, `Transpose`, `Dot`.
* Control flow: `Label`, `Branch`, `BranchConditional`, `Switch`, `Phi` (swap-safe), `LoopMerge/SelectionMerge` (ignored hints), `FunctionCall` with arguments and `ReturnValue` (no recursion), `Return`, `Kill`.
* `GLSL.std.450`: Round RoundEven Trunc FAbs SAbs FSign SSign Floor Ceil Fract Sin Cos Tan Atan Atan2 Pow Exp Log Exp2 Log2 Sqrt InverseSqrt FMin/UMin/SMin FMax/UMax/SMax FClamp/UClamp/SClamp FMix Step SmoothStep Fma Length Distance Cross Normalize Reflect.

## Measured
* `tests/2920`-`2925` green in dev-fast, `--no-opt`, release-safe, release-fast and (2921-2925) aarch64 under qemu -- floats are bit-equal with Firn f32 arithmetic where the operation is exact.
* Fuzz (2925): 7500 mutated modules (5 shaders), no crash / hang / arithmetic panic.
* Speed: ~10 M SPIR-V instructions/s (release-fast, busy host; `flow.frag`, 1850 instructions per invocation).

## NOT supported (errors `E_OP` / `E_TYPE`, listed so nobody guesses)
* Images, samplers, `OpImageSample*`, derivatives (`OpDPdx...`), subgroup / atomic / barrier ops (barriers are ignored: ONE invocation), ray tracing.
* 16/64-bit scalars (`Int64`, `Float64`, `Float16`), `OpSpecConstantOp`, `OpRuntimeArray`, `OpPtrAccessChain`, `OpArrayLength`, physical storage buffers, `OpBitFieldInsert/Extract`, `OpBitCount`, `OpFwidth`...
* GLSL.std.450 not listed above (Asin Acos Sinh... Determinant MatrixInverse Frexp Ldexp, Pack/Unpack, Interpolate*, FaceForward, Refract, NMin/NMax/NClamp).
* `Offset`/`ArrayStride`/`Binding`/`DescriptorSet` decorations are ignored: uniform/storage buffers are plain variables set word by word, not a std140 byte image.
* Transcendentals (`Sin Cos Exp Log Pow ...`) are f64 `std.math` rounded to f32: NOT the bits of a GPU, error < 1e-5 in the tests. `Fma` is not fused. `Dot` sums left to right.
* One invocation at a time, no quad / helper lanes, no vertex fetch / rasterizer / blending (the 2924 test has a 40-line edge-function rasterizer as a demo).
* No JIT (needs executable memory + an x86 emitter in Firn: separate project), no SPIR-V validator (use `spirv-val`), no name-based `spv_find_entry` (first entry of the model).
