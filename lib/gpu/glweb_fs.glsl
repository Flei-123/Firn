#version 300 es
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/glweb_fs.glsl -- fragment shader of the WebGL2 back end (lib/gpu/glweb.fi), GLSL ES 3.00.
// The integer shading of lib/gpu/mc_fs.glsl with u_filter == 0 (exact texelFetch like mc_raster): nearest texel with
// tile-local wrap, alpha cut-out at 128, fq = floor(100 + 52 ao), total = fq * face_k >> 8, channel * total >> 8, fog in
// 1/256 steps. The filtered path (mipmaps / anisotropy) is not in the prototype (docs/WEBGL2.md section 3).
precision highp float;
precision highp int;
precision highp sampler2DArray;
uniform sampler2DArray u_atlas;   // atlas page 0 (the whole atlas when it fits into MAX_ARRAY_TEXTURE_LAYERS)
uniform sampler2DArray u_atlas1;  // atlas pages 1..3 (glweb.be_atlas splits a bigger atlas); unused ones alias page 0
uniform sampler2DArray u_atlas2;
uniform sampler2DArray u_atlas3;
uniform int u_pagel;    // layers per page; >= 1048576 = single array (page 0 only)
uniform vec2 u_tiles;   // x = tile edge in texels (power of two)
uniform vec4 u_fog;     // x = 1 fog on, y = start, z = end
uniform vec3 u_fogc;    // fog colour 0..255
uniform vec4 u_light;   // x = day (sky light multiplier 0..256), yzw = block light tint per channel 0..256
uniform vec4 u_fade;    // x = fade a (>= 64 off), y = 1 inverted, z = frame height: screen-door of the LOD cross-fade (mc_raster.bayer8)
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
vec3 vanilla_light(float sl, float bl, float skyd256, float gamma256) {
    float sd = skyd256 / 256.0;
    float fs = sl / 15.0;
    float fb = bl / 15.0;
    float m = (fs / (4.0 - 3.0 * fs)) * (sd * 0.95 + 0.05);
    float n = (fb / (4.0 - 3.0 * fb)) * 1.5;
    vec3 v = vec3(n, n * ((n * 0.6 + 0.4) * 0.6 + 0.4), n * (n * n * 0.6 + 0.4));
    v += mix(vec3(sd, sd, 1.0), vec3(1.0), 0.35) * m;
    v = mix(v, vec3(0.75), 0.04);
    v = clamp(v, 0.0, 1.0);
    vec3 q = vec3(1.0) - v;
    q = q * q;
    vec3 gm = vec3(1.0) - q * q;
    v = mix(v, gm, gamma256 / 256.0);
    v = mix(v, vec3(0.75), 0.04);
    return clamp(v, 0.0, 1.0);
}
void main() {
    int tsz = int(u_tiles.x);
    int tx = int(floor(v_uvao.x * float(tsz))) & (tsz - 1);
    int ty = int(floor(v_uvao.y * float(tsz))) & (tsz - 1);
    int tl = v_tile - 1;
    int pg = 0;
    if (u_pagel < 1048576) {
        pg = min(tl / u_pagel, 3);
        tl = tl - pg * u_pagel;
    }
    ivec4 t = ivec4(fetch_page(pg, ivec3(tx, ty, tl)) * 255.0 + 0.5);
    if (t.a < 128) discard;
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
    int fkv = v_fk;
    ivec3 tot;
    if (u_light.x < 0.0) {
        if (fkv == 204) fkv = 153; else if (fkv == 153) fkv = 204;
        ivec3 lmc = ivec3(floor(vanilla_light(v_light.x, v_light.y, -1.0 - u_light.x, u_light.y) * 256.0 + 0.5));
        tot = (((fq * fkv) >> 8) * lmc) >> 8;
    } else {
        int total = (fq * fkv) >> 8;
        int s256 = clamp(int(floor(v_light.x * u_light.x / 15.0 + 0.5)), 0, 256);
        int b256 = clamp(int(floor(v_light.y * 256.0 / 15.0 + 0.5)), 0, 256);
        ivec3 lum = max(ivec3(s256), (b256 * ivec3(u_light.yzw)) >> 8);
        ivec3 lf = 24 + ((lum * 232) >> 8);
        tot = (total * lf) >> 8;
    }
    ivec3 c = (t.rgb * tot) >> 8;
    if (u_fog.x > 0.5) {
        float tt = clamp((v_depth - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, 1.0);
}
