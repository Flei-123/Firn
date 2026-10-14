#version 450
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/sky_vs.glsl -- Vulkan port of lib/gpu/gl_sky_vs.glsl: full-screen strip (4 vertices, no vertex buffer) for the
// sky AND the light tint pass. z = w = 1: depth 1.0 exactly in Vulkan too (its depth is z/w directly, no remap needed), so
// the tint pass can keep finding the pixels the world covered (depth test GREATER). Draw: vkCmdDraw(4, 1, 0, 0), TRIANGLE_STRIP.
void main() {
    vec2 p = vec2(float(gl_VertexIndex & 1), float((gl_VertexIndex >> 1) & 1));
    gl_Position = vec4(p * 2.0 - 1.0, 1.0, 1.0);
}
