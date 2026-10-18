#version 450
#extension GL_GOOGLE_include_directive : require
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/vk_glsl/mc_model_fs.glsl -- Vulkan port of lib/gpu/mc_model_fs.glsl (MODEL quad pass fragment shader = mc_fs + tint): the INTEGER shading of
// lib/mc/mc_raster.fi, unchanged. u_filter == 0: nearest texel with tile-local wrap, cut-out at alpha 128; u_filter == 1: mipmapped
// + anisotropic sample (layer = tile - 1). Differences to GL:
//   * uniforms live in the Frame UBO (vk_frame.glsl), atlas pages are bindings 1..4
//   * gl_FragCoord.y is already counted from the TOP in Vulkan (framebuffer origin upper left), so the screen-door row is
//     `py = int(gl_FragCoord.y)` instead of `u_fade.z - 1 - int(gl_FragCoord.y)`. The pipeline MUST be drawn with the flipped
//     viewport (y = height, height = -height, core in Vulkan 1.1 / VK_KHR_maintenance1): then the same GL clip coordinates
//     give an upright image, front faces stay counter-clockwise and gl_FragCoord.y = row + 0.5.
#include "vk_frame.glsl"
layout(set = 0, binding = 1) uniform sampler2DArray u_atlas;
layout(set = 0, binding = 2) uniform sampler2DArray u_atlas1;
layout(set = 0, binding = 3) uniform sampler2DArray u_atlas2;
layout(set = 0, binding = 4) uniform sampler2DArray u_atlas3;
layout(location = 0) in vec2 v_light;
layout(location = 1) in vec3 v_uvao;
layout(location = 2) flat in int v_tile;
layout(location = 3) flat in int v_fk;
layout(location = 4) in float v_depth;
layout(location = 5) flat in int v_tint;
layout(location = 0) out vec4 o_color;
// page `pg` = tile / u_pagel (flat per primitive), layer inside it = tile % u_pagel
vec4 fetch_page(int pg, ivec3 c) {
    if (pg == 0) return texelFetch(u_atlas, c, 0);
    if (pg == 1) return texelFetch(u_atlas1, c, 0);
    if (pg == 2) return texelFetch(u_atlas2, c, 0);
    return texelFetch(u_atlas3, c, 0);
}
// explicit gradients (taken before the branches) so the mip level / anisotropy do not depend on the divergent page branch
vec4 grad_page(int pg, vec3 c, vec2 dx, vec2 dy) {
    if (pg == 0) return textureGrad(u_atlas, c, dx, dy);
    if (pg == 1) return textureGrad(u_atlas1, c, dx, dy);
    if (pg == 2) return textureGrad(u_atlas2, c, dx, dy);
    return textureGrad(u_atlas3, c, dx, dy);
}
void main() {
    // r266: undo the r265 tie bias (vertices moved 1/256 px) for the interpolated attributes: g(y) = f(y + e) => f = g - e * dFdy (Vulkan window y is down)
    vec3 uvao = v_uvao - 0.00390625 * dFdy(v_uvao);
    vec2 lightv = v_light - 0.00390625 * dFdy(v_light);
    float depthv = v_depth - 0.00390625 * dFdy(v_depth);
    int tsz = int(u_tiles.x);
    ivec4 t;
    int tl = v_tile - 1;
    int pg = 0;
    if (u_pagel < 1048576) {
        pg = min(tl / u_pagel, 3);
        tl = tl - pg * u_pagel;
    }
    if (u_filter == 0) {
        int tx = int(floor(uvao.x * float(tsz))) & (tsz - 1);
        int ty = int(floor(uvao.y * float(tsz))) & (tsz - 1);
        t = ivec4(fetch_page(pg, ivec3(tx, ty, tl)) * 255.0 + 0.5);
        if (u_trans == 0 ? t.a < 128 : t.a < 1) discard;
    } else {
        vec2 gdx = dFdx(uvao.xy);
        vec2 gdy = dFdy(uvao.xy);
        vec4 s;
        if (u_pagel >= 1048576) s = texture(u_atlas, vec3(uvao.xy, float(tl)));
        else s = grad_page(pg, vec3(uvao.xy, float(tl)), gdx, gdy);
        if (u_trans == 0 ? s.a < 0.5 : s.a < 0.004) discard;
        t = ivec4(s * 255.0 + 0.5);
    }
    if (v_tint >= 0) t.rgb = (t.rgb * (ivec3(v_tint & 255, (v_tint >> 8) & 255, (v_tint >> 16) & 255) + 1)) >> 8;
    if (u_fade.x < 63.5) {
        int px = int(gl_FragCoord.x);
        int py = int(gl_FragCoord.y);
        int xr = (px ^ py) & 7;
        int yy = py & 7;
        int bay = ((xr & 1) << 5) | ((yy & 1) << 4) | ((xr & 2) << 2) | ((yy & 2) << 1) | ((xr & 4) >> 1) | ((yy & 4) >> 2);
        int fa = int(u_fade.x + 0.5);
        if (u_fade.y > 0.5 ? bay < fa : bay >= fa) discard;
    }
    int fq = clamp(int(floor(100.0 + 52.0 * uvao.z)), 0, 256);
    int total = (fq * v_fk) >> 8;
    // light: same integer arithmetic as mc_raster.fi (s256 / b256 / per channel factor 24..256)
    int s256 = clamp(int(floor(lightv.x * u_light.x / 15.0 + 0.5)), 0, 256);
    int b256 = clamp(int(floor(lightv.y * 256.0 / 15.0 + 0.5)), 0, 256);
    ivec3 lum = max(ivec3(s256), (b256 * ivec3(u_light.yzw)) >> 8);
    ivec3 lf = 24 + ((lum * 232) >> 8);
    ivec3 tot = (total * lf) >> 8;
    ivec3 c = (t.rgb * tot) >> 8;
    if (u_fog.x > 0.5) {
        float tt = clamp((depthv - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, u_trans == 0 ? 1.0 : float(t.a) / 255.0);
}
