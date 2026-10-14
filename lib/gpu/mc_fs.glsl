#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/mc_fs.glsl -- fragment shader of the voxel back end. u_filter == 0 (default): the arithmetic is the INTEGER
// shading of lib/mc/mc_raster.fi (docs/mc/raster.md 6-8): nearest texel with tile-local wrap, alpha cut-out at 128,
// fq = floor(100 + 52 ao), total = fq * face_k >> 8, channel * total >> 8, fog in 1/256 steps.
// u_filter == 1: the same shading on a MIPMAPPED + anisotropic sample of the tile (texture array layer = tile - 1, GL_REPEAT
// wraps a greedy quad inside its own layer, so mip levels never bleed into the neighbour tile); cut-out at alpha 0.5.
uniform sampler2DArray u_atlas;    // atlas page 0 (the whole atlas when it fits into GL_MAX_ARRAY_TEXTURE_LAYERS)
uniform sampler2DArray u_atlas1;   // atlas pages 1..3 (gl_mc.be_atlas splits a bigger atlas); unused ones alias page 0
uniform sampler2DArray u_atlas2;
uniform sampler2DArray u_atlas3;
uniform int u_pagel;    // layers per page; >= 1048576 = single array (page 0 only, the plain texture() path)
uniform vec2 u_tiles;   // x = tile edge in texels (power of two), y = unused (kept for the host code)
uniform vec4 u_fog;     // x = 1 fog on, y = start, z = end
uniform vec3 u_fogc;    // fog colour 0..255
uniform int u_trans;    // 1 = translucent pass: discard only alpha 0, output alpha = texel alpha (blend SRC_ALPHA, ONE_MINUS_SRC_ALPHA)
uniform int u_filter;   // 0 = exact texelFetch like mc_raster, 1 = filtered (mipmaps / anisotropy)
uniform vec4 u_fade;    // x = fade a (>= 64 off), y = 1 inverted, z = frame height: screen-door of the LOD cross-fade (mc_raster.bayer8)
uniform vec4 u_light;   // x = day (sky light multiplier 0..256), yzw = block light tint per channel 0..256
in vec2 v_light;
in vec3 v_uvao;
flat in int v_tile;
flat in int v_fk;
in float v_depth;
out vec4 o_color;
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
    int tsz = int(u_tiles.x);
    ivec4 t;
    int tl = v_tile - 1;
    int pg = 0;
    if (u_pagel < 1048576) {
        pg = min(tl / u_pagel, 3);
        tl = tl - pg * u_pagel;
    }
    if (u_filter == 0) {
        int tx = int(floor(v_uvao.x * float(tsz))) & (tsz - 1);
        int ty = int(floor(v_uvao.y * float(tsz))) & (tsz - 1);
        t = ivec4(fetch_page(pg, ivec3(tx, ty, tl)) * 255.0 + 0.5);
        if (u_trans == 0 ? t.a < 128 : t.a < 1) discard;
    } else {
        vec2 gdx = dFdx(v_uvao.xy);
        vec2 gdy = dFdy(v_uvao.xy);
        vec4 s;
        if (u_pagel >= 1048576) s = texture(u_atlas, vec3(v_uvao.xy, float(tl)));
        else s = grad_page(pg, vec3(v_uvao.xy, float(tl)), gdx, gdy);
        if (u_trans == 0 ? s.a < 0.5 : s.a < 0.004) discard;
        t = ivec4(s * 255.0 + 0.5);
    }
    if (u_fade.x < 63.5) {
        int px = int(gl_FragCoord.x);
        int py = int(u_fade.z) - 1 - int(gl_FragCoord.y);
        int xr = (px ^ py) & 7;
        int yy = py & 7;
        int bay = ((xr & 1) << 5) | ((yy & 1) << 4) | ((xr & 2) << 2) | ((yy & 2) << 1) | ((xr & 4) >> 1) | ((yy & 4) >> 2);
        int fa = int(u_fade.x + 0.5);
        if (u_fade.y > 0.5 ? bay < fa : bay >= fa) discard;
    }
    int fq = clamp(int(floor(100.0 + 52.0 * v_uvao.z)), 0, 256);
    int total = (fq * v_fk) >> 8;
    // light: same integer arithmetic as mc_raster.fi (s256 / b256 / per channel factor 24..256)
    int s256 = clamp(int(floor(v_light.x * u_light.x / 15.0 + 0.5)), 0, 256);
    int b256 = clamp(int(floor(v_light.y * 256.0 / 15.0 + 0.5)), 0, 256);
    ivec3 lum = max(ivec3(s256), (b256 * ivec3(u_light.yzw)) >> 8);
    ivec3 lf = 24 + ((lum * 232) >> 8);
    ivec3 tot = (total * lf) >> 8;
    ivec3 c = (t.rgb * tot) >> 8;
    if (u_fog.x > 0.5) {
        float tt = clamp((v_depth - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, u_trans == 0 ? 1.0 : float(t.a) / 255.0);
}
