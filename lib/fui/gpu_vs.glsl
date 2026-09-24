#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/fui/gpu_vs.glsl -- the vertex shader of lib/fui/gpu.fi (one quad per instance)
precision highp float;
layout(location = 0) in vec2 a_corner;
layout(location = 1) in vec4 a_box;
layout(location = 2) in vec4 a_c0;
layout(location = 3) in vec4 a_c1;
layout(location = 4) in vec4 a_g;
layout(location = 5) in vec4 a_p;
layout(location = 6) in vec4 a_t;
layout(location = 7) in vec4 a_clip;
layout(location = 8) in vec4 a_k;
uniform vec2 u_size;
flat out vec4 v_c0;
flat out vec4 v_c1;
flat out vec4 v_g;
flat out vec4 v_p;
flat out vec4 v_t;
flat out vec4 v_clip;
flat out vec4 v_k;
void main() {
    vec2 q = mix(a_box.xy, a_box.zw, a_corner);
    gl_Position = vec4(q.x / u_size.x * 2.0 - 1.0, 1.0 - q.y / u_size.y * 2.0, 0.0, 1.0);
    v_c0 = a_c0; v_c1 = a_c1; v_g = a_g; v_p = a_p; v_t = a_t; v_clip = a_clip; v_k = a_k;
}
