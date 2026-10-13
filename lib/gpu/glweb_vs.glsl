#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/glweb_vs.glsl -- vertex shader of the WebGL2 back end (lib/gpu/glweb.fi), GLSL ES 3.00.
// Same arithmetic as lib/gpu/mc_vs.glsl (GL 3.3), minus the pool path: WebGL2 has no buffer textures
// (usamplerBuffer / samplerBuffer), no base vertex draws and no gl_DrawID, so the section origin is always the
// uniform u_origin (one draw call per section = MODE_LEGACY of gl_mc.fi). docs/WEBGL2.md section 3.
precision highp float;
precision highp int;
layout(location = 0) in uvec2 a_w;
uniform mat4 u_rv;
uniform vec3 u_origin;
out vec2 v_light;      // vertex light: sky 0..15, block 0..15 (smooth lighting, interpolated)
out vec3 v_uvao;
flat out int v_tile;
flat out int v_fk;
out float v_depth;
const int FK[8] = int[8](204, 204, 128, 256, 153, 153, 256, 256);
void main() {
    uint w0 = a_w.x;
    uint w1 = a_w.y;
    vec3 p = vec3(float(w0 & 255u), float((w0 >> 8) & 255u), float((w0 >> 16) & 255u)) + u_origin;
    gl_Position = u_rv * vec4(p, 1.0);
    v_depth = gl_Position.w;
    v_uvao = vec3(float((w1 >> 14) & 31u), float((w1 >> 19) & 31u), float((w0 >> 27) & 3u));
    v_tile = int(w1 & 16383u);
    v_light = vec2(float((w1 >> 24) & 15u), float(w1 >> 28));
    v_fk = FK[int((w0 >> 24) & 7u)];
}
