#!/bin/sh
# SPDX-License-Identifier: MPL-2.0
# lib/fui/spv/build.sh -- builds the SPIR-V of the fUi VULKAN backend (lib/fui/gpu_vk.fi) from the SAME GLSL ES 3.00 sources
# that the GLES / WebGL2 backend (lib/fui/gpu.fi) uses: gpu_vs.glsl, gpu_fs.glsl, gpu_fs_photo.glsl. One source of truth, so the
# arithmetic of every quad kind is identical on all backends; only the interface lines are rewritten for Vulkan:
#   #version 300 es      -> #version 450
#   precision ...;       -> dropped
#   uniform vec2/int/... -> one std140 block `P` at set 0 binding 0 (u_size, u_srcsize, u_nspot, u_spot[8]; 160 octets)
#   uniform sampler2D    -> set 0 binding 1 (u_atlas), 2 (u_stream), 3 (u_src)
#   flat in/out vec4 v_x -> explicit locations 0..6 (c0 c1 g p t clip k), out vec4 o -> location 0
# The .spv files are committed; the tests do not need the tools. Needs glslangValidator + spirv-val.
set -e
cd "$(dirname "$0")"
conv() {
    sed -e 's/^#version 300 es/#version 450\nlayout(set = 0, binding = 0, std140) uniform P { vec2 u_size; vec2 u_srcsize; int u_nspot; vec4 u_spot[8]; };/' \
        -e '/^precision /d' \
        -e '/^uniform vec2 u_size;/d' -e '/^uniform vec2 u_srcsize;/d' -e '/^uniform vec4 u_spot\[8\];/d' -e '/^uniform int u_nspot;/d' \
        -e 's/^uniform sampler2D u_atlas;/layout(set = 0, binding = 1) uniform sampler2D u_atlas;/' \
        -e 's/^uniform sampler2D u_stream;/layout(set = 0, binding = 2) uniform sampler2D u_stream;/' \
        -e 's/^uniform sampler2D u_src;/layout(set = 0, binding = 3) uniform sampler2D u_src;/' \
        -e 's/^flat \(in\|out\) vec4 v_c0;/layout(location = 0) flat \1 vec4 v_c0;/' \
        -e 's/^flat \(in\|out\) vec4 v_c1;/layout(location = 1) flat \1 vec4 v_c1;/' \
        -e 's/^flat \(in\|out\) vec4 v_g;/layout(location = 2) flat \1 vec4 v_g;/' \
        -e 's/^flat \(in\|out\) vec4 v_p;/layout(location = 3) flat \1 vec4 v_p;/' \
        -e 's/^flat \(in\|out\) vec4 v_t;/layout(location = 4) flat \1 vec4 v_t;/' \
        -e 's/^flat \(in\|out\) vec4 v_clip;/layout(location = 5) flat \1 vec4 v_clip;/' \
        -e 's/^flat \(in\|out\) vec4 v_k;/layout(location = 6) flat \1 vec4 v_k;/' \
        -e 's/^out vec4 o;/layout(location = 0) out vec4 o;/' "$1"
}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
conv ../gpu_vs.glsl > "$T/gpu_vk.vert"
conv ../gpu_fs.glsl > "$T/gpu_vk.frag"
conv ../gpu_fs_photo.glsl > "$T/gpu_vk_photo.frag"
for s in gpu_vk.vert gpu_vk.frag gpu_vk_photo.frag; do
    glslangValidator -V --target-env vulkan1.0 -o "$s.spv" "$T/$s" > "$T/$s.log" 2>&1 || { cat "$T/$s.log"; exit 1; }
    spirv-val --target-env vulkan1.0 "$s.spv"
done
ls -l *.spv
