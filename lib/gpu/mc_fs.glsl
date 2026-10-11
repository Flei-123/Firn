#version 330 core
// SPDX-License-Identifier: MPL-2.0
// lib/gpu/mc_fs.glsl -- fragment shader of the voxel back end. The arithmetic is the INTEGER shading of
// lib/mc/mc_raster.fi (docs/mc/raster.md 6-8): nearest texel with tile-local wrap, alpha cut-out at 128,
// fq = floor(100 + 52 ao), total = fq * face_k >> 8, channel * total >> 8, fog in 1/256 steps.
uniform sampler2D u_atlas;
uniform vec2 u_tiles;   // x = tile edge in texels (power of two), y = atlas columns (floats: no glUniform2i wrapper needed)
uniform vec4 u_fog;     // x = 1 fog on, y = start, z = end
uniform vec3 u_fogc;    // fog colour 0..255
in vec3 v_uvao;
flat in int v_tile;
flat in int v_fk;
in float v_depth;
out vec4 o_color;
void main() {
    ivec2 tl = ivec2(u_tiles);
    int tsz = tl.x;
    int tx = int(floor(v_uvao.x * float(tsz))) & (tsz - 1);
    int ty = int(floor(v_uvao.y * float(tsz))) & (tsz - 1);
    int col = (v_tile - 1) % tl.y;
    int row = (v_tile - 1) / tl.y;
    ivec4 t = ivec4(texelFetch(u_atlas, ivec2(col * tsz + tx, row * tsz + ty), 0) * 255.0 + 0.5);
    if (t.a < 128) discard;
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
