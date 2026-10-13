#version 300 es
precision highp float;
precision highp int;
// ES 3.00 twin of gl_sky_fs.glsl for WebGL2 (docs/WEBGL2.md): body identical except where noted; keep in sync.
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/gl_sky_fs.glsl -- the background pass of lib/mc/mc_sky.fi (sky_draw_plain, docs/mc/sky.md) per pixel on the GPU:
// view ray from the camera basis, gradient index f = floor(256 gg), integer lerps (a (256-t) + b t) >> 8, sunset glow,
// sun / moon disc. Float maths instead of f64: pixels may differ by 1 level or at a disc / glow threshold.
uniform vec3 u_r;       // camera right / up / forward (world)
uniform vec3 u_u;
uniform vec3 u_f;
uniform vec4 u_cam;     // half width, half height, focal length x, focal length y (pixels)
uniform float u_h;      // frame height
uniform vec3 u_zen;     // zenith colour 0..255
uniform vec3 u_hor;     // horizon colour
uniform vec3 u_glc;     // sunset glow colour
uniform vec4 u_sun;     // xyz = unit direction to the sun, w = glow strength 0..256
uniform int u_discs;
out vec4 o_color;
ivec3 lerp3(ivec3 a, ivec3 b, int t) {
    return (a * (256 - t) + b * t) >> 8;
}
void main() {
    float ex = (gl_FragCoord.x - u_cam.x) / u_cam.z;
    float ey = (u_cam.y - (u_h - gl_FragCoord.y)) / u_cam.w;
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
