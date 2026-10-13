#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/mc_fs.glsl -- fragment shader of the voxel back end. u_filter == 0 (default): the arithmetic is the INTEGER
// shading of lib/mc/mc_raster.fi (docs/mc/raster.md 6-8): nearest texel with tile-local wrap, alpha cut-out at 128,
// fq = floor(100 + 52 ao), total = fq * face_k >> 8, channel * total >> 8, fog in 1/256 steps.
// u_filter == 1: the same shading on a MIPMAPPED + anisotropic sample of the tile (texture array layer = tile - 1, GL_REPEAT
// wraps a greedy quad inside its own layer, so mip levels never bleed into the neighbour tile); cut-out at alpha 0.5.
uniform sampler2DArray u_atlas;
uniform vec2 u_tiles;   // x = tile edge in texels (power of two), y = unused (kept for the host code)
uniform vec4 u_fog;     // x = 1 fog on, y = start, z = end
uniform vec3 u_fogc;    // fog colour 0..255
uniform int u_trans;    // 1 = translucent pass: discard only alpha 0, output alpha = texel alpha (blend SRC_ALPHA, ONE_MINUS_SRC_ALPHA)
uniform int u_filter;   // 0 = exact texelFetch like mc_raster, 1 = filtered (mipmaps / anisotropy)
uniform vec4 u_light;   // x = day (sky light multiplier 0..256), yzw = block light tint per channel 0..256
in vec2 v_light;
in vec3 v_uvao;
flat in int v_tile;
flat in int v_fk;
in float v_depth;
out vec4 o_color;
void main() {
    int tsz = int(u_tiles.x);
    ivec4 t;
    if (u_filter == 0) {
        int tx = int(floor(v_uvao.x * float(tsz))) & (tsz - 1);
        int ty = int(floor(v_uvao.y * float(tsz))) & (tsz - 1);
        t = ivec4(texelFetch(u_atlas, ivec3(tx, ty, v_tile - 1), 0) * 255.0 + 0.5);
        if (u_trans == 0 ? t.a < 128 : t.a < 1) discard;
    } else {
        vec4 s = texture(u_atlas, vec3(v_uvao.xy, float(v_tile - 1)));
        if (u_trans == 0 ? s.a < 0.5 : s.a < 0.004) discard;
        t = ivec4(s * 255.0 + 0.5);
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
