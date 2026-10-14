#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/sky_fs.glsl -- Vulkan port of lib/gpu/gl_sky_fs.glsl: the background pass of lib/mc/mc_sky.fi per pixel
// (view ray from the camera basis, gradient, sunset glow, sun / moon discs). Differences to GL: uniforms are in the Frame UBO;
// gl_FragCoord.y is already the row counted from the top (flipped viewport, see mc_fs.glsl), so `u_h - gl_FragCoord.y` becomes
// `gl_FragCoord.y`. Float maths as in GL: pixels may differ by 1 level or at a disc / glow threshold.
#include "vk_frame.glsl"
layout(location = 0) out vec4 o_color;
ivec3 lerp3(ivec3 a, ivec3 b, int t) {
    return (a * (256 - t) + b * t) >> 8;
}
void main() {
    float ex = (gl_FragCoord.x - u_cam.x) / u_cam.z;
    float ey = (u_cam.y - gl_FragCoord.y) / u_cam.w;
    vec3 d = u_f + u_r * ex + u_u * ey;
    vec3 n = d / sqrt(dot(d, d));
    float g = clamp(n.y * 1.6, 0.0, 1.0);
    float gg = 1.0 - (1.0 - g) * (1.0 - g);
    int f = int(floor(gg * 256.0));
    ivec3 c = lerp3(ivec3(u_hor + 0.5), ivec3(u_zen + 0.5), f);
    float dsun = dot(n, u_sun.xyz);
    if (dsun > 0.0 && u_sun.w > 0.0) {
        float d3 = (dsun * dsun) * dsun;
        int fg = int(floor(d3 * (1.0 - gg) * u_sun.w));
        c = lerp3(c, ivec3(u_glc + 0.5), fg);
    }
    if (u_discs != 0) {
        if (dsun >= 0.9976) {
            c = ivec3(255, 248, 200);
        } else if (-dsun >= 0.9984) {
            c = ivec3(226, 228, 240);
        }
    }
    o_color = vec4(vec3(c) / 255.0, 1.0);
}
