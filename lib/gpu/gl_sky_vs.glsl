#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_sky_vs.glsl -- full-screen strip (4 vertices, no vertex buffer) for the sky and the light tint pass of
// lib/gpu/gl_scene.fi. z = w = 1: depth 1.0 exactly (the tint pass uses that to find the pixels the world covered).
void main() {
    vec2 p = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
    gl_Position = vec4(p * 2.0 - 1.0, 1.0, 1.0);
}
