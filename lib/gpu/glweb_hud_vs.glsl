#version 300 es
precision highp float;
precision highp int;
// ES 3.00 twin of gl_hud_vs.glsl for WebGL2 (docs/WEBGL2.md): body identical except where noted; keep in sync.
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_hud_vs.glsl -- HUD rectangles (one instance per rectangle recorded by mc_hud.hud_record): x0 y0 x1 y1 in pixels,
// top row = 0, plus the colour 0xAABBGGRR. Strip of 4 vertices per instance.
layout(location = 0) in uvec4 a_rect;   // ES: attribute base type must match vertexAttribIPointer(UNSIGNED_INT)
layout(location = 1) in uint a_col;
uniform vec2 u_size;
flat out vec4 v_col;
void main() {
    vec2 c = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
    vec2 p = mix(vec2(a_rect.xy), vec2(a_rect.zw), c);
    gl_Position = vec4(p.x / u_size.x * 2.0 - 1.0, 1.0 - p.y / u_size.y * 2.0, 0.0, 1.0);
    v_col = vec4(float(a_col & 255u), float((a_col >> 8) & 255u), float((a_col >> 16) & 255u), float(a_col >> 24)) / 255.0;
}
