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
uniform int u_filter;   // 0 = exact texelFetch like mc_raster, 1 = filtered (mipmaps / anisotropy)
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
        if (t.a < 128) discard;
    } else {
        vec4 s = texture(u_atlas, vec3(v_uvao.xy, float(v_tile - 1)));
        if (s.a < 0.5) discard;
        t = ivec4(s * 255.0 + 0.5);
    }
    int fq = clamp(int(floor(100.0 + 52.0 * v_uvao.z)), 0, 256);
    int total = (fq * v_fk) >> 8;
    ivec3 c = (t.rgb * total) >> 8;
    if (u_fog.x > 0.5) {
        float tt = clamp((v_depth - u_fog.y) / (u_fog.z - u_fog.y), 0.0, 1.0);
        int f = int(floor(tt * 256.0));
        c = (c * (256 - f) + ivec3(u_fogc) * f) >> 8;
    }
    o_color = vec4(vec3(c) / 255.0, 1.0);
}
