#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/ent_vs.glsl -- Vulkan port of lib/gpu/gl_ent_vs.glsl: instanced entity boxes. One unit cube (24 vertices), one
// instance per entity. Vertex buffer 0 (per vertex, stride 16): location 0 = R32G32B32_SFLOAT position (-0.5..0.5 box corner),
// location 1 = R32_SFLOAT face number 0..7. Vertex buffer 1 (per INSTANCE, stride 48): location 2 = i0 (centre minus camera,
// yaw sine), location 3 = i1 (size xyz, yaw cosine), location 4 = i2 (colour 0..255), all R32G32B32A32_SFLOAT.
// Differences to GL: u_rv from the Frame UBO, interface locations, clip z remapped to 0..w (v_depth = w taken before).
#include "vk_frame.glsl"
layout(location = 0) in vec3 a_pos;
layout(location = 1) in float a_face;
layout(location = 2) in vec4 a_i0;
layout(location = 3) in vec4 a_i1;
layout(location = 4) in vec4 a_i2;
layout(location = 0) flat out vec3 v_col;
layout(location = 1) flat out int v_fk;
layout(location = 3) flat out float v_em;
layout(location = 2) out float v_depth;
const int FK[8] = int[8](204, 204, 128, 256, 153, 153, 256, 256);
void main() {
    vec3 q = a_pos * a_i1.xyz;
    float s = a_i0.w;
    float c = a_i1.w;
    vec3 p = vec3(q.x * c + q.z * s, q.y, 0.0 - q.x * s + q.z * c) + a_i0.xyz;
    gl_Position = u_rv * vec4(p, 1.0);
    v_depth = gl_Position.w;
    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;
    v_col = a_i2.xyz;
    v_em = a_i2.w;
    v_fk = FK[int(a_face + 0.5)];
    if (a_i2.w > 0.5) v_fk = 256;
}
