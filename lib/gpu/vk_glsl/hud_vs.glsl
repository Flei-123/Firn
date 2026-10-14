#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/hud_vs.glsl -- Vulkan port of lib/gpu/gl_hud_vs.glsl: HUD rectangles, one INSTANCE per rectangle, strip of 4
// vertices. Vertex buffer 0 (per instance, stride 20): location 0 = R32G32B32A32_SINT x0 y0 x1 y1 (pixels, top row = 0),
// location 1 = R32_UINT colour 0xAABBGGRR. u_size from the Frame UBO. Y: NDC +1 is the TOP row with the flipped viewport
// (see mc_fs.glsl), so the GL formula `1 - y / h * 2` is unchanged. z = 0.5 equals the window depth GL gets from z = 0.
#include "vk_frame.glsl"
layout(location = 0) in ivec4 a_rect;
layout(location = 1) in uint a_col;
layout(location = 0) flat out vec4 v_col;
void main() {
    vec2 c = vec2(float(gl_VertexIndex & 1), float((gl_VertexIndex >> 1) & 1));
    vec2 p = mix(vec2(a_rect.xy), vec2(a_rect.zw), c);
    gl_Position = vec4(p.x / u_size.x * 2.0 - 1.0, 1.0 - p.y / u_size.y * 2.0, 0.5, 1.0);
    v_col = vec4(float(a_col & 255u), float((a_col >> 8) & 255u), float((a_col >> 16) & 255u), float(a_col >> 24)) / 255.0;
}
