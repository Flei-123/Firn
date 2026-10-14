# gpu.spirv -- SPIR-V reader + interpreter (G7)

`lib/gpu/spirv.fi`, tests `2920`-`2925` + `2936`-`2939` (images, quad, pack, fuzz), test shaders `lib/gpu/spv/test/*.frag` (+ committed `.spv`, rebuild with `build.sh`
there; the tests need no tool). First step of "interpreter first, JIT later" from `docs/GRAPHICS-STACK-E-REQUIREMENTS.md`.

## API (module `gpu.spirv`)

| call | meaning |
|---|---|
| `spv_new(code_ptr, octets, &m) -> bool` | reads + validates the structure, copies the module, evaluates constants. On `false`: `m.err`, `m.err_op` |
| `spv_find_entry(&m, model) -> u32` | function id of the first `OpEntryPoint` of the model (0 Vertex, 4 Fragment, 5 GLCompute); 0 = none |
| `spv_find_location(&m, storage, loc)` / `spv_find_builtin(&m, storage, builtin)` | word offset of an Input (1) / Output (3) / ... variable; `SPV_NONE` = not found. Built-ins inside a block struct (`gl_Position`) work |
| `spv_mem_set/get(&m, base, k[, v])`, `spv_mem_set_f32/get_f32` | host side of the I/O: word `k` of the variable at `base` |
| `spv_run(&m, fn_id, max_steps) -> bool` | one invocation; `false` + `m.err` (`E_STEPS` after `max_steps` instructions, `E_OP` = unsupported opcode, ...) |
| `spv_reset`, `spv_free`, `spv_killed`, `spv_killed_lane(&m, l)` | zero memory + re-apply initialisers (all lanes) / free / `OpKill` seen |
| `spv_tex_set(&m, slot, fmt, px, w, h)`, `spv_tex_set_level(&m, slot, level, px, w, h)`, `spv_tex_clear` | CPU texture `slot` (16 slots, 12 levels): `fmt` `SPV_FMT_RGBA8` (4 bytes/texel) or `SPV_FMT_RGBA32F` (4 f32); rows top to bottom; pixels stay owned by the host. `false` on bad slot/size |
| `spv_sampler_set(&m, slot, min, mag, mip, wrap_s, wrap_t)`, `spv_sampler_border(&m, slot, r, g, b, a)` | `SPV_FILTER_NEAREST/LINEAR`, `SPV_MIP_NONE/NEAREST/LINEAR`, `SPV_WRAP_REPEAT/CLAMP/MIRROR/BORDER`. Default: nearest, no mips, repeat |
| `spv_quad_enable(&m)`, `spv_lane(&m, l)`, `spv_run_quad(&m, fn, max_steps)` | 4 lanes = 2x2 quad (0 = (x,y), 1 = (x+1,y), 2 = (x,y+1), 3 = (x+1,y+1)). `spv_lane` picks the lane `spv_mem_*` / `spv_run` act on. `spv_run_quad` runs all four in lockstep at derivative / implicit-LOD instructions; `max_steps` is per lane |

Texture / sampler slot of a shader variable = its `Binding` decoration (`DescriptorSet` ignored); arrays of `sampler2D` get `Binding + index`.

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

* Images (2D, non-arrayed, non-multisampled, float sampled type): `OpTypeImage/Sampler/SampledImage`, `OpSampledImage`, `OpImage`, `OpImageSampleImplicitLod/ExplicitLod`, `OpImageSampleProjImplicitLod/ProjExplicitLod`, `OpImageFetch`, `OpImageQuerySize/SizeLod/Levels/Lod/Samples`; image operands `Bias Lod Grad ConstOffset Offset`. GL rules: nearest/bilinear, trilinear, LOD from level-0 gradients (`log2 max(|dP/dx|,|dP/dy|)`), magnification at LOD <= 0, 4 wrap modes. Missing texture: (0,0,0,1); fetch out of range / missing level: zeros.
* Derivatives `OpDPdx/Dy/Fwidth` + `Fine` + `Coarse` (quad); without a quad (`spv_run`) they are 0 and implicit LOD = magnification. Diverging lanes never deadlock (a lane that ended leaves stale values: undefined, like on a GPU).
* `GLSL.std.450` PackSnorm4x8 PackUnorm4x8 PackSnorm2x16 PackUnorm2x16 PackHalf2x16 UnpackSnorm2x16 UnpackUnorm2x16 UnpackHalf2x16 UnpackSnorm4x8 UnpackUnorm4x8 (half: round to nearest even, bit-equal with numpy float16 on 96 cases; round-trip of every 5th half).

## Measured
* `tests/2920`-`2925` green in dev-fast, `--no-opt`, release-safe, release-fast and (2921-2925) aarch64 under qemu -- floats are bit-equal with Firn f32 arithmetic where the operation is exact.
* Fuzz (2925): 7500 mutated modules (5 shaders), no crash / hang / arithmetic panic. Fuzz (2939): 6000 mutants of tex.frag + pack.frag with textures bound, random inputs, single and quad runs.
* Sampling (2936/2937) against an independent f32 reference (written from the GL rules, tolerance 3e-6 relative): 6 uv x 6 LOD, 4 sampler variants x wrap modes, 5 quad scales x 4 lanes; derivatives bit-equal.
* Speed: ~10 M SPIR-V instructions/s (release-fast, busy host; `flow.frag`, 1850 instructions per invocation).

## NOT supported (errors `E_OP` / `E_TYPE`, listed so nobody guesses)
* Images other than 2D/float: 1D, 3D, Cube, Rect, Buffer, arrayed, multisampled, integer sampled types; `OpImageSampleDref*`, `OpImageGather`, `OpImageDrefGather`, `OpImageRead/Write`, storage images, `MinLod`, `Sample`, `ConstOffsets`, `OpImageQueryFormat/Order`; anisotropic filtering; sRGB decode; texel formats other than RGBA8 / RGBA32F; texture arrays only via `Binding + index` handles.
* Subgroup / atomic / barrier ops (barriers are ignored), ray tracing. Quad derivatives only via `spv_run_quad` (always 4 lanes, no helper-lane masking of `OpKill`).
* 16/64-bit scalars (`Int64`, `Float64`, `Float16`), `OpSpecConstantOp`, `OpRuntimeArray`, `OpPtrAccessChain`, `OpArrayLength`, physical storage buffers, `OpBitFieldInsert/Extract`, `OpBitCount`, `OpFwidth`...
* GLSL.std.450 not listed above (Asin Acos Sinh... Determinant MatrixInverse Frexp Ldexp, PackDouble2x32/UnpackDouble2x32, Interpolate*, FaceForward, Refract, NMin/NMax/NClamp).
* `Offset`/`ArrayStride`/`Binding`/`DescriptorSet` decorations are ignored: uniform/storage buffers are plain variables set word by word, not a std140 byte image.
* Transcendentals (`Sin Cos Exp Log Pow ...`) are f64 `std.math` rounded to f32: NOT the bits of a GPU, error < 1e-5 in the tests. `Fma` is not fused. `Dot` sums left to right.
* One invocation (or one 4-lane quad) at a time, no vertex fetch / rasterizer / blending (the 2924 test has a 40-line edge-function rasterizer as a demo).
* No JIT (needs executable memory + an x86 emitter in Firn: separate project), no SPIR-V validator (use `spirv-val`), no name-based `spv_find_entry` (first entry of the model).
