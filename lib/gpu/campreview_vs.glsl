#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/campreview_vs.glsl -- the vertex shader of the camera preview (lib/gpu/campreview.fi): one
// quad from gl_VertexID (a triangle strip of 4 vertices, no buffers). v_d = where the pixel lies in the
// DISPLAY rectangle, (0,0) top left. u_quad scales the quad around the middle (1,1 = whole target; less =
// letterbox for "contain").
precision highp float;
uniform vec2 u_quad;
out vec2 v_d;

void main() {
    vec2 p = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    v_d = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4((p * 2.0 - 1.0) * u_quad, 0.0, 1.0);
}
