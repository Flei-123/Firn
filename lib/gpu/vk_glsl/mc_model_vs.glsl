#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/mc_model_vs.glsl -- Vulkan port of lib/gpu/mc_model_vs.glsl (vertex shader of the MODEL quad pass).
// Float vertices (12 per vertex, stride 48): pos xyz (section relative, block units) | u v tile fk | sky block tint ao.
// Differences to GL: u_rv lives in the Frame UBO, u_origin is a push constant (same Obj block as mc_vs.glsl, u_pool unused),
// clip z remapped from -w..w to 0..w (v_depth is taken before).
#include "vk_frame.glsl"
layout(push_constant) uniform Obj {
    vec3 u_origin;      // section origin minus camera (f64 on the CPU)
    int u_pool;         // unused by the model pass (kept so the push range is the same as the voxel pass)
};
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_uvt;   // u, v in tile units, tile id, face brightness 0..256
layout(location = 2) in vec4 a_lt;    // sky, block light 0..15, tint (R | G << 8 | B << 16, -1 none), ao 0..3
layout(location = 0) out vec2 v_light;
layout(location = 1) out vec3 v_uvao;
layout(location = 2) flat out int v_tile;
layout(location = 3) flat out int v_fk;
layout(location = 4) out float v_depth;
layout(location = 5) flat out int v_tint;
void main() {
    gl_Position = u_rv * vec4(a_pos + u_origin, 1.0);
    v_depth = gl_Position.w;
    // r261: snap the vertex to the 1/16 pixel grid of lib/mc/mc_raster.fi (snap()), so llvmpipe / lavapipe / a GPU (1/256 or 1/4096 px) see the
    // same triangle corners as the CPU rasterizer. u_fade.w = frame width in pixels (0 = off); u_fade.z = frame height.
    if (u_fade.w > 0.5 && gl_Position.w > 0.0001) {
        vec2 hs = vec2(u_fade.w, u_fade.z) * 0.5;
        vec2 ndc = gl_Position.xy / gl_Position.w;
        vec2 sc = floor(vec2(hs.x + ndc.x * hs.x, hs.y - ndc.y * hs.y) * 16.0 + 0.5) * 0.0625;
        sc.y -= 0.00390625;
        gl_Position.xy = vec2((sc.x - hs.x) / hs.x, (hs.y - sc.y) / hs.y) * gl_Position.w;
    }
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    v_uvao = vec3(a_uvt.xy, a_lt.w);
    v_tile = int(a_uvt.z + 0.5);
    v_fk = int(a_uvt.w + 0.5);
    v_light = a_lt.xy;
    v_tint = int(a_lt.z); // exact: tint <= 0xFFFFFF fits a float, +0.5 would not
}
