#!/usr/bin/env bash
# tools/gpu/ext/run.sh -- lib/gpu building blocks (docs/GPU.md "Renderer building blocks"):
#   cull.fi : CPU frustum / occlusion culling against brute-force references (no GL needed)
#   gl.fi   : program + uniform cache + UBO + instancing + multi indirect + timer queries + textures + MRT/MSAA (Mesa llvmpipe)
# Prints SKIP for the GL part when libEGL is missing. Usage: bash tools/gpu/ext/run.sh [firnc]
set -u
cd "$(dirname "$0")/../../.."
FIRNC=${1:-${FIRNC:-compiler/target/release/firnc}}
export FIRNLIB="$(pwd)/lib"
D=$(mktemp -d)
trap 'rm -rf "$D"' EXIT
"$FIRNC" -o "$D/cull" tools/gpu/ext/cull.fi || { echo "   FAIL: compile cull.fi"; exit 1; }
timeout 300 "$D/cull" | sed 's/^/   /'; rc=${PIPESTATUS[0]}
[ "$rc" -eq 0 ] || { echo "   FAIL: cull exit $rc"; exit 1; }
echo "   ok:   gpu.cull vs brute force (frustum, far origin, occlusion)"
[ -e /usr/lib/x86_64-linux-gnu/libEGL.so.1 ] || { echo "   SKIP: no libEGL.so.1 (GL part)"; exit 0; }
"$FIRNC" -o "$D/gl" tools/gpu/ext/gl.fi || { echo "   FAIL: compile gl.fi"; exit 1; }
env -u DISPLAY -u EGL_PLATFORM timeout 300 "$D/gl" | sed 's/^/   /'; rc=${PIPESTATUS[0]}
[ "$rc" -eq 0 ] || { echo "   FAIL: gl exit $rc"; exit 1; }
echo "   ok:   lib/gpu building blocks on llvmpipe"
