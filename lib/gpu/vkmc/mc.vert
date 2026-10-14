#version 450
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vkmc/mc.vert -- Vulkan twin of lib/gpu/mc_vs.glsl (u_pool == 0 path: one section per draw, origin in a push constant).
// Interface contract (VK2 swaps the SPIR-V without touching vk_mc.fi): location 0 = uvec2 mesh vertex; set 0 binding 0 = UBO
// (rv, fog, fogc, tiles), binding 1 = sampler2DArray atlas; push constant = org.xyz + trans flag, fade, light.
// Vulkan clip space: depth 0..1 -> z is remapped here; y is flipped by the viewport (y = h, height = -h, Vulkan 1.1 core), so the host matrix stays the GL one AND the rasterisation rules match GL.
layout(location = 0) in uvec2 a_w;
layout(set = 0, binding = 0) uniform U {
    mat4 rv;
    vec4 fog;    // x = on, y = start, z = end
    vec4 fogc;   // fog colour 0..255
    vec4 tiles;  // x = tile edge in texels
} u;
layout(push_constant) uniform P {
    vec4 org;    // xyz = section origin minus camera, w = 1 translucent pass
    vec4 fade;   // x = fade a (>= 64 off), y = inverted, z = frame height
    vec4 light;  // x = day 0..256, yzw = block tint 0..256
} pc;
layout(location = 0) out vec2 v_light;
layout(location = 1) out vec3 v_uvao;
layout(location = 2) flat out int v_tile;
layout(location = 3) flat out int v_fk;
layout(location = 4) out float v_depth;
const int FK[8] = int[8](204, 204, 128, 256, 153, 153, 256, 256);
void main() {
    uint w0 = a_w.x;
    uint w1 = a_w.y;
    vec3 p = vec3(float(w0 & 255u), float((w0 >> 8) & 255u), float((w0 >> 16) & 255u)) + pc.org.xyz;
    vec4 c = u.rv * vec4(p, 1.0);
    gl_Position = vec4(c.x, c.y, (c.z + c.w) * 0.5, c.w); // y stays GL: the viewport has a negative height
    v_depth = c.w;
    v_uvao = vec3(float((w1 >> 14) & 31u), float((w1 >> 19) & 31u), float((w0 >> 27) & 3u));
    v_tile = int(w1 & 16383u);
    v_light = vec2(float((w1 >> 24) & 15u), float(w1 >> 28));
    v_fk = FK[int((w0 >> 24) & 7u)];
}
