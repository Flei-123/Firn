#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/tint_fs.glsl -- Vulkan port of lib/gpu/gl_tint_fs.glsl: light tint of mc_sky.sky_tint_frame. Vertex stage = sky_vs
// (strip at depth 1.0, depth test GREATER, no depth write); blend (DST_COLOR, ZERO) multiplies pixels that have geometry by u_tint / 256.
#include "vk_frame.glsl"
layout(location = 0) out vec4 o_color;
void main() {
    o_color = vec4(u_tint, 1.0);
}
