# NDARRAY-PLAN -- typed n-dimensional arrays for Firn (design only, nothing built)

Status 2026-10-19. Extends `docs/NDARRAY.md` (design notes); this file fixes the build order.

## Goal
A NumPy-like `NdArray[T]` for `f64`, `f32`, `i64`, `i32`, `u8`, `bool` with shape, strides, views, elementwise ops,
reductions and matmul -- enough to port LogicLab numeric code and to beat Python+NumPy on small/medium sizes.

## Data model
- `struct NdArray[T] { data: *mut T, shape: Vec[usize], strides: Vec[isize], offset: usize, owner: bool }`
- Row-major default, arbitrary strides for views (`t()`, `slice`, `reshape` when contiguous, `broadcast_to`).
- Rank <= 8. Shape/stride vectors are small; keep inline `[usize; 4]` fast path for rank <= 4.
- Views share `data`; one owner frees (no destructors in Firn: explicit `nd_free`).

## Build order (each step: lib + Python/NumPy cross-check test, zero deviations)
1. Core: `nd_zeros/ones/full/arange/linspace/from_vec`, `shape/len/get/set`, `reshape`, `transpose`, `slice`, `copy`. (M)
2. Elementwise: `add/sub/mul/div/neg/abs/sqrt/exp/ln/pow` with broadcasting; scalar forms; in-place `_to`. (M)
3. Reductions: `sum/prod/min/max/argmin/argmax/mean/var/std` over all or one axis (pairwise summation like NumPy). (M)
4. Linear algebra: `dot`, `matmul` (blocked, SIMD where the backend allows), `solve` (LU, partial pivot), `inv`, `det`. (L)
5. Indexing: boolean masks, fancy index by `Vec[usize]`, `where`, `sort/argsort`, `cumsum`. (M)
6. I/O: `.npy` read/write (header parse), CSV via std.csv. (S)

## Cross-check
`tools/libs_py_cross/nd_ref.py` runs NumPy on LCG-driven inputs; the test mirrors it. Float results compared bit-exactly for
elementwise ops and add/mul/sum with pairwise summation; matmul and LU within 1e-12 relative (summation order).

## Open decisions
- Operator syntax (`a + b`) needs operator overloading (language gap G27); until then named functions.
- Float width default: f64. f32 kernels only after f64 passes.
- SIMD: use the xmm-ra backend work (Roadmap r58) -- no new intrinsics before the scalar version is verified.
