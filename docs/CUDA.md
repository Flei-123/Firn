# CUDA from Firn (lib/cuda)

NVIDIA Driver API and NVRTC through `std.dynlib` (nvcuda.dll / nvrtc64_120_0.dll on Windows, libcuda.so.1 / libnvrtc.so on Linux). No compiler change, no CUDA SDK needed at build time.

```firn
import cuda.driver as cu
import cuda.kernel as kn

cu.init()                                  // library, cuInit, device 0, primary context
let m: u64 = kn.load_source(KERNELS)       // NVRTC (sm_XY cubin, PTX fallback) + cuModuleLoadData
let f: u64 = kn.func(m, "name")
let d: u64 = cu.alloc(bytes)
let a: u64 = kn.args_new()
kn.arg_ptr(a, d)  kn.arg_i32(a, n)  kn.arg_f32(a, v)
kn.launch(f, grid_x, grid_y, block_x, shared, a)
cu.sync()  cu.download(host, d, bytes)  cu.upload(d, host, bytes)  cu.copy  cu.memset32  cu.free
```

Modules: `cuda.driver` (context, memory, errors), `cuda.nvrtc` (compile), `cuda.kernel` (module, function, arguments, launch, `set_debug`), `cuda.loader` (+ `.windows` twin: library names, toolkit folders v12.0-v13.0), `cuda.cutil`.

Tested: `tests/6100_cuda_graceful.fi` (no driver: prints a skip line, exit 0 on Linux and Wine). On a Windows PC with an RTX 3060 (driver 610, CUDA 12.8 toolkit): device name, NVRTC compile, kernel launch with pointer/int/float arguments, device to host copy, all correct.

Traps: driver results are `int` (mask the upper 32 bits); `CUDA_PATH` may point at an old toolkit, so the loader searches the default v12.x folders and calls `SetDllDirectoryA` for `nvrtc-builtins`; call from the main thread only (see docs/FFI.md).
