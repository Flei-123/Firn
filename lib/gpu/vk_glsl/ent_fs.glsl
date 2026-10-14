#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/ent_fs.glsl -- Vulkan port of lib/gpu/gl_ent_fs.glsl: entity box shading = mc_raster shading with ao = 3, then fog.
// Differences to GL: uniforms in the Frame UBO, interface locations.
#include "vk_frame.glsl"
layout(location = 0) flat in vec3 v_col;
layout(location = 1) flat in int v_fk;
layout(location = 2) in float v_depth;
layout(location = 0) out vec4 o_color;
void main() {
    ivec3 c = (ivec3(v_col + 0.5) * v_fk) >> 8;
    if (u_fog.x > 0.5) {
        float tt = clamp((v_depth - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, 1.0);
}
