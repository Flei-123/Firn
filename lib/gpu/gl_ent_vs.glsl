#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_ent_vs.glsl -- instanced entity boxes (lib/gpu/gl_scene.fi). One unit cube (24 vertices), one instance per entity:
//   i0 = centre minus camera (f64 on the CPU -> f32), yaw sine     i1 = size xyz, yaw cosine     i2 = colour 0..255
layout(location = 0) in vec3 a_pos;
layout(location = 1) in float a_face;
layout(location = 2) in vec4 a_i0;
layout(location = 3) in vec4 a_i1;
layout(location = 4) in vec4 a_i2;
uniform mat4 u_rv;
flat out vec3 v_col;
flat out int v_fk;
out float v_depth;
const int FK[8] = int[8](204, 204, 128, 256, 153, 153, 256, 256);
void main() {
    vec3 q = a_pos * a_i1.xyz;
    float s = a_i0.w;
    float c = a_i1.w;
    vec3 p = vec3(q.x * c + q.z * s, q.y, 0.0 - q.x * s + q.z * c) + a_i0.xyz;
    gl_Position = u_rv * vec4(p, 1.0);
    v_depth = gl_Position.w;
    v_col = a_i2.xyz;
    v_fk = FK[int(a_face + 0.5)];
}
