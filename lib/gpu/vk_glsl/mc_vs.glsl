#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/mc_vs.glsl -- Vulkan port of lib/gpu/mc_vs.glsl (voxel vertex shader). Same arithmetic, same vertex layout.
// Vertex input: binding 0, stride 8, location 0 = R32G32_UINT (the 8 byte mc_mesh vertex, lib/mc/mc_mesh.fi).
// Differences to GL (everything else is line for line the same, tools/gpu/mkspv.sh --parity lists them):
//   * u_origin / u_pool are push constants (vkCmdPushConstants per draw) instead of loose uniforms
//   * u_gran / u_org are texel buffers at bindings 5 / 6
//   * gl_VertexID -> gl_VertexIndex (= index + vertexOffset, the same value glDrawElementsBaseVertex gives)
//   * clip z remapped from OpenGL's -w..w to Vulkan's 0..w (v_depth is taken before: it is w)
#include "vk_frame.glsl"
layout(push_constant) uniform Obj {
    vec3 u_origin;      // section origin minus camera (f64 on the CPU); pool == 0: one draw per section
    int u_pool;         // 1: origin = u_org[u_gran[vertex / 64]] (one multi-draw for many sections)
};
layout(set = 0, binding = 5) uniform usamplerBuffer u_gran;
layout(set = 0, binding = 6) uniform samplerBuffer u_org;
layout(location = 0) in uvec2 a_w;
layout(location = 0) out vec2 v_light;      // vertex light: sky 0..15, block 0..15
layout(location = 1) out vec3 v_uvao;       // tile repeat u, v and ambient occlusion 0..3
layout(location = 2) flat out int v_tile;
layout(location = 3) flat out int v_fk;     // face brightness out of 256
layout(location = 4) out float v_depth;     // view depth (distance along the view direction), for fog
const int FK[8] = int[8](204, 204, 128, 256, 153, 153, 256, 256);
void main() {
    uint w0 = a_w.x;
    uint w1 = a_w.y;
    vec3 org = u_origin;
    if (u_pool != 0) {
        int slot = int(texelFetch(u_gran, gl_VertexIndex >> 6).r);
        org = texelFetch(u_org, slot).xyz;
    }
    vec3 p = vec3(float(w0 & 255u), float((w0 >> 8) & 255u), float((w0 >> 16) & 255u)) + org;
    gl_Position = u_rv * vec4(p, 1.0);
    v_depth = gl_Position.w;
    // r261: snap the vertex to the 1/16 pixel grid of lib/mc/mc_raster.fi (snap()), so llvmpipe / lavapipe / a GPU (1/256 or 1/4096 px) see the
    // same triangle corners as the CPU rasterizer. u_fade.w = frame width in pixels (0 = off); u_fade.z = frame height.
    if (u_fade.w > 0.5 && gl_Position.w > 0.0001) {
        vec2 hs = vec2(u_fade.w, u_fade.z) * 0.5;
        vec2 ndc = gl_Position.xy / gl_Position.w;
        vec2 sc = floor(vec2(hs.x + ndc.x * hs.x, hs.y - ndc.y * hs.y) * 16.0 + 0.5) * 0.0625;
        gl_Position.xy = vec2((sc.x - hs.x) / hs.x, (hs.y - sc.y) / hs.y) * gl_Position.w;
    }
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    v_uvao = vec3(float((w1 >> 14) & 31u), float((w1 >> 19) & 31u), float((w0 >> 27) & 3u));
    v_tile = int(w1 & 16383u);
    v_light = vec2(float((w1 >> 24) & 15u), float(w1 >> 28));
    v_fk = FK[int((w0 >> 24) & 7u)];
}
